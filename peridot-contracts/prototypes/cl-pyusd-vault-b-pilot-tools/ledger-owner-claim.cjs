// Ledger-sourced builder for the holder's AQUA reward claim on the Vault-B pilot (Stellar Mainnet). Builds ONE unsigned
// transaction `vault.claim_rewards(owner)` with the owner's Ledger account (GC4T5M…T7FP) as source, after fresh reads and a
// fresh simulation taken right before the file is written. It never signs, never sends and never reads keys: the operator
//   stellar tx sign --sign-with-ledger --rpc-url "$RPC_URL" --network-passphrase "$NET" F.unsigned.xdr > F.signed.xdr
// checks it with `verify-signed`, sends it with `stellar tx send`, reads the outcome with `status` and checks it with `readback`.
//
//   read                                      reward_account, reward_totals, pool pending, checkpoint probe, holder AQUA, pins
//   prepare-claim --out F [--allow-zero]      claim_rewards(owner); refuses a simulated 0 unless --allow-zero
//   verify-signed F.unsigned.xdr F.signed.xdr same hash, exactly one valid owner signature, still exactly claim_rewards(owner)
//   status <hash>                             RPC + Horizon outcome (read-only)
//   readback F.unsigned.json [hash]           AQUA balance delta == on-chain return == paid delta, and >= simulated return
//
// Contract semantics (cl-share-vault rewards.rs `claim`, same source as the deployed e6638030… build): owner.require_auth();
// a venue claim pool.claim(vault) is TRIED (failure is swallowed, already-booked rewards still pay); a successful positive
// delta is added to the per-share index (or quarantined if an emergency boundary left the interval open, or supply is 0);
// the owner is settled on its beneficial custody shares and all booked `pending` is transferred in AQUA to the owner.
// The vault authorizes the venue claim and the AQUA transfer as the direct invoker, so the owner signs only the root call.
//
// Fixed: host CPU stop 97M, SDK entries (memory reads + disk reads + writes) <= 100, unique keys <= 100, writes <= 50,
// fee <= 0.5 XLM, 40-ledger validity window. Pins are shared with ledger-owner-call.cjs, whose reviewed hash is pinned too.
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const L = require('./ledger-owner-call.cjs');

const REVIEWED_OWNER_CALL_SHA256 = '77a79a6cc96a0ece84c2b549942e9b6b533c925ac76bffc555369bae7a4f1b08';
assert.equal(L.sourceSha256(), REVIEWED_OWNER_CALL_SHA256, 'ledger-owner-call.cjs differs from the reviewed version; re-review before claiming');
const PINS = Object.freeze({...L.PINS,
  pool: 'CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX',       // Aquarius PYUSD/USDC concentrated pool (vault get_pool)
  aqua: 'CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK',       // AQUA SAC (addresses.md; vault get_primary_reward_token)
  aquaCode: 'AQUA', aquaIssuer: 'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA'});
const FIXED = Object.freeze({cpuStop: 97000000, sdkEntries: 100, uniqueKeys: 100, writes: 50, window: 40, timeoutSeconds: 240,
  bidStroops: 10000n, maxFeeStroops: 5000000n, minXlmAfterFeeStroops: 10000000n, maxBoundaryProbe: 16});
const u = x => BigInt(String(x));
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2);
const addr = a => S.nativeToScVal(a, {type: 'address'});
const aquaDecimal = raw => { const n = u(raw); return `${n / 10000000n}.${String(n % 10000000n).padStart(7, '0')}`; };

// ---------------------------------------------------------------- pure checks (unit-tested)
// The invoked function of the operation must be exactly vault.claim_rewards(owner).
function checkClaimCall(op, {owner = PINS.owner} = {}) {
  assert.equal(op.body().switch().name, 'invokeHostFunction', 'operation is not a contract invocation');
  const hf = op.body().invokeHostFunctionOp().hostFunction();
  assert.equal(hf.switch().name, 'hostFunctionTypeInvokeContract', 'host function is not a contract call');
  const ic = hf.invokeContract();
  assert.equal(S.Address.fromScAddress(ic.contractAddress()).toString(), PINS.vault, 'call is not on the pinned vault');
  assert.equal(ic.functionName().toString(), 'claim_rewards', 'function is not claim_rewards');
  const args = ic.args(); assert.equal(args.length, 1, 'claim_rewards takes exactly one argument');
  assert.equal(args[0].switch().name, 'scvAddress', 'holder argument is not an address');
  assert.equal(S.scValToNative(args[0]), owner, 'claim for a holder other than the owner');
}
// Authorization: exactly one source-account entry whose root is the operation itself, and NOTHING nested.
function checkClaimAuth(auth, op) {
  checkClaimCall(op);
  const root = L.checkRootAuth(auth, op);
  assert.equal(root.subInvocations().length, 0, 'claim_rewards must not authorize anything nested');
  return root;
}
// The whole transaction (prepared or signed): owner source, exactly one operation, the claim call with its envelope auth.
function checkClaimShape(tx) {
  assert.equal(tx.source, PINS.owner, 'transaction source is not the owner Ledger account');
  assert.equal(tx.operations.length, 1, 'exactly one operation required');
  const op = tx.toEnvelope().v1().tx().operations()[0];
  assert(!op.sourceAccount(), 'operation-level source account is not allowed');
  checkClaimAuth(op.body().invokeHostFunctionOp().auth(), op);
  return op;
}
const MEMORY_TYPES = new Set(['contractData', 'contractCode']);
function checkClaimResources(data, minResourceFee) {
  const r = data.resources(), fp = r.footprint(), ro = fp.readOnly(), rw = fp.readWrite();
  const keys = [...ro, ...rw].map(k => k.toXDR('base64')), unique = new Set(keys).size;
  const memoryReads = [...ro, ...rw].filter(k => MEMORY_TYPES.has(k.switch().name)).length, diskReads = keys.length - memoryReads;
  const sdkEntries = memoryReads + diskReads + rw.length;
  assert(Number(r.instructions()) <= FIXED.cpuStop, `declared host CPU ${r.instructions()} above the 97M stop`);
  assert(sdkEntries <= FIXED.sdkEntries, `${sdkEntries} SDK entries (memory reads + disk reads + writes) above 100`);
  assert(unique <= FIXED.uniqueKeys && unique === keys.length, 'more than 100 or duplicate unique transaction keys');
  assert(rw.length <= FIXED.writes, 'more than 50 writes');
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'archived entries: restore first (outside this tool)');
  assert(u(data.resourceFee().toString()) >= u(minResourceFee), 'resource fee below the simulated minimum');
  return {cpu: Number(r.instructions()), sdkEntries, memoryReads, diskReads, writes: rw.length, uniqueKeys: unique,
    readBytes: r.diskReadBytes(), writeBytes: r.writeBytes(), resourceFeeStroops: data.resourceFee().toString()};
}
function checkReturn(ret, {allowZero = false} = {}) {
  assert.equal(typeof ret, 'bigint', 'claim_rewards must return a u128');
  assert(ret >= 0n);
  if (ret === 0n && !allowZero) throw Error('simulated claim_rewards returns 0 AQUA: nothing booked or accrued (see `read`: poolPending, checkpointClaimable); rerun with --allow-zero only to prove the path');
  return ret;
}
function checkTrustline(balances, expected = 0n) {
  const t = (balances || []).find(b => b.asset_code === PINS.aquaCode && b.asset_issuer === PINS.aquaIssuer);
  assert(t, 'owner has no AQUA trustline: a non-zero claim would roll back');
  assert.equal(t.is_authorized, true, 'owner AQUA trustline not authorized');
  const raw = s => BigInt(String(s).replace('.', ''));   // Horizon 7-decimal strings
  assert(raw(t.limit) - raw(t.balance) - raw(t.buying_liabilities) >= u(expected), 'AQUA trustline limit too low for the claim');
  return {balance: t.balance, limit: t.limit, authorized: true};
}
// Post-claim comparison. The on-chain amount can exceed the simulated one (the venue keeps accruing until inclusion).
function compareReadback({simulated, returned, holderBefore, holderAfter, paidBefore, paidAfter, pendingAfter}) {
  const delta = u(holderAfter) - u(holderBefore), paidDelta = u(paidAfter) - u(paidBefore);
  const checks = {balanceDeltaEqualsReturn: delta === u(returned), paidDeltaEqualsReturn: paidDelta === u(returned),
    returnAtLeastSimulated: u(returned) >= u(simulated), pendingCleared: u(pendingAfter) === 0n};
  return {status: Object.values(checks).every(Boolean) ? 'READBACK_OK' : 'READBACK_MISMATCH', checks,
    aquaRaw: {simulated: String(simulated), returned: String(returned), balanceDelta: String(delta), paidDelta: String(paidDelta),
      aboveSimulation: String(u(returned) - u(simulated))}, aqua: {returned: aquaDecimal(returned), balanceDelta: aquaDecimal(delta < 0n ? 0n : delta)}};
}

// ---------------------------------------------------------------- chain
function chain(rpc = new S.rpc.Server(PINS.rpc, {timeout: 60})) {
  const view = async (contract, method, args = []) => {
    const tx = new S.TransactionBuilder(new S.Account(PINS.owner, '0'), {fee: '100', networkPassphrase: PINS.network})
      .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
    const sim = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(sim)) throw Error(`view ${method} failed: ` + String(sim.error || '').split('\n')[0].slice(0, 160));
    return {value: S.scValToNative(sim.result.retval), ledger: sim.latestLedger};
  };
  const codeHash = async c => {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k); return r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');
  };
  const horizonAccount = async () => (await fetch(`${PINS.horizon}/accounts/${PINS.owner}`)).json();
  return {rpc, view, codeHash, horizonAccount};
}
async function snapshot(c) {
  const n = await c.rpc.getNetwork(); assert.equal(n.passphrase, PINS.network);
  assert.equal(await c.codeHash(PINS.vault), PINS.vaultWasm, 'vault code differs from the pilot pin');
  assert.equal(await c.codeHash(PINS.receipt), PINS.receiptWasm, 'receipt code differs from the pilot pin');
  const v = async (ct, m, a) => (await c.view(ct, m, a)).value, o = addr(PINS.owner);
  const [pool, rewardToken, aquaName] = await Promise.all([v(PINS.vault, 'get_pool'), v(PINS.vault, 'get_primary_reward_token'), v(PINS.aqua, 'name')]);
  assert.equal(pool, PINS.pool, 'vault pool differs from the pin'); assert.equal(rewardToken, PINS.aqua, 'vault reward token is not the pinned AQUA SAC');
  assert.equal(aquaName, `${PINS.aquaCode}:${PINS.aquaIssuer}`, 'AQUA SAC is not the pinned classic asset');
  const [account, totals, holderAquaRaw, vaultAquaRaw, beneficial, supply, poolPending, checkpointClaimable] = await Promise.all([
    v(PINS.vault, 'reward_account', [o]), v(PINS.vault, 'reward_totals'), v(PINS.aqua, 'balance', [o]), v(PINS.aqua, 'balance', [addr(PINS.vault)]),
    v(PINS.vault, 'stage_beneficial', [o]), v(PINS.vault, 'total_supply'), v(PINS.pool, 'get_user_reward', [addr(PINS.vault)]).catch(() => null),
    v(PINS.vault, 'checkpoint_rewards').catch(() => null)]);
  let boundaries = 0;   // emergency boundaries recorded (ids are dense from 0); any one may have left the interval open
  while (boundaries < FIXED.maxBoundaryProbe && (await v(PINS.vault, 'reward_boundary', [S.nativeToScVal(BigInt(boundaries), {type: 'u64'})])) !== null) boundaries++;
  const h = await c.horizonAccount(), native = h.balances.find(b => b.asset_type === 'native');
  const warnings = [];
  if (boundaries > 0) warnings.push(`${boundaries} emergency boundary record(s): if the interval is still open, venue rewards claimed now are quarantined for attribution, not paid`);
  if (checkpointClaimable === false) warnings.push('venue claim currently fails: only already-booked rewards would be paid');
  if (u(beneficial) === 0n) warnings.push('owner has no beneficial shares: only already-booked rewards can be paid');
  return {rewardAccount: account, rewardTotals: totals, holderAquaRaw: String(holderAquaRaw), vaultAquaRaw: String(vaultAquaRaw),
    beneficialShares: String(beneficial), totalSupply: String(supply), poolPendingForVaultRaw: poolPending === null ? null : String(poolPending),
    checkpointClaimable, boundaries, trustlineBalances: h.balances, wallet: {xlm: native.balance, subentries: h.subentry_count}, warnings};
}
async function build(c, {op, check}) {
  const acc = await c.rpc.getAccount(PINS.owner), latest = await c.rpc.getLatestLedger();
  const tx = new S.TransactionBuilder(acc, {fee: String(FIXED.bidStroops), networkPassphrase: PINS.network}).addOperation(op)
    .setLedgerbounds(latest.sequence + 1, latest.sequence + FIXED.window).setTimeout(FIXED.timeoutSeconds).build();
  const sim = await c.rpc.simulateTransaction(tx);   // the simulation right before the Ledger signature
  assert(S.rpc.Api.isSimulationSuccess(sim), 'simulation failed: ' + String(sim.error || '').split('\n')[0].slice(0, 200));
  assert(!sim.restorePreamble, 'restoration needed: outside this tool');
  check(sim.result.auth || [], tx.toEnvelope().v1().tx().operations()[0], sim);
  const prepared = S.rpc.assembleTransaction(tx, sim).build();
  checkClaimShape(prepared);
  const data = prepared.toEnvelope().v1().tx().ext().sorobanData(), metrics = checkClaimResources(data, sim.minResourceFee);
  assert.equal(u(prepared.fee), FIXED.bidStroops + u(data.resourceFee().toString()), 'fee is not bid + resource fee');
  assert(u(prepared.fee) <= FIXED.maxFeeStroops, `fee ${prepared.fee} above the 0.5 XLM cap`);
  assert.equal(prepared.signatures.length, 0);
  return {prepared, sim, metrics, simulatedLedger: sim.latestLedger, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + FIXED.window}};
}
const jsonPath = out => out.replace(/\.xdr$/, '') + '.json';
async function prepareClaim(c, {out, allowZero = false}) {
  assert(out && /\.xdr$/.test(out) && !fs.existsSync(out) && !fs.existsSync(jsonPath(out)), '--out FILE.unsigned.xdr required and must not exist');
  const s = await snapshot(c);
  const op = new S.Contract(PINS.vault).call('claim_rewards', addr(PINS.owner));
  let ret;
  const res = await build(c, {op, check: (auth, o, sim) => { checkClaimAuth(auth, o); ret = checkReturn(S.scValToNative(sim.result.retval), {allowZero}); }});
  const trustline = checkTrustline(s.trustlineBalances, ret);
  const xlmStroops = BigInt(Math.round(Number(s.wallet.xlm) * 1e7));
  assert(xlmStroops - u(res.prepared.fee) >= FIXED.minXlmAfterFeeStroops, 'owner XLM too low for the fee');
  fs.writeFileSync(out, res.prepared.toXDR() + '\n', {mode: 0o600, flag: 'wx'});
  const record = {status: 'UNSIGNED_PREPARED_SIGN_ON_LEDGER', signing: false, submission: false, file: out, hash: res.prepared.hash().toString('hex'),
    step: 'CLAIM', call: 'claim_rewards', args: {holder: PINS.owner}, source: PINS.owner, sequence: res.prepared.sequence,
    maxFeeStroops: res.prepared.fee, validLedgers: res.validLedgers, simulatedLedger: res.simulatedLedger, metrics: res.metrics,
    expectedAquaRaw: String(ret), expectedAqua: aquaDecimal(ret), expectedIsLowerBound: 'simulated amount; the on-chain amount can be higher (accrual until inclusion)',
    allowZero, zeroClaim: ret === 0n,
    before: {holderAquaRaw: s.holderAquaRaw, rewardTotals: s.rewardTotals, rewardAccount: s.rewardAccount, vaultAquaRaw: s.vaultAquaRaw,
      poolPendingForVaultRaw: s.poolPendingForVaultRaw, checkpointClaimable: s.checkpointClaimable, boundaries: s.boundaries,
      beneficialShares: s.beneficialShares, totalSupply: s.totalSupply, trustline, wallet: s.wallet},
    warnings: s.warnings, toolSha256: sourceSha256(),
    next: `stellar tx sign --sign-with-ledger --rpc-url ${PINS.rpc} --network-passphrase "${PINS.network}" ${out} > ${out.replace(/\.unsigned\.xdr$|\.xdr$/, '')}.signed.xdr`};
  fs.writeFileSync(jsonPath(out), json(record) + '\n', {mode: 0o600, flag: 'wx'});
  return record;
}
function verifySigned(unsignedText, signedText) {
  const r = L.verifySigned(unsignedText, signedText);
  checkClaimShape(S.TransactionBuilder.fromXDR(signedText.trim(), PINS.network));
  return {...r, call: 'claim_rewards', holder: PINS.owner};
}
async function status(c, hash) {
  assert(/^[0-9a-f]{64}$/.test(hash));
  const r = await c.rpc.getTransaction(hash), h = await fetch(`${PINS.horizon}/transactions/${hash}`).then(x => x.ok ? x.json() : null);
  return {rpc: {status: r.status, ledger: r.ledger ?? null, returnValue: r.returnValue ? S.scValToNative(r.returnValue) : null},
    horizon: h ? {successful: h.successful, ledger: h.ledger, feeChargedStroops: h.fee_charged, source: h.source_account} : 'NOT_FOUND', latestLedger: r.latestLedger};
}
async function readback(c, summaryFile, hashArg) {
  const sm = JSON.parse(fs.readFileSync(summaryFile, 'utf8'));
  assert.equal(sm.call, 'claim_rewards'); const hash = hashArg || sm.hash; assert.equal(hash, sm.hash, 'hash differs from the prepared claim');
  const st = await status(c, hash);
  assert.equal(st.rpc.status, 'SUCCESS', `claim not SUCCESS on RPC (${st.rpc.status})`);
  assert(st.horizon !== 'NOT_FOUND' && st.horizon.successful === true, 'claim not successful on Horizon');
  const v = async (ct, m, a) => (await c.view(ct, m, a)).value, o = addr(PINS.owner);
  const [holderAfter, totals, account] = await Promise.all([v(PINS.aqua, 'balance', [o]), v(PINS.vault, 'reward_totals'), v(PINS.vault, 'reward_account', [o])]);
  return {hash, ledger: st.rpc.ledger, feeChargedStroops: st.horizon.feeChargedStroops, ...compareReadback({simulated: sm.expectedAquaRaw, returned: st.rpc.returnValue,
    holderBefore: sm.before.holderAquaRaw, holderAfter, paidBefore: sm.before.rewardTotals.paid, paidAfter: totals.paid, pendingAfter: account.pending}),
    rewardTotalsAfter: totals, rewardAccountAfter: account, note: 'balance/paid deltas assume no other AQUA movement for the owner between prepare and readback'};
}
async function read(c) { const s = await snapshot(c); return {...s, pins: {vault: PINS.vault, vaultWasm: PINS.vaultWasm, pool: PINS.pool, aqua: PINS.aqua}, signing: false, submission: false}; }
function sourceSha256() { return crypto.createHash('sha256').update(fs.readFileSync(__filename)).digest('hex'); }

module.exports = {PINS, FIXED, REVIEWED_OWNER_CALL_SHA256, checkClaimCall, checkClaimAuth, checkClaimShape, checkClaimResources, checkReturn,
  checkTrustline, compareReadback, verifySigned, aquaDecimal, sourceSha256};
if (require.main === module) {
  const [mode, ...args] = process.argv.slice(2), opt = n => { const i = args.indexOf(n); return i < 0 ? undefined : args[i + 1]; };
  const c = mode === 'verify-signed' ? null : chain();
  const run = {
    read: () => read(c),
    'prepare-claim': () => prepareClaim(c, {out: opt('--out'), allowZero: args.includes('--allow-zero')}),
    'verify-signed': async () => verifySigned(fs.readFileSync(args[0], 'utf8'), fs.readFileSync(args[1], 'utf8')),
    status: () => status(c, args[0]),
    readback: () => readback(c, args[0], args[1]),
  }[mode];
  if (!run) { console.error('usage: read | prepare-claim --out F [--allow-zero] | verify-signed U S | status <hash> | readback F.unsigned.json [hash]'); process.exit(1); }
  run().then(r => console.log(json(r))).catch(e => { console.error('REFUSED: ' + e.message); process.exitCode = 1; });
}
