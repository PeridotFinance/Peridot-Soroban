// Read-only: builds the Mainnet ledger fork for harness/margin-fork (tests/margin.rs).
// Never signs, never reads keys or env secrets. Only simulateTransaction / getLedgerEntries / getLatestLedger / getLedgers.
//   cd harness/margin-fork/tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork.json
// Contents: live controller + receipts + SACs + Reflector (keys as in research/cash-borrowing-upgrade-20261118 fetch-snapshot),
// the Reflector admin account (for the mocked set_price in the test), and the REAL Aquarius XLM/USDC concentrated pool:
// discovered on-chain via the router get_pools (never the HTTP /pools/ API), all entries from the RPC footprints of
// read-only swap simulations in both directions at several sizes, plus the code of every fetched instance.
'use strict';
const fs = require('node:fs');
const S = require('@stellar/stellar-sdk');
const RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const NETWORK = S.Networks.PUBLIC;
const opt = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const rpc = async (method, params) => { const r = await (await fetch(RPC, {method: 'POST', headers: {'content-type': 'application/json'},
  body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
const J = x => JSON.stringify(x, (_, v) => typeof v === 'bigint' ? String(v) : v);

const ADMIN = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'; // view-simulation source only
const USDC_ISSUER = 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'; // simulation source for USDC-in swaps (SAC mints)
const CONTROLLER = 'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK';
const M = {
  USDC: {receipt: 'CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', underlying: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'},
  XLM: {receipt: 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS', underlying: 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA'},
  PYUSD: {receipt: 'CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC', underlying: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2'},
  EURC: {receipt: 'CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4', underlying: 'CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV'},
};
const CL_STAGED_RECEIPT = 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI';
const REFLECTOR = 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN';
const ROUTER = 'CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK';
const EXPECTED_POOL = 'CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV';
const XLM = M.XLM.underlying, USDC = M.USDC.underlying;

const sym = s => S.xdr.ScVal.scvSymbol(s), a = x => S.nativeToScVal(x, {type: 'address'});
const u32 = x => S.nativeToScVal(x, {type: 'u32'}), u128 = x => S.nativeToScVal(BigInt(x), {type: 'u128'});
const vec = (n, ...args) => S.xdr.ScVal.scvVec([sym(n), ...args]);
const dataKey = (contract, scKey, dur = 'persistent') => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
  contract: new S.Address(contract).toScAddress(), key: scKey, durability: S.xdr.ContractDataDurability[dur]()}));
const inst = c => dataKey(c, S.xdr.ScVal.scvLedgerKeyContractInstance());
const accountKey = g => S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.Keypair.fromPublicKey(g).xdrAccountId()}));
const names = Object.keys(M), receipts = [...names.map(n => M[n].receipt), CL_STAGED_RECEIPT], tokens = names.map(n => M[n].underlying);
const CTRL_UNIT = ['Admin', 'PendingAdmin', 'Initialized', 'PauseGuardian', 'PauseExpiryMigrationDone', 'PauseExpiryMigrationCursor', 'SupportedMarkets',
  'Oracle', 'CloseFactorScaled', 'LiquidationIncentiveScaled', 'MarginLiquidationControllers', 'ReserveRecipient', 'PauseBorrow', 'PauseBorrowUntil',
  'PauseRedeem', 'PauseRedeemUntil', 'PauseLiquidation', 'PauseLiquidationUntil', 'PauseDeposit', 'PauseDepositUntil', 'LiquidationFeeScaled',
  'OracleMaxAgeMultiplier', 'PeridotToken', 'MarketUserCounts', 'PendingUpgradeHash', 'PendingUpgradeEta', 'PendingOracle', 'PendingOracleEta',
  'PendingCloseFactorScaled', 'PendingCloseFactorEta', 'PendingLiqIncentiveScaled', 'PendingLiqIncentiveEta'];
const CTRL_MARKET = ['MarketCF', 'SupplySpeed', 'BorrowSpeed', 'SupplyIndex', 'BorrowIndex', 'SupplyIndexTime', 'BorrowIndexTime', 'MarketUnderlying',
  'MarketZeroTotalsVerifiedAt', 'PendingMarketCF', 'PendingMarketCFEta', 'BoostedVaultOwner'];
const CTRL_TOKEN = ['OracleAssetSymbol', 'PriceCache', 'FallbackPrice', 'FallbackPriceSetAt', 'SupportedToken'];
const RV_UNIT = ['UnderlyingToken', 'ManagedCash', 'TotalDeposited', 'InterestRatePerSecond', 'LastUpdateTime', 'AccumulatedInterest', 'YearlyRateScaled',
  'InitialExchangeRate', 'TotalBorrowed', 'BorrowIndex', 'BorrowYearlyRateScaled', 'CollateralFactorScaled', 'Admin', 'PendingAdmin', 'Peridottroller',
  'InterestModel', 'ReserveFactorScaled', 'AdminFeeScaled', 'FlashLoanFeeScaled', 'TotalAdminFees', 'TotalReserves', 'SupplyCap', 'BorrowCap',
  'Initialized', 'BoostedVault', 'BoostedUnderlyingCached', 'BoostedUnderlyingUpdatedAt', 'TotalBorrowPrincipal', 'RatesReady', 'IdleCashBufferBps',
  'FlashLoanActive', 'MarginController', 'PendingUpgradeHash', 'PendingUpgradeEta', 'DebtStateVersion', 'DebtStateMigratedAt', 'TotalBadDebt',
  'BoostedAssetCount', 'BoostedHealthCache', 'Meta', 'TotalSupply'];

const simulate = async (source, contract, fn, args) => {
  const tx = new S.TransactionBuilder(new S.Account(source, '0'), {fee: '100', networkPassphrase: NETWORK})
    .addOperation(new S.Contract(contract).call(fn, ...args)).setTimeout(0).build();
  const sim = await rpc('simulateTransaction', {transaction: tx.toXDR()});
  if (sim.error) return {error: String(sim.error).split('\n')[0].slice(0, 300), ledger: sim.latestLedger};
  const data = S.xdr.SorobanTransactionData.fromXDR(sim.transactionData, 'base64'), fp = data.resources().footprint();
  const auth = (sim.results[0].auth || []).map(x => {
    const show = inv => { const c = inv.function().contractFn();
      return {call: `${S.Address.fromScAddress(c.contractAddress()).toString()}.${c.functionName().toString()}`, sub: inv.subInvocations().map(show)}; };
    return show(S.xdr.SorobanAuthorizationEntry.fromXDR(x, 'base64').rootInvocation()); });
  return {keys: [...fp.readOnly(), ...fp.readWrite()], readOnly: fp.readOnly().length, readWrite: fp.readWrite().length, restore: !!sim.restorePreamble,
    cpu: Number(data.resources().instructions()), ledger: sim.latestLedger, auth,
    result: S.scValToNative(S.xdr.ScVal.fromXDR(sim.results[0].xdr, 'base64'))};
};
const view = async (c, fn, args = []) => { const r = await simulate(ADMIN, c, fn, args); if (r.error) throw Error(`${fn}: ${r.error}`); return r; };

(async () => {
  const keys = [], push = k => keys.push(k);
  for (const c of [CONTROLLER, ...receipts, ...tokens, REFLECTOR]) push(inst(c));
  for (const k of CTRL_UNIT) push(dataKey(CONTROLLER, vec(k)));
  for (const m of receipts) for (const k of CTRL_MARKET) push(dataKey(CONTROLLER, vec(k, a(m))));
  for (const t of tokens) for (const k of CTRL_TOKEN) push(dataKey(CONTROLLER, vec(k, a(t))));
  for (const r of names.map(n => M[n].receipt)) {
    for (const k of RV_UNIT) push(dataKey(r, vec(k)));
    for (const who of [r, ADMIN, CONTROLLER]) push(dataKey(r, vec('Balance', a(who))));
  }
  for (const n of names) for (const who of [M[n].receipt, CONTROLLER]) push(dataKey(M[n].underlying, vec('Balance', a(who))));
  // Reflector: footprints of the exact calls the controller makes, plus its admin account (test mocks set_price auth).
  const probes = [['decimals', []], ['resolution', []], ['last_timestamp', []], ['assets', []], ['admin', []]];
  for (const s of ['USDC', 'XLM', 'EURC']) probes.push(['lastprice', [vec('Other', sym(s))]]);
  probes.push(['lastprice', [vec('Stellar', a(M.PYUSD.underlying))]]);
  const reflectorProbes = [];
  for (const [fn, args] of probes) {
    const r = await view(REFLECTOR, fn, args); r.keys.forEach(push);
    reflectorProbes.push({fn, args: args.map(x => S.scValToNative(x)), cpu: r.cpu, footprint: r.keys.length, result: r.result});
    if (fn === 'admin') push(accountKey(r.result));
  }

  // ---- real Aquarius pool, discovered on-chain
  const sorted = [XLM, USDC].sort((x, y) => Buffer.compare(new S.Address(x).toScAddress().toXDR(), new S.Address(y).toScAddress().toXDR()));
  const pools = (await view(ROUTER, 'get_pools', [S.nativeToScVal(sorted.map(x => new S.Address(x)))])).result;
  const routerPools = [];
  for (const addr of Object.values(pools)) routerPools.push({address: addr, type: (await view(addr, 'pool_type')).result});
  const POOL = EXPECTED_POOL;
  if (!routerPools.some(p => p.address === POOL && p.type === 'concentrated')) throw Error(`expected concentrated pool ${POOL} not in router get_pools: ${J(routerPools)}`);
  const poolTokens = (await view(POOL, 'get_tokens')).result;
  if (poolTokens[0] !== XLM || poolTokens[1] !== USDC) throw Error(`unexpected pool token order ${J(poolTokens)}`);
  const poolState = async () => ({slot0: (await view(POOL, 'get_slot0')).result, liquidity: (await view(POOL, 'get_active_liquidity')).result,
    quotes: await Promise.all(QUOTES.map(async ([i, o, amt]) => ({in: i, out: o, amount: String(amt), estimate: String((await view(POOL, 'estimate_swap', [u32(i), u32(o), u128(amt)])).result)})))});
  // Quotes the test compares on the fork (proves the snapshot holds the pool state the swap math needs).
  const QUOTES = [[1, 0, 150_000_000], [1, 0, 1_500_000_000], [0, 1, 1_200_000_000], [0, 1, 12_000_000_000]];
  const before = await poolState();
  // Footprints of real swaps: USDC in (user = USDC issuer, SAC mints) and XLM in (user = XLM receipt contract, which holds XLM;
  // recording-mode simulation records its auth without enforcing it), several sizes.
  const swapSims = [];
  for (const [i, o, src, amts] of [[1, 0, USDC_ISSUER, [15, 150, 1500, 5000]], [0, 1, M.XLM.receipt, [120, 1200, 12000, 40000]]]) for (const units of amts) {
    const amt = BigInt(units) * 10_000_000n;
    const r = await simulate(src.startsWith('G') ? src : ADMIN, POOL, 'swap', [a(src), u32(i), u32(o), u128(amt), u128(1)]);
    if (!r.error) r.keys.forEach(push);
    swapSims.push({in: i, out: o, amount: String(amt), error: r.error, out_amount: r.error ? undefined : String(r.result), cpu: r.cpu,
      readOnly: r.readOnly, readWrite: r.readWrite, restore: r.restore, ledger: r.ledger, recordedAuth: r.auth});
  }

  const uniq = [...new Map(keys.map(k => [k.toXDR('base64'), k])).values()];
  const fetchAll = async ks => { const out = []; for (let i = 0; i < ks.length; i += 200) {
    const r = await rpc('getLedgerEntries', {keys: ks.slice(i, i + 200).map(k => k.toXDR('base64'))}); out.push(...(r.entries || [])); } return out; };
  let entries = await fetchAll(uniq);
  const codes = new Set();
  for (const e of entries) { const d = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64');
    if (d.switch().name === 'contractData' && d.contractData().key().switch().name === 'scvLedgerKeyContractInstance') {
      const ex = d.contractData().val().instance().executable(); if (ex.switch().name === 'contractExecutableWasm') codes.add(ex.wasmHash().toString('hex')); } }
  const have = new Set(entries.map(e => e.key));
  const codeKeys = [...codes].map(h => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(h, 'hex')}))).filter(k => !have.has(k.toXDR('base64')));
  entries = entries.concat(await fetchAll(codeKeys));
  const latest = await rpc('getLatestLedger');
  const lg = await rpc('getLedgers', {startLedger: latest.sequence - 1, pagination: {limit: 1}});
  const closeTime = Number(lg.ledgers[0].ledgerCloseTime);
  const after = await poolState();
  const instanceHash = {};
  for (const e of entries) { const d = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64');
    if (d.switch().name === 'contractData' && d.contractData().key().switch().name === 'scvLedgerKeyContractInstance') {
      const ex = d.contractData().val().instance().executable();
      instanceHash[S.Address.fromScAddress(d.contractData().contract()).toString()] = ex.switch().name === 'contractExecutableWasm' ? ex.wasmHash().toString('hex') : 'stellar_asset'; } }
  const poolStable = J(before) === J(after);
  const out = {schema: 'margin-fork-v2', network: NETWORK, rpc: RPC, fetchedAt: new Date().toISOString(), ledger: latest.sequence - 1,
    closeTime, protocolVersion: latest.protocolVersion, requestedKeys: uniq.length + codeKeys.length, found: entries.length, instanceHash, reflectorProbes,
    pool: {address: POOL, router: ROUTER, routerPools, tokens: poolTokens, stable: poolStable, before, after, swapSims},
    entries: entries.map(e => ({key: e.key, xdr: e.xdr, lastModified: e.lastModifiedLedgerSeq, liveUntil: e.liveUntilLedgerSeq ?? null}))};
  fs.writeFileSync(opt('--out', 'fork.json'), J(out) + '\n');
  console.log(JSON.stringify({ledger: out.ledger, closeTime, protocol: out.protocolVersion, requested: out.requestedKeys, found: out.found,
    pool: {...out.pool, swapSims: swapSims.map(({recordedAuth, ...s}) => s), recordedAuth: swapSims[0].recordedAuth}}, (_, x) => typeof x === 'bigint' ? String(x) : x, 1));
  if (!poolStable) console.error('WARNING: pool state changed while fetching (a trade landed); re-run for a coherent snapshot');
})().catch(e => { console.error(e); process.exit(1); });
