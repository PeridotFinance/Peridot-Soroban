'use strict';
// Probe tests. The chain is a fake that applies the frozen vault's counting rule (vault.rs:147-153) to landed
// observations. No keys exist here: the "signer" returns the unsigned envelope and verification is injected.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const P = require('../keeper-streak-probe.cjs');

const guardVal = g => S.xdr.ScVal.scvMap([
  new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('healthy_count'), val: S.nativeToScVal(g.healthy_count, {type: 'u32'})}),
  new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('inside'), val: S.xdr.ScVal.scvBool(g.inside)}),
  new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('observed_ledger'), val: S.nativeToScVal(g.observed_ledger, {type: 'u32'})}),
  new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol('observed_tick'), val: S.nativeToScVal(g.tick ?? -2, {type: 'i32'})})]);

// delays: per submission, how many ledgers after the "current" ledger it lands (1 = next ledger); null = never lands.
function fakeChain({delays = [], inside = () => true, ledgerMs = 5000, balance = 199400000n, openJobs = 0} = {}) {
  let t = 1794830000000, seq = 1000n, guard = null, sends = 0;
  const pending = new Map(), done = new Map();
  const ledger = () => 65510000 + Math.floor((t - 1794830000000) / ledgerMs);
  const settle = () => {
    for (const [h, p] of [...pending]) if (ledger() >= p.at) {
      pending.delete(h);
      if (p.at >= p.max || BigInt(p.tx.sequence) !== seq + 1n) { continue; }   // outside bounds / bad sequence: never applied
      seq += 1n; const L = p.at, ins = inside(L);
      if (guard) assert(L > guard.observed_ledger, 'one observation per ledger');
      const count = !ins ? 0 : (guard && guard.inside && L === guard.observed_ledger + 1) ? Math.min(guard.healthy_count + 1, 3) : 1;
      guard = {observed_ledger: L, healthy_count: count, inside: ins};
      done.set(h, {status: 'SUCCESS', ledger: L, returnValue: guardVal(guard), resultXdr: {feeCharged: () => ({toString: () => '330000'})}});
    }
  };
  const chain = {
    now: () => t, sleep: async ms => { t += ms; settle(); },
    latest: async () => { settle(); return {sequence: ledger(), closeTime: String(Math.floor(t / 1000))}; },
    network: async () => ({passphrase: S.Networks.PUBLIC, protocolVersion: 29}),
    feeStats: async () => ({sorobanInclusionFee: {min: '100', p10: '100', p50: '200', p90: '500000', p95: '1000000', p99: '5049045'}}),
    account: async () => { settle(); return {sequence: seq, balance, subEntries: 0, signers: 0, ledger: ledger()}; },
    // Like stellar-rpc: simulation runs AT the latest closed ledger's sequence, so the contract's
    // "one guard observation per ledger" panics while the last observation is in that very ledger.
    view: async (c, m) => {
      settle(); const L = ledger();
      if (m === 'stage_investment_keeper') return P.PINS.keeper;
      if (m === 'stage_investment_guard') return guard ? {...guard, observed_tick: -2} : null;
      if (m === P.METHOD) { if (guard && L <= guard.observed_ledger) throw Error(`rpc:view:${m}: one guard observation per ledger`); return {inside: inside(L), observed_ledger: L, observed_tick: inside(L) ? -2 : 25}; }
      return {open_jobs: openJobs};
    },
    codeHash: async () => P.PINS.vaultWasm,
    send: async tx => {
      const d = delays[sends++]; t += 150;   // submission round trip
      const h = tx.hash().toString('hex'), lb = tx.ledgerBounds;
      if (d !== null && d !== undefined) pending.set(h, {at: Math.max(ledger() + d, lb.minLedger), max: lb.maxLedger, tx});
      return {status: 'PENDING', hash: h};
    },
    status: async h => { settle(); return done.get(h) || {status: 'NOT_FOUND'}; },
    get guard() { return guard; }, get sends() { return sends; },
  };
  return chain;
}
function approval(dir, over = {}) {
  return {status: 'OPERATOR_APPROVED', scope: P.SCOPE, sourceSha256: P.sourceSha256(), expiresAt: Math.floor(1794830000000 / 1000) + 5 * 3600,
    keeperPublicAddress: P.PINS.keeper, vault: P.PINS.vault, receipt: P.PINS.receipt, methods: [P.METHOD],
    signer: {alias: 'peridot-pyusd-vaultb-keeper', configDirectory: '/x/.config/stellar'}, stateDirectory: dir,
    rpcEndpointFile: '/Users/joshua/Peridot/keeper-pyusd-vaultb/rpc-key', rpcHost: 'stellar-mainnet.g.alchemy.com', maxBandWaitSeconds: 1800, attempts: 20, maxObservationsPerAttempt: 6, maxTransactions: 120,
    maxFeePerTxStroops: '600000', totalFeeCapStroops: '50000000', balanceFloorStroops: '120000000', inclusionBidCapStroops: '10000', ...over};
}
const prepare = async () => ({sorobanData: new S.SorobanDataBuilder().setResourceFee('396968').build(), bid: 1000n, maxFee: 397968n, simulatedAt: 0, declaredInstructions: 3909301});
const verify = (text, tx) => S.TransactionBuilder.fromXDR(text, S.Networks.PUBLIC);
const sign = x => x;
const dir = t => { const d = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'streak-probe-'))); t.after(() => fs.rmSync(d, {recursive: true, force: true})); return d; };

test('build: window is [min, min+11), fee = bid + resource fee, source/sequence exact', () => {
  const tx = P.build(1001n, new S.SorobanDataBuilder().setResourceFee('396968').build(), 1000n, 500, 1794830000);
  assert.equal(tx.ledgerBounds.minLedger, 500); assert.equal(tx.ledgerBounds.maxLedger, 511);
  assert.equal(tx.fee, '397968'); assert.equal(tx.source, P.PINS.keeper); assert.equal(tx.sequence, '1001');
  assert.equal(tx.operations.length, 1); assert.equal(tx.operations[0].func.invokeContract().functionName().toString(), P.METHOD);
});

test('build: carries exactly the keeper source-account authorization for this call (regression: be05d42d failed unauthorized)', () => {
  const tx = P.build(1001n, new S.SorobanDataBuilder().setResourceFee('396968').build(), 1000n, 500, 1794830000);
  const auth = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().auth();
  assert.equal(auth.length, 1);
  assert.equal(auth[0].credentials().switch().name, 'sorobanCredentialsSourceAccount');
  const root = auth[0].rootInvocation();
  assert.equal(root.subInvocations().length, 0);
  assert.equal(root.function().contractFn().toXDR('base64'), tx.operations[0].func.invokeContract().toXDR('base64'));
});

test('build: matches the on-chain failed envelope except for the missing authorization', () => {
  // Envelope of be05d42d... (ledger 65511785, FAILED unauthorized). Same op body; it had zero auth entries.
  const failed = S.TransactionBuilder.fromXDR(fs.readFileSync(path.join(__dirname, 'fixtures/probe-failed-be05d42d.envelope.xdr'), 'utf8').trim(), S.Networks.PUBLIC);
  const fop = failed.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp();
  assert.equal(fop.auth().length, 0);
  const tx = P.build(BigInt(failed.sequence), failed.toEnvelope().v1().tx().ext().sorobanData(), 1000n, failed.ledgerBounds.minLedger, 1794830000);
  const op = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp();
  assert.equal(op.hostFunction().toXDR('base64'), fop.hostFunction().toXDR('base64'));
  assert.equal(op.auth().length, 1);
});

test('approval: scope, single method, caps and source hash are enforced', t => {
  const d = dir(t), now = Math.floor(1794830000000 / 1000);
  P.validateApproval(approval(d), now);
  for (const bad of [{scope: 'cl-vault-b-pilot-keeper-single-step'}, {methods: [P.METHOD, 'stage_invest']}, {maxTransactions: 121}, {attempts: 21},
    {totalFeeCapStroops: '50000001'}, {maxFeePerTxStroops: '1000001'}, {balanceFloorStroops: '49999999'}, {sourceSha256: '0'.repeat(64)},
    {expiresAt: now - 1}, {expiresAt: now + 7 * 3600}, {rpcEndpointFile: 'rel/path'}, {rpcHost: undefined}, {maxBandWaitSeconds: 10}, {status: 'DRAFT'}])
    assert.throws(() => P.validateApproval(approval(d, bad), now), undefined, JSON.stringify(bad));
});

test('every observation lands in the next ledger: 20/20 attempts pass, attempts independent', async t => {
  const d = dir(t), c = fakeChain({delays: Array(200).fill(1)});
  const s = await P.runProbe({approval: approval(d), chain: c, sign, prepare, verify, pollMs: 200});
  assert.equal(s.passes, 20); assert.equal(s.verdict, 'RELIABLE'); assert.equal(s.transactionsSent, 60);
  const j = JSON.parse(fs.readFileSync(path.join(d, 'probe-journal.json')));
  for (const a of j.attempts) { assert.deepEqual(a.counts, [1, 2, 3]); }
  for (let i = 1; i < j.attempts.length; i++) assert(j.attempts[i].landedLedgers[0] >= j.attempts[i - 1].landedLedgers.at(-1) + 2, 'attempts not independent');
});

test('a late landing restarts the count (never fails); streak still reached within 6', async t => {
  const d = dir(t), c = fakeChain({delays: [1, 2, 1, 1, ...Array(200).fill(1)]});
  const s = await P.runProbe({approval: approval(d, {attempts: 1}), chain: c, sign, prepare, verify});
  const j = JSON.parse(fs.readFileSync(path.join(d, 'probe-journal.json')));
  assert.deepEqual(j.attempts[0].counts, [1, 1, 2, 3]); assert.equal(s.passes, 1); assert.deepEqual(s.nextLedgerGapHistogram, {1: 2, 2: 1});
});

test('repeated misses: attempt fails after 6 observations', async t => {
  const d = dir(t), c = fakeChain({delays: Array(6).fill(2)});
  const s = await P.runProbe({approval: approval(d, {attempts: 1}), chain: c, sign, prepare, verify});
  assert.equal(s.passes, 0); assert.equal(s.verdict, 'NOT_RELIABLE'); assert.equal(s.transactionsSent, 6);
});

test('a transaction that never lands is proven expired, its pre-signed successor discarded, same sequence rebuilt', async t => {
  const d = dir(t), c = fakeChain({delays: [1, null, 1, 1, 1]});
  const s = await P.runProbe({approval: approval(d, {attempts: 1}), chain: c, sign, prepare, verify});
  const j = JSON.parse(fs.readFileSync(path.join(d, 'probe-journal.json')));
  const ex = j.txs.find(x => x.status === 'EXPIRED'); assert(ex && ex.expiryProof);
  const after = j.txs[j.txs.indexOf(ex) + 1]; assert.equal(after.sequence, ex.sequence, 'expired sequence must be reused');
  assert.equal(s.passes, 1); assert.equal(s.expired, 1);
});

test('band exit voids the attempt (not a failure); time out of band is measured read-only; a fresh attempt follows', async t => {
  const d = dir(t); let first = null;
  // In band for the first observation, out of band for the following ~4 ledgers, then in band again.
  const c = fakeChain({delays: Array(20).fill(1), inside: L => { first ??= L + 1; return !(L > first && L <= first + 4); } /* first query = read-only pre-check one ledger before the first landing */});
  const s = await P.runProbe({approval: approval(d, {attempts: 1}), chain: c, sign, prepare, verify});
  const j = JSON.parse(fs.readFileSync(path.join(d, 'probe-journal.json')));
  assert.equal(j.attempts[0].result, 'VOID_BAND'); assert.equal(j.attempts[1].result, 'PASS');
  assert.equal(s.voidedAttempts, 1); assert.equal(s.passes, 1); assert.equal(s.attemptsCompleted, 1);
  assert.equal(s.outOfBand.intervals, 1); assert(s.outOfBand.ledgers >= 3 && s.outOfBand.seconds >= 10, JSON.stringify(s.outOfBand));
  assert.equal(s.transactionsSent, 2 + 3, 'only the in-band and the voiding observation, then 3 for the fresh streak');
});
test('out of band before an attempt: nothing is signed while waiting; a timeout stops the probe', async t => {
  const d = dir(t), c = fakeChain({delays: Array(20).fill(1), inside: () => false});
  const s = await P.runProbe({approval: approval(d, {maxBandWaitSeconds: 60}), chain: c, sign, prepare, verify});
  const j = JSON.parse(fs.readFileSync(path.join(d, 'probe-journal.json')));
  assert.equal(s.transactionsSent, 0); assert.equal(j.stoppedReason, 'BAND_WAIT_TIMEOUT'); assert.equal(j.outOfBand[0].timedOut, true);
});
test('endpoint file: full https URL, owner-only, host must equal the approved host; the URL is never returned', t => {
  const d = dir(t), f = path.join(d, 'rpc'); fs.writeFileSync(f, 'https://stellar-mainnet.g.alchemy.com/v2/secretpart\n', {mode: 0o600});
  const e = P.endpoint(f, 'stellar-mainnet.g.alchemy.com'); assert.equal(e.label, 'stellar-mainnet.g.alchemy.com'); assert(!JSON.stringify({label: e.label}).includes('secretpart'));
  assert.throws(() => P.endpoint(f, 'other.example'), /host differs/);
  fs.chmodSync(f, 0o644); assert.throws(() => P.endpoint(f, 'stellar-mainnet.g.alchemy.com'), /readable only by its owner/);
  fs.chmodSync(f, 0o600); fs.writeFileSync(f, 'http://stellar-mainnet.g.alchemy.com/x'); assert.throws(() => P.endpoint(f, 'stellar-mainnet.g.alchemy.com'), /https URL/);
});
test('caps: transaction cap, total fee cap and balance floor stop the probe before sending', async t => {
  let d = dir(t), s = await P.runProbe({approval: approval(d, {maxTransactions: 4}), chain: fakeChain({delays: Array(50).fill(1)}), sign, prepare, verify});
  assert.equal(s.transactionsSent, 4);
  d = dir(t); s = await P.runProbe({approval: approval(d, {totalFeeCapStroops: '1000000'}), chain: fakeChain({delays: Array(50).fill(1)}), sign, prepare, verify});
  assert(BigInt(s.feeChargedStroops) <= 1000000n);
  d = dir(t); s = await P.runProbe({approval: approval(d), chain: fakeChain({delays: Array(50).fill(1), balance: 120100000n}), sign, prepare, verify});
  assert.equal(s.transactionsSent, 0);
});

test('an open command stops the probe (idle vault only)', async t => {
  const d = dir(t), c = fakeChain({delays: Array(10).fill(1), openJobs: 1});
  const s = await P.runProbe({approval: approval(d), chain: c, sign, prepare, verify});
  assert.equal(s.transactionsSent, 0);
});

test('a halted journal refuses to run again', async t => {
  const d = dir(t); fs.writeFileSync(path.join(d, 'probe-journal.json'), JSON.stringify({schema: 'cl-keeper-streak-probe-v1', attempts: [], txs: [], halted: 'FAILED abc'}));
  await assert.rejects(P.runProbe({approval: approval(d), chain: fakeChain(), sign, prepare, verify}), /halted earlier/);
});

test('band check right after a landed observation waits for the next ledger instead of halting (regression: run 2 halted rpc:view)', async () => {
  const chain = fakeChain({delays: [1, 1, 1]});
  const tx = P.build(1001n, new S.SorobanDataBuilder().setResourceFee('396968').build(), 1000n, 65510001, 1794830000);
  await chain.send(tx); await chain.sleep(5000);
  assert.equal(chain.guard.observed_ledger, 65510001);
  await assert.rejects(chain.view(P.PINS.vault, P.METHOD), /one guard observation per ledger/);   // the raw view does panic now
  const b = await P.inBand(chain);
  assert.equal(b.inside, true); assert(b.ledger > 65510001);
});

test('view errors keep the recognised contract reason but never the endpoint URL', async () => {
  const fakeRpc = {simulateTransaction: async () => ({error: 'HostError at https://stellar-mainnet.g.alchemy.com/v2/SECRETKEY: one guard observation per ledger'})};
  const c = P.makeChain(fakeRpc);
  await assert.rejects(c.view(P.PINS.vault, P.METHOD, [new S.Address(P.PINS.keeper).toScVal()]), e => /one guard observation per ledger/.test(e.message) && !/SECRETKEY|https?:/.test(e.message));
  const bad = P.makeChain({simulateTransaction: async () => { throw Error('connect ECONNREFUSED https://x/v2/SECRETKEY'); }});
  await assert.rejects(bad.view(P.PINS.vault, 'stage_state'), e => e.message === 'rpc:view:stage_state');
});

test('band check also retries a message-less trap while our own observation is that recent (lagging backend), then halts on persistent failure', async () => {
  const chain = fakeChain({delays: [1]}); let fails = 2;
  const tx = P.build(1001n, new S.SorobanDataBuilder().setResourceFee('396968').build(), 1000n, 65510001, 1794830000);
  await chain.send(tx); await chain.sleep(10000);
  const real = chain.view; chain.view = async (c, m, a) => { if (m === P.METHOD && fails-- > 0) throw Error(`rpc:view:${m}: UnreachableCodeReached`); return real(c, m, a); };
  assert.equal((await P.inBand(chain)).inside, true);
  chain.view = async (c, m, a) => { if (m === P.METHOD) throw Error(`rpc:view:${m}: UnreachableCodeReached`); return real(c, m, a); };
  await chain.sleep(60000);   // own observation no longer recent: an unexplained failure halts immediately
  await assert.rejects(P.inBand(chain), /UnreachableCodeReached/);
});
