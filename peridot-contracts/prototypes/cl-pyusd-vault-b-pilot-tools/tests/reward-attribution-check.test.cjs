'use strict';
// reward-attribution-check.cjs. Offline; figures are those of the reward coverage probe (REPORT.md section 5).
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), os = require('node:os'), path = require('node:path'), cp = require('node:child_process');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const A = require('../reward-attribution-check.cjs');
const VAULT = S.StrKey.encodeContract(Buffer.alloc(32, 2)), ADMIN = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 1));
const E1 = 'a1'.repeat(32), E2 = 'b2'.repeat(32);
const interval = (over = {}) => ({sealed: true, first_segment: '0', segments: 2, resolved: 0, expected: '0', received: '1343780', remaining_scaled: '1343780000000000000000000',
  original_scaled: '0', redistribution_scaled: '0', waivers: 0, last_reading: null, last_verified: false, direct_governed: false, ...over});
const segment = (over = {}) => ({interval: '0', expected: null, governed: false, evidence: null, supply: '1000', owned_supply: '1000', waived_weight: '0', waiver_cursor: 0, prepared: false, history_complete: true, revision: '2', ...over});
const proposal = (a = '942023', b = '401757', over = {}) => ({vault: VAULT, interval: '0', route: 'segment',
  segments: [{id: '0', amountRaw: a, evidenceSha256: E1}, {id: '1', amountRaw: b, evidenceSha256: E2}], ...over});
const segs = (a = segment(), b = segment()) => ({'0': a, '1': b});
const refused = (pattern, check) => error => {assert(error instanceof A.Refusal, 'expected Refusal: ' + error.message); assert.match(error.message, pattern); if (check) check(error.found); return true;};
test('amounts that sum exactly to the amount received pass and list exactly what is to be signed', () => {
  const r = A.evaluate(proposal(), interval(), segs());
  assert.equal(r.status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED'); assert.equal(r.receivedRaw, '1343780'); assert.equal(r.proposedTotalRaw, '1343780');
  assert.deepEqual(r.toSign.map(x => x.first), [{method: 'propose_reward_segment', args: ['0', '942023', E1]}, {method: 'propose_reward_segment', args: ['1', '401757', E2]}]);
  assert.deepEqual(r.toSign.map(x => x.after86400s.method), ['attribute_reward_segment', 'attribute_reward_segment']);
  assert.equal(r.signing, false); assert.equal(r.submission, false); assert.equal(r.abiGaps.length, 4); assert.match(r.abiGaps[0], /Pending proposal not readable/);
  assert.match(r.finality, /^ONE-SHOT AND FINAL: .*cannot be changed, withdrawn or topped up/); assert.deepEqual(r.rules, {everySegmentListed: true, sumEqualsReceived: true, zeroWeightSegmentsGetZero: true, prepareBeforeSettlement: true});
  // Rule (v): prepare_reward_segment is planned for every segment, then settlement, then the holder's claim.
  assert.deepEqual(r.afterAttribution.slice(0, 2), [{step: 'prepare', method: 'prepare_reward_segment', args: ['0', 4], caller: 'anyone', repeatUntil: 'returns true', alreadyPrepared: false},
    {step: 'prepare', method: 'prepare_reward_segment', args: ['1', 4], caller: 'anyone', repeatUntil: 'returns true', alreadyPrepared: false}]);
  assert.deepEqual(r.afterAttribution.slice(2).map(x => x.method), ['settle_reward_segment', 'claim_rewards']);
  // Any split that sums to the amount received passes: the split itself is the admin's decision, the total is not.
  assert.equal(A.evaluate(proposal('1343780', '0'), interval(), segs()).status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED');
  assert.equal(A.evaluate(proposal('0', '0'), interval({received: '0'}), segs()).status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED');
});
test('NEGATIVE a total above or below the amount received is refused, by one unit as well as by the probe amounts', () => {
  assert.throws(() => A.evaluate(proposal('2826069', '401757'), interval(), segs()), refused(/does not equal the amount received 1343780: OVER by 1884046\. .*shortfall would also fall on the holder who stayed.* Do not sign/, found => {
    assert.equal(found.direction, 'OVER'); assert.equal(found.proposedTotalRaw, '3227826'); assert.equal(found.differenceRaw, '1884046');}));
  assert.throws(() => A.evaluate(proposal('471011', '200878'), interval(), segs()), refused(/UNDER by 671891\. .*no entrypoint to release it/, found => assert.equal(found.direction, 'UNDER')));
  assert.throws(() => A.evaluate(proposal('942024', '401757'), interval(), segs()), refused(/OVER by 1\./));
  assert.throws(() => A.evaluate(proposal('942023', '401756'), interval(), segs()), refused(/UNDER by 1\./));
  assert.throws(() => A.evaluate(proposal('942023', '401757'), interval({received: '1343781'}), segs()), refused(/UNDER by 1\./));
});
test('NEGATIVE inexact inputs are refused: decimals, numbers, signs, leading zeros, missing or zero evidence', () => {
  for (const amount of ['942023.0', 942023, '0942023', '-1', '', ' 942023', '9.42023e5', null, undefined, '942,023'])
    {const p = proposal(); p.segments[0].amountRaw = amount; assert.throws(() => A.evaluate(p, interval(), segs()), refused(/exact raw unsigned integer string/));}
  for (const evidence of ['0'.repeat(64), undefined, 'A1'.repeat(32), 'a1'.repeat(31), 12]) {
    const p = proposal(); p.segments[0].evidenceSha256 = evidence; assert.throws(() => A.evaluate(p, interval(), segs()), refused(/non-zero 32-byte evidence digest/));
  }
  assert.throws(() => A.evaluate(proposal('942023', '401757', {interval: 0}), interval(), segs()), refused(/exact raw unsigned integer string/));
});
test('NEGATIVE the file must cover exactly the segments of the sealed interval', () => {
  const p = proposal();
  assert.throws(() => A.evaluate({...p, segments: [p.segments[0]]}, interval(), segs()), refused(/every segment of the interval must be listed; missing: 1/));
  assert.throws(() => A.evaluate({...p, segments: [...p.segments, {id: '2', amountRaw: '0', evidenceSha256: E1}]}, interval(), segs()), refused(/segment 2 does not belong to interval 0/));
  assert.throws(() => A.evaluate({...p, segments: [p.segments[0], p.segments[0]]}, interval(), segs()), refused(/listed twice/));
  assert.throws(() => A.evaluate({...p, segments: 'all'}, interval(), segs()), refused(/must list every segment/));
  assert.throws(() => A.evaluate(p, interval(), segs(segment(), segment({interval: '1'}))), refused(/belongs to interval 1, not 0/));
  assert.throws(() => A.evaluate(p, interval(), {'0': segment()}), refused(/reward_segment\(1\) does not exist/));
  assert.throws(() => A.evaluate(p, interval({segments: 0}), {}), refused(/has 0 segments/));
  assert.throws(() => A.evaluate(p, interval({segments: 65}), {}), refused(/has 65 segments/));
  // Segment ids follow first_segment, not zero.
  const later = {vault: VAULT, interval: '3', route: 'segment', segments: [{id: '7', amountRaw: '10', evidenceSha256: E1}, {id: '8', amountRaw: '5', evidenceSha256: E2}]};
  assert.equal(A.evaluate(later, interval({first_segment: '7', received: '15'}), {'7': segment({interval: '3'}), '8': segment({interval: '3'})}).status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED');
  assert.throws(() => A.evaluate(proposal('10', '5', {interval: '3'}), interval({first_segment: '7', received: '15'}), {}), refused(/segment 0 does not belong to interval 3/));
});
test('NEGATIVE unsealed, missing or directly governed intervals and the direct route are refused', () => {
  assert.throws(() => A.evaluate(proposal(), interval({sealed: false}), segs()), refused(/is not sealed/));
  assert.throws(() => A.evaluate(proposal(), null, segs()), refused(/reward_interval\(0\) does not exist/));
  assert.throws(() => A.evaluate(proposal(), interval({direct_governed: true}), segs()), refused(/already paid through the direct route/));
  for (const route of ['attribute_rewards', undefined, 'both', 'Segment'])
    assert.throws(() => A.evaluate(proposal('942023', '401757', {route}), interval(), segs()), refused(/route must be 'segment' .* or 'direct'/));
});
test('between the two signatures: already attributed segments must keep their on-chain amount and the total must still equal received', () => {
  const half = interval({resolved: 1, expected: '942023'}), on = segs(segment({expected: '942023', governed: true, evidence: E1}), segment());
  const r = A.evaluate(proposal(), half, on);
  assert.equal(r.alreadyAttributedRaw, '942023'); assert.deepEqual(r.toSign.map(x => x.segment), ['1']); assert.equal(r.segments[0].status, 'ALREADY_ATTRIBUTED_ON_CHAIN');
  assert.throws(() => A.evaluate(proposal('942000', '401780'), half, on), refused(/segment 0 is already attributed 942023 on chain; the file says 942000/));
  assert.throws(() => A.evaluate(proposal('942023', '401758'), half, on), refused(/OVER by 1\./));
  assert.throws(() => A.evaluate(proposal(), interval({resolved: 2, expected: '942023'}), on), refused(/interval and segment records disagree/));
  assert.throws(() => A.evaluate(proposal(), interval({resolved: 1, expected: '942024'}), on), refused(/interval and segment records disagree/));
  const done = A.evaluate(proposal(), interval({resolved: 2, expected: '1343780'}), segs(segment({expected: '942023'}), segment({expected: '401757'})));
  assert.deepEqual(done.toSign, []); assert.equal(done.status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED');
});
function fakeRpc(options = {}) {
  const sym = o => S.xdr.ScVal.scvMap(Object.entries(o).map(([k, v]) => new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol(k), val: v})));
  const u128 = v => S.nativeToScVal(BigInt(v), {type: 'u128'}), u64 = v => S.nativeToScVal(BigInt(v), {type: 'u64'}), u32 = v => S.nativeToScVal(v, {type: 'u32'}), bool = v => S.xdr.ScVal.scvBool(v), none = S.xdr.ScVal.scvVoid();
  const calls = []; let latest = 0;
  return {calls, getNetwork: async () => ({passphrase: S.Networks.PUBLIC}), getLatestLedger: async () => ({sequence: options.ledgerMoves && ++latest > 1 ? 101 : 100, closeTime: '1000'}),
    getLedgerEntries: async () => ({latestLedger: 100, entries: options.absent ? [] : [{val: {contractData: () => ({val: () => ({instance: () => ({executable: () =>
      S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(options.otherCode ? '9'.repeat(64) : A.VAULT_WASM_SHA256, 'hex'))})})})}}]}),
    getAccount: async id => new S.Account(id, '7'),
    simulateTransaction: async tx => {
      assert.equal(tx.signatures.length, 0); const fn = tx.operations[0].func.invokeContract(), method = fn.functionName().toString(), id = String(S.scValToNative(fn.args()[0])); calls.push(method + ':' + id);
      const wrap = retval => ({_parsed: true, latestLedger: 100, transactionData: new S.SorobanDataBuilder(), result: {auth: [], retval}});
      if (options.direct && method === 'reward_boundary') return wrap(Number(id) < options.direct.boundaries ? sym({holder: new S.Address(options.direct.holders[Number(id)] ?? options.direct.holders[0]).toScVal(), shares: u128(400), interval: u64(0), waived: bool(false)}) : none);
      if (options.direct && method === 'reward_history_count') return wrap(u32(1));
      if (options.direct && method === 'reward_weight') return wrap(sym({revision: u64(1), weight: u128(options.direct.weight)}));
      assert(['reward_interval', 'reward_segment'].includes(method), 'only the two frozen-ABI views may be read');
      let retval = method === 'reward_interval'
        ? sym({sealed: bool(options.sealed ?? true), first_segment: u64(0), segments: u32(2), resolved: u32(0), expected: u128(0), received: u128(options.received ?? 1343780), remaining_scaled: u128(options.direct ? 1343780n * 10n ** 18n : 0), original_scaled: u128(0), redistribution_scaled: u128(0), direct_governed: bool(false), waivers: u32(0)})
        : sym({interval: u64(0), expected: none, governed: bool(false), supply: u128(1000), owned_supply: u128(1000), waived_weight: u128(0), prepared: bool(false), history_complete: bool(true), revision: u64(2)});
      const result = {_parsed: true, latestLedger: 100, transactionData: new S.SorobanDataBuilder(), result: {auth: [], retval}};
      if (options.archived && method === 'reward_segment') result.restorePreamble = {};
      return result;
    }};
}
const config = {readOnly: true, network: S.Networks.PUBLIC, rpcUrl: 'https://rpc.invalid', admin: ADMIN, vault: VAULT};
test('fake RPC: one coherent ledger, frozen vault code, exactly the two views; result is bound to ledger and file', async () => {
  const rpc = fakeRpc(), r = await A.check(config, proposal(), rpc);
  assert.equal(r.status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED'); assert.equal(r.ledger, 100); assert.match(r.proposalSha256, /^[0-9a-f]{64}$/); assert.match(r.validity, /before EVERY signature/);
  assert.deepEqual(rpc.calls, ['reward_interval:0', 'reward_segment:0', 'reward_segment:1']);
  await assert.rejects(A.check(config, proposal('942023', '401756'), fakeRpc()), refused(/UNDER by 1\./));
  await assert.rejects(A.check(config, proposal(), fakeRpc({received: 1343779})), refused(/OVER by 1\./));
});
test('NEGATIVE fake RPC: other vault, other code, unsealed, archived record, moving ledger and direct route all refuse', async () => {
  await assert.rejects(A.check(config, {...proposal(), vault: S.StrKey.encodeContract(Buffer.alloc(32, 3))}, fakeRpc()), refused(/another vault/));
  await assert.rejects(A.check(config, proposal(), fakeRpc({otherCode: true})), /vault is not the frozen candidate/);
  await assert.rejects(A.check(config, proposal(), fakeRpc({absent: true})), refused(/not found or archived/));
  await assert.rejects(A.check(config, proposal(), fakeRpc({sealed: false})), refused(/is not sealed/));
  await assert.rejects(A.check(config, proposal(), fakeRpc({archived: true})), refused(/never treated as empty/));
  await assert.rejects(A.check(config, proposal(), fakeRpc({ledgerMoves: true})), refused(/ledger changed during the reads/));
  const unknown = fakeRpc(); await assert.rejects(A.check(config, {...proposal(), route: 'both'}, unknown), refused(/route must be/)); assert.deepEqual(unknown.calls, []);
  await assert.rejects(A.check({...config, readOnly: false}, proposal(), fakeRpc())); await assert.rejects(A.check({...config, network: S.Networks.TESTNET}, proposal(), fakeRpc()));
});
test('NEGATIVE command line exits non-zero with a clear message when it refuses (no network is reached)', t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'attribution-check-')); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const c = path.join(dir, 'config.json'), p = path.join(dir, 'proposal.json'), out = path.join(dir, 'report.json');
  fs.writeFileSync(c, JSON.stringify(config)); fs.writeFileSync(p, JSON.stringify({...proposal(), route: 'both'}));
  const run = cp.spawnSync(process.execPath, [path.join(__dirname, '../reward-attribution-check.cjs'), c, p, out], {encoding: 'utf8', timeout: 30000, env: {PATH: '/usr/bin:/bin'}});
  assert.equal(run.status, 1); assert.match(run.stderr, /REFUSED - do not sign any attribution: route must be/);
  const printed = JSON.parse(run.stdout); assert.equal(printed.status, 'REFUSED_DO_NOT_SIGN'); assert.equal(printed.abiGaps.length, 4); assert.equal(fs.existsSync(out), false);
  const usage = cp.spawnSync(process.execPath, [path.join(__dirname, '../reward-attribution-check.cjs')], {encoding: 'utf8', timeout: 30000, env: {PATH: '/usr/bin:/bin'}});
  assert.equal(usage.status, 1); assert.match(usage.stderr, /usage: node reward-attribution-check\.cjs/);
});

test('NEGATIVE rule (iii): any amount on a segment with zero eligible weight is refused; exactly 0 there passes', () => {
  for (const dead of [{owned_supply: '1000', waived_weight: '1000', prepared: true}, {supply: '0', owned_supply: '0'}, {owned_supply: '0'}, {history_complete: false}]) {
    const on = segs(segment(), segment(dead));
    assert.throws(() => A.evaluate(proposal(), interval(), on), refused(/segment 1 has zero eligible weight .*exactly 0 must be attributed to it, the file says 401757\. Any other amount can never be settled\. Do not sign/));
    const r = A.evaluate(proposal('1343780', '0'), interval(), on); assert.equal(r.segments[1].zeroEligibleWeight, true); assert.equal(r.segments[1].amountRaw, '0'); assert.equal(r.segments[0].zeroEligibleWeight, false);
  }
  // Every segment dead and something received: no file can satisfy rules (ii) and (iii) together, so nothing is signable.
  const allDead = segs(segment({owned_supply: '0'}), segment({owned_supply: '0'}));
  assert.throws(() => A.evaluate(proposal('0', '0'), interval(), allDead), refused(/UNDER by 1343780/)); assert.throws(() => A.evaluate(proposal(), interval(), allDead), refused(/zero eligible weight/));
});
test('NEGATIVE rule (v): with waivers in the interval nothing is signable until prepare_reward_segment completed for every segment', () => {
  const waivers = interval({waivers: 1});
  assert.throws(() => A.evaluate(proposal(), waivers, segs()), refused(/segment 0: this interval contains 1 waiver\(s\) and prepare_reward_segment has not completed .*prepare_reward_segment\(0, 4\) until it returns true/));
  assert.throws(() => A.evaluate(proposal(), waivers, segs(segment({prepared: true}), segment())), refused(/segment 1: this interval contains 1 waiver/));
  // Prepared: the final waived weight decides. A sole holder who waived leaves zero eligible weight in that segment.
  const prepared = segs(segment({prepared: true, waived_weight: '1000'}), segment({prepared: true}));
  assert.throws(() => A.evaluate(proposal(), waivers, prepared), refused(/segment 0 has zero eligible weight \(supply 1000, owned 1000, waived 1000/));
  const r = A.evaluate(proposal('0', '1343780'), waivers, prepared); assert.equal(r.afterAttribution[0].alreadyPrepared, true); assert.equal(r.status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED');
});
const H = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 8)), OTHER = S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, 9));
const boundary = (id, over = {}) => ({id: String(id), holder: H, shares: '400', interval: '0', waived: false, proven: false, ...over});
const direct = (rows = [{id: '0', amountRaw: '1343780', evidenceSha256: E1}], over = {}) => ({vault: VAULT, interval: '0', route: 'direct', boundaries: rows, ...over});
const sole = (over = {}) => ({boundaries: [boundary(0)], weights: {[H]: [{revision: '1', weight: '1000'}]}, ...over});
const full = (over = {}) => interval({remaining_scaled: '1343780000000000000000000', ...over});
test('rule (iv): the per-boundary route passes only for a proven sole owner, and then the total must equal what the interval still holds', () => {
  const r = A.evaluate(direct(), full(), segs(), sole());
  assert.equal(r.status, 'ATTRIBUTION_SUM_EQUALS_RECEIVED'); assert.equal(r.route, 'direct'); assert.equal(r.soleOwner, H); assert.match(r.finality, /^ONE-SHOT AND FINAL/);
  assert.deepEqual(r.toSign[0].first, {method: 'propose_reward_attribution', args: ['0', '1343780', E1]}); assert.equal(r.toSign[0].after86400s.method, 'attribute_rewards');
  for (const amount of ['1343779', '1343781']) assert.throws(() => A.evaluate(direct([{id: '0', amountRaw: amount, evidenceSha256: E1}]), full(), segs(), sole()), refused(/does not equal the amount still held for the interval 1343780/));
  // Two boundaries of the same sole owner, one already attributed earlier: only the remainder is signable.
  const two = sole({boundaries: [boundary(0), boundary(1)]}), part = full({direct_governed: true, remaining_scaled: '343780000000000000000000'});
  assert.equal(A.evaluate(direct([{id: '1', amountRaw: '343780', evidenceSha256: E2}]), part, segs(), two).remainingRaw, '343780');
  assert.throws(() => A.evaluate(direct([{id: '1', amountRaw: '1343780', evidenceSha256: E2}]), part, segs(), two), refused(/OVER by 1000000/));
});
test('NEGATIVE rule (iv): attribute_rewards is refused whenever the boundary holder was not the interval only owner', () => {
  const no = (history, i = full(), sg = segs(), p = direct()) => assert.throws(() => A.evaluate(p, i, sg, history), refused(/per-boundary route \(attribute_rewards\) refused: .*Use the segment route\. Do not sign|could not be read/));
  no(sole({weights: {[H]: [{revision: '1', weight: '667'}]}}));                       // another owner held the rest (the probe: A two thirds, B one third)
  assert.throws(() => A.evaluate(direct(), full(), segs(), sole({weights: {[H]: [{revision: '1', weight: '667'}]}})), refused(/was NOT the interval's only owner: at segment 0 the holder's recorded weight is 667 of owned supply 1000; the other owners would receive nothing/));
  no(sole({weights: {[H]: [{revision: '1', weight: '1000'}, {revision: '2', weight: '600'}]}}));   // weight fell before the segment snapshot (revision 2)
  no(sole({weights: {[H]: [{revision: '3', weight: '1000'}]}}));                    // held nothing yet at the snapshot
  no(sole({boundaries: [boundary(0), boundary(1, {holder: OTHER})], weights: {[H]: [{revision: '1', weight: '1000'}], [OTHER]: []}})); // two holders exited
  no(sole({boundaries: [boundary(0, {waived: true})]})); no(sole(), full({waivers: 1}));
  no(sole(), full({resolved: 1, expected: '1'})); no(sole(), full({original_scaled: '5'}));
  no(sole({boundaries: [boundary(0, {interval: '1'})]})); no(sole({boundaries: []})); no(null); no({boundaries: [boundary(0)], weights: {}});
  no(sole(), full(), segs(segment(), segment({history_complete: false}))); no(sole(), full(), segs(segment({owned_supply: '0'}), segment()));
  no(sole(), full({remaining_scaled: '1343780000000000000000001'})); no(sole(), full({remaining_scaled: '1343779000000000000000000'}));
  no(sole(), full(), segs(), direct([{id: '5', amountRaw: '1343780', evidenceSha256: E1}])); no(sole({boundaries: [boundary(0, {shares: '0'})]}));
  for (const rows of [[], [{id: '0', amountRaw: '0', evidenceSha256: E1}], [{id: '0', amountRaw: '1343780', evidenceSha256: '0'.repeat(64)}], [{id: '0', amountRaw: '1', evidenceSha256: E1}, {id: '0', amountRaw: '1343779', evidenceSha256: E1}]])
    assert.throws(() => A.evaluate(direct(rows), full(), segs(), sole()), A.Refusal);
  assert.throws(() => A.evaluate(direct(), full({sealed: false}), segs(), sole()), refused(/not sealed/));
});

test('fake RPC, per-boundary route: boundaries and the holder weight history are read through the frozen views; a second owner refuses', async () => {
  const p = {vault: VAULT, interval: '0', route: 'direct', boundaries: [{id: '0', amountRaw: '1343780', evidenceSha256: E1}]};
  const rpc = fakeRpc({direct: {boundaries: 1, holders: [H], weight: 1000}}), r = await A.check(config, p, rpc);
  assert.equal(r.route, 'direct'); assert.equal(r.soleOwner, H); assert.equal(r.ledger, 100);
  assert(rpc.calls.includes('reward_boundary:0') && rpc.calls.includes('reward_history_count:' + H) && rpc.calls.includes('reward_weight:' + H));
  await assert.rejects(A.check(config, p, fakeRpc({direct: {boundaries: 1, holders: [H], weight: 667}})), refused(/was NOT the interval's only owner/));
  await assert.rejects(A.check(config, p, fakeRpc({direct: {boundaries: 2, holders: [H, OTHER], weight: 1000}})), refused(/boundaries of 2 different holders/));
  await assert.rejects(A.check(config, p, fakeRpc({direct: {boundaries: 0, holders: [H], weight: 1000}})), refused(/has no boundary/));
});
