import {execFileSync} from 'node:child_process';
import * as S from '@stellar/stellar-sdk';
import {loadKeyFile} from './keyfile.mjs';

const addr = a => S.nativeToScVal(a, {type: 'address'});
const u128 = x => S.nativeToScVal(BigInt(x), {type: 'u128'});
const u64 = x => S.nativeToScVal(BigInt(x), {type: 'u64'});
const u32 = x => S.nativeToScVal(Number(x), {type: 'u32'});

// Decoded contract events from a simulation's diagnostic events: [{contract, topics, data}] (natives). Undecodable
// entries are skipped; callers fall back to the contract-math model when the event they need is missing.
export function decodeSimEvents(diags) {
  const out = [];
  for (const d of diags ?? []) {
    try {
      const ev = typeof d === 'string' ? S.xdr.DiagnosticEvent.fromXDR(d, 'base64').event() : d.event();
      const id = ev.contractId();
      const body = ev.body().v0();
      out.push({contract: id ? S.StrKey.encodeContract(Buffer.from(id)) : null, topics: body.topics().map(t => S.scValToNative(t)), data: S.scValToNative(body.data())});
    } catch { /* skip undecodable */ }
  }
  return out;
}
const sym = s => S.xdr.ScVal.scvSymbol(s);

export function makeChain(cfg) {
  const rpc = new S.rpc.Server(cfg.rpcUrl, {timeout: 30, allowHttp: cfg.rpcUrl.startsWith('http://')});
  // SDK 16.x cannot decode newer-protocol headers/metas: fetch latest ledger and tx status as raw JSON.
  const call = async (method, params) => { const r = await (await fetch(cfg.rpcUrl, {method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
  // Protocol 30: never decode the header with SDK 16.3.x. closeTime (seconds) is read from the JSON when the RPC returns it.
  rpc.getLatestLedger = async () => { const r = await call('getLatestLedger'); return {sequence: Number(r.sequence), closeTime: r.closeTime != null ? Number(r.closeTime) : null}; };
  rpc.getTransaction = async hash => { const r = await call('getTransaction', {hash});
    let feeCharged; try { if (r.resultXdr) feeCharged = String(S.xdr.TransactionResult.fromXDR(r.resultXdr, 'base64').feeCharged().toString()); } catch { /* undecodable: caller falls back to max fee */ }
    return {status: r.status, ledger: r.ledger, feeCharged}; };
  const tx = (account, op, fee = cfg.inclusionFee, timeoutSec = 120) => new S.TransactionBuilder(account, {fee: String(fee), networkPassphrase: cfg.network}).addOperation(op).setTimeout(timeoutSec).build();
  async function view(contract, method, args = []) {
    const sim = await rpc.simulateTransaction(tx(new S.Account(cfg.publicKey, '0'), new S.Contract(contract).call(method, ...args)));
    if (!S.rpc.Api.isSimulationSuccess(sim)) throw Error(`${method}: ${String(sim.error).split('\n')[0].slice(0, 200)}`);
    return S.scValToNative(sim.result.retval);
  }
  async function codeHash(c) {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k); return r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');
  }
  // Raw persistent contract-data read (e.g. controller PauseGuardian, PriceCache(token)); null when absent.
  async function raw(contract, scKey, durability = 'persistent') {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(), key: scKey,
      durability: S.xdr.ContractDataDurability[durability]()}));
    const r = await rpc.getLedgerEntries(k);
    return r.entries.length ? S.scValToNative(r.entries[0].val.contractData().val()) : null;
  }
  // Simulate an invocation from the signer account; returns the assembled tx, return value and resource metrics.
  // opts.inclusionFee / opts.maxFeeStroops override the liquidation caps (the price refresher and pause watcher use their own).
  async function prepare(contract, method, args, opts = {}) {
    const maxFee = opts.maxFeeStroops ?? cfg.maxFeeStroops;
    const acc = await rpc.getAccount(cfg.publicKey);
    const t = tx(acc, new S.Contract(contract).call(method, ...args), opts.inclusionFee ?? cfg.inclusionFee, opts.timeoutSec ?? 120);
    const sim = await rpc.simulateTransaction(t);
    if (!S.rpc.Api.isSimulationSuccess(sim)) return {ok: false, error: String(sim.error).split('\n')[0].slice(0, 300)};
    if (sim.restorePreamble) return {ok: false, error: 'restore required'};
    // Pad the resource fee (+20% + 10000 stroops): refundable rent can exceed the simulated figure; unused part is refunded.
    sim.transactionData.setResourceFee(BigInt(sim.minResourceFee) * 120n / 100n + 10000n); sim.minResourceFee = String(BigInt(sim.minResourceFee) * 120n / 100n + 10000n);
    const prepared = S.rpc.assembleTransaction(t, sim).build();
    const d = prepared.toEnvelope().v1().tx().ext().sorobanData(), r = d.resources(), fp = r.footprint();
    const metrics = {cpu: Number(r.instructions()), readOnly: fp.readOnly().length, readWrite: fp.readWrite().length, fee: String(prepared.fee)};
    if (metrics.cpu > (opts.cpuStop ?? cfg.cpuStop)) return {ok: false, error: `cpu ${metrics.cpu} above stop`, metrics};
    if (BigInt(prepared.fee) > BigInt(maxFee)) return {ok: false, error: `fee ${prepared.fee} above cap`, metrics};
    return {ok: true, prepared, auth: sim.result ? sim.result.auth || [] : [], retval: sim.result ? S.scValToNative(sim.result.retval) : undefined, metrics, events: decodeSimEvents(sim.events)};
  }
  let keypair = null;   // loaded lazily from cfg.keyFile at the first signature, kept in memory only
  function sign(prepared) {
    if (cfg.keyFile) {
      if (!keypair) keypair = loadKeyFile(cfg.keyFile, {expectedPublicKey: cfg.publicKey});
      if (prepared.source !== cfg.publicKey) throw Error('refused: transaction source is not the configured signer');
      prepared.sign(keypair); return prepared;
    }
    if (cfg.signerCliAlias) {
      // Testnet rehearsal: the Stellar CLI holds the Testnet-only identity; this process never sees the secret.
      const out = execFileSync('stellar', ['tx', 'sign', '--sign-with-key', cfg.signerCliAlias, '--rpc-url', cfg.rpcUrl, '--network-passphrase', cfg.network], {input: prepared.toXDR()}).toString().trim();
      return S.TransactionBuilder.fromXDR(out, cfg.network);
    }
    // Legacy App Platform path only; the pause watcher sets allowEnvSecret=false and never reads this variable.
    const secret = cfg.allowEnvSecret === false ? null : process.env.LIQUIDATOR_SECRET;
    if (!secret) throw Error('no signer configured');
    const kp = S.Keypair.fromSecret(secret);
    if (kp.publicKey() !== cfg.publicKey) throw Error('secret does not match LIQUIDATOR_PUBLIC_KEY');
    prepared.sign(kp); return prepared;
  }
  async function submit(signed, timeoutMs = 90_000) {
    const hash = signed.hash().toString('hex');
    const r = await rpc.sendTransaction(signed);
    if (r.status === 'ERROR') return {hash, status: 'ERROR', error: r.errorResult ? r.errorResult.toXDR('base64') : 'send error'};
    const end = Date.now() + timeoutMs;
    while (Date.now() < end) {
      const g = await rpc.getTransaction(hash);
      if (g.status !== 'NOT_FOUND') return {hash, status: g.status, ledger: g.ledger, feeCharged: g.feeCharged};
      await new Promise(res => setTimeout(res, 2000));
    }
    return {hash, status: 'UNKNOWN'};
  }
  async function borrowEvents(contractIds, {startLedger, cursor}) {
    const filters = [{type: 'contract', contractIds, topics: [[sym('borrow_event').toXDR('base64'), '*']]}];
    const req = cursor ? {filters, cursor, limit: 200} : {filters, startLedger, limit: 200};
    const r = await rpc.getEvents(req);
    const events = r.events.map(e => ({ledger: e.ledger, contract: e.contractId?.toString?.() ?? String(e.contractId), borrower: S.scValToNative(e.topic[1])}));
    return {events, cursor: r.cursor, latestLedger: r.latestLedger, oldestLedger: r.oldestLedger};
  }
  // Margin pass: raw JSON getEvents (no SDK response parsing). Returns decoded single-symbol-topic events.
  async function rawEvents({contractId, names, startLedger, endLedger, cursor, limit = 100}) {
    const filters = [{type: 'contract', contractIds: [contractId], topics: names.map(n => [sym(n).toXDR('base64')])}];
    const params = cursor ? {filters, pagination: {cursor, limit}} : {filters, startLedger, pagination: {limit}};
    if (endLedger != null) params.endLedger = endLedger;
    const r = await call('getEvents', params);
    const events = (r.events ?? []).map(e => {
      const topics = (e.topic ?? []).map(t => S.scValToNative(S.xdr.ScVal.fromXDR(t, 'base64')));
      return {ledger: Number(e.ledger), id: e.id, name: String(topics[0]), data: S.scValToNative(S.xdr.ScVal.fromXDR(e.value, 'base64'))};
    });
    return {events, cursor: r.cursor, latestLedger: r.latestLedger, oldestLedger: r.oldestLedger};
  }
  // Contract instance: WASM hash plus instance storage lookup by key ScVal (e.g. Key::Fees = Vec[Symbol("Fees")]).
  async function instance(contract) {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k);
    if (!r.entries.length) throw Error(`instance of ${contract} not found`);
    const inst = r.entries[0].val.contractData().val().instance();
    const m = new Map();
    for (const e of inst.storage() ?? []) m.set(e.key().toXDR('base64'), e.val());
    return {wasm: inst.executable().wasmHash().toString('hex'), get: key => { const v = m.get(key.toXDR('base64')); return v == null ? null : S.scValToNative(v); }};
  }
  const latestLedger = () => rpc.getLatestLedger();
  return {rpc, view, raw, codeHash, prepare, sign, submit, borrowEvents, rawEvents, instance, latestLedger, addr, u128, u64, u32, sym};
}
