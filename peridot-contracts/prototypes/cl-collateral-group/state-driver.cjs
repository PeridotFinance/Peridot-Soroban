#!/usr/bin/env node
// READ-ONLY live-state fetcher (copied unchanged in behaviour from prototypes/cl-collateral-spike, 18 Nov 2026).
// Public RPC reads only: getLatestLedger, getLedgerEntries, simulateTransaction (no signature, no submission).
// No keys, no keychain, no environment printing. Writes only into $SPIKE_STATE_DIR (default .scratch/cl-collateral-group/state).
//   node state-driver.cjs init      -> simulate seed calls for their footprints, fetch those entries + config settings
//   node state-driver.cjs fill      -> fetch keys listed in <state>/misses.json
//   node state-driver.cjs refresh   -> refetch every known key at the newest ledger (consistency pass)
'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../..');
const S = require(path.join(ROOT, 'bots/margin-liquidator/node_modules/@stellar/stellar-sdk'));
const URL = 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const rpc = new S.rpc.Server(URL, {timeout: 60});
const R = 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI', V = 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63';
const O = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP', KEEPER = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
const ORACLE = 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M';
const PYUSD = 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const DIR = process.env.SPIKE_STATE_DIR || path.join(ROOT, '.scratch/cl-collateral-group/state');
const STATE = path.join(DIR, 'state.json');
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function retry(f) { for (let i = 0; ; i++) { try { return await f(); } catch (e) { if (i > 6) throw e; await sleep(1500 * (i + 1)); } } }
async function post(method, params) {
  return retry(async () => {
    const r = await fetch(URL, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})});
    if (r.status === 429) throw new Error('429');
    const j = await r.json(); if (j.error) throw new Error(JSON.stringify(j.error)); return j.result;
  });
}
async function fetchKeys(keys, st) {
  const ledgers = new Set();
  for (let i = 0; i < keys.length; i += 150) {
    const batch = keys.slice(i, i + 150);
    const res = await post('getLedgerEntries', {keys: batch}); ledgers.add(res.latestLedger);
    const got = new Map((res.entries || []).map(e => [e.key, e]));
    for (const k of batch) {
      const e = got.get(k);
      if (e) { st.entries[k] = {xdr: e.xdr, lastModified: e.lastModifiedLedgerSeq, liveUntil: e.liveUntilLedgerSeq ?? null}; delete st.absent[k]; }
      else { st.absent[k] = true; delete st.entries[k]; }
    }
    await sleep(300);
  }
  return [...ledgers];
}
const CONFIG_IDS = ['configSettingContractMaxSizeBytes','configSettingContractComputeV0','configSettingContractLedgerCostV0','configSettingContractHistoricalDataV0',
  'configSettingContractEventsV0','configSettingContractBandwidthV0','configSettingContractCostParamsCpuInstructions','configSettingContractCostParamsMemoryBytes',
  'configSettingContractDataKeySizeBytes','configSettingContractDataEntrySizeBytes','configSettingStateArchival','configSettingContractExecutionLanes','configSettingContractLedgerCostExtV0'];
async function ledgerInfo(st) {
  const l = await post('getLatestLedger', {});
  st.ledger = l.sequence; st.closeTime = Number(l.closeTime); st.protocolVersion = l.protocolVersion;
  const keys = CONFIG_IDS.map(n => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[n]()})).toXDR('base64'));
  const c = await post('getLedgerEntries', {keys});
  st.config = {latestLedger: c.latestLedger, entries: c.entries.map(e => ({key: e.key, xdr: e.xdr, lastModified: e.lastModifiedLedgerSeq}))};
  const byName = {};
  for (const e of c.entries) { const cs = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64').configSetting(); byName[cs.switch().name] = {cs, lastModified: e.lastModifiedLedgerSeq}; }
  const sa = byName.configSettingStateArchival.cs.stateArchivalSettings();
  st.minPersistentTtl = sa.minPersistentTtl(); st.minTemporaryTtl = sa.minTemporaryTtl(); st.maxEntryTtl = sa.maxEntryTtl(); st.baseReserve = 5000000;
  const comp = byName.configSettingContractComputeV0.cs.contractCompute(), lc = byName.configSettingContractLedgerCostV0.cs.contractLedgerCost();
  const ext = byName.configSettingContractLedgerCostExtV0.cs.contractLedgerCostExt();
  st.networkLimits = {ledger: c.latestLedger,
    txMaxInstructions: String(comp.txMaxInstructions()), txMemoryLimit: comp.txMemoryLimit(),
    txMaxDiskReadEntries: lc.txMaxDiskReadEntries(), txMaxDiskReadBytes: lc.txMaxDiskReadBytes(), txMaxWriteLedgerEntries: lc.txMaxWriteLedgerEntries(), txMaxWriteBytes: lc.txMaxWriteBytes(),
    txMaxFootprintEntries: ext.txMaxFootprintEntries(),
    costParamsCpuLastModified: byName.configSettingContractCostParamsCpuInstructions.lastModified, costParamsMemLastModified: byName.configSettingContractCostParamsMemoryBytes.lastModified,
    computeLastModified: byName.configSettingContractComputeV0.lastModified, ledgerCostLastModified: byName.configSettingContractLedgerCostV0.lastModified, ledgerCostExtLastModified: byName.configSettingContractLedgerCostExtV0.lastModified};
}
const addr = a => new S.Address(a).toScVal(), u128 = n => S.nativeToScVal(BigInt(n), {type: 'u128'}), u32 = n => S.nativeToScVal(n, {type: 'u32'}), i128 = n => S.nativeToScVal(BigInt(n), {type: 'i128'});
const asset = t => S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), addr(t)]);
function accKey(g) { return S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.Keypair.fromPublicKey(g).xdrPublicKey()})).toXDR('base64'); }
function codeKey(h) { return S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(h, 'hex')})).toXDR('base64'); }
async function main() {
  const mode = process.argv[2];
  fs.mkdirSync(DIR, {recursive: true});
  if (mode === 'init') {
    assert(!fs.existsSync(STATE), 'state exists');
    const st = {schema: 'cl-collateral-spike-state-v1', rpc: URL, entries: {}, absent: {}, fetchLedgers: [], sims: []};
    await ledgerInfo(st);
    const keys = new Set([accKey(O), accKey(KEEPER)]);
    const acc = await rpc.getAccount(O);
    const calls = [
      [V, 'stage_quote_open', [u128('10000000000'), u32(20), u32(5000)]],
      [V, 'stage_quote_close', [u128('1000000'), u32(20)]],
      [V, 'get_asset_amounts_per_shares', [i128('1000000')]],
      [V, 'checkpoint_rewards', []],
      [V, 'stage_checkpoint_investment', [addr(KEEPER)]],
      [R, 'stage_account', [addr(O)]],
      [R, 'stage_state', []],
      [ORACLE, 'lastprice', [asset(PYUSD)]], [ORACLE, 'lastprice', [asset(USDC)]], [ORACLE, 'decimals', []], [ORACLE, 'resolution', []],
      [PYUSD, 'balance', [addr(O)]], [USDC, 'balance', [addr(O)]], [PYUSD, 'admin', []], [USDC, 'admin', []],
    ];
    for (const [c, f, args] of calls) {
      const tx = new S.TransactionBuilder(acc, {fee: '100', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(c).call(f, ...args)).setTimeout(0).build();
      const sim = await rpc.simulateTransaction(tx);
      if (!S.rpc.Api.isSimulationSuccess(sim)) { st.sims.push({c, f, error: String(sim.error).slice(0, 300)}); continue; }
      const res = sim.transactionData.build().resources(); const fp = res.footprint();
      for (const k of [...fp.readOnly(), ...fp.readWrite()]) keys.add(k.toXDR('base64'));
      st.sims.push({c, f, latestLedger: sim.latestLedger, cpu: res.instructions(), readOnly: fp.readOnly().length, readWrite: fp.readWrite().length,
        retval: JSON.stringify(S.scValToNative(sim.result.retval), (k, v) => typeof v === 'bigint' ? v.toString() : v).slice(0, 800)});
      await sleep(300);
    }
    st.fetchLedgers.push(...await fetchKeys([...keys], st));
    fs.writeFileSync(STATE, JSON.stringify(st, null, 1));
    console.log(JSON.stringify({ledger: st.ledger, entries: Object.keys(st.entries).length, absent: Object.keys(st.absent).length, sims: st.sims.map(s => [s.f, s.error ? 'ERR ' + s.error.slice(0, 120) : s.cpu, s.retval && s.retval.slice(0, 200)])}, null, 1));
  } else if (mode === 'fill') {
    const st = JSON.parse(fs.readFileSync(STATE)); const m = JSON.parse(fs.readFileSync(path.join(DIR, 'misses.json')));
    const keys = m.filter(k => !(k in st.entries) && !(k in st.absent));
    st.fetchLedgers.push(...await fetchKeys(keys, st));
    fs.writeFileSync(STATE, JSON.stringify(st, null, 1));
    console.log(JSON.stringify({fetched: keys.length, entries: Object.keys(st.entries).length, absent: Object.keys(st.absent).length}));
  } else if (mode === 'refresh') {
    const st = JSON.parse(fs.readFileSync(STATE)); await ledgerInfo(st);
    const all = [...Object.keys(st.entries), ...Object.keys(st.absent)];
    const ls = await fetchKeys(all, st); st.refreshLedgers = ls; st.refreshedAt = new Date().toISOString();
    fs.writeFileSync(STATE, JSON.stringify(st, null, 1));
    console.log(JSON.stringify({ledger: st.ledger, closeTime: st.closeTime, refreshLedgers: ls, entries: Object.keys(st.entries).length, absent: Object.keys(st.absent).length}));
  } else throw new Error('usage: init|fill|refresh');
}
main().catch(e => { console.error(String(e && e.stack || e)); process.exit(1); });
