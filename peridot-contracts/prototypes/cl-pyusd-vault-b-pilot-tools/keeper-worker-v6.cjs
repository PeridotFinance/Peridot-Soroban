// Operator-run, single-step pilot keeper. Defaults to public inspection only.
//
// v6 (2026-11-21). Copy of keeper-worker-v5.cjs (205e9785, unchanged; its approvals do not transfer) for the fresh-stack
// CL collateral group: vault v2 (upgrade-gated) and receipt StgV2. The contract interface the keeper uses is identical
// (`stellar contract info interface`: every called function and every user type equal between CD6N/CCGK and the 96e7b3c
// candidates). ONLY changes:
//  (1) validateConfig refuses the retired pilot pair CCGK/CD6N, any PLACEHOLDER/<...> value, and a stateDirectory that is
//      not named state-v6* or lies inside an earlier keeper state directory (state, state-v4, state-v5).
//  (2) pending upgrade -> refuse. Execution reads the vault's and the receipt's persistent PendingUpgradeHash entries
//      before inspecting, and prepare re-reads them in the SAME burst-1 getLedgerEntries as the code instances (no extra
//      round trip on the job-2 latency path). Either entry present: hard refusal before anything is journaled or signed.
//      Read-only inspection reports them.
//  (3) approval scope 'cl-vault-b-keeper-v6'; bindings name keeper-worker-v6, keeper-plan-v4 (unchanged), keeper-inspect-v5
//      (unchanged).
// Everything else (allowlist, journal, approval gates, fee/CPU/resource ceilings incl. the 97M stop, auth checks, windows,
// signing path) is the v5 text.
//
// v5 (2026-11-17). Successor of keeper-worker-v4.cjs (unchanged; its approvals do not transfer). Liveness fix only; the
// allowlist, journal, approval gates, fee/CPU/resource ceilings, auth checks, windows and signing path are v4's.
// 17 Nov on Alchemy, v4 prepared almost nothing: getLatestLedger/getLedgers took 2-21 s (full ledger metadata), and v4
// required latest, account, header, config and simulation to report the snapshot ledger (sameLedger). A second instance
// of the same rule in assertFresh (getLatestLedger then the account entry) threw LEDGER_MOVED outside prepare's catch,
// which the loop turns into SAFETY_REFUSAL (or, after signing, HALTED_UNKNOWN for a transaction never submitted).
//  (1) prepare = two parallel bursts. Burst 1: network, ONE getLedgerEntries (keeper account, 4 config settings, both
//      code instances), getEvents (latest ledger + close time) and, at most every 120 ledgers, the header of the snapshot
//      ledger (base fee/reserve; getLedgers is the slow call, so it is cached per process). Burst 2: the step simulation
//      next to a re-read of every decision input (inspect-v5 decisionLabels).
//  (2) The simulation ledger L_sim is the reference. Accepted only if no response is behind the snapshot ledger R, every
//      decision input is proven unchanged at L_sim (guard/reward values re-read at >= L_sim and equal; owner commands
//      and checkpoint_rewards read at exactly L_sim), the planner re-run at L_sim gives the same first step (for stage_invest the same [O+1, O+4)
//      bounds, i.e. O+1 <= L_sim <= O+2), and at least one ledger is left after L_sim. Otherwise nothing is journaled or
//      signed: LEDGER_CHANGED_READ_AGAIN (reason code) or LEDGER_REGRESSED_READ_AGAIN. Only when all of that holds is a
//      failed simulation a hard refusal (v4: a failed simulation in the snapshot ledger).
//  (3) The signed bytes are the simulated bytes: bounds stay anchored at R ([R+1, R+21), observation [R+1, R+11),
//      invest [max(R+1, O+1), O+4)), never wider than bounds anchored at L_sim >= R.
//  (4) assertFresh reads ledger and account from one getLedgerEntries response. Before journaling, a closing window or a
//      lagging response is a soft read-again; an account change is still a hard refusal. After signing: v4 semantics.
//  (5) deferObservation (loop only): an observation plan returns OBSERVATION_STREAK_REQUIRED before anything is
//      journaled, so the loop no longer needs a separate read-only inspection every ledger.
//  (6) approval scope 'cl-vault-b-pilot-keeper-v5'; bindings name keeper-worker-v5, keeper-plan-v4, keeper-inspect-v5.
// v4 (2026-11-16). Minimal-diff successor of keeper-worker-v3.cjs (unchanged; its approvals do not transfer).
// ONLY changes, all operational (frozen contracts unchanged):
//  (1) submission windows: observations [L+1, L+11) (planner v4), command steps [L+1, L+21); invest unchanged [O+1, O+4).
//  (2) assertFresh no longer refuses because a new ledger closed: it requires the keeper account (sequence, balance,
//      liabilities) unchanged and at least one ledger left in the window. v3 refused whenever the ledger moved
//      during signing, i.e. most of the time on 5 s ledgers.
//  (3) optional dedicated RPC: config.rpcEndpointFile (mode 600, one https URL incl. key) + config.rpcHost (pinned).
//      The URL never enters the config digest, the journal, any output or any process argument.
//  (4) approval scope 'cl-vault-b-pilot-keeper-v4'; source bindings name the v4 worker/planner/inspector.
//
// v3 (2026-11-15). Minimal-diff successor of keeper-worker-v2.cjs (unchanged; its approvals do not transfer).
// ONLY changes: (1) USER RULE 15 Nov 2026: a prepared keeper transaction whose RPC host preflight declares more than
// 97,000,000 instructions is refused before it is journaled or signed (the 100M ceiling stays); >= 98,000,000 is
// reported as AT-LIMIT. (2) the source binding names keeper-worker-v3.cjs (plan and inspect stay v2, unchanged).
// (3) every prepared/refused report carries declaredInstructions and cpuLabel. Everything else is the v2 text.
//
// v2 (2026-11-14). Minimal-diff successor of prototypes/cl-pyusd-vault-b/keeper-worker.cjs.
// Reviews, approvals, test evidence and source hashes recorded for v1 do NOT transfer.
// ONLY change: the permissionless vault call `checkpoint_rewards` is an allowlisted, planned
// step, and while a reward interval is open (after any emergency exit) it must go first.
// Changed functions: validatePlan, validateJournal and the unsigned `prepare` adapter (plus the METHODS
// allowlist, sourceBindings names and the inspect import). NOT changed: validateApproval, runSingleStep,
// reconcile, and the sign / submit / publicAddress adapters. This file therefore still contains the v1 gated
// --execute path (operator approval file + secure-store attestation), and checkpoint_rewards becomes
// executable under that same gate. It was exercised only with injected fakes; no real signer was touched.
// No key creation/export, service installation, owner commands, or automatic retry.
// Execution requires --execute and a separately reviewed approval file. An OS
// secure-store provisioning attestation is required: public-key lookup alone
// cannot establish how an existing CLI alias stores its key.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const cp = require('node:child_process');
const assert = require('node:assert/strict');
const {performance} = require('node:perf_hooks');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {inspect, rpcUrlOf, rereadDecisions, knownAt, planAt, canonical: canonicalValue} = require('./keeper-inspect-v5.cjs');
const {OBSERVATION_WINDOW} = require('./keeper-plan-v4.cjs');
const STEP_WINDOW = 21; // command steps: [L+1, L+21)
const HEADER_MAX_AGE_LEDGERS = 120; // v5: base fee / base reserve header reused for at most ~11 minutes
// v5: stage_invest with only its last ledger left is journaled only if the RPC is answering fast enough to sign, re-check
// and submit within that ledger; otherwise read again (soft) rather than risk a post-signing halt.
const LAST_LEDGER_MAX_FRESH_MS = 1500;
// Signing never contacts the RPC; a public URL is passed so the API key never appears in a process argument list.
const PUBLIC_SIGNING_RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm';

const METHODS = new Set(['stage_checkpoint_investment', 'stage_convert', 'stage_mint',
  'stage_invest', 'stage_finalise', 'stage_withdraw', 'stage_close_convert', 'stage_emergency_pay', 'checkpoint_rewards']);
const VAULT_METHODS = new Set(['stage_checkpoint_investment', 'checkpoint_rewards']);
// Steps with no reward dependency. Only these may run while an open interval cannot be checkpointed.
const RELEASE_METHODS = new Set(['stage_emergency_pay', 'stage_finalise']);
const HOST_STOP = 97000000, AT_LIMIT = 98000000;
const cpuLabel = cpu => cpu >= AT_LIMIT ? 'AT-LIMIT' : cpu > HOST_STOP ? 'ABOVE_97M_STOP' : 'WITHIN_POLICY';
const LIMITS = Object.freeze({instructions: 100000000, memory: 41943040, keys: 100,
  writes: 50, sdkEntries: 100, readBytes: 200000, writeBytes: 132096,
  events: 16384, transactionBytes: 132096});
const stringify = value => JSON.stringify(value, (_, v) => typeof v === 'bigint' ? String(v) : v, 2) + '\n';
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
const canonical = value => JSON.stringify(sort(value));
function sort(value) {
  if (Array.isArray(value)) return value.map(sort);
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(k => [k, sort(value[k])]));
  return value;
}
function uint(value, label = 'integer') {
  assert(/^(0|[1-9][0-9]*)$/.test(String(value)), `${label} must be an unsigned integer`);
  return BigInt(value);
}
function xlm(value) {
  assert(typeof value === 'string' && /^(0|[1-9][0-9]*)(\.[0-9]{1,7})?$/.test(value), 'feeFloorXlm requires an exact decimal string');
  const [whole, frac = ''] = value.split('.');
  return BigInt(whole) * 10000000n + BigInt(frac.padEnd(7, '0'));
}
function sourceBindings() {
  return Object.fromEntries(['keeper-worker-v6.cjs', 'keeper-plan-v4.cjs', 'keeper-inspect-v5.cjs'].map(name => [name, sha(fs.readFileSync(path.join(__dirname, name)))]));
}
function configDigest(config) { return sha(canonical(config)); }
// v5: soft refusals. Raised only before anything is journaled or signed; runSingleStep turns them into a read-again
// status with the code as `reason`. Every other failure still refuses (throws) as in v4.
const SOFT_CODES = new Set(['LEDGER_REGRESSED', 'DECISION_INPUT_CHANGED', 'NOT_AT_REFERENCE_LEDGER', 'CONFIRMATION_BEHIND_REFERENCE',
  'CHECKPOINT_NOT_AT_REFERENCE_LEDGER', 'COMMANDS_NOT_AT_REFERENCE_LEDGER', 'READ_AFTER_REFERENCE', 'PLAN_CHANGED_AT_SIMULATION_LEDGER', 'WINDOW_CLOSING']);
function soft(code) { assert(SOFT_CODES.has(code)); return Object.assign(Error('read again: ' + code), {code, soft: true}); }
function softStatus(error) {
  if (error?.soft !== true || !SOFT_CODES.has(error.code)) return null;
  return {status: error.code === 'LEDGER_REGRESSED' ? 'LEDGER_REGRESSED_READ_AGAIN' : 'LEDGER_CHANGED_READ_AGAIN', reason: error.code, signing: false, submission: false};
}
function executable(name) {
  for (const directory of (process.env.PATH || '').split(path.delimiter)) {
    if (!path.isAbsolute(directory)) continue;
    const candidate = path.join(directory, name);
    try { fs.accessSync(candidate, fs.constants.X_OK); if (fs.statSync(candidate).isFile()) return fs.realpathSync(candidate); } catch (_) { /* continue */ }
  }
  throw Error('Stellar CLI executable is unavailable');
}
function cleanEnvironment() {
  return Object.fromEntries(['PATH', 'HOME', 'USER', 'TMPDIR'].filter(k => process.env[k]).map(k => [k, process.env[k]]));
}
let cachedRuntime;
function runtimeBindings() {
  if (cachedRuntime) return structuredClone(cachedRuntime);
  const cli = executable('stellar');
  const node = fs.realpathSync(process.execPath);
  const sdkRoot = path.resolve(__dirname, '../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
  const sdkEntry = fs.realpathSync(require.resolve('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk'));
  const sdkPackage = JSON.parse(fs.readFileSync(path.join(sdkRoot, 'package.json')));
  let cliVersion;
  try { cliVersion = cp.execFileSync(cli, ['--version'], {encoding: 'utf8', timeout: 10000,
    env: cleanEnvironment(), stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 16384}).trim(); }
  catch (_) { throw Error('Stellar CLI public version lookup failed'); }
  cachedRuntime = {node: {version: process.version, executable: node, sha256: sha(fs.readFileSync(node))},
    stellarCli: {version: cliVersion, executable: cli, sha256: sha(fs.readFileSync(cli))},
    stellarSdk: {version: sdkPackage.version, entry: sdkEntry, entrySha256: sha(fs.readFileSync(sdkEntry)),
      packageJsonSha256: sha(fs.readFileSync(path.join(sdkRoot, 'package.json'))),
      packageLockSha256: sha(fs.readFileSync(path.resolve(__dirname, '../../bots/margin-liquidator/package-lock.json')))},
    trustBoundary: 'Operator-controlled installed runtime and dependencies must remain immutable during the invocation; package-lock pins transitive dependencies but is not a recursive installed-file attestation.'};
  return structuredClone(cachedRuntime);
}
// v6: the keeper v5 scope. v6 never targets these contracts nor shares their state directories.
const RETIRED_CONTRACTS = Object.freeze(['CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI', 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63']);
const EARLIER_STATE_DIRECTORIES = Object.freeze(['/Users/joshua/Peridot/keeper-pyusd-vaultb/state', '/Users/joshua/Peridot/keeper-pyusd-vaultb/state-v4',
  '/Users/joshua/Peridot/keeper-pyusd-vaultb/state-v5']);
function assertNoPlaceholder(value, where = 'config') {
  if (typeof value === 'string') assert(!/PLACEHOLDER|<[^>]*>|FILL[_ ]?ME/i.test(value), `placeholder left in ${where}`);
  else if (Array.isArray(value)) value.forEach((v, i) => assertNoPlaceholder(v, `${where}[${i}]`));
  else if (value && typeof value === 'object') for (const [k, v] of Object.entries(value)) if (k !== 'operatorFillsOnKeeperHost' && k !== 'notes') assertNoPlaceholder(v, `${where}.${k}`);
}
function validateConfig(config) {
  assertNoPlaceholder(config);
  for (const field of ['receipt', 'vault']) assert(!RETIRED_CONTRACTS.includes(config[field]), `retired pilot ${field}: keeper v6 targets the fresh stack only`);
  assert(typeof config.stateDirectory === 'string' && path.isAbsolute(config.stateDirectory) && /^state-v6/.test(path.basename(config.stateDirectory)), 'v6 state directory must be absolute and named state-v6*');
  for (const earlier of EARLIER_STATE_DIRECTORIES) {
    const rel = path.relative(earlier, config.stateDirectory);
    assert(rel !== '' && (rel.startsWith('..') || path.isAbsolute(rel)), 'v6 state directory must not be (inside) an earlier keeper state directory');
  }
  assert.equal(config.network, S.Networks.PUBLIC, 'only the explicitly approved public network is supported');
  // v4: exactly one of rpcUrl (no credentials) or rpcEndpointFile + rpcHost (URL with key kept out of the config).
  assert((config.rpcUrl === undefined) !== (config.rpcEndpointFile === undefined), 'configure exactly one of rpcUrl or rpcEndpointFile');
  const endpoint = new URL(config.rpcUrl ?? `https://${config.rpcHost}/`);
  assert.equal(endpoint.protocol, 'https:');
  assert(!endpoint.username && !endpoint.password && !endpoint.hash, 'RPC credentials must not be embedded');
  if (config.rpcEndpointFile !== undefined) {
    assert(path.isAbsolute(config.rpcEndpointFile) && /^[a-z0-9.-]+$/.test(config.rpcHost ?? ''), 'rpcEndpointFile must be absolute and rpcHost a plain host');
  } else assert.equal(config.rpcHost, undefined);
  for (const field of ['receipt', 'vault', 'fundingAsset']) assert(S.StrKey.isValidContract(config[field]), `invalid ${field}`);
  assert.notEqual(config.receipt, config.vault);
  assert(S.StrKey.isValidEd25519PublicKey(config.admin));
  for (const role of ['receipt', 'vault']) assert(/^[0-9a-f]{64}$/.test(config.wasmSha256?.[role]), `missing ${role} code pin`);
  assert(Array.isArray(config.pilotOwners) && config.pilotOwners.length > 0 && new Set(config.pilotOwners).size === config.pilotOwners.length);
  for (const owner of config.pilotOwners) assert(S.StrKey.isValidEd25519PublicKey(owner), 'pilot discovery requires explicit account owners');
}
function validateApproval(config, approval, now, runtime = runtimeBindings()) {
  validateConfig(config);
  assert.equal(config.status, 'CONFIGURED_FOR_PILOT_EXECUTION', 'draft configuration cannot execute');
  assert.equal(config.readOnly, false, 'execution configuration must be explicit');
  assert.equal(approval?.status, 'OPERATOR_APPROVED');
  assert.equal(approval.scope, 'cl-vault-b-keeper-v6');
  assert.equal(approval.configDigestSha256, configDigest(config), 'approved configuration differs');
  assert.deepEqual(approval.sourceSha256, sourceBindings(), 'worker/planner/inspector changed after approval');
  assert.deepEqual(approval.runtimeProvenance, runtime, 'Node/CLI/SDK runtime differs from approval');
  assert(Number.isSafeInteger(approval.expiresAt) && approval.expiresAt > now, 'approval expired or absent');
  assert(S.StrKey.isValidEd25519PublicKey(config.keeperPublicAddress), 'keeper account is not configured');
  assert.notEqual(config.keeperPublicAddress, config.admin, 'dedicated keeper must differ from the Ledger admin');
  assert(!config.pilotOwners.includes(config.keeperPublicAddress), 'keeper must not be an owner');
  const signer = config.signer;
  assert.equal(signer?.storage, 'os-secure-store');
  assert(/^[a-zA-Z][a-zA-Z0-9_-]{0,63}$/.test(signer.alias), 'identity alias only; raw keys and seed phrases are forbidden');
  assert(!S.StrKey.isValidEd25519SecretSeed(signer.alias));
  assert(path.isAbsolute(signer.configDirectory), 'explicit CLI configuration directory required');
  assert.deepEqual(approval.secureStoreProvisioning, {alias: signer.alias,
    publicAddress: config.keeperPublicAddress, configDirectory: signer.configDirectory, reviewed: true},
  'separate secure-store provisioning review is required');
  assert(path.isAbsolute(config.stateDirectory), 'approved absolute journal directory required');
  assert.equal(config.protocolVersion, 29, 'protocol changes require requalification');
  for (const role of ['cpu', 'memory']) assert(/^[0-9a-f]{64}$/.test(config.costModelXdrSha256?.[role]), 'qualified cost-model XDR pins required');
  const budget = uint(config.feeBudgetStroops, 'fee budget');
  const ceiling = uint(config.maxTransactionFeeStroops, 'transaction fee ceiling');
  const bid = uint(config.inclusionFeeStroops, 'inclusion fee');
  const cap = uint(config.inclusionFeeCapStroops, 'inclusion cap');
  assert(budget > 0n && ceiling > 0n && ceiling <= budget);
  assert(bid >= 100n && bid <= cap && cap <= ceiling);
  assert(xlm(config.feeFloorXlm) > 0n, 'positive keeper balance floor required');
  assert(Number.isSafeInteger(config.maxSnapshotAgeSeconds) && config.maxSnapshotAgeSeconds > 0 && config.maxSnapshotAgeSeconds <= 30);
  assert(Number.isSafeInteger(config.transactionTimeoutSeconds) && config.transactionTimeoutSeconds > 0 && config.transactionTimeoutSeconds <= 60);
}
function validatePlan(config, report, plan) {
  assert.equal(report.status, 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION');
  assert(Number.isSafeInteger(report.ledger) && report.ledger > 0);
  assert.equal(report.plan.snapshotLedger, report.ledger);
  assert.equal(report.plan.executeAtMostOneThenRefresh, true);
  assert(METHODS.has(plan.method), 'method is outside the keeper allowlist');
  assert.equal(plan.target, VAULT_METHODS.has(plan.method) ? config.vault : config.receipt);
  // Checkpoint-first rule, enforced here independently of the planner.
  const rewards = report.rewards;
  assert(rewards && typeof rewards.open === 'boolean', 'reward interval state missing from the snapshot');
  if (plan.method === 'checkpoint_rewards') {
    assert(rewards.open === true, 'checkpoint_rewards is planned only while a reward interval is open');
    assert(rewards.checkpointClaimable === true, 'venue claim unavailable: checkpoint_rewards would change nothing');
    assert(Array.isArray(plan.args) && plan.args.length === 0, 'checkpoint_rewards takes no arguments');
  } else {
    if (rewards.open) assert(rewards.checkpointClaimable === false && RELEASE_METHODS.has(plan.method),
      'reward interval is open after an emergency exit: checkpoint_rewards goes before anything else for this vault');
    assert(Array.isArray(plan.args) && plan.args.length === 1);
  }
  if (plan.method === 'stage_checkpoint_investment') {
    assert.equal(plan.args[0], config.keeperPublicAddress);
    assert.equal(plan.minLedger, report.ledger + 1);
    assert.equal(plan.maxLedgerExclusive, report.ledger + OBSERVATION_WINDOW);
  } else if (plan.method !== 'checkpoint_rewards') assert(uint(plan.args[0]) <= 18446744073709551615n, 'command ID exceeds u64');
  if (plan.method === 'stage_invest') {
    const guard = report.reads.guard.value;
    assert(guard.inside && guard.healthy_count === 3);
    assert.equal(plan.minLedger, guard.observed_ledger + 1);
    assert.equal(plan.maxLedgerExclusive, guard.observed_ledger + 4);
    assert(guard.observed_ledger <= report.ledger && report.ledger + 1 < plan.maxLedgerExclusive);
  } else if (plan.method !== 'stage_checkpoint_investment') {
    assert(plan.minLedger === undefined && plan.maxLedgerExclusive === undefined, 'unexpected custom ledger bounds');
  }
}
function validatePrepared(config, report, prepared, spent, now) {
  // v5: bounds are anchored at the snapshot ledger R; the simulation ran at L_sim >= R with every decision input proven there.
  assert.equal(prepared.ledger, report.ledger, 'prepared transaction must be anchored at the snapshot ledger');
  assert(Number.isSafeInteger(prepared.simulationLedger) && prepared.simulationLedger >= report.ledger, 'simulation older than the snapshot');
  assert.equal(prepared.source, config.keeperPublicAddress);
  assert(/^[0-9a-f]{64}$/.test(prepared.hash));
  assert(uint(prepared.sequence) === uint(prepared.accountSequence) + 1n);
  assert(Number.isSafeInteger(prepared.closeTime) && prepared.closeTime <= now && now - prepared.closeTime <= config.maxSnapshotAgeSeconds, 'stale ledger clock');
  assert(prepared.maxTime > now && prepared.minLedger >= report.ledger + 1 && prepared.maxLedgerExclusive > prepared.minLedger);
  const plan = report.plan.plans[0];
  assert.equal(prepared.minLedger, Math.max(report.ledger + 1, plan.minLedger || 0));
  assert.equal(prepared.maxLedgerExclusive, plan.maxLedgerExclusive || report.ledger + STEP_WINDOW);
  assert(prepared.maxTime <= prepared.closeTime + config.transactionTimeoutSeconds);
  assert(prepared.simulationLedger + 1 < prepared.maxLedgerExclusive, 'no ledger left after the simulation ledger');
  if (plan.method === 'stage_invest') {
    const observed = report.reads.guard.value.observed_ledger;
    assert(prepared.simulationLedger >= observed + 1 && prepared.simulationLedger <= observed + 2, 'stage_invest simulated outside [O+1, O+2]');
  }
  const m = prepared.metrics;
  for (const [field, limit] of Object.entries({instructions: LIMITS.instructions, keys: LIMITS.keys,
    writes: LIMITS.writes, readBytes: LIMITS.readBytes, writeBytes: LIMITS.writeBytes,
    signedEnvelopeBytes: LIMITS.transactionBytes})) {
    assert(Number.isSafeInteger(m[field]) && m[field] >= 0 && m[field] <= limit, `resource ceiling: ${field}`);
  }
  assert(m.keys + m.writes <= LIMITS.sdkEntries, 'SDK entry accounting ceiling');
  // v3: host-preflight stop (user rule 15 Nov 2026). Runs before the journal entry and before signing.
  assert(m.instructions <= HOST_STOP, `${cpuLabel(m.instructions)}: host preflight ${m.instructions} > ${HOST_STOP}; refused, nothing journaled or signed`);
  assert.equal(prepared.memoryAndEventsEnforcedByNetwork, true, 'simulation must enforce verified network memory/event ceilings');
  const fee = uint(prepared.maxFeeStroops);
  assert(fee > 0n && fee <= uint(config.maxTransactionFeeStroops), 'transaction fee exceeds ceiling');
  assert(uint(prepared.inclusionFeeStroops) === uint(config.inclusionFeeStroops));
  assert(uint(prepared.inclusionFeeStroops) <= uint(config.inclusionFeeCapStroops));
  assert(spent + fee <= uint(config.feeBudgetStroops), 'cumulative approved fee budget exceeded');
  const after = uint(prepared.balanceStroops) - uint(prepared.sellingLiabilitiesStroops) - fee;
  assert(after >= xlm(config.feeFloorXlm) && after >= uint(prepared.minimumReserveStroops), 'keeper balance floor or account reserve would be breached');
}

function atomicWrite(file, value) {
  const temp = file + '.tmp-' + crypto.randomUUID();
  const fd = fs.openSync(temp, 'wx', 0o600);
  try { fs.writeFileSync(fd, stringify(value)); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.renameSync(temp, file);
  const dir = fs.openSync(path.dirname(file), 'r');
  try { fs.fsyncSync(dir); } finally { fs.closeSync(dir); }
}
function validateJournal(config, journal) {
  assert.equal(journal.schema, 'cl-pilot-keeper-journal-v1');
  assert.equal(journal.configDigestSha256, configDigest(config), 'journal belongs to another approved configuration; review migration explicitly');
  assert(Array.isArray(journal.entries)); assert.equal(typeof journal.halted, 'boolean');
  assert(journal.active === null || Number.isSafeInteger(journal.active) && journal.active === journal.entries.length - 1 && journal.entries[journal.active]);
  const hashes = new Set(); let sequence = -1n;
  for (const [index, record] of journal.entries.entries()) {
    assert(/^[0-9a-f]{64}$/.test(record.hash) && !hashes.has(record.hash), 'invalid or duplicate journal hash'); hashes.add(record.hash);
    assert.equal(record.source, config.keeperPublicAddress);
    const next = uint(record.sequence); assert(next > sequence, 'journal sequence reused'); sequence = next;
    assert(METHODS.has(record.method)); assert.equal(record.target, VAULT_METHODS.has(record.method) ? config.vault : config.receipt);
    assert(Array.isArray(record.args) && record.args.length === (record.method === 'checkpoint_rewards' ? 0 : 1));
    if (record.method === 'stage_checkpoint_investment') assert.equal(record.args[0], config.keeperPublicAddress);
    else if (record.method !== 'checkpoint_rewards') uint(record.args[0]);
    assert(uint(record.maxFeeStroops) <= uint(config.maxTransactionFeeStroops));
    assert(Array.isArray(record.events) && record.events.every(event => ['PREPARED_BEFORE_SIGNING', 'SUBMITTING_ONCE',
      'SUBMITTED', 'UNKNOWN', 'RECONCILIATION_UNKNOWN', 'CONFIRMED_SUCCESS', 'CONFIRMED_FAILED', 'HALTED_AFTER_PREPARATION'].includes(event)));
    if (index === journal.active) {
      assert(['PREPARED', 'SUBMITTING', 'SUBMITTED', 'UNKNOWN'].includes(record.status));
      assert(record.actualFeeStroops === undefined);
    } else {
      assert(['SUCCESS', 'FAILED'].includes(record.status), 'unresolved journal entry lost its active reservation');
      assert(uint(record.actualFeeStroops) <= uint(record.maxFeeStroops));
      if (record.status === 'FAILED') assert(journal.halted, 'confirmed failure must remain halted');
    }
  }
}
function openJournal(config) {
  fs.mkdirSync(config.stateDirectory, {recursive: true, mode: 0o700});
  assert.equal(fs.realpathSync(config.stateDirectory), config.stateDirectory, 'journal path must be canonical and not a symlink');
  const lock = path.join(config.stateDirectory, 'worker.lock');
  const token = crypto.randomUUID();
  const fd = fs.openSync(lock, 'wx', 0o600); // No stale-lock guessing or lock stealing.
  fs.writeFileSync(fd, stringify({token, pid: process.pid})); fs.fsyncSync(fd); fs.closeSync(fd);
  const release = () => { assert.equal(JSON.parse(fs.readFileSync(lock)).token, token); fs.unlinkSync(lock); };
  try {
    const file = path.join(config.stateDirectory, 'journal.json');
    if (fs.existsSync(file)) assert(fs.lstatSync(file).isFile() && !fs.lstatSync(file).isSymbolicLink(), 'journal must be a regular file');
    const journal = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : {
      schema: 'cl-pilot-keeper-journal-v1', configDigestSha256: configDigest(config), entries: [], active: null, halted: false};
    validateJournal(config, journal);
    return {journal, save: () => atomicWrite(file, journal), release};
  } catch (error) { release(); throw error; }
}
async function reconcile(store, deps) {
  const record = store.journal.entries[store.journal.active];
  let result;
  try { result = await deps.reconcile(record); } catch (_) { result = {status: 'UNKNOWN'}; }
  if (!['SUCCESS', 'FAILED'].includes(result.status)) {
    record.status = 'UNKNOWN'; record.events.push('RECONCILIATION_UNKNOWN'); store.save();
    return {status: 'HALTED_UNKNOWN', hash: record.hash, signing: false, submission: false};
  }
  assert.equal(result.hash, record.hash); assert.equal(result.source, record.source);
  assert.equal(String(result.sequence), record.sequence);
  const fee = uint(result.feeChargedStroops);
  assert(fee <= uint(record.maxFeeStroops), 'confirmed fee exceeds reserved maximum');
  assert(Number.isSafeInteger(result.ledger) && result.ledger >= record.minLedger && result.ledger < record.maxLedgerExclusive, 'confirmation ledger violates approved bounds');
  record.status = result.status; record.actualFeeStroops = String(fee); record.ledger = result.ledger;
  record.events.push('CONFIRMED_' + result.status); store.journal.active = null;
  if (result.status === 'FAILED') store.journal.halted = true;
  store.save();
  return {status: result.status === 'SUCCESS' ? 'RECONCILED_SUCCESS_REFRESH_REQUIRED' : 'HALTED_CONFIRMED_FAILURE',
    hash: record.hash, feeChargedStroops: String(fee), signing: false, submission: false};
}

async function runSingleStep(config, {execute = false, reconcileOnly = false, approval = null, deferObservation = false, deps = null} = {}) {
  assert(!(execute && reconcileOnly), 'reconciliation cannot enable execution');
  assert(!deferObservation || execute, 'deferObservation applies to execution only');
  validateConfig(config);
  deps ??= productionDependencies(config);
  const now = () => Math.floor(deps.now());
  if (reconcileOnly) {
    assert(path.isAbsolute(config.stateDirectory), 'original approved journal directory required');
    if (!fs.existsSync(path.join(config.stateDirectory, 'journal.json'))) return {status: 'NOTHING_TO_RECONCILE', signing: false, submission: false};
    const store = openJournal(config);
    try {
      await deps.assertNetwork(false); // Existing transaction lookup needs the network identity, not an unchanged protocol.
      if (store.journal.active === null) return {status: store.journal.halted ? 'HALTED_REQUIRES_OPERATOR_REVIEW' : 'NOTHING_TO_RECONCILE', signing: false, submission: false};
      return await reconcile(store, deps);
    } finally { store.release(); }
  }
  if (!execute) {
    const report = await deps.inspect({...config, readOnly: true});
    let pendingUpgrades;
    try { pendingUpgrades = await deps.pendingUpgrades(); } catch (_) { pendingUpgrades = null; }
    let runtimeProvenance;
    try { runtimeProvenance = deps.runtimeBindings(); } catch (_) { runtimeProvenance = null; }
    return {...report, pendingUpgrades, ...(pendingUpgrades?.length ? {executionWouldRefuse: 'PENDING_UPGRADE'} : {}), configDigestSha256: configDigest(config), signing: false, submission: false,
      executionEnabled: false, sourceSha256: sourceBindings(), runtimeProvenance};
  }
  validateApproval(config, approval, now(), deps.runtimeBindings());
  const store = openJournal(config);
  try {
    await deps.assertNetwork();
    if (store.journal.active !== null) return await reconcile(store, deps); // Never sign while outcome is unknown.
    if (store.journal.halted) return {status: 'HALTED_REQUIRES_OPERATOR_REVIEW', signing: false, submission: false};
    assert.equal(await deps.publicAddress(config.signer), config.keeperPublicAddress, 'secure-store alias public key differs');
    assert.equal((await deps.pendingUpgrades()).length, 0, 'pending upgrade on the vault or receipt: refuse');
    const report = await deps.inspect({...config, readOnly: true});
    if (report.status !== 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION') return {status: report.status, ...(report.reason ? {reason: report.reason} : {}), signing: false, submission: false};
    const plan = report.plan.plans[0];
    if (!plan?.method) return {status: 'OWNER_RECOVERY_REQUIRED', signing: false, submission: false};
    validatePlan(config, report, plan);
    // v5: the loop runs observations as a consecutive-ledger streak (keeper-streak-probe); hand it back unsigned.
    if (deferObservation && plan.method === 'stage_checkpoint_investment') return {status: 'OBSERVATION_STREAK_REQUIRED', ledger: report.ledger, signing: false, submission: false};
    let prepared;
    try { prepared = await deps.prepare(plan, report); }
    catch (error) { const softResult = softStatus(error); if (softResult) return softResult; throw error; }
    const spent = store.journal.entries.reduce((sum, item) => sum + uint(item.actualFeeStroops ?? '0'), 0n);
    validatePrepared(config, report, prepared, spent, now());
    try { await deps.assertFresh(prepared, {preJournal: true, lastLedgerBudget: plan.method === 'stage_invest'}); }
    catch (error) { const softResult = softStatus(error); if (softResult) return softResult; throw error; }
    const record = {hash: prepared.hash, source: prepared.source, sequence: String(prepared.sequence), declaredInstructions: prepared.metrics.instructions, cpuLabel: cpuLabel(prepared.metrics.instructions),
      snapshotLedger: prepared.ledger, simulationLedger: prepared.simulationLedger, minLedger: prepared.minLedger, maxLedgerExclusive: prepared.maxLedgerExclusive,
      maxTime: prepared.maxTime, method: plan.method, target: plan.target, args: plan.args,
      maxFeeStroops: String(prepared.maxFeeStroops), status: 'PREPARED', events: ['PREPARED_BEFORE_SIGNING']};
    store.journal.entries.push(record); store.journal.active = store.journal.entries.length - 1; store.save();
    try {
      // Signed XDR exists only inside these adapters and memory. Never journal it.
      const signed = await deps.sign(prepared);
      await deps.assertFresh(prepared);
      assert(now() < prepared.maxTime, 'expired during secure-store signing');
      record.status = 'SUBMITTING'; record.events.push('SUBMITTING_ONCE'); store.save();
      const result = await deps.submit(signed);
      assert.equal(result.hash, record.hash, 'RPC returned another transaction hash');
      record.status = ['PENDING', 'DUPLICATE'].includes(result.status) ? 'SUBMITTED' : 'UNKNOWN';
      record.events.push(record.status); store.save();
      return {status: record.status === 'SUBMITTED' ? 'SUBMITTED_RECONCILE_NEXT_INVOCATION' : 'HALTED_UNKNOWN',
        hash: record.hash, method: plan.method, declaredInstructions: prepared.metrics.instructions, cpuLabel: cpuLabel(prepared.metrics.instructions), signing: true, submission: true};
    } catch (_) {
      record.status = 'UNKNOWN'; record.events.push('HALTED_AFTER_PREPARATION'); store.save();
      // Do not echo CLI/RPC errors: they can contain a signed envelope.
      return {status: 'HALTED_UNKNOWN', hash: record.hash, signingMayHaveOccurred: true,
        submissionMayHaveOccurred: record.events.includes('SUBMITTING_ONCE')};
    }
  } finally { store.release(); }
}

function productionDependencies(config, injected = {}) {
  const rpc = injected.rpc || new S.rpc.Server(rpcUrlOf(config), {timeout: 30});
  const monotonicMs = injected.monotonicMs || (() => performance.now());
  let cliPath = null, header = null; // v5: cached {ledger, baseFee, baseReserve, closeTime} of one recent snapshot ledger
  const accountKey = () => S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId:
    S.xdr.AccountId.publicKeyTypeEd25519(S.StrKey.decodeEd25519PublicKey(config.keeperPublicAddress))}));
  const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: S.xdr.ContractDataDurability.persistent()}));
  // v6: vault v2 and receipt StgV2 keep a proposed upgrade in persistent DataKey::PendingUpgradeHash (+ ...Eta).
  const pendingKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('PendingUpgradeHash')]),
    durability: S.xdr.ContractDataDurability.persistent()}));
  const isInstanceRow = row => row.val.contractData().key().switch().name === 'scvLedgerKeyContractInstance';
  const pendingOf = rows => rows.filter(row => !isInstanceRow(row)).map(row => S.Address.fromScAddress(row.val.contractData().contract()).toString());
  const names = ['configSettingContractComputeV0', 'configSettingContractEventsV0',
    'configSettingContractCostParamsCpuInstructions', 'configSettingContractCostParamsMemoryBytes'];
  const configKey = name => S.xdr.LedgerKey.configSetting(new S.xdr.LedgerKeyConfigSetting({configSettingId: S.xdr.ConfigSettingId[name]()}));
  const checkNetwork = (n, requireProtocol = true) => {
    assert.equal(n.passphrase, config.network);
    if (requireProtocol) assert.equal(n.protocolVersion, config.protocolVersion);
  };
  const assertNetwork = async (requireProtocol = true) => checkNetwork(await rpc.getNetwork(), requireProtocol);
  // v4 account() checks, without its own ledger read (v5 takes the ledger from the same response).
  const parseAccount = value => {
    assert.equal(S.StrKey.encodeEd25519PublicKey(value.accountId().ed25519()), config.keeperPublicAddress);
    assert.equal(value.numSubEntries(), 0, 'pilot keeper must have no trustlines/data/signers');
    assert.equal(value.signers().length, 0); assert.equal(value.thresholds()[0], 1);
    assert([...value.thresholds()].slice(1).every(n => n <= 1), 'unsupported keeper thresholds');
    let selling = 0n;
    if (value.ext().switch() === 1) {
      const v1 = value.ext().v1(); selling = BigInt(v1.liabilities().selling().toString());
      if (v1.ext().switch() === 2) {
        assert.equal(v1.ext().v2().numSponsored(), 0); assert.equal(v1.ext().v2().numSponsoring(), 0);
      } else assert.equal(v1.ext().switch(), 0);
    } else assert.equal(value.ext().switch(), 0);
    return {sequence: value.seqNum().toString(), balance: value.balance().toString(), selling: String(selling)};
  };
  const byKind = entries => {
    const out = {account: [], configSetting: [], contractData: []};
    for (const row of entries) { const kind = row.val.switch().name; assert(out[kind], 'unexpected ledger entry kind'); out[kind].push(row); }
    return out;
  };
  // Base fee and base reserve (network-upgrade parameters) from the header of a recent snapshot ledger. getLedgers carries
  // the ledger's full metadata and took 2-21 s on Alchemy, so one header is reused for HEADER_MAX_AGE_LEDGERS and refreshed
  // in the background once it is half that old; a missing or too old header is fetched before anything is prepared.
  let refreshing = null;
  const fetchHeader = async reference => {
    const r = await rpc.getLedgers({startLedger: reference, pagination: {limit: 1}});
    assert.equal(r.ledgers.length, 1);
    const h = r.ledgers[0].headerXdr.header(); assert.equal(h.ledgerSeq(), reference);
    const fresh = {ledger: reference, baseFee: BigInt(h.baseFee()), baseReserve: BigInt(h.baseReserve()), closeTime: Number(h.scpValue().closeTime().toString())};
    if (!header || fresh.ledger > header.ledger) header = fresh;
    return fresh;
  };
  const networkParameters = async reference => {
    if (header && reference >= header.ledger && reference - header.ledger <= HEADER_MAX_AGE_LEDGERS) {
      if (reference - header.ledger > HEADER_MAX_AGE_LEDGERS / 2 && !refreshing) refreshing = fetchHeader(reference).catch(() => null).finally(() => { refreshing = null; });
      return header;
    }
    return fetchHeader(reference - 2); // margin for a lagging backend; a header up to two ledgers older carries the same parameters
  };
  const cli = injected.cli || ((args, input) => {
    cliPath ??= executable('stellar');
    try { return cp.execFileSync(cliPath, args, {input, encoding: 'utf8', timeout: 60000,
      env: cleanEnvironment(), stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 1024 * 1024}).trim(); }
    catch (_) { throw Error('Stellar CLI failed; output deliberately withheld'); }
  });
  async function prepare(plan, report) {
    const R = report.ledger;
    assert(Array.isArray(report.decisionLabels) && report.decisions && report.viewSource && report.planInput, 'v5 snapshot required');
    // Burst 1 (parallel).
    const [network, entries, events, params] = await Promise.all([rpc.getNetwork(),
      rpc.getLedgerEntries(accountKey(), ...names.map(configKey), instanceKey(config.receipt), instanceKey(config.vault), pendingKey(config.receipt), pendingKey(config.vault)),
      rpc.getEvents({startLedger: Math.max(1, R - 5), filters: [{type: 'contract', contractIds: [config.vault]}], limit: 1}), // margin: a lagging backend answers instead of erroring
      networkParameters(R)]);
    checkNetwork(network);
    assert(Number.isSafeInteger(entries.latestLedger) && Number.isSafeInteger(events.latestLedger));
    if (entries.latestLedger < R || events.latestLedger < R) throw soft('LEDGER_REGRESSED');
    const rows = byKind(entries.entries);
    assert.equal(rows.account.length, 1, 'keeper account missing');
    const a = parseAccount(rows.account[0].val.account());
    assert.equal(pendingOf(rows.contractData).length, 0, 'pending upgrade on the vault or receipt: refuse');
    const instances = rows.contractData.filter(isInstanceRow);
    assert.equal(instances.length, 2, 'code instance missing');
    for (const row of instances) {
      const id = S.Address.fromScAddress(row.val.contractData().contract()).toString();
      const role = id === config.receipt ? 'receipt' : id === config.vault ? 'vault' : null; assert(role, 'unexpected instance');
      assert.equal(row.val.contractData().val().instance().executable().wasmHash().toString('hex'), config.wasmSha256[role], 'executable identity mismatch');
    }
    assert.equal(rows.configSetting.length, names.length);
    const settings = new Map(rows.configSetting.map(row => { const v = row.val.configSetting(); return [v.switch().name, v]; }));
    assert(settings.get(names[0]).contractCompute().txMemoryLimit() <= LIMITS.memory, 'unmeasured higher network memory ceiling');
    assert(settings.get(names[1]).contractEvents().txMaxContractEventsSizeBytes() <= LIMITS.events, 'unmeasured higher network event ceiling');
    assert.equal(sha(settings.get(names[2]).toXDR()), config.costModelXdrSha256.cpu, 'CPU cost model changed; requalify');
    assert.equal(sha(settings.get(names[3]).toXDR()), config.costModelXdrSha256.memory, 'memory cost model changed; requalify');
    assert(/^[1-9][0-9]*$/.test(String(events.latestLedgerCloseTime)), 'ledger close time unavailable');
    const closeTime = Number(events.latestLedgerCloseTime);
    assert(params.ledger <= events.latestLedger && params.closeTime <= closeTime, 'ledger clock regressed');
    assert(uint(config.inclusionFeeStroops) >= params.baseFee, 'inclusion fee below current base fee');
    const minLedger = Math.max(R + 1, plan.minLedger || 0);
    const maxLedgerExclusive = plan.maxLedgerExclusive || R + STEP_WINDOW;
    let maxTime = closeTime + config.transactionTimeoutSeconds;
    if (!['stage_checkpoint_investment', 'stage_finalise', 'stage_emergency_pay', 'checkpoint_rewards'].includes(plan.method)) {
      const commands = Object.entries(report.reads).filter(([name]) => name.startsWith('commands:')).flatMap(([, row]) => row.value);
      const matches = commands.filter(row => String(row.id) === String(plan.args[0]));
      assert.equal(matches.length, 1, 'command deadline not uniquely present in coherent snapshot');
      const deadline = uint(matches[0].job.deadline); assert(deadline <= BigInt(Number.MAX_SAFE_INTEGER));
      maxTime = Math.min(maxTime, Number(deadline));
    }
    const args = plan.method === 'checkpoint_rewards' ? []
      : plan.method === 'stage_checkpoint_investment' ? [new S.Address(config.keeperPublicAddress).toScVal()]
      : [S.nativeToScVal(uint(plan.args[0]), {type: 'u64'})];
    const unsigned = new S.TransactionBuilder(new S.Account(config.keeperPublicAddress, a.sequence),
      {fee: String(config.inclusionFeeStroops), networkPassphrase: config.network})
      .addOperation(new S.Contract(plan.target).call(plan.method, ...args))
      .setLedgerbounds(minLedger, maxLedgerExclusive).setTimebounds(closeTime, maxTime).build();
    // Burst 2 (parallel): the step simulation and a re-read of every decision input.
    const [simulation, rereads] = await Promise.all([rpc.simulateTransaction(unsigned),
      rereadDecisions(rpc, config, report.decisionLabels, report.viewSource)]);
    const simulationLedger = simulation.latestLedger;
    assert(Number.isSafeInteger(simulationLedger), 'simulation ledger missing');
    if (simulationLedger < R || Object.values(rereads).some(row => row.ledger < R)) throw soft('LEDGER_REGRESSED');
    const unproven = knownAt(report.decisionLabels, report.decisions, rereads, simulationLedger);
    if (unproven) throw soft(unproven);
    // The inputs are now proven at L_sim; the planner must choose the same first step there.
    const again = planAt(report, simulationLedger, closeTime).plans[0];
    const same = again && again.method === plan.method && again.target === plan.target && canonicalValue(again.args) === canonicalValue(plan.args)
      && (plan.method !== 'stage_invest' || (again.minLedger === plan.minLedger && again.maxLedgerExclusive === plan.maxLedgerExclusive));
    if (!same) throw soft('PLAN_CHANGED_AT_SIMULATION_LEDGER');
    if (!(simulationLedger + 1 < maxLedgerExclusive)) throw soft('WINDOW_CLOSING');
    // From here a failure is a refusal, as v4's failures in a coherent snapshot ledger were.
    assert(S.rpc.Api.isSimulationSuccess(simulation), 'keeper simulation failed');
    assert(!simulation.restorePreamble, 'restoration requires separate operator maintenance');
    assert(simulation.result, 'missing invocation result');
    const auth = simulation.result.auth;
    if (plan.method === 'stage_checkpoint_investment') {
      assert.equal(auth.length, 1, 'checkpoint requires exactly one source authorization');
      assert.equal(auth[0].credentials().switch().name, 'sorobanCredentialsSourceAccount');
      const root = auth[0].rootInvocation(); assert.equal(root.subInvocations().length, 0, 'unexpected nested keeper authorization');
      assert.equal(root.function().switch().name, 'sorobanAuthorizedFunctionTypeContractFn');
      assert.equal(root.function().contractFn().toXDR('base64'), unsigned.operations[0].func.invokeContract().toXDR('base64'));
    } else assert.equal(auth.length, 0, 'permissionless step unexpectedly requires account authorization');
    // checkpoint_rewards returns false when the venue claim fails; that transaction would seal nothing.
    if (plan.method === 'checkpoint_rewards') assert(simulation.result.retval.switch().name === 'scvBool' && simulation.result.retval.b() === true,
      'checkpoint_rewards would not claim (venue claim unavailable); nothing is prepared');
    const tx = S.rpc.assembleTransaction(unsigned, simulation).build();
    assert.equal(tx.signatures.length, 0); assert.equal(tx.operations.length, 1);
    const data = tx.toEnvelope().v1().tx().ext().sorobanData();
    assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'automatic restoration forbidden');
    const resources = data.resources(), fp = resources.footprint();
    const keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
    assert.equal(new Set(keys).size, keys.length, 'duplicate footprint keys');
    assert.equal(uint(tx.fee), uint(config.inclusionFeeStroops) + uint(data.resourceFee().toString()), 'assembled fee differs from explicit bid + resource fee');
    assert(uint(data.resourceFee().toString()) >= uint(simulation.minResourceFee));
    return {ledger: R, simulationLedger, closeTime, closeTimeLedger: events.latestLedger, headerLedger: params.ledger, source: config.keeperPublicAddress,
      accountSequence: a.sequence, sequence: tx.sequence, hash: tx.hash().toString('hex'), unsignedXdr: tx.toXDR(),
      minLedger, maxLedgerExclusive, maxTime, maxFeeStroops: tx.fee,
      inclusionFeeStroops: String(config.inclusionFeeStroops), balanceStroops: a.balance,
      sellingLiabilitiesStroops: a.selling, minimumReserveStroops: String(params.baseReserve * 2n),
      memoryAndEventsEnforcedByNetwork: true,
      metrics: {instructions: resources.instructions(), keys: keys.length, writes: fp.readWrite().length,
        readBytes: resources.diskReadBytes(), writeBytes: resources.writeBytes(),
        signedEnvelopeBytes: Buffer.from(tx.toXDR(), 'base64').length + 72}}; // One Ed25519 decorated signature.
  }
  return {
    now: () => Math.floor(Date.now() / 1000), inspect: c => inspect(c, rpc), assertNetwork,
    runtimeBindings: () => { const pins = runtimeBindings(); if (!injected.cli) { cliPath ??= executable('stellar'); assert.equal(pins.stellarCli.executable, cliPath); } return pins; },
    publicAddress: signer => cli(['keys', 'public-key', signer.alias, '--config-dir', signer.configDirectory, '--quiet', '--no-cache']),
    prepare,
    assertFresh: async (prepared, {preJournal = false, lastLedgerBudget = false} = {}) => {
      // v4 rule (account unchanged, at least one ledger left), with ledger and account from ONE response (v5).
      const started = monotonicMs();
      const [network, response] = await Promise.all([rpc.getNetwork(), rpc.getLedgerEntries(accountKey())]);
      checkNetwork(network);
      const latest = response.latestLedger; assert(Number.isSafeInteger(latest));
      if (!(latest >= prepared.ledger && latest + 1 < prepared.maxLedgerExclusive)) {
        if (preJournal) throw soft(latest < prepared.ledger ? 'LEDGER_REGRESSED' : 'WINDOW_CLOSING');
        assert.fail('submission window nearly closed; refresh');
      }
      if (preJournal && lastLedgerBudget && latest + 2 === prepared.maxLedgerExclusive && monotonicMs() - started > LAST_LEDGER_MAX_FRESH_MS) throw soft('WINDOW_CLOSING');
      assert.equal(response.entries.length, 1, 'keeper account missing');
      const a = parseAccount(response.entries[0].val.account());
      assert.equal(a.sequence, prepared.accountSequence); assert.equal(a.balance, prepared.balanceStroops); assert.equal(a.selling, prepared.sellingLiabilitiesStroops);
    },
    sign: async prepared => {
      const text = cli(['tx', 'sign', '--sign-with-key', config.signer.alias, '--config-dir', config.signer.configDirectory,
        '--rpc-url', config.rpcUrl ?? PUBLIC_SIGNING_RPC, '--network-passphrase', config.network, '--quiet', '--no-cache'], prepared.unsignedXdr);
      const signed = S.TransactionBuilder.fromXDR(text, config.network);
      assert.equal(signed.hash().toString('hex'), prepared.hash, 'signer changed transaction body');
      assert.equal(signed.signatures.length, 1);
      assert(S.Keypair.fromPublicKey(config.keeperPublicAddress).verify(signed.hash(), signed.signatures[0].signature()), 'wrong signer');
      assert(Buffer.from(signed.toXDR(), 'base64').length <= LIMITS.transactionBytes);
      return signed;
    },
    submit: signed => rpc.sendTransaction(signed),
    reconcile: async record => {
      const result = await rpc.getTransaction(record.hash);
      if (!['SUCCESS', 'FAILED'].includes(result.status)) return {status: 'UNKNOWN'};
      assert(result.envelopeXdr && result.resultXdr, 'confirmed transaction metadata missing');
      const tx = S.TransactionBuilder.fromXDR(result.envelopeXdr.toXDR('base64'), config.network);
      assert.equal(tx.hash().toString('hex'), record.hash);
      return {status: result.status, hash: record.hash, source: tx.source, sequence: tx.sequence,
        ledger: result.ledger, feeChargedStroops: result.resultXdr.feeCharged().toString()};
    },
    // v6: contracts (vault/receipt) with a pending upgrade proposal; [] when none.
    pendingUpgrades: async () => pendingOf(byKind((await rpc.getLedgerEntries(pendingKey(config.receipt), pendingKey(config.vault))).entries).contractData),
    // v5 (loop): public head ledger without getLatestLedger (slow on Alchemy), and a read-only header warm-up.
    headLedger: async () => (await rpc.getLedgerEntries(instanceKey(config.vault))).latestLedger,
    warmNetworkParameters: async () => { const latest = (await rpc.getLedgerEntries(instanceKey(config.vault))).latestLedger;
      if (!(header && latest >= header.ledger && latest - header.ledger <= HEADER_MAX_AGE_LEDGERS / 2)) await fetchHeader(latest - 1); },
  };
}

async function main() {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === '--runtime-provenance') {
    console.log(stringify({sourceSha256: sourceBindings(), runtimeProvenance: runtimeBindings()})); return;
  }
  if (args.includes('--help')) {
    console.log('Usage: node keeper-worker-v6.cjs <public-config.json> [--execute --approval <approved.json> | --reconcile]\nPublic runtime pins only: --runtime-provenance\nDefault: public read-only inspection. Explicit execution advances at most one step, or only reconciles a pending hash. --reconcile only reads public transaction status and updates the original journal; it cannot sign or submit and does not need a renewed execution approval.\nRequired execution config: status=CONFIGURED_FOR_PILOT_EXECUTION, readOnly=false, dedicated keeperPublicAddress, signer={alias,storage:"os-secure-store",configDirectory}, stateDirectory, protocolVersion=29, costModelXdrSha256={cpu,memory}, feeBudgetStroops, feeFloorXlm, maxTransactionFeeStroops, inclusionFeeStroops, inclusionFeeCapStroops, maxSnapshotAgeSeconds<=30, transactionTimeoutSeconds<=60.\nApproval: status=OPERATOR_APPROVED, scope=cl-vault-b-keeper-v6, configDigestSha256, exact sourceSha256 and runtimeProvenance from read-only report, expiresAt, secureStoreProvisioning={alias,publicAddress,configDirectory,reviewed:true}.\nUnknown/failed outcomes and stale locks require operator review. No automatic expiry release, restoration, fee bumps, public-owner indexing, cron, or TTL maintenance. No signed XDR is written or logged. Memory/event ceilings are enforced by simulation under verified network limits, not represented as independently measured RPC metrics.');
    return;
  }
  assert(args[0] && !args[0].startsWith('--'), 'public configuration file required');
  const execute = args.includes('--execute');
  const reconcileOnly = args.includes('--reconcile');
  assert(!(execute && reconcileOnly), 'choose execution or read-only reconciliation');
  const approvalAt = args.indexOf('--approval');
  const allowed = new Set([args[0], '--execute', '--reconcile', '--approval', ...(approvalAt >= 0 ? [args[approvalAt + 1]] : [])]);
  assert(args.every(arg => allowed.has(arg)), 'unknown worker option');
  assert(!execute || approvalAt >= 0 && args[approvalAt + 1], '--execute requires a separate approval file');
  assert(execute || approvalAt < 0, '--approval without --execute is not supported');
  const config = JSON.parse(fs.readFileSync(args[0]));
  const approval = execute ? JSON.parse(fs.readFileSync(args[approvalAt + 1])) : null;
  console.log(stringify(await runSingleStep(config, {execute, reconcileOnly, approval})));
}
if (require.main === module) main().catch(() => { console.error('Keeper refused or halted. Inspect the public configuration and journal; no sensitive CLI/RPC output is printed.'); process.exitCode = 1; });
module.exports = {RETIRED_CONTRACTS, EARLIER_STATE_DIRECTORIES, assertNoPlaceholder, soft, softStatus, SOFT_CODES, HEADER_MAX_AGE_LEDGERS, LAST_LEDGER_MAX_FRESH_MS, OBSERVATION_WINDOW, STEP_WINDOW, rpcUrlOf, HOST_STOP, AT_LIMIT, cpuLabel, METHODS, RELEASE_METHODS, runSingleStep, validateConfig, validateApproval, validatePlan, validatePrepared,
  productionDependencies, configDigest, sourceBindings, runtimeBindings, LIMITS};
