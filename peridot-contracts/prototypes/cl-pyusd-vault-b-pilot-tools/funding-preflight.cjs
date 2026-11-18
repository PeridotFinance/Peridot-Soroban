// Read-only funding pre-flight for the operator account, to be run BEFORE step 1 and before every funding
// step (new on 2026-11-14; no v1). No signer, no submission, one getLedgerEntries call.
// It answers: does the Ledger account hold enough PYUSD and XLM, and the USDC and AQUA trustlines, for the
// deployment and the pilot? Any shortfall or any requirement that has not been supplied means BLOCKED.
// The execution reserve can only be QUOTED once the vault exists; before that the operator supplies an
// explicit allowance, and pilot-ceremony-v2.cjs checks the fresh quote again before the seed.
'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const json = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
const u = (value, label) => {assert(typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value), `${label} must be an exact raw unsigned integer string`); return BigInt(value);};
const dec = raw => {const n = BigInt(raw), neg = n < 0n, v = neg ? -n : n; return (neg ? '-' : '') + (v / 10000000n) + '.' + String(v % 10000000n).padStart(7, '0');};
const PYUSD_PARTS = ['seedPrincipalRaw', 'permanentFloorRaw', 'reserveAllowanceRaw'];
const XLM_PARTS = ['uploadsStroops', 'creationAndConfigurationStroops', 'pilotFeesStroops', 'walletFloorStroops'];
// Pure. observed = {xlmSpendableStroops, trustlines: {funding|other|reward: {code, present, authorized, balance}}}
function evaluate(requirements, observed) {
  assert.equal(requirements.pyusd?.seedPrincipalRaw, '10000000000', 'seed principal is fixed at 1,000 PYUSD'); assert.equal(requirements.pyusd?.permanentFloorRaw, '10000000', 'floor is fixed at 1 PYUSD, on top of the principal');
  const shortfalls = [], notSupplied = [], sumKnown = (group, parts) => parts.reduce((total, name) => {
    if (group?.[name] === null || group?.[name] === undefined) {notSupplied.push(name); return total;} return total + u(group[name], name);}, 0n);
  const pyusdNeed = sumKnown(requirements.pyusd, PYUSD_PARTS), xlmNeed = sumKnown(requirements.xlm, XLM_PARTS);
  const funding = observed.trustlines?.funding, pyusdHave = funding?.present ? u(String(funding.balance), 'PYUSD balance') : 0n, xlmHave = u(String(observed.xlmSpendableStroops), 'XLM balance');
  const partial = names => names.some(n => notSupplied.includes(n)) ? ' AT LEAST (some requirements are not supplied yet)' : '';
  if (pyusdHave < pyusdNeed) shortfalls.push(`PYUSD: holds ${dec(pyusdHave)}, needs${partial(PYUSD_PARTS)} ${dec(pyusdNeed)} (1,000 principal + 1 floor + reserve allowance): short by ${dec(pyusdNeed - pyusdHave)}`);
  if (xlmHave < xlmNeed) shortfalls.push(`XLM: spendable ${dec(xlmHave)}, needs${partial(XLM_PARTS)} ${dec(xlmNeed)}: short by ${dec(xlmNeed - xlmHave)}`);
  const why = {funding: 'the seed is paid in PYUSD', other: 'in-kind and emergency payouts are paid in both pool tokens', reward: 'a reward claim pays AQUA and rolls back without the trustline'};
  for (const role of ['funding', 'other', 'reward']) {
    const line = observed.trustlines?.[role], code = requirements.assets[role].code;
    if (!line?.present) shortfalls.push(`TRUSTLINE: no ${code} trustline (${why[role]})`);
    else if (line.authorized !== true) shortfalls.push(`TRUSTLINE: ${code} trustline is not authorized (${why[role]})`);
  }
  if (requirements.keeperPublicAddress === null || requirements.keeperPublicAddress === undefined) notSupplied.push('keeperPublicAddress');
  else {assert(S.StrKey.isValidEd25519PublicKey(requirements.keeperPublicAddress)); assert.notEqual(requirements.keeperPublicAddress, requirements.account, 'the keeper must not be the Ledger account');}
  const ready = shortfalls.length === 0 && notSupplied.length === 0;
  return {status: ready ? 'FUNDING_READY_FOR_STEP_1' : 'BLOCKED', shortfalls, notSupplied,
    required: {pyusdRaw: String(pyusdNeed), xlmStroops: String(xlmNeed)}, observed, signing: false, submission: false};
}
async function check(requirements, rpc = null) { // rpc injectable for offline tests only
  assert.equal(requirements.network, S.Networks.PUBLIC); assert(S.StrKey.isValidEd25519PublicKey(requirements.account));
  rpc ??= new S.rpc.Server(requirements.rpcUrl, {timeout: 30});
  assert.equal((await rpc.getNetwork()).passphrase, requirements.network);
  const accountId = new S.Address(requirements.account).toScAddress().accountId(), roles = ['funding', 'other', 'reward'];
  const keys = [S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId})), ...roles.map(role => S.xdr.LedgerKey.trustline(new S.xdr.LedgerKeyTrustLine({accountId,
    asset: new S.Asset(requirements.assets[role].code, requirements.assets[role].issuer).toTrustLineXDRObject()})))];
  const got = await rpc.getLedgerEntries(...keys), map = new Map(got.entries.map(row => [row.key.toXDR('base64'), row]));
  const account = map.get(keys[0].toXDR('base64')); assert(account, 'operator account not found');
  const a = account.val.account(), v1 = a.ext().switch() === 1 ? a.ext().v1() : null;
  const trustlines = Object.fromEntries(roles.map((role, i) => {const t = map.get(keys[i + 1].toXDR('base64'))?.val.trustLine();
    return [role, t ? {code: requirements.assets[role].code, present: true, authorized: (t.flags() & 1) === 1, balance: t.balance().toString()} : {code: requirements.assets[role].code, present: false}];}));
  const observed = {ledger: got.latestLedger, xlmBalanceStroops: a.balance().toString(), xlmSellingLiabilitiesStroops: v1 ? v1.liabilities().selling().toString() : '0', trustlines};
  observed.xlmSpendableStroops = String(BigInt(observed.xlmBalanceStroops) - BigInt(observed.xlmSellingLiabilitiesStroops));
  return evaluate(requirements, observed);
}
module.exports = {evaluate, check};
if (require.main === module) (async () => {
  const [file, output, ...extra] = process.argv.slice(2);
  assert(file && !extra.length, 'usage: node funding-preflight.cjs <funding-requirements.json> [new-report.json]');
  if (output) assert(!fs.existsSync(output), 'preserve previous reports');
  const result = await check(JSON.parse(fs.readFileSync(file)));
  if (output) fs.writeFileSync(output, json(result), {flag: 'wx'});
  process.stdout.write(json(result)); if (result.status !== 'FUNDING_READY_FOR_STEP_1') {process.stderr.write('BLOCKED: ' + [...result.shortfalls, ...result.notSupplied.map(n => 'not supplied: ' + n)].join(' | ') + '\n'); process.exitCode = 1;}
})().catch(error => {process.stderr.write(error.message + '\n'); process.exitCode = 1;});
