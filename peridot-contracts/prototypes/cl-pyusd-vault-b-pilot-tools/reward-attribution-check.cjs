// Read-only reward attribution check (new on 2026-11-14; no v1). No signer, no submission.
//
// USER DECISION 14 Nov 2026: attributed reward amounts must ALWAYS equal the amount actually received,
// checked by a read before the admin signs. This program is that read. It exits non-zero unless the
// amounts the admin is about to sign for ONE sealed interval sum EXACTLY to that interval's `received`.
//
// Why exact: on the frozen bytes a governed total ABOVE the amount received scales every segment of the
// interval, including the post-boundary segment of a holder who stayed; a total BELOW it leaves the excess
// in quarantine with no release entrypoint (reward coverage REPORT.md, finding 2).
//
// The five rules enforced before ANY attribution signature (independent review, 14 Nov 2026):
//   (i)   every segment of the interval is attributed (the file lists all of them);
//   (ii)  the amounts sum to exactly interval.received;
//   (iii) exactly 0 is attributed to a segment with zero eligible weight (nothing could ever settle it);
//   (iv)  the per-boundary route (attribute_rewards) is refused unless the boundary holder was the
//         interval's only owner;
//   (v)   prepare_reward_segment is planned for every segment before settlement (and must already have
//         completed before signing when the interval contains waivers, because waived weight is only final then).
// Attributions are ONE-SHOT AND FINAL: an attributed amount cannot be changed, withdrawn or topped up.
//
// Reads used (frozen vault ABI only): reward_interval(id), reward_segment(id); for the per-boundary route
// also reward_boundary(id), reward_history_count(holder), reward_weight(holder, n).
// What the frozen ABI does NOT expose, stated in every output (ABI_GAPS):
//  - a pending proposal. propose_reward_segment stores {amount, evidence, execute_after} under a private
//    key with no view; this check therefore compares the ADMIN'S FILE, not the on-chain proposal.
//  - the per-boundary route (propose_reward_attribution / attribute_rewards): no view of a pending proposal
//    or of which boundary was already attributed, and no event. Only the interval's remaining total is readable.
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const VAULT_WASM_SHA256 = 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9';
const MAX_SEGMENTS = 64, MAX_BOUNDARIES = 256, MAX_CHECKPOINTS = 256, SCALE = 1000000000000000000n;
const {lastBoundaryId} = require('./keeper-plan-v2.cjs');
const FINALITY = 'ONE-SHOT AND FINAL: once attribute_reward_segment / attribute_rewards is applied the amount cannot be changed, withdrawn or topped up. A wrong amount can only be lived with. Do not sign unless this check passed on this exact file in this ledger.';
const ABI_GAPS = Object.freeze([
  'Pending proposal not readable: propose_reward_segment stores HistoryProposal(segment) {amount, evidence, execute_after} with no view. The amounts checked are the ones in the admin file. The contract rejects attribute_reward_segment unless amount and evidence equal the stored proposal and 86,400 s have passed.',
  'Timelock expiry not readable: execute_after appears only in the reward_attribution_proposed event.',
  'Per-boundary route: propose_reward_attribution / attribute_rewards have no view of a pending proposal or of which boundary was already attributed, and emit no event; only the interval remaining total (remaining_scaled) is readable. Accepted only when one holder owned the whole interval.',
  'No view of the current interval id or of the open flag: the admin names the sealed interval; this check verifies sealed == true on that record.',
]);
const json = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
class Refusal extends Error {constructor(reason, found) {super(reason); this.name = 'Refusal'; this.found = JSON.parse(json(found ?? null));}}
const refuse = (reason, found) => {throw new Refusal(reason, found);};
const raw = (value, label) => {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) refuse(`${label} must be an exact raw unsigned integer string (no decimals, no numbers)`, {value});
  return BigInt(value);
};
// Pure. `interval` and `segments` (id -> record or null) are what the two views returned on one ledger.
function evaluate(proposal, interval, segments, history = null) {
  if (!proposal || typeof proposal !== 'object') refuse('proposal file required');
  if (proposal.route === 'direct') return evaluateDirect(proposal, interval, segments, history);
  if (proposal.route !== 'segment') refuse("route must be 'segment' (propose_reward_segment / attribute_reward_segment) or 'direct' (per-boundary attribute_rewards, sole owner only)", {route: proposal.route ?? null});
  const id = raw(proposal.interval, 'interval');
  if (!interval) refuse(`reward_interval(${id}) does not exist`);
  const view = {sealed: interval.sealed, direct_governed: interval.direct_governed, first_segment: String(interval.first_segment), segments: Number(interval.segments),
    resolved: Number(interval.resolved), expected: String(interval.expected), received: String(interval.received), remaining_scaled: String(interval.remaining_scaled), waivers: Number(interval.waivers ?? 0)};
  if (interval.sealed !== true) refuse(`interval ${id} is not sealed: no amount has been received for it yet`, view);
  if (interval.direct_governed !== false) refuse(`interval ${id} was already paid through the direct route; segment attribution is no longer possible`, view);
  const count = view.segments, first = BigInt(view.first_segment), received = raw(view.received, 'interval.received');
  if (!(count >= 1 && count <= MAX_SEGMENTS)) refuse(`interval ${id} has ${count} segments; this check handles 1..${MAX_SEGMENTS}`, view);
  if (!Array.isArray(proposal.segments)) refuse('proposal.segments must list every segment of the interval');
  const ids = Array.from({length: count}, (_, i) => String(first + BigInt(i))), rows = new Map();
  for (const row of proposal.segments) {
    const key = String(raw(row?.id, 'segment id'));
    if (rows.has(key)) refuse(`segment ${key} is listed twice`, {segment: key});
    if (!ids.includes(key)) refuse(`segment ${key} does not belong to interval ${id} (its segments are ${ids[0]}..${ids.at(-1)})`, {segment: key, intervalSegments: ids});
    rows.set(key, row);
  }
  const missing = ids.filter(key => !rows.has(key));
  if (missing.length) refuse(`every segment of the interval must be listed; missing: ${missing.join(', ')}`, {missing, intervalSegments: ids});
  let total = 0n, alreadyAttributed = 0n, resolved = 0; const table = [], toSign = [], afterAttribution = [], warnings = [];
  for (const key of ids) {
    const segment = segments[key], row = rows.get(key), amount = raw(row.amountRaw, `segment ${key} amountRaw`);
    if (!segment) refuse(`reward_segment(${key}) does not exist`, {segment: key});
    if (String(segment.interval) !== String(id)) refuse(`reward_segment(${key}) belongs to interval ${segment.interval}, not ${id}`, {segment: key});
    const onChain = segment.expected === null || segment.expected === undefined ? null : raw(String(segment.expected), `segment ${key} on-chain expected`);
    // Rule (v), early part: with waivers in the interval the waived weight is final only after preparation.
    if (view.waivers > 0 && segment.prepared !== true) refuse(`segment ${key}: this interval contains ${view.waivers} waiver(s) and prepare_reward_segment has not completed for it, so its waived weight is not final. Send the permissionless prepare_reward_segment(${key}, 4) until it returns true, then run this check again. Do not sign`, {segment: key, waivers: view.waivers, waiver_cursor: segment.waiver_cursor ?? null});
    // Rule (iii): zero eligible weight means no holder can ever settle this segment's amount.
    const supply = raw(String(segment.supply), `segment ${key} supply`), owned = raw(String(segment.owned_supply), `segment ${key} owned_supply`), waived = raw(String(segment.waived_weight), `segment ${key} waived_weight`);
    const eligible = owned >= waived ? owned - waived : 0n, zeroWeight = supply === 0n || eligible === 0n || segment.history_complete !== true;
    if (zeroWeight && amount !== 0n) {
      const reason = `segment ${key} has zero eligible weight (supply ${supply}, owned ${owned}, waived ${waived}, history_complete ${segment.history_complete === true}): exactly 0 must be attributed to it, the file says ${amount}. Any other amount can never be settled`;
      if (onChain === null) refuse(reason + '. Do not sign', {segment: key, supply: String(supply), owned_supply: String(owned), waived_weight: String(waived)});
      warnings.push(reason + ' (already attributed on chain; it cannot be undone)');
    }
    afterAttribution.push({step: 'prepare', method: 'prepare_reward_segment', args: [key, 4], caller: 'anyone', repeatUntil: 'returns true', alreadyPrepared: segment.prepared === true});
    if (onChain !== null) {
      resolved++; alreadyAttributed += onChain;
      if (amount !== onChain) refuse(`segment ${key} is already attributed ${onChain} on chain; the file says ${amount}. An attributed amount cannot be changed`, {segment: key, onChain: String(onChain), file: String(amount)});
    } else {
      if (typeof row.evidenceSha256 !== 'string' || !/^[0-9a-f]{64}$/.test(row.evidenceSha256) || /^0{64}$/.test(row.evidenceSha256)) refuse(`segment ${key} needs a non-zero 32-byte evidence digest (lower-case hex); the contract rejects a zero digest`, {segment: key});
      toSign.push({segment: key, amountRaw: String(amount), evidenceSha256: row.evidenceSha256,
        first: {method: 'propose_reward_segment', args: [key, String(amount), row.evidenceSha256]},
        after86400s: {method: 'attribute_reward_segment', args: [key, String(amount), row.evidenceSha256]}});
    }
    total += amount; table.push({segment: key, amountRaw: String(amount), status: onChain !== null ? 'ALREADY_ATTRIBUTED_ON_CHAIN' : 'TO_BE_PROPOSED_AND_ATTRIBUTED', governed: segment.governed === true,
      eligibleWeight: String(eligible), zeroEligibleWeight: zeroWeight, prepared: segment.prepared === true});
  }
  if (resolved !== view.resolved || alreadyAttributed !== raw(view.expected, 'interval.expected')) refuse('interval and segment records disagree (resolved count or expected total); read again, do not sign', {...view, segmentsResolved: resolved, segmentsExpectedTotal: String(alreadyAttributed)});
  if (total !== received) {
    const over = total > received, difference = over ? total - received : received - total;
    refuse(`attribution total ${total} does not equal the amount received ${received}: ${over ? 'OVER' : 'UNDER'} by ${difference}. ` + (over
      ? 'A total above the amount received scales every segment, so the shortfall would also fall on the holder who stayed.'
      : 'A total below the amount received leaves the excess in quarantine; these bytes have no entrypoint to release it.') + ' Do not sign.',
    {...view, proposedTotalRaw: String(total), receivedRaw: String(received), differenceRaw: String(difference), direction: over ? 'OVER' : 'UNDER', segments: table});
  }
  afterAttribution.push({step: 'settle', method: 'settle_reward_segment', args: ['<holder>', '<segment>', '<holder weight checkpoint>'], caller: 'anyone', note: 'per holder and segment, only after EVERY segment is attributed and prepared'},
    {step: 'claim', method: 'claim_rewards', args: ['<holder>'], caller: 'the holder'});
  return {status: 'ATTRIBUTION_SUM_EQUALS_RECEIVED', route: 'segment', rules: {everySegmentListed: true, sumEqualsReceived: true, zeroWeightSegmentsGetZero: true, prepareBeforeSettlement: true},
    finality: FINALITY, interval: String(id), receivedRaw: String(received), proposedTotalRaw: String(total),
    alreadyAttributedRaw: String(alreadyAttributed), segments: table, toSign, afterAttribution, warnings, intervalRecord: view, abiGaps: ABI_GAPS, signing: false, submission: false};
}
// Rule (iv). The per-boundary route credits ONE boundary holder directly and closes the segment route for the
// interval. It is acceptable only if that holder was the interval's only owner: every boundary of the interval is
// the same holder's, nobody waived, and at every segment snapshot the holder's recorded weight equals the owned supply.
// history = {boundaries: [{id, ...Boundary}] (ALL boundaries of the vault), weights: {holder: [{revision, weight}]}}
function evaluateDirect(proposal, interval, segments, history) {
  const id = raw(proposal.interval, 'interval');
  if (!interval) refuse(`reward_interval(${id}) does not exist`);
  if (interval.sealed !== true) refuse(`interval ${id} is not sealed: no amount has been received for it yet`);
  if (!history || !Array.isArray(history.boundaries)) refuse('per-boundary route: the boundary history could not be read; refused');
  const mine = history.boundaries.filter(b => String(b.interval) === String(id)), holders = [...new Set(mine.map(b => b.holder))];
  const no = (reason, found) => refuse(`per-boundary route (attribute_rewards) refused: ${reason}. Use the segment route. Do not sign`, found);
  if (mine.length === 0) no(`interval ${id} has no boundary`, {});
  if (holders.length !== 1) no(`interval ${id} has boundaries of ${holders.length} different holders; attribute_rewards can pay boundary holders only and would leave every other owner out`, {holders});
  if (Number(interval.waivers ?? 0) !== 0 || mine.some(b => b.waived)) no('a waiver exists in this interval', {waivers: interval.waivers});
  if (Number(interval.resolved) !== 0 || String(interval.original_scaled) !== '0' || String(interval.redistribution_scaled) !== '0') no('segment attribution or settlement has already started for this interval', {});
  const holder = holders[0], points = history.weights?.[holder]; if (!Array.isArray(points)) no('the holder weight history could not be read', {holder});
  const weightAt = revision => {let w = 0n; for (const p of points) if (BigInt(p.revision) <= BigInt(revision)) w = BigInt(p.weight); return w;};
  const count = Number(interval.segments), first = BigInt(interval.first_segment);
  if (!(count >= 1 && count <= MAX_SEGMENTS)) no(`interval has ${count} segments`, {});
  for (let i = 0n; i < BigInt(count); i++) {
    const key = String(first + i), segment = segments[key]; if (!segment) no(`reward_segment(${key}) does not exist`, {segment: key});
    const owned = BigInt(segment.owned_supply), w = weightAt(segment.revision);
    if (owned === 0n || w !== owned || segment.history_complete !== true) no(`the boundary holder was NOT the interval's only owner: at segment ${key} the holder's recorded weight is ${w} of owned supply ${owned}` + (segment.history_complete === true ? '' : ' (weight history incomplete)') + '; the other owners would receive nothing', {holder, segment: key, holderWeight: String(w), owned_supply: String(owned)});
  }
  const remainingScaled = raw(String(interval.remaining_scaled), 'remaining_scaled'), received = raw(String(interval.received), 'received');
  if (remainingScaled % SCALE !== 0n) no('the interval remaining backing is not a whole raw amount', {remaining_scaled: String(remainingScaled)});
  if (interval.direct_governed !== true && remainingScaled !== received * SCALE) no('interval records disagree (remaining differs from received before any attribution)', {});
  const remaining = remainingScaled / SCALE, seen = new Set(), toSign = []; let total = 0n;
  if (!Array.isArray(proposal.boundaries) || proposal.boundaries.length === 0) refuse('proposal.boundaries must list the boundaries to attribute');
  for (const row of proposal.boundaries) {
    const key = String(raw(row?.id, 'boundary id')), b = mine.find(x => String(x.id) === key), amount = raw(row.amountRaw, `boundary ${key} amountRaw`);
    if (seen.has(key)) refuse(`boundary ${key} is listed twice`); seen.add(key);
    if (!b) no(`boundary ${key} does not belong to interval ${id}`, {boundary: key});
    if (BigInt(b.shares) === 0n) no(`boundary ${key} has zero shares; the contract rejects it`, {boundary: key});
    if (amount === 0n) refuse(`boundary ${key}: the contract rejects a zero attribution`);
    if (typeof row.evidenceSha256 !== 'string' || !/^[0-9a-f]{64}$/.test(row.evidenceSha256) || /^0{64}$/.test(row.evidenceSha256)) refuse(`boundary ${key} needs a non-zero 32-byte evidence digest (lower-case hex)`);
    total += amount; toSign.push({boundary: key, holder, amountRaw: String(amount), first: {method: 'propose_reward_attribution', args: [key, String(amount), row.evidenceSha256]},
      after86400s: {method: 'attribute_rewards', args: [key, String(amount), row.evidenceSha256]}});
  }
  if (total !== remaining) {const over = total > remaining, difference = over ? total - remaining : remaining - total;
    refuse(`attribution total ${total} does not equal the amount still held for the interval ${remaining} (received ${received}): ${over ? 'OVER' : 'UNDER'} by ${difference}. Do not sign.`,
      {proposedTotalRaw: String(total), remainingRaw: String(remaining), receivedRaw: String(received), differenceRaw: String(difference), direction: over ? 'OVER' : 'UNDER'});}
  return {status: 'ATTRIBUTION_SUM_EQUALS_RECEIVED', route: 'direct', rules: {soleOwnerProven: true, sumEqualsRemainingOfReceived: true}, finality: FINALITY, interval: String(id), soleOwner: holder,
    receivedRaw: String(received), remainingRaw: String(remaining), proposedTotalRaw: String(total), toSign,
    afterAttribution: [{step: 'claim', method: 'claim_rewards', args: [holder], caller: 'the holder'}], warnings: ['The per-boundary route closes the segment route for this interval for good.'],
    abiGaps: ABI_GAPS, signing: false, submission: false};
}
// Coherent one-ledger reads of the frozen vault. `rpc` is injectable for offline tests only.
async function check(config, proposal, rpc = null) {
  assert.equal(config.network, S.Networks.PUBLIC); assert.equal(config.readOnly, true, 'read-only public configuration required');
  assert(S.StrKey.isValidContract(config.vault) && S.StrKey.isValidEd25519PublicKey(config.admin));
  if (proposal?.vault !== config.vault) refuse('proposal names another vault', {proposal: proposal?.vault ?? null, config: config.vault});
  if (!['segment', 'direct'].includes(proposal.route)) evaluate(proposal, null, {}); // refuse before any read
  raw(proposal.interval, 'interval');
  rpc ??= new S.rpc.Server(config.rpcUrl, {timeout: 30});
  assert.equal((await rpc.getNetwork()).passphrase, config.network);
  const start = await rpc.getLatestLedger(), ledgers = [start.sequence];
  const key = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(config.vault).toScAddress(),
    key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
  const instance = await rpc.getLedgerEntries(key); ledgers.push(instance.latestLedger);
  if (instance.entries.length !== 1) refuse('vault instance not found or archived; nothing can be checked');
  const executable = instance.entries[0].val.contractData().val().instance().executable();
  assert.equal(executable.switch().name, 'contractExecutableWasm'); assert.equal(executable.wasmHash().toString('hex'), VAULT_WASM_SHA256, 'vault is not the frozen candidate');
  const source = await rpc.getAccount(config.admin), u64 = id => S.nativeToScVal(BigInt(id), {type: 'u64'});
  const call = async (method, args, label) => {
    const tx = new S.TransactionBuilder(new S.Account(config.admin, source.sequenceNumber()), {fee: '1000', networkPassphrase: config.network})
      .addOperation(new S.Contract(config.vault).call(method, ...args)).setTimeout(60).build();
    const sim = await rpc.simulateTransaction(tx); ledgers.push(sim.latestLedger);
    if (!S.rpc.Api.isSimulationSuccess(sim) || sim.restorePreamble || !sim.result) refuse(`${label} could not be read (failed or archived); an unreadable record is never treated as empty`);
    assert.equal(sim.result.auth.length, 0); return S.scValToNative(sim.result.retval);
  };
  const read = (method, id) => call(method, [u64(id)], `${method}(${id})`);
  const interval = await read('reward_interval', proposal.interval), segments = {};
  if (interval && Number(interval.segments) >= 1 && Number(interval.segments) <= MAX_SEGMENTS)
    for (let i = 0n; i < BigInt(interval.segments); i++) {const id = String(BigInt(interval.first_segment) + i); segments[id] = await read('reward_segment', id);}
  let history = null;
  if (proposal.route === 'direct') {
    const cache = new Map(), boundary = async id => {const k = String(id); if (!cache.has(k)) cache.set(k, await read('reward_boundary', k)); return cache.get(k);};
    const last = await lastBoundaryId(async id => (await boundary(id)) != null, BigInt(MAX_BOUNDARIES) * 4n);
    if (last !== null && last >= BigInt(MAX_BOUNDARIES)) refuse(`more than ${MAX_BOUNDARIES} boundaries exist; the per-boundary route is not checked at this scale`);
    history = {boundaries: [], weights: {}};
    for (let i = 0n; last !== null && i <= last; i++) history.boundaries.push({id: String(i), ...(await boundary(i))});
    for (const holder of new Set(history.boundaries.filter(b => String(b.interval) === String(proposal.interval)).map(b => b.holder))) {
      const n = Number(await call('reward_history_count', [new S.Address(holder).toScVal()], `reward_history_count(${holder})`));
      if (n > MAX_CHECKPOINTS) refuse(`holder has ${n} weight checkpoints; not checked at this scale`);
      history.weights[holder] = [];
      for (let i = 0; i < n; i++) history.weights[holder].push(await call('reward_weight', [new S.Address(holder).toScVal(), S.nativeToScVal(i, {type: 'u32'})], `reward_weight(${holder}, ${i})`));
    }
  }
  ledgers.push((await rpc.getLatestLedger()).sequence);
  if (!ledgers.every(l => l === start.sequence)) refuse('ledger changed during the reads; run the check again', {ledgers});
  const normal = v => JSON.parse(json(v));
  return {...evaluate(proposal, normal(interval), normal(segments), normal(history)), ledger: start.sequence, closeTime: Number(start.closeTime), vault: config.vault,
    proposalSha256: crypto.createHash('sha256').update(JSON.stringify(proposal)).digest('hex'),
    validity: 'Valid for this ledger and this exact file. Run it again immediately before EVERY signature (each propose and each attribute).'};
}
module.exports = {evaluate, evaluateDirect, check, Refusal, ABI_GAPS, FINALITY, VAULT_WASM_SHA256, MAX_SEGMENTS};
if (require.main === module) (async () => {
  const [configFile, proposalFile, output, ...extra] = process.argv.slice(2);
  assert(configFile && proposalFile && !extra.length, 'usage: node reward-attribution-check.cjs <public-readonly-config.json> <attribution-proposal.json> [new-report.json]');
  if (output) assert(!fs.existsSync(output), 'preserve previous reports');
  const result = await check(JSON.parse(fs.readFileSync(configFile)), JSON.parse(fs.readFileSync(proposalFile)));
  if (output) fs.writeFileSync(output, json(result), {flag: 'wx'});
  process.stdout.write(json(result));
  process.stderr.write('CHECK PASSED for this ledger and this exact file. ' + FINALITY + '\n');
})().catch(error => {
  process.stdout.write(json({status: 'REFUSED_DO_NOT_SIGN', reason: error.message, found: error.found ?? null, abiGaps: ABI_GAPS, signing: false, submission: false}));
  process.stderr.write('REFUSED - do not sign any attribution: ' + error.message + '\n'); process.exitCode = 1;
});
