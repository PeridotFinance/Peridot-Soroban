// Shared read-only helpers for the cash-borrowing activation tooling. Never signs, never reads keys.
'use strict';
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const MAINNET = Object.freeze({
  network: S.Networks.PUBLIC, rpc: 'https://soroban-rpc.mainnet.stellar.gateway.fm', horizon: 'https://horizon.stellar.org',
  admin: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  controller: 'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK',
  controllerWasm: '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902',
  marketWasm: '4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e',
  markets: {
    USDC: {receipt: 'CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', underlying: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'},
    XLM: {receipt: 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS', underlying: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA'},
    PYUSD: {receipt: 'CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC', underlying: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2'},
    EURC: {receipt: 'CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4', underlying: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'},
  },
  // Listed on the same controller; MUST stay CF 0 / borrow-disabled. Refused as a target by every builder.
  clStagedReceipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI',
  oracles: {external: 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN', stellarDex: 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M'},
});
const u = x => BigInt(String(x));
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2);
const addr = a => S.nativeToScVal(a, {type: 'address'});
const u128 = x => S.nativeToScVal(u(x), {type: 'u128'});
const u32 = x => S.nativeToScVal(Number(x), {type: 'u32'});
const u64 = x => S.nativeToScVal(u(x), {type: 'u64'});
const bool = b => S.xdr.ScVal.scvBool(!!b);
const sym = s => S.xdr.ScVal.scvSymbol(s);
const optSym = s => s == null ? S.xdr.ScVal.scvVoid() : sym(s);
const key = (name, ...args) => S.xdr.ScVal.scvVec([sym(name), ...args]);
function chain(cfg, rpc = new S.rpc.Server(cfg.rpc, {timeout: 60, allowHttp: cfg.rpc.startsWith('http://')})) {
  patchRpc(rpc, cfg.rpc);
  const viewSource = cfg.viewSource || cfg.admin;
  const view = async (contract, method, args = []) => {
    const tx = new S.TransactionBuilder(new S.Account(viewSource, '0'), {fee: '100', networkPassphrase: cfg.network})
      .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
    const sim = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(sim)) { const e = Error(`view ${method} failed: ${String(sim.error).split('\n')[0].slice(0, 200)}`); e.sim = sim; throw e; }
    return {value: sim.result ? S.scValToNative(sim.result.retval) : undefined, ledger: sim.latestLedger};
  };
  const tryView = async (c, m, a) => { try { return (await view(c, m, a)).value; } catch (e) { return {error: e.message}; } };
  const raw = async (contract, scKey, durability = 'persistent') => {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(), key: scKey,
      durability: S.xdr.ContractDataDurability[durability]()}));
    const r = await rpc.getLedgerEntries(k);
    if (!r.entries.length) return null;
    return {value: S.scValToNative(r.entries[0].val.contractData().val()), liveUntil: r.entries[0].liveUntilLedgerSeq};
  };
  const codeHash = async c => {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k); return r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');
  };
  const instance = async c => {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k); const st = r.entries[0].val.contractData().val().instance().storage() || [];
    const o = {}; for (const e of st) { const kk = S.scValToNative(e.key()); o[Array.isArray(kk) ? kk.join(':') : String(kk)] = S.scValToNative(e.val()); } return o;
  };
  return {rpc, view, tryView, raw, codeHash, instance};
}
// SDK 16.x cannot decode ledger headers / metas of newer protocols (Testnet runs ahead of Mainnet). Use raw JSON-RPC for
// the two calls that decode them; everything the tools need (sequence, status, ledger) is plain JSON.
function patchRpc(rpc, url) {
  const call = async (method, params) => { const r = await (await fetch(url, {method: 'POST', headers: {'content-type': 'application/json'},
    body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
  rpc.getLatestLedger = async () => { const r = await call('getLatestLedger'); return {sequence: r.sequence, protocolVersion: r.protocolVersion, id: r.id}; };
  rpc.getTransaction = async hash => { const r = await call('getTransaction', {hash}); return {status: r.status, ledger: r.ledger, raw: r}; };
  return rpc;
}
// Refundable rent can exceed the simulated figure between simulation and application (observed on Testnet:
// 13987 > 11499 stroops for withdraw-with-debt). Pad the resource fee: +20% + 10000 stroops. Unused refundable fee is refunded.
function padResourceFee(sim) {
  const padded = BigInt(sim.minResourceFee) * 120n / 100n + 10000n;
  sim.transactionData.setResourceFee(padded); sim.minResourceFee = String(padded); return sim;
}
module.exports = {padResourceFee, patchRpc, S, MAINNET, u, json, addr, u128, u32, u64, bool, sym, optSym, key, chain};
