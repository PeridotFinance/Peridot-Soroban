// v3.1 (2026-11-15). Generator of the FULL_RELEASE_STEPS_1_TO_20 successor config for step 20 (activate-receipt),
// chained to the CONFIRMED 19-step journal. No signer, no key access, no sendTransaction, never writes the journal.
//
// Builds on operator-deployment-draft-v3.cjs (unchanged; its readLive/build are reused) and on the UNCHANGED
// operator-deployment-v3.cjs helper (sha256 325a7247…92a4f7, pinned by every config in the confirmed chain).
// v3.1 adds:
//  (a) releaseStage FULL_RELEASE_STEPS_1_TO_20 with releaseApproval APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE, whose two
//      helper-checked pins are the independent check (findings.json) and the signed Core results (results.json),
//      plus further pins (NOTES.md, SHA256SUMS, RUN-APPROVAL.json, earlier reward-coverage review). Each pinned file's
//      verdict, plan hash and frozen pair are asserted programmatically. The user's ratification (AGENTS.md, 15 Nov
//      2026) is recorded as TEXT with its date: AGENTS.md is not a file pin.
//  (b) at generation: live Mainnet limit and fee settings are re-read on one ledger and compared byte-for-byte with
//      the 13 Nov capture (network-config-20261113.json, ledger 65455793); the CPU/memory cost model must equal the
//      capture hashes 60af76f6…/7403b2ae…. Any difference refuses (the live state-size window sample is recorded only).
//  (c) the exact step-20 transaction is prepared IN MEMORY through the helper's own read-only adapter (all before
//      read-backs, auth root, one ledger) and validated with the helper's validatePrepared. Only figures are saved,
//      never a signable XDR. Refused if the RPC (host) preflight declares > 97,000,000 instructions; >= 98,000,000 is
//      labelled AT-LIMIT (user decision 15 Nov ~20:15 UTC).
//  (d) the config is written with status TOOL_GENERATED_FULL_RELEASE_PENDING_OPERATOR_REVIEW. The helper accepts only
//      REVIEWED_UNSIGNED_CEREMONY; that status is set by the OPERATOR with `mark-reviewed` after reading the file.
//      Agents never run mark-reviewed.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const D = require('./operator-deployment-v3.cjs');
const G = require('./operator-deployment-draft-v3.cjs');
const ROOT = path.resolve(__dirname, '../..');
const RES = 'research/cl-pyusd-tight-pilot-20261111/';
const PLAN = RES + 'vault-b-pilot-release-20261114/deploy/mainnet-deployment-plan-v3-seed800.json';
const CONFIRMED_JOURNAL = RES + 'vault-b-pilot-release-20261114/deploy/run-seed800-20261115T132829Z/journal';
const CONFIRMED_CONFIG = RES + 'vault-b-pilot-release-20261114/deploy/run-seed800-20261115T132829Z/deploy-config-pre-release-20261115T132827Z-L65492653.json';
const CONFIRMED_CONFIG_SHA256 = '6de7ef1c54301428bc8abf917d78da3e378d7f256c21b0f3611983be673ae008';
const HELPER_SHA256 = '325a72475193ac059c229848b11bd6b5debbc3e6490eee31f520972ee792a4f7';
const CAPTURE = RES + 'vault-b-20261113/network-config-20261113.json';
const CAPTURE_SHA256 = '63c88e402b89621b387514a6dbefe1573fb18d8f4ed31542d4d4b59b0803b5e6';
const COST_MODEL_20261113 = Object.freeze({cpu: '60af76f608f91916b5dce4b3562f465e409a5ce926b6557c07b02432264f62ce',
  memory: '7403b2aeed595c2a5cb0563504d027e9880b8ba86b761c95fe3f01379cd232e4'});
const SAMPLE_ONLY = new Set(['configSettingLiveSorobanStateSizeWindow']);
const KEEPER = G.KEEPER_SUPPLIED_20261115;
const HOST_STOP = 97000000, AT_LIMIT = 98000000;
const FULL_PENDING_STATUS = 'TOOL_GENERATED_FULL_RELEASE_PENDING_OPERATOR_REVIEW';
const RATIFICATION = Object.freeze({date: '2026-11-15', recordedIn: 'AGENTS.md top entry "USER RATIFIED the signed add-on and ACCEPTED the pilot rules — 2026-11-15" (text record; AGENTS.md is not a file pin)',
  text: 'User ratified both add-on exceptions: the 69 re-signed byte-identical setup envelopes and the lead-approved remaining-only continuation. The add-on counts as evidence for the pilot. User accepted in writing: keeper checkpoint rule (checkpoint_rewards right after any emergency exit and before any exit completion; keeper sends every close; declare from host simulation; stop above ~97M); the five attribution rules; unsigned simulation of the exact 800 PYUSD seed against current Mainnet state must pass first; pause right after the seed with foreign-command checks before and after; storage-rent (TTL) keeper for the new contracts.',
  later: 'User decision 15 Nov ~20:15 UTC (via lead): rows >= 98,000,000 host preflight are AT-LIMIT; the 97,000,000 stop stays; keeper signing on its host approved conditional on an exact key-storage description.'});
// Evidence. `checks` run on the parsed JSON; a file that fails them refuses generation.
const EVIDENCE = Object.freeze({
  independentReview: {path: RES + 'vault-b-signed-addon-review-20261115/findings.json', checks: j => {
    assert.equal(j.schema, 'vault-b-signed-addon-independent-review-v1'); assert.equal(j.verdict, 'YES WITH CONDITIONS');
    assert.equal(j.plan_sha256, '09d5fb4dd12e5a77f94f8d97ec3a5545ac1125220da05c33d443346a59771bf8'); assert.equal(j.date, '2026-11-15');
    assert.match(JSON.stringify(j.q1_counts_signatures_prestates), /e6638030/); assert.match(JSON.stringify(j.q1_counts_signatures_prestates), /140dcaab/); }},
  signedCore: {path: RES + 'vault-b-signed-addon-20261114/results.json', checks: j => {
    assert.equal(j.schema, 'cl-vault-b-signed-addon-results-v1'); assert.equal(j.verdict, 'PASS WITH EXCEPTIONS');
    assert.equal(j.all_prestates_frozen, true); assert.match(j.scope, /e6638030/); assert.match(j.scope, /140dcaab/); }},
  independentReviewNotes: {path: RES + 'vault-b-signed-addon-review-20261115/NOTES.md', text: t => assert.match(t, /\*\*Verdict: YES WITH CONDITIONS\.\*\*/)},
  signedCoreSums: {path: RES + 'vault-b-signed-addon-20261114/SHA256SUMS', text: t => assert(t.split('\n').filter(Boolean).length >= 700)},
  signedRunApproval: {path: RES + 'vault-b-signed-addon-20261114/RUN-APPROVAL.json', checks: j => assert.match(JSON.stringify(j), /09d5fb4dd12e5a77/)},
  earlierReview: {path: RES + 'vault-b-reward-coverage-review-20261114/REVIEW-SUMMARY.md', text: t => assert.match(t, /Verdict: YES WITH CONDITIONS/)},
});
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const read = f => JSON.parse(fs.readFileSync(path.resolve(ROOT, f)));
function evidencePins(root = ROOT) {
  const pins = {};
  for (const [name, e] of Object.entries(EVIDENCE)) {
    const bytes = fs.readFileSync(path.resolve(root, e.path));
    if (e.checks) e.checks(JSON.parse(bytes)); else e.text(bytes.toString('utf8'));
    pins[name] = {path: e.path, sha256: sha(bytes), size: bytes.length};
  }
  return pins;
}
// 13 Nov capture: names -> LedgerEntryData XDR (base64). Asserts the capture's own cost-model hashes.
function capture() {
  const bytes = fs.readFileSync(path.resolve(ROOT, CAPTURE)); assert.equal(sha(bytes), CAPTURE_SHA256, '13 Nov capture file changed');
  const c = JSON.parse(bytes), out = {};
  for (const [name, row] of Object.entries(c.settings)) out[name] = row.xdr;
  for (const [role, name] of [['cpu', 'configSettingContractCostParamsCpuInstructions'], ['memory', 'configSettingContractCostParamsMemoryBytes']])
    assert.equal(sha(S.xdr.LedgerEntryData.fromXDR(out[name], 'base64').configSetting().toXDR()), COST_MODEL_20261113[role]);
  return {ledger: c.ledger, settings: out};
}
// live: Map name -> LedgerEntryData base64. Refuses on any difference except the sampled state-size window.
function compareSettings(live, cap = capture()) {
  const rows = [], refusals = [];
  for (const [name, xdr] of Object.entries(cap.settings)) {
    const now = live[name]; assert(now !== undefined, `live setting ${name} not read`);
    const same = now === xdr, decoded = S.xdr.LedgerEntryData.fromXDR(now, 'base64').configSetting();
    rows.push({name, identicalTo20261113: same, sampleOnly: SAMPLE_ONLY.has(name), liveXdrSha256: sha(Buffer.from(now, 'base64'))});
    if (!same && !SAMPLE_ONLY.has(name)) refusals.push(name);
    if (name === 'configSettingContractCostParamsCpuInstructions') assert.equal(sha(decoded.toXDR()), COST_MODEL_20261113.cpu, 'REFUSED: live CPU cost model differs from the 13 Nov capture');
    if (name === 'configSettingContractCostParamsMemoryBytes') assert.equal(sha(decoded.toXDR()), COST_MODEL_20261113.memory, 'REFUSED: live memory cost model differs from the 13 Nov capture');
  }
  assert.equal(refusals.length, 0, `REFUSED: live Mainnet limit/fee settings differ from the 13 Nov capture: ${refusals.join(', ')}; requalify before step 20`);
  const compute = S.xdr.LedgerEntryData.fromXDR(live.configSettingContractComputeV0, 'base64').configSetting().contractCompute();
  assert(compute.txMaxInstructions().toBigInt() >= 100000000n && compute.txMemoryLimit() >= D.LIMITS.memory, 'network limits below the ceremony limits');
  return {captureLedger: cap.ledger, rows, txMaxInstructions: String(compute.txMaxInstructions().toBigInt()), txMemoryLimit: compute.txMemoryLimit()};
}
async function readSettings(rpc, ledger) {
  const names = Object.keys(capture().settings);
  const got = await rpc.getLedgerEntries(...names.map(n => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[n]()}))));
  if (ledger !== undefined) assert.equal(got.latestLedger, ledger, 'settings read crossed a ledger; rerun');
  assert.equal(got.entries.length, names.length);
  return {ledger: got.latestLedger, live: Object.fromEntries(got.entries.map(r => [r.val.configSetting().switch().name, r.val.toXDR('base64')]))};
}
const cpuLabel = cpu => cpu >= AT_LIMIT ? 'AT-LIMIT' : cpu > HOST_STOP ? 'ABOVE_97M_STOP' : 'WITHIN_POLICY';
// Pure: turn a pre-release-style build into the FULL successor (unreviewed).
function fullFrom(base, evidence, settings, predecessor) {
  const config = structuredClone(base);
  config.status = FULL_PENDING_STATUS; config.releaseStage = D.STAGES.FULL;
  config.reviewedBy = 'tool (operator-deployment-draft-v3_1.cjs full-release); NOT operator-reviewed until mark-reviewed is run by the operator';
  config.releaseApproval = {status: 'APPROVED_EXACT_PAIR_AFTER_SIGNED_CORE', wasmSha256: {...D.PAIR},
    independentReview: evidence.independentReview, signedCore: evidence.signedCore,
    furtherEvidence: Object.fromEntries(Object.entries(evidence).filter(([k]) => !['independentReview', 'signedCore'].includes(k))),
    userRatification: RATIFICATION,
    basis: 'Signed add-on PASS WITH EXCEPTIONS; independent check YES WITH CONDITIONS; user ratified both exceptions and accepted the rules (15 Nov 2026).'};
  config.predecessorConfig = predecessor;
  config.networkSettingsCheck = settings;
  config.notes = [...(config.notes || []).filter(n => !/^Keeper-dependent/.test(n)),
    'FULL_RELEASE_STEPS_1_TO_20 successor of the confirmed 19-step pre-release journal: only step 20 (activate-receipt, backing cap 8,000,000,000 raw) remains.',
    'Activation opens deposits to anyone: run the pilot-ceremony-v3 pre-activation check first and prepare the seed immediately after activation is CONFIRMED.',
    'Any other transaction from the Ledger account voids this config (sequence). Regenerate it; never sign a stale file.',
    `Host-preflight stop ${HOST_STOP}; >= ${AT_LIMIT} is AT-LIMIT (user decisions 15 Nov 2026).`];
  return config;
}
// Offline validation of an (unreviewed or reviewed) FULL config: everything the helper checks, with the status
// substituted only in memory for the unreviewed file.
function validateFull(plan, config, now) {
  assert.equal(sha(fs.readFileSync(path.join(__dirname, 'operator-deployment-v3.cjs'))), HELPER_SHA256, 'operator-deployment-v3.cjs changed; the confirmed chain would not validate');
  assert([FULL_PENDING_STATUS, 'REVIEWED_UNSIGNED_CEREMONY'].includes(config.status));
  assert.equal(config.releaseStage, D.STAGES.FULL); assert.equal(config.keeperPublicAddress, KEEPER);
  const probe = {...config, status: 'REVIEWED_UNSIGNED_CEREMONY'};
  D.validateCeremony(plan, probe, now);
  assert.deepEqual(probe.costModelXdrSha256, {...COST_MODEL_20261113}, 'config cost model differs from the 13 Nov capture');
  for (const [name, pin] of Object.entries({...config.releaseApproval.furtherEvidence, independentReview: config.releaseApproval.independentReview, signedCore: config.releaseApproval.signedCore}))
    assert.equal(sha(fs.readFileSync(path.resolve(ROOT, pin.path))), pin.sha256, `evidence ${name} changed`);
  assert.deepEqual(config.releaseApproval.userRatification, {...RATIFICATION});
  const activation = plan.steps.find(s => s.step === 'activate-receipt'); assert.equal(activation.args[3], '8000000000');
  assert.equal(config.readbacks['activate-receipt'].after.some(r => r.method === 'stage_config'), true);
  return probe;
}
// Read-only: chain check against the confirmed journal (no lock, no write).
function chainCheck(plan, probe, journalDir) {
  const journal = D.loadJournal(path.resolve(ROOT, journalDir), plan, probe);
  assert(!journal.attached && journal.active === null && !journal.halted, 'journal must be the confirmed 19-step journal, not yet continued');
  assert.equal(journal.entries.length, 19); assert(journal.entries.every(r => r.status === 'CONFIRMED'));
  assert.equal(journal.entries.at(-1).step, 'observation-keeper');
  assert(BigInt(probe.initialSourceSequence) >= BigInt(journal.entries.at(-1).sequence));
  return {entries: 19, lastSequence: journal.entries.at(-1).sequence, segments: journal.segments.length};
}
const transient = e => /429|Too Many|rate|crossed a ledger|ledger changed|ledger advanced|ECONNRESET|ETIMEDOUT|socket hang up|timeout|503|502/i.test(String(e?.message || e));
// Read-only retry: a crossed ledger retries at once (nothing was prepared or journaled); HTTP 429 / network errors back off.
const rateLimited = e => /429|Too Many|rate|ECONNRESET|ETIMEDOUT|socket hang up|timeout|503|502/i.test(String(e?.message || e));
async function retry(fn, {tries = 25, wait = ms => new Promise(r => setTimeout(r, ms)), log = () => {}} = {}) {
  for (let i = 1, backoff = 0; ; i++) {
    try { return await fn(); } catch (e) {
      if (!transient(e) || i >= tries) throw e;
      const ms = rateLimited(e) ? Math.min(2000 * 2 ** backoff++, 30000) : 250;
      log(`transient (${String(e.message).split('\n')[0].slice(0, 80)}); retry ${i} in ${ms} ms`); await wait(ms);
    }
  }
}
// Read-only RPC wrapper for the UNCHANGED helper adapter: caches the network passphrase (constant), limits
// concurrent simulations (HTTP 429), and lets an attempt start right after a ledger closes so the helper's
// one-ledger rule can be met. It never alters a response; every ledger check of the helper still applies.
function paced(rpc, {concurrency = 12, sleep = ms => new Promise(r => setTimeout(r, ms))} = {}) {
  let network = null, active = 0; const queue = [];
  const slot = async () => { while (active >= concurrency) await new Promise(r => queue.push(r)); active++; };
  const free = () => { active--; const next = queue.shift(); if (next) next(); };
  const limited = name => async (...a) => { await slot(); try { return await rpc[name](...a); } finally { free(); } };
  return {getNetwork: async () => network ??= await rpc.getNetwork(), getLatestLedger: (...a) => rpc.getLatestLedger(...a),
    getLedgerEntries: limited('getLedgerEntries'), simulateTransaction: limited('simulateTransaction'), getLedgers: (...a) => rpc.getLedgers(...a),
    getFeeStats: (...a) => rpc.getFeeStats(...a), getTransaction: (...a) => rpc.getTransaction(...a), getAccount: (...a) => rpc.getAccount(...a),
    // Wait until a NEW ledger closes, then return at once (the next attempt then has the whole close interval).
    async alignToNewLedger(maxMs = 15000) { const start = (await rpc.getLatestLedger()).sequence, t0 = Date.now();
      while (Date.now() - t0 < maxMs) { await sleep(200); if ((await rpc.getLatestLedger()).sequence > start) return; } }};
}
// The exact step-20 transaction, prepared in memory only, validated by the helper. Returns figures, never XDR.
async function step20(plan, probe, rpc) {
  if (rpc.alignToNewLedger) await rpc.alignToNewLedger();
  const deps = D.adapter(plan, probe, rpc), sequence = String(BigInt(probe.initialSourceSequence) + 1n);
  const row = await deps.prepare('activate-receipt', sequence);
  D.validatePrepared(plan, probe, row, sequence, '0');
  const tx = S.TransactionBuilder.fromXDR(row.unsignedXdr, plan.network), data = tx.toEnvelope().v1().tx().ext().sorobanData(), r = data.resources(), fp = r.footprint();
  const cpu = r.instructions();
  const out = {status: cpu > HOST_STOP ? 'REFUSED_ABOVE_97M' : 'SIMULATED_ONLY_NOT_SIGNABLE', label: cpuLabel(cpu), ledger: row.ledger, closeTime: row.closeTime,
    sequence, declaredInstructionsFromRpcHostPreflight: cpu, diskReadBytes: r.diskReadBytes(), writeBytes: r.writeBytes(),
    footprint: {readOnly: fp.readOnly().length, readWrite: fp.readWrite().length},
    footprintKeysXdr: {readOnly: fp.readOnly().map(k => k.toXDR('base64')), readWrite: fp.readWrite().map(k => k.toXDR('base64'))},
    resourceFeeStroops: data.resourceFee().toString(), simulationMinResourceFeeStroops: row.simulationResourceFeeStroops,
    maxFeeStroops: row.maxFeeStroops, inclusionFeeStroops: probe.inclusionFeeStroops, beforeReadbacks: row.before.length,
    unsignedHashForReferenceOnly: row.hash, note: 'Not saved as XDR and never signable: the operator prepares step 20 afresh with operator-deployment-v3.cjs prepare.'};
  assert(cpu <= HOST_STOP, `REFUSED: step 20 host preflight ${cpu} > ${HOST_STOP}`);
  return out;
}
async function fullRelease(outputDir, {rpc = new S.rpc.Server('https://rpc.lightsail.network', {timeout: 60}), rpcUrl = 'https://rpc.lightsail.network',
  journalDir = CONFIRMED_JOURNAL, predecessorFile = CONFIRMED_CONFIG, log = () => {}} = {}) {
  const plan = read(PLAN); D.validatePlan(plan);
  const predBytes = fs.readFileSync(path.resolve(ROOT, predecessorFile));
  assert.equal(sha(predBytes), CONFIRMED_CONFIG_SHA256, 'predecessor is not the config that confirmed steps 1-19');
  const predecessor = {path: path.relative(ROOT, path.resolve(ROOT, predecessorFile)), sha256: sha(predBytes)};
  const evidence = evidencePins();
  const {live, settings, base} = await retry(async () => {
    const live = await G.readLive(rpc, 19);
    const s = await readSettings(rpc, live.ledger);
    const settings = {...compareSettings(s.live), liveLedger: s.ledger};
    assert.deepEqual(live.costModelXdrSha256, {...COST_MODEL_20261113});
    const base = G.build(live, KEEPER, predecessor); base.rpcUrl = rpcUrl;
    return {live, settings, base};
  }, {log});
  const config = fullFrom(base, evidence, settings, predecessor);
  config.provenance.generatorV31 = {path: path.relative(ROOT, __filename), sha256: sha(fs.readFileSync(__filename))};
  const now = Math.floor(Date.parse(live.observedAt) / 1000), probe = validateFull(plan, config, now);
  const chain = chainCheck(plan, probe, journalDir);
  const sim = await retry(() => step20(plan, probe, rpc), {log});
  fs.mkdirSync(outputDir, {recursive: true});
  const stamp = live.observedAt.replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z');
  const files = {config: path.join(outputDir, `deploy-config-full-release-${stamp}-L${live.ledger}.UNREVIEWED.json`),
    simulation: path.join(outputDir, `step20-simulation-${stamp}-L${sim.ledger}.json`)};
  for (const f of Object.values(files)) assert(!fs.existsSync(f), `preserve previous output ${f}`);
  fs.writeFileSync(files.config, json(config), {flag: 'wx'});
  fs.writeFileSync(files.simulation, json({status: sim.status, signing: false, submission: false, configFile: path.relative(ROOT, files.config),
    configSha256: sha(fs.readFileSync(files.config)), journalChain: chain, networkSettings: settings, step20: sim}), {flag: 'wx'});
  return {files: Object.fromEntries(Object.entries(files).map(([k, v]) => [k, path.relative(ROOT, v)])), ledger: live.ledger,
    initialSourceSequence: live.sequence, step20Sequence: sim.sequence, step20: {cpu: sim.declaredInstructionsFromRpcHostPreflight, label: sim.label,
      resourceFeeStroops: sim.resourceFeeStroops, maxFeeStroops: sim.maxFeeStroops}, expiresAt: config.expiresAt, journalChain: chain, signing: false, submission: false};
}
// OPERATOR ONLY, offline: after reading the unreviewed file, set REVIEWED_UNSIGNED_CEREMONY in a NEW file.
function markReviewed(file, now = Math.floor(Date.now() / 1000)) {
  assert(file.endsWith('.UNREVIEWED.json'));
  const plan = read(PLAN), config = JSON.parse(fs.readFileSync(file)); assert.equal(config.status, FULL_PENDING_STATUS);
  validateFull(plan, config, now); chainCheck(plan, {...config, status: 'REVIEWED_UNSIGNED_CEREMONY'}, CONFIRMED_JOURNAL);
  const out = file.replace(/\.UNREVIEWED\.json$/, '.json'); assert(!fs.existsSync(out));
  const reviewed = {...config, status: 'REVIEWED_UNSIGNED_CEREMONY', operatorReview: {by: 'operator (mark-reviewed)', atUnix: now, unreviewedSha256: sha(fs.readFileSync(file))}};
  fs.writeFileSync(out, json(reviewed), {flag: 'wx'});
  return {status: 'REVIEWED_UNSIGNED_CEREMONY', file: out, sha256: sha(fs.readFileSync(out)), signing: false, submission: false};
}
// The 97M host-preflight stop runs inside the adapter, i.e. BEFORE the helper journals the prepared row.
function stopAbove97M(deps, plan) {
  return {...deps, prepare: async (...a) => { const row = await deps.prepare(...a);
    const cpu = S.TransactionBuilder.fromXDR(row.unsignedXdr, plan.network).toEnvelope().v1().tx().ext().sorobanData().resources().instructions();
    assert(cpu <= HOST_STOP, `REFUSED (${cpuLabel(cpu)}): host preflight ${cpu} > ${HOST_STOP}; nothing journaled`); return row; }};
}
// (e) Operator flow for step 20 through the UNCHANGED helper: D.prepare / D.reconcile with the paced read-only RPC
// and read-only retries. D.prepare journals only after its adapter returned a coherent one-ledger preparation, so a
// retry after "ledger changed" or HTTP 429 cannot duplicate an entry; an outstanding hash is never re-prepared
// (the helper refuses) and reconcile reports UNKNOWN_NO_RETRY instead of guessing.
async function operatorStep(mode, planFile, configFile, journalDir, {url = 'https://rpc.lightsail.network', log = m => process.stderr.write(m + '\n')} = {}) {
  assert(['prepare', 'reconcile'].includes(mode));
  const plan = JSON.parse(fs.readFileSync(planFile)), config = JSON.parse(fs.readFileSync(configFile));
  assert.equal(config.releaseStage, D.STAGES.FULL, 'step-20 wrapper is for the FULL_RELEASE config only');
  assert.equal(config.status, 'REVIEWED_UNSIGNED_CEREMONY', 'run mark-reviewed (operator) first');
  validateFull(plan, config, mode === 'prepare' ? Math.floor(Date.now() / 1000) : null);
  const rpc = paced(new S.rpc.Server(url, {timeout: 60}));
  return retry(async () => {
    if (mode === 'prepare') await rpc.alignToNewLedger();
    const out = await D[mode](plan, config, path.resolve(journalDir), stopAbove97M(D.adapter(plan, config, rpc), plan));
    if (mode === 'prepare') out.cpuLabel = cpuLabel(out.declaredInstructions);
    return out;
  }, {log});
}
module.exports = {operatorStep, stopAbove97M, EVIDENCE, RATIFICATION, COST_MODEL_20261113, HOST_STOP, AT_LIMIT, FULL_PENDING_STATUS, CONFIRMED_JOURNAL, CONFIRMED_CONFIG, PLAN,
  paced, evidencePins, capture, compareSettings, readSettings, cpuLabel, fullFrom, validateFull, chainCheck, step20, retry, transient, fullRelease, markReviewed};
if (require.main === module) {
  const args = process.argv.slice(2), mode = args.shift(), opt = n => { const i = args.indexOf(n); return i < 0 ? null : args.splice(i, 2)[1]; };
  Promise.resolve().then(async () => {
    if (mode === 'prepare-step20' || mode === 'reconcile-step20') { const url = opt('--rpc') || 'https://rpc.lightsail.network'; assert.equal(args.length, 3, 'usage: prepare-step20|reconcile-step20 PLAN FULL_CONFIG JOURNAL_DIRECTORY [--rpc URL]');
      return operatorStep(mode.replace('-step20', ''), args[0], args[1], args[2], {url}); }
    if (mode === 'mark-reviewed') { assert(args.includes('--i-reviewed-this-config') && args.length === 2, 'usage: mark-reviewed FILE.UNREVIEWED.json --i-reviewed-this-config  (OPERATOR ONLY)');
      return markReviewed(path.resolve(args.find(a => !a.startsWith('--')))); }
    assert.equal(mode, 'full-release', 'usage: full-release OUTPUT_DIRECTORY [--rpc URL] | mark-reviewed FILE.UNREVIEWED.json --i-reviewed-this-config');
    const url = opt('--rpc') || 'https://rpc.lightsail.network'; assert.equal(args.length, 1);
    return fullRelease(path.resolve(args[0]), {rpc: paced(new S.rpc.Server(url, {timeout: 60})), rpcUrl: url, log: m => process.stderr.write(m + '\n')});
  }).then(v => console.log(json(v))).catch(e => { console.error(e.message); process.exitCode = 1; });
}
