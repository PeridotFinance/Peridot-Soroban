'use strict';
// Latency-injecting end-to-end tests: keeper-loop-v5 + keeper-worker-v5 + keeper-inspect-v5 on the offline world stub
// (support/chain-stub-v5.cjs: virtual clock, seeded RNG, ledgers every 5.5 s +/- 0.6 s, the stub simulates at the latest
// closed ledger and enforces the contract's phase and invest-window rules). v4 runs through the same stub as the control.
// Profiles: TASK = every call 2.7 s median (the figure in the bug report); MEASURED = 17 Nov Alchemy, read-only
// (fast calls ~0.22 s median, getLatestLedger ~6 s, getLedgers ~6.5 s). No keys; the injected "signer" signs nothing.
const test = require('node:test');
const assert = require('node:assert/strict');
const {harness} = require('./support/keeper-v5-harness.cjs');
const START = 1000, BASE = 1794920000, DL = BASE + 3600, SEEDS = [1, 2, 3, 4, 5, 6, 7, 8];
const STEPS = {stage_convert: [0, 1], stage_mint: [0, 2], stage_invest: [0, 3], stage_finalise: [0, 5], stage_withdraw: [1, 10], stage_close_convert: [1, 11]};
const schedule = {pollMs: 1000, maxRunSeconds: 240, pendingTimeoutSeconds: 120, maxAttempts: 1};
async function run(t, version, profile, method, seed) {
  const [kind, phase] = STEPS[method];
  const h = harness(t, {profile, seed, startLedger: START, baseTime: BASE, commands: [{id: 5, kind, phase, deadline: DL}]}, {version});
  let O = START;
  if (method === 'stage_invest') {
    // The session is already running when the third observation lands (the loop warmed the header cache at start).
    if (version === 5) await h.world.clock.drive(h.base.warmNetworkParameters());
    O = h.world.ledgerNow(); h.world.mutate(O, s => { s.guard = {observed_ledger: O, healthy_count: 3, inside: true}; });
  }
  const t0 = h.world.ledgerNow();
  const {result, events} = await h.loop(schedule);
  const submitted = events.find(e => e.status === 'SUBMITTED_RECONCILE_NEXT_INVOCATION');
  const landed = [...h.world.landed.values()];
  return {result, submitted, landed, O, t0, soft: events.filter(e => /READ_AGAIN/.test(e.status)).map(e => e.reason || e.status),
    expired: h.world.stats.expired || 0, journal: h.journal()};
}
const summary = [];
for (const profile of ['MEASURED', 'TASK']) {
  test(`v5 ${profile}: every keeper step is prepared and lands within a few ledgers (8 seeds each); invest only inside [O+1, O+4)`, async t => {
    for (const method of Object.keys(STEPS)) {
      const ledgers = [];
      for (const seed of SEEDS) {
        const r = await run(t, 5, profile, method, seed);
        assert.notEqual(r.result.status, 'SAFETY_REFUSAL_OPERATOR_REVIEW_REQUIRED', `${method} seed ${seed}`);
        assert.equal(r.expired, 0, 'no transaction is ever sent outside its window');
        for (const l of r.landed) assert.equal(l.status, 'SUCCESS', 'every landed keeper transaction succeeds');
        const step = r.landed.find(l => l.method === method);
        if (method === 'stage_invest') {
          if (step) assert(step.ledger >= r.O + 1 && step.ledger <= r.O + 3, 'invest landed outside [O+1, O+3]');
          if (profile === 'MEASURED') assert(step, `invest prepared in the window (seed ${seed})`);
          ledgers.push(step ? step.ledger - r.O : 'missed');
        } else {
          assert(step, `${method} landed (seed ${seed}, ${profile})`);
          ledgers.push(step.ledger - r.t0);
          assert(step.ledger - r.t0 <= (profile === 'MEASURED' ? 5 : 12), `${method} took ${step.ledger - r.t0} ledgers`);
        }
      }
      summary.push(`v5 ${profile.padEnd(8)} ${method.padEnd(20)} landed after ledgers: ${ledgers.join(' ')}`);
    }
  });
}
test('v4 control on the same stub: no command step is prepared within 240 s under either profile (reproduces 17 Nov)', async t => {
  for (const profile of ['MEASURED', 'TASK']) for (const method of ['stage_convert', 'stage_withdraw']) {
    const outcomes = [];
    for (const seed of SEEDS.slice(0, 4)) {
      const r = await run(t, 4, profile, method, seed);
      outcomes.push(r.submitted ? 'SUBMITTED' : `${r.result.status}(${r.soft.length} read-again)`);
      assert.equal(r.submitted, undefined, `v4 unexpectedly prepared ${method}`);
    }
    summary.push(`v4 ${profile.padEnd(8)} ${method.padEnd(20)} ${outcomes.join(' ')}`);
  }
});
// The real PASS -> invest handoff: the loop asks for a streak (OBSERVATION_STREAK_REQUIRED), a fake streak writes three
// consecutive observations into the world and returns PASS ~0.3-1.5 s after the third lands, then the loop invokes the
// worker at once. LIVE is calibrated to the live v5 runs (in-burst simulations ~0.6 s, getEvents ~1.5 s).
async function handoff(t, profile, seed) {
  const h = harness(t, {profile, seed, startLedger: START, baseTime: BASE, commands: [{id: 5, kind: 0, phase: 3, deadline: DL}]});
  let streaks = 0, O = null;
  const streak = async () => { streaks++; const w = h.world, L1 = w.ledgerNow() + 1;
    for (let k = 0; k < 3; k++) w.mutate(L1 + k, s => { s.guard = {observed_ledger: L1 + k, healthy_count: k + 1, inside: true}; });
    while (w.ledgerNow() < L1 + 2) await w.clock.sleep(100);
    await w.clock.sleep(300 + 1200 * w.random()); if (O === null) O = L1 + 2; return 'PASS'; };
  const {events} = await h.loop({pollMs: 1000, maxRunSeconds: 120, pendingTimeoutSeconds: 60, maxAttempts: 1}, {streak});
  const inv = [...h.world.landed.values()].find(l => l.method === 'stage_invest');
  if (h.world.stats.expired) return 'LATE_SUBMISSION';
  if (events.some(e => e.status === 'HALTED_UNKNOWN')) return 'POST_SIGN_HALT';
  if (inv && inv.status === 'SUCCESS' && inv.ledger - O >= 1 && inv.ledger - O <= 3 && streaks === 1) return 'LANDED_FIRST_STREAK_O+' + (inv.ledger - O);
  return inv ? 'LANDED_AFTER_ANOTHER_STREAK' : 'SOFT_MISS_NOT_LANDED';
}
test('v5 PASS -> invest handoff through the loop under latency (40 seeds per profile): never late, never a post-signing halt', async t => {
  for (const profile of ['LIVE', 'MEASURED', 'TASK']) {
    const counts = {};
    for (let seed = 1; seed <= 40; seed++) { const k = await handoff(t, profile, seed); counts[k] = (counts[k] || 0) + 1; }
    summary.push(`v5 ${profile.padEnd(8)} invest via streak handoff  ${JSON.stringify(counts)}`);
    assert(!counts.LATE_SUBMISSION && !counts.POST_SIGN_HALT, profile);
    const first = Object.entries(counts).filter(([k]) => k.startsWith('LANDED_FIRST_STREAK')).reduce((n, [, v]) => n + v, 0);
    if (profile === 'MEASURED') assert.equal(first, 40);
    if (profile === 'LIVE') assert(first >= 36, `LIVE first-streak invest ${first}/40`);
  }
});
test.after(() => { console.log('\n' + summary.join('\n')); });
