// Read-only: per-transaction Soroban limits from the network's ConfigSetting ledger entries.
'use strict';
const {S} = require('./lib.cjs');
const ids = ['configSettingContractComputeV0', 'configSettingContractLedgerCostV0', 'configSettingContractEventsV0', 'configSettingContractBandwidthV0'];
(async () => {
  const out = {};
  for (const [name, url] of [['mainnet', 'https://soroban-rpc.mainnet.stellar.gateway.fm'], ['testnet', 'https://soroban-testnet.stellar.org']]) {
    const keys = ids.map(i => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[i]()})).toXDR('base64'));
    const r = await (await fetch(url, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLedgerEntries', params: {keys}})})).json();
    const lat = await (await fetch(url, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method: 'getLatestLedger'})})).json();
    const o = {protocol: lat.result.protocolVersion, ledger: lat.result.sequence};
    for (const e of r.result.entries) {
      let cs; try { cs = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64').configSetting(); } catch (err) { o.decodeError = err.message; continue; }
      const v = cs.value(), n = cs.switch().name;
      if (n === 'configSettingContractComputeV0') Object.assign(o, {txMaxInstructions: String(v.txMaxInstructions()), txMemoryLimit: v.txMemoryLimit()});
      if (n === 'configSettingContractLedgerCostV0') { for (const f of ['txMaxDiskReadEntries', 'txMaxDiskReadBytes', 'txMaxWriteLedgerEntries', 'txMaxWriteBytes']) try { o[f] = v[f](); } catch {} }
    }
    out[name] = o;
  }
  console.log(JSON.stringify(out, null, 1));
})().catch(e => { console.error(e.message); process.exit(1); });
