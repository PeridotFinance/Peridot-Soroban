'use strict';
const J = require('./support/pre-activation-journal.cjs');
// Offline tests for operator-deployment-draft-v3_1.cjs (FULL_RELEASE generator for step 20). No RPC, key or signer.
const test = require('node:test'), assert = require('node:assert/strict'), fs = require('node:fs'), os = require('node:os'), path = require('node:path'), crypto = require('node:crypto');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const G = require('../operator-deployment-draft-v3_1.cjs'), D = require('../operator-deployment-v3.cjs');
const ROOT = path.resolve(__dirname, '../../..'), REL = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/';
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const read = f => JSON.parse(fs.readFileSync(path.join(ROOT, f)));
const FULL = REL + 'activate/' + fs.readdirSync(path.join(ROOT, REL, 'activate')).filter(f => /^deploy-config-full-release-.*\.UNREVIEWED\.json$/.test(f)).sort()[0];
const plan = read(G.PLAN), generatedNow = c => Math.floor(Date.parse(c.generatedAt) / 1000) + 1;
const liveFromCapture = () => Object.fromEntries(Object.entries(G.capture().settings));

test('helper and confirmed chain are untouched: helper hash pinned, predecessor = the config that confirmed steps 1-19', () => {
  assert.equal(sha(fs.readFileSync(path.join(__dirname, '../operator-deployment-v3.cjs'))), '325a72475193ac059c229848b11bd6b5debbc3e6490eee31f520972ee792a4f7');
  assert.equal(sha(fs.readFileSync(path.join(ROOT, G.CONFIRMED_CONFIG))), '6de7ef1c54301428bc8abf917d78da3e378d7f256c21b0f3611983be673ae008');
});
test('evidence pins: the two helper-checked pins plus further evidence; verdicts, plan hash and frozen pair are asserted', () => {
  const pins = G.evidencePins();
  assert.deepEqual(Object.keys(pins), ['independentReview', 'signedCore', 'independentReviewNotes', 'signedCoreSums', 'signedRunApproval', 'earlierReview']);
  for (const p of Object.values(pins)) assert.equal(sha(fs.readFileSync(path.join(ROOT, p.path))), p.sha256);
  // A tampered copy with another verdict is refused.
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'v31-ev-'));
  try {
    for (const e of Object.values(G.EVIDENCE)) {fs.mkdirSync(path.dirname(path.join(dir, e.path)), {recursive: true}); fs.copyFileSync(path.join(ROOT, e.path), path.join(dir, e.path));}
    G.evidencePins(dir);
    const f = path.join(dir, G.EVIDENCE.independentReview.path), j = JSON.parse(fs.readFileSync(f)); j.verdict = 'NO'; fs.writeFileSync(f, JSON.stringify(j));
    assert.throws(() => G.evidencePins(dir), /YES WITH CONDITIONS|deepStrictEqual|strictEqual|Expected/);
  } finally {fs.rmSync(dir, {recursive: true, force: true});}
  assert.equal(G.RATIFICATION.date, '2026-11-15'); assert.match(G.RATIFICATION.text, /User ratified both add-on exceptions/);
});
test('network settings: identical to the 13 Nov capture passes; any limit/fee change or cost-model change refuses; the state-size sample is ignored', () => {
  const cap = G.capture(); assert.equal(cap.ledger, 65455793);
  const ok = G.compareSettings(liveFromCapture()); assert.equal(ok.txMaxInstructions, '400000000'); assert(ok.rows.every(r => r.identicalTo20261113));
  // Sample-only window: a different value is recorded but not refused.
  const w = liveFromCapture(), win = S.xdr.LedgerEntryData.fromXDR(w.configSettingLiveSorobanStateSizeWindow, 'base64');
  win.configSetting().liveSorobanStateSizeWindow().push(S.xdr.Uint64.fromString('1')); w.configSettingLiveSorobanStateSizeWindow = win.toXDR('base64');
  assert.equal(G.compareSettings(w).rows.find(r => r.name === 'configSettingLiveSorobanStateSizeWindow').identicalTo20261113, false);
  // A fee change refuses.
  const fee = liveFromCapture(), led = S.xdr.LedgerEntryData.fromXDR(fee.configSettingContractEventsV0, 'base64');
  led.configSetting().contractEvents().feeContractEvents1Kb(S.xdr.Int64.fromString('2501')); fee.configSettingContractEventsV0 = led.toXDR('base64');
  assert.throws(() => G.compareSettings(fee), /differ from the 13 Nov capture: configSettingContractEventsV0/);
  // A cost-model change refuses by name.
  const cm = liveFromCapture(), c = S.xdr.LedgerEntryData.fromXDR(cm.configSettingContractCostParamsCpuInstructions, 'base64');
  const e0 = c.configSetting().contractCostParamsCpuInsns()[0]; e0.constTerm(S.xdr.Int64.fromString(String(BigInt(e0.constTerm().toString()) + 1n)));
  cm.configSettingContractCostParamsCpuInstructions = c.toXDR('base64');
  assert.throws(() => G.compareSettings(cm), /live CPU cost model differs from the 13 Nov capture/);
  const missing = liveFromCapture(); delete missing.configSettingContractComputeV0; assert.throws(() => G.compareSettings(missing), /not read/);
});
test('the generated FULL config: stage, approval pins, ratification text, cap 8,000,000,000, keeper, chained to the real confirmed journal (read-only)', t => {
  const config = read(FULL); assert.equal(config.status, G.FULL_PENDING_STATUS); assert.equal(config.releaseStage, D.STAGES.FULL);
  assert.equal(config.releaseApproval.status, 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE'); assert.deepEqual(config.releaseApproval.userRatification, {...G.RATIFICATION});
  assert.deepEqual(config.costModelXdrSha256, {...G.COST_MODEL_20261113}); assert.equal(config.keeperPublicAddress, 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4');
  assert.deepEqual(config.predecessorConfig, {path: G.CONFIRMED_CONFIG, sha256: '6de7ef1c54301428bc8abf917d78da3e378d7f256c21b0f3611983be673ae008'});
  const probe = G.validateFull(plan, config, generatedNow(config));
  // 16 Nov: the real journal has since been continued by step 20. The generator's check runs on the derived
  // pre-activation journal (19 steps, cross-checked with the operator's backup); on the continued journal it refuses.
  assert.deepEqual(G.chainCheck(plan, probe, J.writePreActivation(t)), {entries: 19, lastSequence: '246894391124295849', segments: 1});
  assert.throws(() => G.chainCheck(plan, probe, G.CONFIRMED_JOURNAL));
  const live = J.live(); assert.deepEqual(live.segments.map(s => s.releaseStage), [D.STAGES.PRE, D.STAGES.FULL]);
  assert.equal(live.entries.length, 20); assert(live.entries.every(r => r.status === 'CONFIRMED')); assert.equal(live.entries[19].hash, J.STEP20);
  const after = config.readbacks['activate-receipt'].after.find(r => r.method === 'stage_config'); assert.equal(after.expectedNative.backing_cap, '8000000000');
  // Unreviewed status is refused by the helper itself; the in-memory probe is the only way it validates here.
  assert.throws(() => D.validateCeremony(plan, config, generatedNow(config)), /REVIEWED_UNSIGNED_CEREMONY|strictEqual|Expected/);
  for (const bad of [c => c.keeperPublicAddress = 'GBHWIIIGBBEPRJSJIDSPWZE5USLY7TPUTHROK3VZ3ZTJ2GEKTBOPZON4', c => c.releaseStage = D.STAGES.PRE,
    c => c.costModelXdrSha256 = {cpu: 'a'.repeat(64), memory: c.costModelXdrSha256.memory}, c => c.releaseApproval.userRatification = {...c.releaseApproval.userRatification, text: 'x'},
    c => c.releaseApproval.signedCore = {...c.releaseApproval.signedCore, sha256: '0'.repeat(64)}]) {
    const c = structuredClone(config); bad(c); assert.throws(() => G.validateFull(plan, c, generatedNow(config)));
  }
  assert.throws(() => G.validateFull(plan, config, config.expiresAt + 1), /expired/);
});
test('mark-reviewed (operator) writes a NEW reviewed file the unchanged helper accepts; it refuses anything already reviewed', t => {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'v31-mark-'))); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  const config = read(FULL), file = path.join(dir, 'x.UNREVIEWED.json'); fs.writeFileSync(file, JSON.stringify(config));
  // 16 Nov: the real journal is continued (step 20 confirmed), so mark-reviewed now refuses: the review is done.
  assert.throws(() => G.markReviewed(file, generatedNow(config)), /belongs to another config|not yet continued|19-step/);
  assert(!fs.existsSync(file.replace(/\.UNREVIEWED\.json$/, '.json')), 'a refused mark-reviewed writes nothing');
  // What mark-reviewed checked and produced, on the derived pre-activation journal: the helper accepts the reviewed file.
  G.validateFull(plan, config, generatedNow(config)); G.chainCheck(plan, {...config, status: 'REVIEWED_UNSIGNED_CEREMONY'}, J.writePreActivation(t));
  const reviewed = {...config, status: 'REVIEWED_UNSIGNED_CEREMONY'}; D.validateCeremony(plan, reviewed, generatedNow(config));
  // The operator's actual reviewed FULL config is the one the real journal's segment 1 belongs to.
  const actual = read(FULL.replace(/\.UNREVIEWED\.json$/, '.json')); assert.equal(actual.status, 'REVIEWED_UNSIGNED_CEREMONY');
  assert.equal(actual.operatorReview.unreviewedSha256, sha(fs.readFileSync(path.resolve(ROOT, FULL))));
});
test('97M host stop runs inside the adapter (before the helper journals); AT-LIMIT label from 98M', async () => {
  assert.equal(G.cpuLabel(96999999), 'WITHIN_POLICY'); assert.equal(G.cpuLabel(97000001), 'ABOVE_97M_STOP'); assert.equal(G.cpuLabel(98000000), 'AT-LIMIT');
  const config = read(FULL), op = D.operation(plan, config, 'activate-receipt');
  for (const [cpu, ok] of [[8880945, true], [97000000, true], [97000001, false], [98500000, false]]) {
    const data = new S.SorobanDataBuilder().setResources(cpu, 0, 1).setResourceFee('1').build();
    const tx = new S.TransactionBuilder(new S.Account(plan.admin, '1'), {fee: '1000', networkPassphrase: plan.network}).addOperation(op).setSorobanData(data).setTimeout(0).build();
    const deps = G.stopAbove97M({prepare: async () => ({unsignedXdr: tx.toXDR()})}, plan);
    if (ok) await deps.prepare(); else await assert.rejects(deps.prepare(), new RegExp(`REFUSED \\((${G.cpuLabel(cpu)})\\).*nothing journaled`));
  }
});
test('read-only retry: crossed ledger immediately, 429 with backoff, other errors once', async () => {
  const waits = []; let n = 0;
  assert.equal(await G.retry(async () => {if (++n < 3) throw new Error(n === 1 ? 'ledger changed; no unsigned transaction prepared' : 'Request failed with status code 429'); return 1;}, {wait: async ms => waits.push(ms)}), 1);
  assert.deepEqual(waits, [250, 2000]); n = 0;
  await assert.rejects(G.retry(async () => {n++; throw new Error('source sequence changed; stop ceremony');}, {wait: async () => {}}), /sequence changed/); assert.equal(n, 1);
});
test('step-20 simulation evidence file matches the generated config and stays within policy', () => {
  const sims = fs.readdirSync(path.join(ROOT, REL, 'activate')).filter(f => /^step20-simulation-/.test(f)).sort(), sim = read(REL + 'activate/' + sims[0]);
  assert.equal(sim.signing, false); assert.equal(sim.submission, false); assert.equal(sim.configSha256, sha(fs.readFileSync(path.join(ROOT, FULL))));
  assert(sim.step20.declaredInstructionsFromRpcHostPreflight <= 97000000); assert.equal(sim.step20.label, 'WITHIN_POLICY');
  assert.equal(sim.step20.unsignedXdr, undefined, 'no signable XDR is saved');
});
