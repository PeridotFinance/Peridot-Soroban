// CL collateral group, fresh stack (option A): LIVE Mainnet simulation and fee quote. READ-ONLY.
// Uses only getLatestLedger, getAccount, getLedgerEntries and simulateTransaction against public Mainnet RPC.
// It never signs, never sends, never writes a transaction file and never reads a key, keychain or environment secret.
//
//   node mainnet-sims.cjs upload <wasm file>             simulate uploadContractWasm of these exact bytes (admin source)
//   node mainnet-sims.cjs create <on-chain sha256> [salt hex]  simulate createCustomContract from code ALREADY on chain
//   node mainnet-sims.cjs invoke <contract> <method> <json args> [--types t1,t2..]   simulate one call on a LIVE contract
//   node mainnet-sims.cjs code-exists <sha256>
//   node mainnet-sims.cjs quote --manifest m.json --out q.json   every step in the manifest; writes the quote JSON
//
// Fee figures: minResourceFee is the simulated resource fee; capStroops = minResourceFee x 1.2 + 10,000 (the cash
// ceremony's padding; the unused refundable part is refunded) + 10,000 inclusion bid. For uploads the actual charge is
// historically ~87% of the bid (pilot: 143.33 / 164.83 XLM vault, 126.25 / 145.19 XLM receipt).
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require(process.env.STELLAR_SDK_DIR || '/Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const NET = 'Public Global Stellar Network ; September 2015';
const RPC = process.env.CLG_RPC || 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const ADMIN = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const BID = 10_000n;
const rpc = new S.rpc.Server(RPC, {timeout: 60});
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2);
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function retry(f) { for (let i = 0; ; i++) { try { return await f(); } catch (e) { if (i > 5 || !/429|ECONN|fetch failed|timeout|50[234]/i.test(String(e.message))) throw e; await sleep(1500 * (i + 1)); } } }
async function rawLatest() { const r = await (await fetch(RPC, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json(); return r.result; }

async function codeExists(hash) {
  const k = S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hash, 'hex')}));
  const r = await retry(() => rpc.getLedgerEntries(k));
  return r.entries && r.entries.length ? {onChain: true, liveUntil: r.entries[0].liveUntilLedgerSeq} : {onChain: false};
}
function metrics(sim) {
  const d = sim.transactionData.build(), r = d.resources(), fp = r.footprint();
  return {cpu: r.instructions(), diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes(),
    readOnly: fp.readOnly().length, readWrite: fp.readWrite().length};
}
async function simulate(op, label) {
  const acc = await retry(() => rpc.getAccount(ADMIN)), latest = await rawLatest();
  const tx = new S.TransactionBuilder(acc, {fee: String(BID), networkPassphrase: NET}).addOperation(op).setTimeout(0).build();
  const sim = await retry(() => rpc.simulateTransaction(tx));
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {label, ok: false, ledger: sim.latestLedger || latest.sequence, error: String(sim.error || '').split('\n')[0].slice(0, 300)};
  const min = BigInt(sim.minResourceFee), cap = min * 120n / 100n + 10_000n + BID;
  const auth = (sim.result && sim.result.auth || []).map(a => a.credentials().switch().name);
  return {label, ok: true, ledger: sim.latestLedger, protocol: latest.protocolVersion, source: ADMIN,
    minResourceFeeStroops: String(min), minResourceFeeXlm: Number(min) / 1e7, capStroops: String(cap), capXlm: Number(cap) / 1e7,
    restoreRequired: !!sim.restorePreamble, auth, ...metrics(sim),
    retval: sim.result && sim.result.retval ? safeNative(sim.result.retval) : null};
}
function safeNative(v) { try { const n = S.scValToNative(v); return Buffer.isBuffer(n) ? n.toString('hex') : n; } catch { return v.toXDR('base64'); } }
async function upload(file) {
  const wasm = fs.readFileSync(file), h = sha(wasm);
  const ex = await codeExists(h);
  const r = await simulate(S.Operation.uploadContractWasm({wasm}), `upload ${path.basename(file)} (${wasm.length} B, ${h})`);
  return {...r, kind: 'upload', sha256: h, bytes: wasm.length, under131072: wasm.length < 131072, alreadyOnChain: ex.onChain};
}
async function create(hash, salt) {
  const ex = await codeExists(hash);
  assert(ex.onChain, `code ${hash} is not on chain: create cannot be simulated live yet`);
  const s = Buffer.from(salt || sha(Buffer.from('clg-proxy-' + hash)), 'hex');
  const op = S.Operation.createCustomContract({address: new S.Address(ADMIN), wasmHash: Buffer.from(hash, 'hex'), salt: s});
  const id = S.StrKey.encodeContract(S.hash(S.xdr.HashIdPreimage.envelopeTypeContractId(new S.xdr.HashIdPreimageContractId({
    networkId: S.hash(Buffer.from(NET)), contractIdPreimage: S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(
      new S.xdr.ContractIdPreimageFromAddress({address: new S.Address(ADMIN).toScAddress(), salt: s}))})).toXDR()));
  return {...(await simulate(op, `createCustomContract(${hash.slice(0, 8)}…)`)), kind: 'create', wasmHash: hash, salt: s.toString('hex'), wouldCreate: id};
}
const TYPES = {address: v => new S.Address(v).toScVal(), u128: v => S.nativeToScVal(BigInt(v), {type: 'u128'}), i128: v => S.nativeToScVal(BigInt(v), {type: 'i128'}),
  u32: v => S.nativeToScVal(Number(v), {type: 'u32'}), u64: v => S.nativeToScVal(BigInt(v), {type: 'u64'}), bool: v => S.xdr.ScVal.scvBool(!!v),
  bytes32: v => S.xdr.ScVal.scvBytes(Buffer.from(v, 'hex')), symbol: v => S.xdr.ScVal.scvSymbol(v), xdr: v => S.xdr.ScVal.fromXDR(v, 'base64')};
async function invoke(contract, method, args, types) {
  const vals = args.map((a, i) => TYPES[types[i]](a));
  return {...(await simulate(new S.Contract(contract).call(method, ...vals), `${contract.slice(0, 6)}….${method}`)), kind: 'invoke', contract, method, args};
}
async function quote(manifest) {
  const m = JSON.parse(fs.readFileSync(manifest, 'utf8')), rows = [];
  for (const s of m.steps) {
    let r;
    if (s.kind === 'upload') r = await upload(path.resolve(path.dirname(manifest), s.file));
    else if (s.kind === 'create') r = await create(s.wasmHash, s.salt);
    else if (s.kind === 'invoke') r = await invoke(s.contract, s.method, s.args, s.types);
    else throw Error('unknown kind ' + s.kind);
    rows.push({step: s.step, proxyFor: s.proxyFor || null, ...r}); await sleep(400);
  }
  return {observedAt: new Date().toISOString(), rpc: RPC, network: NET, admin: ADMIN, rows,
    totalMinResourceXlm: rows.filter(r => r.ok).reduce((a, r) => a + r.minResourceFeeXlm, 0),
    totalCapXlm: rows.filter(r => r.ok).reduce((a, r) => a + r.capXlm, 0)};
}
async function main(argv) {
  const opt = k => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : null; };
  const cmd = argv[0];
  if (cmd === 'upload') return upload(argv[1]);
  if (cmd === 'create') return create(argv[1], argv[2]);
  if (cmd === 'code-exists') return {hash: argv[1], ...(await codeExists(argv[1]))};
  if (cmd === 'invoke') return invoke(argv[1], argv[2], JSON.parse(argv[3] || '[]'), (opt('--types') || '').split(',').filter(Boolean));
  if (cmd === 'quote') { const q = await quote(opt('--manifest')); if (opt('--out')) fs.writeFileSync(opt('--out'), json(q) + '\n'); return q; }
  throw Error('usage: see header');
}
module.exports = {simulate, upload, create, invoke, codeExists};
if (require.main === module) main(process.argv.slice(2)).then(r => console.log(json(r))).catch(e => { console.error('FAILED:', e.message); process.exit(1); });
