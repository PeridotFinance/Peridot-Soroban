// Read-only funding pre-flight for the PRE-RELEASE deployment stage (v3, 2026-11-15). Copy-in-spirit of
// funding-preflight.cjs (left unchanged; its READY status also demands the keeper and every trustline, which the
// pre-release stage does not). No signer, no submission: one getLedgerEntries call on one ledger.
// Gates:
//   step 1  : spendable XLM (balance - selling liabilities) >= requiredBeforeStep1Stroops from the generated
//             requirements (max of lead formula and fee budget + floor + margin), and the source sequence still
//             equals the reviewed config's initialSourceSequence (otherwise a successor config is needed first).
//   step 16 : PYUSD >= 1,001 (1,000 principal + 1 permanent floor that leaves the wallet at step 16).
// Informational: AQUA trustline (needed before the seed, not for steps 1-18), keeper account existence (needed
// before step 19).
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const ROOT = path.resolve(__dirname, '../..');
const json = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const u = (v, label) => { assert(typeof v === 'string' && /^(0|[1-9][0-9]*)$/.test(v), `${label} must be an exact raw unsigned integer string`); return BigInt(v); };
const dec = raw => { const n = BigInt(raw), neg = n < 0n, v = neg ? -n : n; return (neg ? '-' : '') + (v / 10000000n) + '.' + String(v % 10000000n).padStart(7, '0'); };
const ASSETS = {PYUSD: {code: 'PYUSD', issuer: 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5'},
  USDC: {code: 'USDC', issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'},
  AQUA: {code: 'AQUA', issuer: 'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA'}};
// Pure. observed = {ledger, sequence, xlmBalanceStroops, xlmSellingLiabilitiesStroops, trustlines: {PYUSD|USDC|AQUA}, keeperAccountExists}
// gate 'step1' (default): XLM + sequence gate the first step of this config. gate 'step16': only PYUSD gates (run it
// mid-ceremony right before step 16; XLM/sequence are then reported, not gating, because earlier steps spent them).
function evaluate(requirements, config, observed, gate = 'step1') {
  assert(['step1', 'step16'].includes(gate), 'gate must be step1 or step16');
  assert.equal(requirements.status, 'PRE_RELEASE_FUNDING_REQUIREMENTS'); assert.equal(config.releaseStage, 'PRE_RELEASE_DEPLOYMENT_STEPS_1_TO_18');
  // USER DECISION 15 Nov 2026 ~13:15 UTC: seed = cap = 800 PYUSD; floor 1 PYUSD on top.
  assert.equal(requirements.pyusd.requiredBeforeStep16Raw, '8010000000', '800 seed + 1 floor is fixed');
  const need = u(requirements.xlm.requiredBeforeStep1Stroops, 'XLM requirement');
  const floorCheck = u(config.feeBudgetStroops, 'budget') + u(config.balanceFloorStroops, 'floor');
  assert(need >= floorCheck, 'requirement below fee budget + balance floor: regenerate requirements');
  const spendable = u(observed.xlmBalanceStroops, 'balance') - u(observed.xlmSellingLiabilitiesStroops, 'liabilities');
  const step1 = [], step16 = [], info = [];
  if (spendable < need) step1.push(`XLM: spendable ${dec(spendable)}, needs ${dec(need)} before step 1: short by ${dec(need - spendable)} XLM`);
  if (observed.sequence !== config.initialSourceSequence)
    step1.push(`SEQUENCE: account sequence ${observed.sequence} differs from reviewed initialSourceSequence ${config.initialSourceSequence}: regenerate a (successor) config before step 1`);
  const py = observed.trustlines.PYUSD, pyHave = py?.present ? u(py.balance, 'PYUSD') : 0n, pyNeed = u(requirements.pyusd.requiredBeforeStep16Raw, 'PYUSD need');
  if (!py?.present || py.authorized !== true) step16.push('TRUSTLINE: PYUSD trustline missing or not authorized');
  if (pyHave < pyNeed) step16.push(`PYUSD: holds ${dec(pyHave)}, needs ${dec(pyNeed)} before step 16: short by ${dec(pyNeed - pyHave)} PYUSD`);
  const reserveRoom = pyHave - pyNeed;
  if (reserveRoom >= 0n) info.push(`PYUSD after floor and 800 seed: ${dec(reserveRoom)} PYUSD left for the execution reserve. The reserve (quoted minimum + 50%) can only be quoted once the vault exists; the seed tool must re-check it on the seed ledger and refuse if it does not fit.`);
  else info.push(`PYUSD: nothing is left for the execution reserve after floor and seed (short ${dec(-reserveRoom)} before any reserve).`);
  for (const code of ['USDC', 'AQUA']) { const t = observed.trustlines[code]; if (!t?.present || t.authorized !== true) info.push(`${code} trustline missing or not authorized (required before the seed, not for steps 1-18). Add it BEFORE generating the config for step 1, or only after step 18 with a successor config.`); }
  if (config.keeperPublicAddress && !observed.keeperAccountExists) info.push(`keeper account ${config.keeperPublicAddress} does not exist yet (required before step 19). Funding it from the Ledger account changes the sequence: do it before generating the step-1 config, or after step 18 with a successor config.`);
  if (gate === 'step16') return {gate, status: step16.length ? 'BLOCKED_BEFORE_STEP_16' : 'PYUSD_READY_FOR_STEP_16', step16: step16.length ? 'PYUSD_NOT_READY_FOR_STEP_16' : 'PYUSD_READY_FOR_STEP_16',
    shortfallsBeforeStep16: step16, notGatingHere: step1, information: info, required: {pyusdBeforeStep16Raw: String(pyNeed)},
    observed: {...observed, xlmSpendableStroops: String(spendable), xlmSpendable: dec(spendable)}, signing: false, submission: false};
  return {gate, status: step1.length ? 'BLOCKED_BEFORE_STEP_1' : 'PRE_RELEASE_FUNDED_FOR_STEP_1',
    step16: step16.length ? 'PYUSD_NOT_READY_FOR_STEP_16' : 'PYUSD_READY_FOR_STEP_16',
    shortfallsBeforeStep1: step1, shortfallsBeforeStep16: step16, information: info,
    required: {xlmBeforeStep1Stroops: String(need), xlmBeforeStep1: dec(need), pyusdBeforeStep16Raw: String(pyNeed)},
    observed: {...observed, xlmSpendableStroops: String(spendable), xlmSpendable: dec(spendable)}, signing: false, submission: false};
}
async function check(requirements, config, rpc = null, gate = 'step1') {
  assert.equal(requirements.network, S.Networks.PUBLIC); assert.equal(requirements.account, config.source);
  rpc ??= new S.rpc.Server(requirements.rpcUrl, {timeout: 30});
  assert.equal((await rpc.getNetwork()).passphrase, requirements.network);
  const accountId = new S.Address(requirements.account).toScAddress().accountId(), codes = Object.keys(ASSETS);
  const keys = [S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId})),
    ...codes.map(c => S.xdr.LedgerKey.trustline(new S.xdr.LedgerKeyTrustLine({accountId, asset: new S.Asset(ASSETS[c].code, ASSETS[c].issuer).toTrustLineXDRObject()})))];
  if (config.keeperPublicAddress) keys.push(S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(config.keeperPublicAddress).toScAddress().accountId()})));
  const got = await rpc.getLedgerEntries(...keys), map = new Map(got.entries.map(row => [row.key.toXDR('base64'), row]));
  const row = map.get(keys[0].toXDR('base64')); assert(row, 'operator account not found');
  const a = row.val.account(), v1 = a.ext().switch() === 1 ? a.ext().v1() : null;
  const trustlines = Object.fromEntries(codes.map((c, i) => { const t = map.get(keys[i + 1].toXDR('base64'))?.val.trustLine();
    return [c, t ? {present: true, authorized: (t.flags() & 1) === 1, balance: t.balance().toString()} : {present: false}]; }));
  const observed = {ledger: got.latestLedger, sequence: a.seqNum().toString(), subentries: a.numSubEntries(), xlmBalanceStroops: a.balance().toString(),
    xlmSellingLiabilitiesStroops: v1 ? v1.liabilities().selling().toString() : '0', trustlines,
    keeperAccountExists: config.keeperPublicAddress ? map.has(keys.at(-1).toXDR('base64')) : null};
  try { const l = await rpc.getLedgers({startLedger: got.latestLedger, pagination: {limit: 1}});
    observed.ledgerCloseUtc = new Date(Number(l.ledgers[0].headerXdr.header().scpValue().closeTime().toString()) * 1000).toISOString(); } catch { observed.ledgerCloseUtc = null; }
  return evaluate(requirements, config, observed, gate);
}
module.exports = {evaluate, check};
if (require.main === module) (async () => {
  const args = process.argv.slice(2), g = args.indexOf('--gate'), gate = g < 0 ? 'step1' : args.splice(g, 2)[1];
  const [file, output, ...extra] = args;
  assert(file && !extra.length, 'usage: node funding-preflight-v3.cjs <funding-requirements-pre-release.json> [new-report.json] [--gate step1|step16]');
  if (output) assert(!fs.existsSync(output), 'preserve previous reports');
  const requirements = JSON.parse(fs.readFileSync(file)), pin = requirements.deploymentConfig;
  const bytes = fs.readFileSync(path.resolve(ROOT, pin.path)); assert.equal(sha(bytes), pin.sha256, 'deployment config changed since the requirements were generated');
  const result = await check(requirements, JSON.parse(bytes), null, gate);
  if (output) fs.writeFileSync(output, json(result), {flag: 'wx'});
  process.stdout.write(json(result));
  const ok = gate === 'step16' ? result.status === 'PYUSD_READY_FOR_STEP_16' : result.status === 'PRE_RELEASE_FUNDED_FOR_STEP_1';
  if (!ok) { process.stderr.write('BLOCKED: ' + (gate === 'step16' ? result.shortfallsBeforeStep16 : result.shortfallsBeforeStep1).join(' | ') + '\n'); process.exitCode = 1; }
})().catch(error => { process.stderr.write(error.message + '\n'); process.exitCode = 1; });
