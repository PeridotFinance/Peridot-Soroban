// TESTNET-ONLY helpers for the real-pool rehearsal. Keys never enter this process: transactions are
// built and simulated here, signed by `stellar tx sign --sign-with-key <identity-alias>` (the CLI holds
// the key) and submitted by RPC. Every call asserts the Testnet passphrase.
//   node tn.mjs measure <hash> [label]           -> one JSON row (footprint, writes, entries, fee, cpu/mem)
//   node tn.mjs measure-journal <journal> <out>  -> rows for every journalled hash not yet measured
//   node tn.mjs invest <state.env> <open-id>     -> 3 keeper observations on consecutive ledgers + stage_invest
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import fs from 'node:fs';
const RPC = 'https://soroban-testnet.stellar.org';
const PASS = 'Test SDF Network ; September 2015';
const SDK_DIR = process.env.STELLAR_SDK_DIR || '/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/bots/cash-liquidator';
const S = createRequire(SDK_DIR + '/package.json')('@stellar/stellar-sdk');
const rpc = new S.rpc.Server(RPC);
if (PASS !== S.Networks.TESTNET) throw Error('testnet only');
const cliEnv = () => { const e = {...process.env}; for (const k of Object.keys(e)) if (/^(STELLAR|SOROBAN)_/.test(k) && k !== 'STELLAR_CONFIG_HOME') delete e[k]; return e; };
const alias = a => { if (!/^[a-z][a-z0-9-]{1,40}$/.test(a)) throw Error('identity alias only'); return a; };
const latestSeq = async () => (await (await fetch(RPC, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json()).result.sequence;
const sleep = ms => new Promise(r => setTimeout(r, ms));

export async function measure(hash, label = '') {
  let tx;
  for (let i = 0; i < 30; i++) { tx = await rpc.getTransaction(hash); if (tx.status !== 'NOT_FOUND') break; await sleep(2000); }
  if (tx.status === 'NOT_FOUND') return {hash, label, status: 'NOT_FOUND'};
  const env = tx.envelopeXdr; const inner = env.switch().name === 'envelopeTypeTxFeeBump' ? env.feeBump().tx().innerTx().v1() : env.v1();
  const t = inner.tx(); const ext = t.ext(); const row = {hash, label, status: tx.status, ledger: tx.ledger, closeTime: tx.createdAt,
    feeCharged: Number(tx.resultXdr.feeCharged().toBigInt ? tx.resultXdr.feeCharged().toBigInt() : tx.resultXdr.feeCharged()), maxFee: Number(t.fee())};
  if (ext.switch() === 1 || ext.switch()?.value === 1) {
    const sd = ext.sorobanData(); const r = sd.resources(); const fp = r.footprint();
    const ro = fp.readOnly().length, rw = fp.readWrite().length;
    row.keys = ro + rw; row.readOnly = ro; row.readWrite = rw; row.entries = ro + 2 * rw;   // = keys + writes (BUILD-REPORT metric)
    row.instructionsDeclared = r.instructions(); row.diskReadBytes = r.diskReadBytes ? r.diskReadBytes() : r.readBytes?.(); row.writeBytes = r.writeBytes();
    row.resourceFee = Number(sd.resourceFee().toBigInt ? sd.resourceFee().toBigInt() : sd.resourceFee());
    // Disk (classic/archived) vs in-memory reads are not separable from the footprint; key types are:
    const kinds = {}; for (const k of [...fp.readOnly(), ...fp.readWrite()]) { const n = k.switch().name; kinds[n] = (kinds[n] || 0) + 1; } row.keyKinds = kinds;
  }
  // Actual resource use from the core_metrics diagnostic events, when the RPC returns them.
  try {
    const diag = tx.diagnosticEventsXdr || [];
    for (const d of diag) { const ev = d.event(); const topics = ev.body().v0().topics().map(x => S.scValToNative(x));
      if (topics[0] === 'core_metrics') row['m_' + topics[1]] = Number(S.scValToNative(ev.body().v0().data())); }
  } catch {}
  return row;
}

function signSend(xdr, who) {
  return execFileSync('stellar', ['tx', 'sign', '--sign-with-key', alias(who), '--rpc-url', RPC, '--network-passphrase', PASS, '--quiet'],
    {input: xdr, env: cliEnv(), encoding: 'utf8'}).trim();
}
async function buildCall(source, contract, fn, args, seq) {
  const acct = new S.Account(source, String(seq));
  let tx = new S.TransactionBuilder(acct, {fee: '1000', networkPassphrase: PASS}).addOperation(new S.Contract(contract).call(fn, ...args)).setTimeout(60).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) {
    const ev = (sim.events || []).slice(-6).map(e => { try { const b = e.event().body().v0(); return JSON.stringify([b.topics().map(x => S.scValToNative(x)), S.scValToNative(b.data())], (_, v) => typeof v === 'bigint' ? String(v) : v).slice(0, 300); } catch { return '?'; } });
    throw Error(`${fn} simulation: ${String(sim.error).split('\n')[0]} | ${ev.join(' | ')}`);
  }
  const sd = sim.transactionData.build(); const r = sd.resources();
  r.instructions(Math.ceil(r.instructions() * 1.25)); sd.resourceFee(S.xdr.Int64.fromString(String(Math.ceil(Number(sd.resourceFee().toString()) * 1.5) + 50000)));
  tx = S.rpc.assembleTransaction(tx, sim).setSorobanData(sd).build();
  return {tx, sim};
}
async function submit(xdr) {
  const r = await rpc.sendTransaction(S.TransactionBuilder.fromXDR(xdr, PASS));
  if (r.status === 'ERROR') throw Error('send error ' + JSON.stringify(r.errorResult?.toXDR?.('base64')));
  for (;;) { const g = await rpc.getTransaction(r.hash); if (g.status !== 'NOT_FOUND') return {hash: r.hash, status: g.status, ledger: g.ledger}; await sleep(250); }
}

async function invest(stateFile, id) {
  const st = Object.fromEntries(fs.readFileSync(stateFile, 'utf8').split('\n').filter(l => l.includes('=')).map(l => l.split('=')));
  const adm = st.ADM, vault = st.VAULT, receipt = st.RECEIPT; const keeper = 'clg-admin';
  const out = [];
  for (let attempt = 0; attempt < 6; attempt++) {
    let prevLedger = null; let ok = true; out.length = 0;
    // Simulation runs at the latest closed ledger, where the previous observation already sits, so it
    // cannot be repeated between consecutive observations ("one guard observation per ledger"). Simulate
    // once, then reuse the identical operation (source-account auth) and Soroban data with the next sequence.
    let acc = await rpc.getAccount(adm);
    const tmpl = (await buildCall(adm, vault, 'stage_checkpoint_investment', [S.nativeToScVal(adm, {type: 'address'})], acc.sequenceNumber())).tx;
    let seq = BigInt(acc.sequenceNumber());
    for (let i = 0; i < 3; i++) {
      const tx = new S.TransactionBuilder(new S.Account(adm, String(seq)), {fee: tmpl.fee, networkPassphrase: PASS})
        .addOperation(tmpl.toEnvelope().v1().tx().operations()[0]).setSorobanData(tmpl.toEnvelope().v1().tx().ext().sorobanData()).setTimeout(60).build();
      seq += 1n;
      const r = await submit(signSend(tx.toXDR(), keeper));
      out.push({step: `observation ${i + 1}`, ...r});
      console.error(JSON.stringify(out.at(-1)));
      if (r.status !== 'SUCCESS' || (prevLedger !== null && r.ledger !== prevLedger + 1)) { ok = false; break; }
      prevLedger = r.ledger;
    }
    if (!ok) { console.error('not consecutive; retry'); await sleep(6000); continue; }
    while ((await latestSeq()) < prevLedger + 1) await sleep(200);
    acc = await rpc.getAccount(adm);
    const {tx} = await buildCall(adm, receipt, 'stage_invest', [S.nativeToScVal(BigInt(id), {type: 'u64'})], acc.sequenceNumber());
    const r = await submit(signSend(tx.toXDR(), keeper));
    out.push({step: 'stage_invest', ...r, observedLedger: prevLedger});
    console.error(JSON.stringify(out.at(-1)));
    if (r.status === 'SUCCESS') { console.log(JSON.stringify(out)); return; }
  }
  console.log(JSON.stringify(out)); process.exit(1);
}

const [cmd, a, b] = process.argv.slice(2);
if (cmd === 'measure') console.log(JSON.stringify(await measure(a, b || '')));
else if (cmd === 'measure-journal') {
  const done = new Set(fs.existsSync(b) ? fs.readFileSync(b, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l).hash) : []);
  for (const l of fs.readFileSync(a, 'utf8').split('\n').filter(Boolean)) {
    const j = JSON.parse(l); if (!j.tx || j.tx.length !== 64 || done.has(j.tx) || j.kind === 'simulate') continue;
    const row = await measure(j.tx, j.label || j.var || j.kind); fs.appendFileSync(b, JSON.stringify(row) + '\n'); done.add(j.tx);
  }
} else if (cmd === 'measure-account') {
  // Every transaction of a PUBLIC account since <cursor-iso> (Horizon), labelled with the invoked function.
  const [, acct, since, outFile] = process.argv.slice(2);
  const done = new Set(fs.existsSync(outFile) ? fs.readFileSync(outFile, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l).hash) : []);
  let url = `https://horizon-testnet.stellar.org/accounts/${acct}/transactions?order=asc&limit=200`;
  for (;;) {
    const r = await (await fetch(url)).json(); const recs = r._embedded?.records ?? []; if (!recs.length) break;
    for (const x of recs) {
      if (x.created_at < since || done.has(x.hash)) continue;
      let fn = 'classic';
      try { const env = S.xdr.TransactionEnvelope.fromXDR(x.envelope_xdr, 'base64'); const op = env.v1().tx().operations()[0].body();
        if (op.switch().name === 'invokeHostFunction') { const hf = op.invokeHostFunctionOp().hostFunction(); if (hf.switch().name === 'hostFunctionTypeInvokeContract') fn = hf.invokeContract().functionName().toString(); } } catch {}
      const row = await measure(x.hash, fn); row.source = acct; fs.appendFileSync(outFile, JSON.stringify(row) + '\n'); done.add(x.hash);
    }
    url = r._links.next.href;
  }
} else if (cmd === 'invest') await invest(a, b);
else { console.error('usage: measure <hash> | measure-journal <journal> <out> | invest <state.env> <id>'); process.exit(2); }
