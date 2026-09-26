// Public, read-only LP release evidence. No identity/key reads, signatures or submissions.
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const require = createRequire(new URL('../bots/margin-liquidator/package.json', import.meta.url));
const S = require('@stellar/stellar-sdk');
const cfg = JSON.parse(readFileSync(new URL('../config/lp-lending-mainnet.json', import.meta.url)));
const profiles = execFileSync('stellar', ['network', 'ls', '--long'], {encoding:'utf8', stdio:['ignore','pipe','pipe']});
const profile = profiles.split(/\n\n/).find(p => p.startsWith('Name: mainnet-gateway\n'));
assert(profile, 'mainnet-gateway profile required');
const field = name => profile.split('\n').find(l => l.startsWith(`${name}: `))?.slice(name.length + 2);
assert.equal(field('Network passphrase'), S.Networks.PUBLIC);
assert.equal(field('RPC headers'), 'not set');
const rpc = new S.rpc.Server(field('RPC url'));
assert.equal((await rpc.getNetwork()).passphrase, S.Networks.PUBLIC);
const deployer = 'GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL';
const account = await rpc.getAccount(deployer);
const address = a => new S.Address(a).toScVal();
const symbol = s => S.xdr.ScVal.scvSymbol(s);
assert(process.argv.slice(2).every(arg => arg === '--migration-preview'), 'Unknown option; this script has no execution mode');
const migrationPreview = process.argv.includes('--migration-preview');
async function preview(id, method, args=[]) {
  const tx = new S.TransactionBuilder(account, {fee:'100000', networkPassphrase:S.Networks.PUBLIC})
    .addOperation(new S.Contract(id).call(method, ...args)).setTimeout(60).build();
  const sim = await rpc.simulateTransaction(tx);
  if (!S.rpc.Api.isSimulationSuccess(sim)) return {ledger:sim.latestLedger,error:sim.error ?? 'unavailable'};
  const r = sim.transactionData.build().resources();
  const transfers = [];
  for (const d of sim.events ?? []) {
    if (!d.inSuccessfulContractCall()) continue;
    const event = d.event();
    if (event.type().name !== 'contract' || !event.contractId()) continue;
    const body = event.body().v0(), topics = body.topics().map(S.scValToNative);
    if (topics[0] === 'transfer') transfers.push({asset:S.StrKey.encodeContract(event.contractId()),
      from:topics[1],to:topics[2],amount:S.scValToNative(body.data())});
  }
  return {ledger:sim.latestLedger,value:S.scValToNative(sim.result.retval),
    requiresRestore:!!sim.restorePreamble,
    sourceAuthOnly:(sim.result.auth ?? []).every(a => a.credentials().switch().name === 'sorobanCredentialsSourceAccount'),
    resources:{instructions:r.instructions(),entries:r.footprint().readOnly().length+r.footprint().readWrite().length,
      writes:r.footprint().readWrite().length,minResourceFee:sim.minResourceFee},transfers};
}
async function view(id, method, args=[]) {
  const tx = new S.TransactionBuilder(account, {fee:'100', networkPassphrase:S.Networks.PUBLIC})
    .addOperation(new S.Contract(id).call(method, ...args)).setTimeout(60).build();
  const sim = await rpc.simulateTransaction(tx);
  return S.rpc.Api.isSimulationSuccess(sim)
    ? {ledger:sim.latestLedger, value:S.scValToNative(sim.result.retval)}
    : {ledger:sim.latestLedger, error:sim.error ?? 'unavailable'};
}
async function codeHash(id) {
  const key=S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract:new S.Address(id).toScAddress(),key:S.xdr.ScVal.scvLedgerKeyContractInstance(),
    durability:S.xdr.ContractDataDurability.persistent()}));
  const result=await rpc.getLedgerEntries(key);
  const executable=result.entries[0]?.val.contractData().val().instance().executable();
  return {ledger:result.latestLedger,sha256:executable?.switch().name==='contractExecutableWasm'
    ? executable.wasmHash().toString('hex'):null};
}
const latest = await rpc.getLatestLedger();
const report = {time:new Date().toISOString(), network:{sequence:latest.sequence,protocolVersion:latest.protocolVersion,closeTime:latest.closeTime},
  limitation:'Rolling read-only simulations, not an atomic snapshot. Supply equality does not prove absence of historical/archived claims.',
  deployer, controller:cfg.existingPilotController, oracle:null, markets:[], feeds:[], nativeOracle:{}, limits:[]};
report.oracle = await view(report.controller, 'get_oracle');
assert.equal(typeof report.oracle.value, 'string', 'oracle unavailable');
const assets = new Map();
for (const m of cfg.markets) {
  const r = {asset:m.asset, receipt:m.receipt, views:{}, strategy:{}};
  for (const method of ['get_underlying_token','get_boosted_vault','get_total_ptokens','get_total_borrowed','get_total_underlying','decimals'])
    r.views[method] = await view(m.receipt, method);
  r.views.deployerShares = await view(m.receipt,'get_ptoken_balance',[address(deployer)]);
  r.views.deployerValue = await view(m.receipt,'get_user_balance',[address(deployer)]);
  r.views.collateralFactor = await view(report.controller,'get_market_cf',[address(m.receipt)]);
  r.views.borrowPaused = await view(report.controller,'is_borrow_paused',[address(m.receipt)]);
  r.deployerOwnsEntireLiveSupply = r.views.deployerShares.value !== undefined
    && r.views.deployerShares.value === r.views.get_total_ptokens.value;
  const strategy = r.views.get_boosted_vault.value;
  if (typeof strategy === 'string') {
    r.strategy.id = strategy;
    r.strategy.code = await codeHash(strategy);
    for (const method of ['get_pool','get_receipt_vault','get_primary_reward_token','get_config','get_params','get_admin','get_last_harvest'])
      r.strategy[method] = await view(strategy,method);
    if (typeof r.strategy.get_pool.value === 'string') {
      r.strategy.tokens = await view(r.strategy.get_pool.value,'get_tokens');
      for (const a of r.strategy.tokens.value ?? []) if (!assets.has(a)) assets.set(a, a);
      if (migrationPreview) {
        const pool = r.strategy.get_pool.value;
        r.migration = {note:'Independent simulations against current state, NOT harvest-then-withdraw execution. Harvest success does not prove every soft-failing claim succeeded.'};
        for (const method of ['get_user_reward','gauges_get_reward_info','get_user_position_snapshot'])
          r.migration[method] = await view(pool,method,[address(strategy)]);
        r.migration.gauges = await view(pool,'get_gauges');
        const primary = r.strategy.get_primary_reward_token.value;
        if (typeof primary === 'string') r.migration.primaryCash = await view(primary,'balance',[address(strategy)]);
        r.migration.harvest = await preview(strategy,'harvest',[address(deployer)]);
        if (r.deployerOwnsEntireLiveSupply && r.views.get_total_borrowed.value === 0n
            && r.views.deployerShares.value > 0n && r.strategy.get_receipt_vault.value === m.receipt) {
          const p = await preview(m.receipt,'withdraw',[address(deployer),S.nativeToScVal(r.views.deployerShares.value,{type:'u128'})]);
          if (!p.error) {
            const payouts = p.transfers.filter(t => t.to === deployer);
            p.expectedAssetRecipientOnly = payouts.length === 1 && payouts[0].asset === r.views.get_underlying_token.value
              && payouts[0].from === m.receipt && typeof payouts[0].amount === 'bigint' && payouts[0].amount > 0n
              && !p.transfers.some(t => t.from === deployer);
            p.deployerPayoutRaw = p.expectedAssetRecipientOnly ? payouts[0].amount : null;
          }
          r.migration.withdraw = p;
        } else r.migration.withdraw = {error:'Ownership/debt/binding preconditions not met'};
      }
    }
  }
  assets.set(r.views.get_underlying_token.value, m.asset);
  report.markets.push(r);
}
for (const [a,name] of assets) {
  if (typeof a !== 'string') continue;
  report.feeds.push({asset:name,address:a,encoding:'Stellar', result:await view(report.oracle.value,'lastprice',[S.xdr.ScVal.scvVec([symbol('Stellar'),address(a)])])});
}
for (const name of ['XLM','USDC','PYUSD','yXLM'])
  report.feeds.push({asset:name,encoding:'Other',result:await view(report.oracle.value,'lastprice',[S.xdr.ScVal.scvVec([symbol('Other'),symbol(name)])])});
// Official directory: https://orchestrator.reflector.network/config (via reflector.network).
const nativeOracle = 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M';
report.nativeOracle.id = nativeOracle;
for (const method of ['base','decimals','resolution']) report.nativeOracle[method] = await view(nativeOracle,method);
report.nativeOracle.feeds = [];
for (const a of assets.keys()) {
  if (typeof a !== 'string') continue;
  const asset = S.xdr.ScVal.scvVec([symbol('Stellar'),address(a)]);
  report.nativeOracle.feeds.push({address:a,lastprice:await view(nativeOracle,'lastprice',[asset]),prices:await view(nativeOracle,'prices',[asset,S.nativeToScVal(6,{type:'u32'})])});
}
for (const id of [S.xdr.ConfigSettingId.configSettingContractComputeV0(),S.xdr.ConfigSettingId.configSettingContractLedgerCostV0()]) {
  const response = await rpc.getLedgerEntries(S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId:id})));
  for (const e of response.entries) {
    const v = e.val.configSetting().value();
    const value = id.name === 'configSettingContractComputeV0'
      ? {txMaxInstructions:v.txMaxInstructions().toString(),txMemoryLimit:v.txMemoryLimit()}
      : {txMaxDiskReadEntries:v.txMaxDiskReadEntries(),txMaxWriteLedgerEntries:v.txMaxWriteLedgerEntries(),txMaxDiskReadBytes:v.txMaxDiskReadBytes(),txMaxWriteBytes:v.txMaxWriteBytes()};
    report.limits.push({ledger:response.latestLedger, name:id.name, value});
  }
}
console.log(JSON.stringify(report, (_,v) => typeof v === 'bigint' ? v.toString() : v, 2));
