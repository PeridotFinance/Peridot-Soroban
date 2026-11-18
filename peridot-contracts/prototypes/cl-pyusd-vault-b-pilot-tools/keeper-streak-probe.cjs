// Keeper streak probe (16 Nov 2026). Measures on Mainnet how often the dedicated keeper lands three
// `stage_checkpoint_investment` observations in CONSECUTIVE ledgers, as the frozen vault requires
// (vault.rs:150-153: count+1 only when ledger == previous.observed_ledger + 1; any gap restarts at 1).
//
// Operations fix only; no contract change. What differs from keeper-worker-v3:
//  * submission window [L+1, L+11) instead of one ledger (a late landing restarts the count, it never fails);
//  * simulation OFF the critical path: one simulation per attempt, resources reused with a 20% margin;
//  * the next observation is signed in advance and sent the moment the previous one is seen confirmed;
//  * inclusion bid max(1000, 5 x p10 of included Soroban inclusion fees), capped by the approval (refuses above it);
//  * a dedicated RPC (the operator's Alchemy Stellar mainnet endpoint, 16 Nov decision) whose full URL, including its
//    key, is read from a 600-permission file; only its pinned host is ever printed, journaled or approved.
//  * an observation outside the band VOIDS the attempt (operator decision 16 Nov); the time out of band is then measured
//    read-only (free simulations, nothing signed) and reported separately, because it limits vault utilisation.
//
// Modes (default is read-only):
//   methods  --rpc-file F --rpc-host H                            read-only: confirms the Soroban RPC methods the probe uses
//   latency  --rpc-file F --rpc-host H [--seconds N]              read-only ledger-detection lag on the endpoint
//   dry-run  --approval A.json                                    checks + one simulation + one UNSIGNED envelope
//   run      --approval A.json --execute                          the probe (keeper signs; needs the user's approval)
//   score    --state DIR                                          prints the tally from the journal
//
// Only call ever signed: vault.stage_checkpoint_investment(keeper), source = the keeper, source-account auth only.
// It moves no funds, needs no open command, and leaves only the vault's investment-guard record changed.
// Never resends an unknown hash; a transaction is re-built (same sequence) only after it is proven expired:
// latest ledger >= its maxLedgerExclusive AND NOT_FOUND on the RPC AND the keeper sequence did not advance.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const cp = require('node:child_process');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');

const SCOPE = 'cl-vault-b-keeper-streak-probe-v1';
const METHOD = 'stage_checkpoint_investment';
const WINDOW = 11;                 // ledgers a submission stays valid: [L+1, L+11)
const MARGIN_PCT = 20n;            // resource margin on the once-per-attempt simulation
const HOST_STOP = 97000000;        // user rule: refuse above 97M declared instructions
const PINS = Object.freeze({
  network: S.Networks.PUBLIC,
  vault: 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63',
  receipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI',
  keeper: 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4',
  admin: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  vaultWasm: 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9',
});
const stringify = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const sourceSha256 = () => sha(fs.readFileSync(__filename));
const uint = (v, label) => { assert(/^(0|[1-9][0-9]*)$/.test(String(v)), `${label} must be an unsigned integer`); return BigInt(v); };

// ---------------------------------------------------------------- approval
function validateApproval(a, now) {
  assert.equal(a?.status, 'OPERATOR_APPROVED', 'operator approval required');
  assert.equal(a.scope, SCOPE, 'wrong approval scope');
  assert.equal(a.sourceSha256, sourceSha256(), 'probe source changed after approval');
  assert(Number.isSafeInteger(a.expiresAt) && a.expiresAt > now, 'approval expired or absent');
  assert(a.expiresAt - now <= 6 * 3600, 'approval longer than 6 hours');
  assert.equal(a.keeperPublicAddress, PINS.keeper); assert.equal(a.vault, PINS.vault); assert.equal(a.receipt, PINS.receipt);
  assert.deepEqual(a.methods, [METHOD], 'only stage_checkpoint_investment is in scope');
  assert(/^[a-zA-Z][a-zA-Z0-9_-]{0,63}$/.test(a.signer?.alias) && path.isAbsolute(a.signer.configDirectory), 'secure-store alias required');
  assert(path.isAbsolute(a.stateDirectory), 'absolute probe state directory required');
  assert(path.isAbsolute(a.rpcEndpointFile ?? '') && /^[a-z0-9.-]+$/.test(a.rpcHost ?? ''), 'rpcEndpointFile (absolute) and rpcHost required');
  assert(Number.isSafeInteger(a.maxBandWaitSeconds) && a.maxBandWaitSeconds >= 60 && a.maxBandWaitSeconds <= 7200, 'maxBandWaitSeconds 60..7200');
  for (const [f, min, max] of [['attempts', 1, 20], ['maxObservationsPerAttempt', 3, 6], ['maxTransactions', 1, 120]]) {
    assert(Number.isSafeInteger(a[f]) && a[f] >= min && a[f] <= max, `approval ${f} out of range`);
  }
  const perTx = uint(a.maxFeePerTxStroops, 'maxFeePerTxStroops'), total = uint(a.totalFeeCapStroops, 'totalFeeCapStroops');
  const floor = uint(a.balanceFloorStroops, 'balanceFloorStroops'), bidCap = uint(a.inclusionBidCapStroops, 'inclusionBidCapStroops');
  assert(perTx > 0n && perTx <= 1000000n, 'per-transaction cap above 0.1 XLM');
  assert(total > 0n && total <= 50000000n, 'total cap above 5 XLM');
  assert(bidCap >= 1000n && bidCap <= 100000n && bidCap < perTx);
  assert(floor >= 50000000n, 'balance floor below 5 XLM');
}

// ---------------------------------------------------------------- RPC endpoint (key never leaves this closure)
function endpoint(file, host) {
  const st = fs.lstatSync(file);
  assert(st.isFile() && (st.mode & 0o077) === 0, 'RPC endpoint file must be a regular file readable only by its owner (chmod 600)');
  const text = fs.readFileSync(file, 'utf8').trim();
  let url; try { url = new URL(text); } catch (_) { throw Error('RPC endpoint file does not hold one https URL'); }
  assert(url.protocol === 'https:' && !url.username && !url.password && !url.hash && !/\s/.test(text), 'RPC endpoint file does not hold one https URL');
  assert.equal(url.host, host, 'RPC endpoint host differs from the approved host');
  return {rpc: new S.rpc.Server(text, {timeout: 20}), label: url.host};
}
// Errors from RPC/CLI may echo the URL or an envelope: callers only ever record a fixed category.
const quiet = async (f, category) => { try { return await f(); } catch (e) { throw e && e.keep ? e : Error(category); } };
const KNOWN_PANICS = ['one guard observation per ledger', 'fixed USD investment band', 'stage unbound', 'fresh token0 price required', 'fresh token1 price required', 'UnreachableCodeReached', 'InvalidAction'];
function simReason(text) { const t = String(text || ''); return KNOWN_PANICS.find(k => t.includes(k)) || 'simulation failed (reason not recognised)'; }

// ---------------------------------------------------------------- journal (append-only, fsynced)
function journal(dir) {
  fs.mkdirSync(dir, {recursive: true, mode: 0o700});
  assert.equal(fs.realpathSync(dir), dir, 'state directory must be canonical');
  const file = path.join(dir, 'probe-journal.json'), lock = path.join(dir, 'probe.lock');
  const fd = fs.openSync(lock, 'wx', 0o600); fs.writeFileSync(fd, String(process.pid)); fs.closeSync(fd);
  const j = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file)) : {schema: 'cl-keeper-streak-probe-v1', attempts: [], txs: [], halted: null};
  const save = () => { const t = file + '.tmp'; const f = fs.openSync(t, 'w', 0o600); fs.writeFileSync(f, stringify(j)); fs.fsyncSync(f); fs.closeSync(f); fs.renameSync(t, file); };
  return {j, save, release: () => fs.unlinkSync(lock)};
}
function spent(j) { return j.txs.reduce((n, t) => n + BigInt(t.feeChargedStroops ?? (['SUCCESS', 'FAILED', 'EXPIRED'].includes(t.status) ? 0 : t.maxFeeStroops)), 0n); }

// ---------------------------------------------------------------- chain reads
function makeChain(rpc, deps = {}) {
  const now = deps.now || (() => Date.now());
  return {
    now,
    sleep: deps.sleep || (ms => new Promise(r => setTimeout(r, ms))),
    latest: () => quiet(() => rpc.getLatestLedger(), 'rpc:getLatestLedger'),
    network: () => quiet(() => rpc.getNetwork(), 'rpc:getNetwork'),
    feeStats: () => quiet(() => rpc.getFeeStats(), 'rpc:getFeeStats'),
    account: () => quiet(async () => {
      const key = S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.xdr.AccountId.publicKeyTypeEd25519(S.StrKey.decodeEd25519PublicKey(PINS.keeper))}));
      const r = await rpc.getLedgerEntries(key); assert.equal(r.entries.length, 1);
      const a = r.entries[0].val.account();
      return {sequence: BigInt(a.seqNum().toString()), balance: BigInt(a.balance().toString()), subEntries: a.numSubEntries(), signers: a.signers().length, ledger: r.latestLedger};
    }, 'rpc:account'),
    view: (contract, method, args = []) => quiet(async () => {
      const tx = new S.TransactionBuilder(new S.Account(PINS.keeper, '0'), {fee: '100', networkPassphrase: PINS.network})
        .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
      const sim = await rpc.simulateTransaction(tx);
      // Keep the contract's reason (e.g. a panic message); never any URL (the endpoint file holds a key).
      if (!S.rpc.Api.isSimulationSuccess(sim)) throw Object.assign(Error(`rpc:view:${method}: ${simReason(sim.error)}`), {keep: true});
      return S.scValToNative(sim.result.retval);
    }, `rpc:view:${method}`),
    codeHash: contract => quiet(async () => {
      const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(contract).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
      const r = await rpc.getLedgerEntries(k); return r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');
    }, 'rpc:codeHash'),
    simulate: tx => quiet(() => rpc.simulateTransaction(tx), 'rpc:simulate'),
    send: tx => quiet(() => rpc.sendTransaction(tx), 'rpc:send'),
    status: hash => quiet(() => rpc.getTransaction(hash), 'rpc:getTransaction'),
  };
}

// ---------------------------------------------------------------- building and signing
function observationArgs() {
  return new S.xdr.InvokeContractArgs({contractAddress: new S.Address(PINS.vault).toScAddress(), functionName: METHOD, args: [new S.Address(PINS.keeper).toScVal()]});
}
// The single authorization the observation needs: the keeper (transaction source) authorizes exactly this call, nothing nested.
// 16 Nov fix: be05d42d... was sent WITHOUT this entry and failed on chain with "Unauthorized function call for address".
function sourceAuth() {
  return new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(observationArgs()), subInvocations: []})});
}
function observationOp(withAuth = true) {
  return S.Operation.invokeHostFunction({func: S.xdr.HostFunction.hostFunctionTypeInvokeContract(observationArgs()), auth: withAuth ? [sourceAuth()] : []});
}
function build(seq, sorobanData, bid, minLedger, nowSec) {
  const tx = new S.TransactionBuilder(new S.Account(PINS.keeper, String(seq - 1n)), {fee: String(bid), networkPassphrase: PINS.network, sorobanData})
    .addOperation(observationOp()).setLedgerbounds(minLedger, minLedger + WINDOW).setTimebounds(0, nowSec + 180).build();
  assert.equal(BigInt(tx.sequence), seq);
  assert.equal(BigInt(tx.fee), BigInt(bid) + BigInt(sorobanData.resourceFee().toString()), 'fee is not bid + resource fee');
  const auth = tx.toEnvelope().v1().tx().operations()[0].body().invokeHostFunctionOp().auth();
  assert.equal(auth.length, 1, 'observation must carry exactly one authorization entry');
  assert.equal(auth[0].toXDR('base64'), sourceAuth().toXDR('base64'), 'observation authorization differs from the keeper source-account entry');
  return tx;
}
// One simulation per attempt; the observation's footprint is fixed (vault instance, pool slot, keeper account).
async function prepareResources(chain, approval) {
  const latest = await chain.latest(), acc = await chain.account();
  const raw = new S.TransactionBuilder(new S.Account(PINS.keeper, acc.sequence.toString()), {fee: '1000', networkPassphrase: PINS.network})
    .addOperation(observationOp(false)).setTimeout(0).build();
  const sim = await chain.simulate(raw);
  assert(S.rpc.Api.isSimulationSuccess(sim), 'observation simulation failed');
  assert(!sim.restorePreamble, 'restoration needed: outside probe scope');
  const auth = sim.result.auth; assert.equal(auth.length, 1); assert.equal(auth[0].credentials().switch().name, 'sorobanCredentialsSourceAccount');
  assert.equal(auth[0].rootInvocation().subInvocations().length, 0, 'unexpected nested authorization');
  assert.equal(auth[0].toXDR('base64'), sourceAuth().toXDR('base64'), 'simulated authorization differs from the entry the probe attaches');
  const data = sim.transactionData.build(), res = data.resources();
  const cpu = BigInt(res.instructions()), withMargin = cpu * (100n + MARGIN_PCT) / 100n;
  assert(Number(withMargin) <= HOST_STOP, `declared instructions ${withMargin} above the 97M stop`);
  const fp = res.footprint(); assert(fp.readWrite().length <= 50 && fp.readOnly().length + fp.readWrite().length <= 100);
  const fee = BigInt(sim.minResourceFee) * (100n + MARGIN_PCT) / 100n;
  const builder = new S.SorobanDataBuilder(data.toXDR('base64')).setResources(Number(withMargin), res.diskReadBytes(), res.writeBytes()).setResourceFee(fee.toString());
  // Bid from p10 of the inclusion fees actually INCLUDED over the last ~50 ledgers. High percentiles reflect bots
  // overbidding (16 Nov: p95 went 200 -> 1,000,000 while min stayed 100, i.e. ledgers were not full). Under real
  // congestion every included fee is at least the clearing price, so p10 rises with it; above the approved bid cap
  // the probe refuses instead of chasing the surge. Stellar charges the effective price, not the bid.
  const stats = await chain.feeStats(), p10 = BigInt(stats.sorobanInclusionFee.p10);
  let bid = p10 * 5n; if (bid < 1000n) bid = 1000n;
  assert(bid <= BigInt(approval.inclusionBidCapStroops), `inclusion bid ${bid} above approved cap`);
  const total = bid + fee; assert(total <= BigInt(approval.maxFeePerTxStroops), `transaction fee ${total} above approved per-transaction cap`);
  const guard = S.scValToNative(sim.result.retval);
  return {feeStats: Object.fromEntries(['min', 'p10', 'p50', 'p90', 'p95', 'p99'].map(k => [k, stats.sorobanInclusionFee[k]])), sorobanData: builder.build(), bid, maxFee: total, simulatedAt: latest.sequence, declaredInstructions: Number(withMargin), simulatedGuard: guard};
}
function cliSigner(signer, network) {
  return xdr => {
    const cli = '/Users/joshua/.local/bin/stellar';
    let text;
    try { text = cp.execFileSync(cli, ['tx', 'sign', '--sign-with-key', signer.alias, '--config-dir', signer.configDirectory, '--rpc-url', 'https://soroban-rpc.mainnet.stellar.gateway.fm', '--network-passphrase', network, '--quiet'],
      {input: xdr, encoding: 'utf8', timeout: 30000, env: {PATH: process.env.PATH, HOME: process.env.HOME}, stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 1 << 20}).trim(); }
    catch (_) { throw Error('signer:cli-failed'); }
    return text;
  };
}
function verifySigned(text, unsigned) {
  const signed = S.TransactionBuilder.fromXDR(text, PINS.network);
  assert.equal(signed.hash().toString('hex'), unsigned.hash().toString('hex'), 'signer changed the transaction');
  assert.equal(signed.signatures.length, 1);
  assert(S.Keypair.fromPublicKey(PINS.keeper).verify(signed.hash(), signed.signatures[0].signature()), 'wrong signer');
  return signed;
}

// ---------------------------------------------------------------- one observation: submit, then follow to a final state
async function follow(chain, store, rec, signed, pollMs, afterSend = () => {}) {
  rec.status = 'SUBMITTING'; rec.sentAtMs = chain.now(); store.save();
  let sent;
  for (let i = 0; ; i++) {
    sent = await chain.send(signed);
    if (sent.status !== 'TRY_AGAIN_LATER' || i >= 5) break;   // same signed envelope, same hash: cannot apply twice
    await chain.sleep(100);
  }
  rec.sendStatus = sent.status; rec.sendReturnedAtMs = chain.now();
  if (sent.status === 'ERROR') {
    let code = 'unknown'; try { code = sent.errorResult.result().switch().name; } catch (_) { /* fixed category only */ }
    rec.sendErrorCode = code;
    // txBadSeq / txTooLate etc.: rejected before entering the queue -> never applied, sequence unchanged.
  } else if (!['PENDING', 'DUPLICATE', 'TRY_AGAIN_LATER'].includes(sent.status)) rec.sendErrorCode = 'unexpected:' + sent.status;
  store.save();
  afterSend();                                            // e.g. pre-sign the successor while this one is in flight
  while (true) {
    const st = await chain.status(rec.hash);
    if (st.status === 'SUCCESS' || st.status === 'FAILED') {
      rec.status = st.status; rec.ledger = st.ledger; rec.confirmedSeenAtMs = chain.now();
      try { rec.feeChargedStroops = st.resultXdr.feeCharged().toString(); } catch (_) { rec.feeChargedStroops = rec.maxFeeStroops; }
      if (st.status === 'SUCCESS') { const g = S.scValToNative(st.returnValue); rec.guard = {observed_ledger: Number(g.observed_ledger), healthy_count: Number(g.healthy_count), inside: g.inside, tick: Number(g.observed_tick)}; }
      store.save(); return rec;
    }
    const latest = await chain.latest();
    if (latest.sequence >= rec.maxLedgerExclusive) {
      // Expiry proof: past the window, still NOT_FOUND, and the keeper sequence did not move past this transaction.
      const again = await chain.status(rec.hash), acc = await chain.account();
      if (again.status === 'NOT_FOUND' && acc.sequence < BigInt(rec.sequence) && acc.ledger >= rec.maxLedgerExclusive) {
        rec.status = 'EXPIRED'; rec.feeChargedStroops = '0'; rec.expiryProof = {latest: acc.ledger, keeperSequence: String(acc.sequence)}; store.save(); return rec;
      }
      if (again.status === 'SUCCESS' || again.status === 'FAILED') continue;
      rec.status = 'UNKNOWN'; store.save(); return rec;
    }
    await chain.sleep(pollMs);
  }
}

// ---------------------------------------------------------------- one attempt: up to N observations, stop at count 3
async function attempt(ctx, index) {
  const {chain, store, approval, sign, pollMs} = ctx, j = store.j, verify = ctx.verify || verifySigned;
  const res = await (ctx.prepare || prepareResources)(chain, approval);
  const a = {index, startedAtMs: chain.now(), simulatedAt: res.simulatedAt, feeStats: res.feeStats, bid: String(res.bid), maxFeeStroops: String(res.maxFee),
    declaredInstructions: res.declaredInstructions, observations: [], result: null};
  j.attempts.push(a); store.save();
  // Independence: the first observation must land at least 2 ledgers after the previous attempt's last one.
  const prevLast = j.txs.filter(t => t.status === 'SUCCESS').map(t => t.ledger).sort((x, y) => y - x)[0] ?? 0;
  let latest = (await chain.latest()).sequence;
  while (latest < prevLast + 1) { await chain.sleep(pollMs); latest = (await chain.latest()).sequence; }
  let seq = (await chain.account()).sequence + 1n, minLedger = Math.max(latest + 1, prevLast + 2);
  const make = (s, min) => {
    const tx = build(s, res.sorobanData, res.bid, min, Math.floor(chain.now() / 1000));
    const t0 = chain.now(), signed = verify(sign(tx.toXDR()), tx);
    return {signed, rec: {attempt: index, hash: signed.hash().toString('hex'), sequence: String(s), minLedger: min, maxLedgerExclusive: min + WINDOW,
      maxFeeStroops: String(BigInt(signed.fee)), signMs: chain.now() - t0, status: 'SIGNED_NOT_SENT'}};
  };
  let next = make(seq, minLedger);
  while (a.observations.length < approval.maxObservationsPerAttempt) {
    if (j.txs.filter(t => t.status !== 'SIGNED_NOT_SENT').length >= approval.maxTransactions) { a.result = 'TX_CAP'; break; }
    if (spent(j) + BigInt(next.rec.maxFeeStroops) > BigInt(approval.totalFeeCapStroops)) { a.result = 'FEE_CAP'; break; }
    const acc = await chain.account();
    if (acc.balance - BigInt(next.rec.maxFeeStroops) < BigInt(approval.balanceFloorStroops)) { a.result = 'BALANCE_FLOOR'; break; }
    if (Math.floor(chain.now() / 1000) >= approval.expiresAt) { a.result = 'APPROVAL_EXPIRED'; break; }
    const cur = next; j.txs.push(cur.rec); store.save();          // journaled (hash, sequence, bounds) BEFORE submission
    // Pre-sign the following observation while this one is in flight (signing leaves the critical path).
    let pre = null;
    await follow(chain, store, cur.rec, cur.signed, pollMs, () => { pre = make(BigInt(cur.rec.sequence) + 1n, cur.rec.minLedger + 1); });
    a.observations.push(cur.rec.hash);
    if (cur.rec.status === 'FAILED' || cur.rec.status === 'UNKNOWN') { j.halted = `${cur.rec.status} ${cur.rec.hash}`; a.result = 'HALTED'; store.save(); return a; }
    if (cur.rec.status === 'EXPIRED') {
      // Never applied; the pre-signed successor (sequence+1) can never be valid now and is discarded unsent.
      const l = (await chain.latest()).sequence; next = make(BigInt(cur.rec.sequence), l + 1); continue;
    }
    const g = cur.rec.guard;
    if (!g.inside) { a.result = 'VOID_BAND'; a.bandExitLedger = cur.rec.ledger; break; }   // operator rule: void, not fail
    if (g.healthy_count >= 3) { a.result = 'PASS'; break; }
    // Successor must land in the ledger right after this one; its window starts there. If the pre-signed one's
    // window no longer covers ledger+1, rebuild (cannot happen with WINDOW 11 unless the RPC is very late).
    next = (pre && pre.rec.minLedger <= cur.rec.ledger + 1 && cur.rec.ledger + 1 < pre.rec.maxLedgerExclusive) ? pre : make(BigInt(cur.rec.sequence) + 1n, cur.rec.ledger + 1);
  }
  a.result ??= 'FAIL_NO_STREAK';
  a.landedLedgers = a.observations.map(h => j.txs.find(t => t.hash === h)).filter(t => t.status === 'SUCCESS').map(t => t.ledger);
  a.counts = a.observations.map(h => j.txs.find(t => t.hash === h)).filter(t => t.status === 'SUCCESS').map(t => t.guard.healthy_count);
  a.finishedAtMs = chain.now(); store.save(); return a;
}

function score(j, attempts = 20, passMark = 18) {
  const done = j.attempts.filter(a => a.result && a.result !== 'VOID_BAND');
  const pass = done.filter(a => a.result === 'PASS').length, voided = j.attempts.filter(a => a.result === 'VOID_BAND').length;
  const oob = j.outOfBand || [];
  const sent = j.txs.filter(t => !['SIGNED_NOT_SENT'].includes(t.status)), landed = sent.filter(t => t.status === 'SUCCESS');
  // Gap distribution: ledgers between a confirmed observation and the next one in the same attempt.
  const gaps = {}; for (const a of j.attempts) for (let i = 1; i < (a.landedLedgers || []).length; i++) { const g = a.landedLedgers[i] - a.landedLedgers[i - 1]; gaps[g] = (gaps[g] || 0) + 1; }
  return {attemptsCompleted: done.length, attemptsPlanned: attempts, passes: pass, passMark, verdict: done.length < attempts ? 'INCOMPLETE' : pass >= passMark ? 'RELIABLE' : 'NOT_RELIABLE',
    voidedAttempts: voided, outOfBand: {intervals: oob.length, ledgers: oob.reduce((n, x) => n + (x.ledgers || 0), 0), seconds: oob.reduce((n, x) => n + (x.seconds || 0), 0), detail: oob},
    transactionsSent: sent.length, landed: landed.length, expired: sent.filter(t => t.status === 'EXPIRED').length,
    feeChargedStroops: String(spent(j)), nextLedgerGapHistogram: gaps, halted: j.halted};
}

// Read-only band check: simulating the observation returns the guard the vault WOULD record, including `inside`.
// stellar-rpc simulates AT the latest closed ledger's sequence. Right after an observation lands in ledger L, simulating
// another one panics "one guard observation per ledger" until L+1 closes (run 2 on 16 Nov halted on exactly this).
// So: wait read-only until a ledger after the stored observation has closed; retry only that panic; bounded at 60 s.
async function inBand(chain) {
  for (let i = 0; ; i++) {
    const stored = await chain.view(PINS.vault, 'stage_investment_guard'), latest = (await chain.latest()).sequence;
    if (stored && latest <= Number(stored.observed_ledger)) { assert(i < 60, 'band check: no ledger closed after the last observation within 60 s'); await chain.sleep(1000); continue; }
    try {
      const g = await chain.view(PINS.vault, METHOD, [new S.Address(PINS.keeper).toScVal()]);
      return {inside: g.inside, ledger: Number(g.observed_ledger), tick: Number(g.observed_tick)};
    } catch (e) {
      // Release WASM often traps without the panic text, and backends behind one endpoint can lag each other by a ledger:
      // retry (read-only) while our own last observation is that recent; anything else halts as before.
      const recent = stored && latest - Number(stored.observed_ledger) <= 2;
      if (i < 60 && (/one guard observation per ledger/.test(e.message) || recent)) { await chain.sleep(1000); continue; }
      throw e;
    }
  }
}
// Returns null when already in band; otherwise waits until back in band (or timeout) and reports the interval.
async function bandWait(chain, approval, pollMs, knownExitLedger = null) {
  let b = await inBand(chain); if (b.inside && knownExitLedger === null) return null;
  const startMs = chain.now(), fromLedger = knownExitLedger ?? b.ledger, ticks = [b.tick]; let lastLedger = b.ledger;
  while (!b.inside) {
    if (chain.now() - startMs > approval.maxBandWaitSeconds * 1000) return {fromLedger, toLedger: null, seconds: Math.round((chain.now() - startMs) / 1000), ledgers: lastLedger - fromLedger, ticks: [Math.min(...ticks), Math.max(...ticks)], timedOut: true};
    await chain.sleep(Math.max(pollMs, 1000));
    b = await inBand(chain); ticks.push(b.tick); lastLedger = b.ledger;
  }
  return {fromLedger, toLedger: b.ledger, ledgers: b.ledger - fromLedger, seconds: Math.round((chain.now() - startMs) / 1000), ticks: [Math.min(...ticks), Math.max(...ticks)]};
}
async function runProbe({approval, chain, sign, pollMs = 200, prepare, verify}) {
  validateApproval(approval, Math.floor(chain.now() / 1000));
  const n = await chain.network(); assert.equal(n.passphrase, PINS.network); assert.equal(n.protocolVersion, 29, 'protocol changed: requalify');
  assert.equal(await chain.codeHash(PINS.vault), PINS.vaultWasm, 'vault code differs from the frozen pilot code');
  assert.equal(await chain.view(PINS.vault, 'stage_investment_keeper'), PINS.keeper, 'keeper not bound');
  const acc = await chain.account(); assert.equal(acc.subEntries, 0); assert.equal(acc.signers, 0);
  const store = journal(approval.stateDirectory);
  try {
    assert(!store.j.halted, `probe halted earlier: ${store.j.halted}; operator review required`);
    assert(!store.j.txs.some(t => ['SUBMITTING', 'UNKNOWN'].includes(t.status)), 'unresolved transaction in journal; reconcile first');
    store.j.outOfBand ??= [];
    while (store.j.attempts.filter(a => a.result && a.result !== 'VOID_BAND').length < approval.attempts) {
      const st = await chain.view(PINS.receipt, 'stage_state');
      if (Number(st.open_jobs) !== 0) { store.j.stoppedReason = 'OPEN_COMMAND_PRESENT'; store.save(); break; }   // scope: idle vault only
      // Never spend fees while the pool is out of band: wait read-only (and measure) first.
      const before = await bandWait(chain, approval, pollMs);
      if (before) { store.j.outOfBand.push({...before, when: 'before-attempt'}); store.save(); if (before.timedOut) { store.j.stoppedReason = 'BAND_WAIT_TIMEOUT'; store.save(); break; } }
      const a = await attempt({chain, store, approval, sign, pollMs, prepare, verify}, store.j.attempts.length + 1);
      if (a.result === 'VOID_BAND') {
        const after = await bandWait(chain, approval, pollMs, a.bandExitLedger);
        store.j.outOfBand.push({...(after || {fromLedger: a.bandExitLedger, toLedger: a.bandExitLedger, seconds: 0}), when: 'voided-attempt', attempt: a.index}); store.save();
        if (after?.timedOut) { store.j.stoppedReason = 'BAND_WAIT_TIMEOUT'; store.save(); break; }
      }
      if (['HALTED', 'TX_CAP', 'FEE_CAP', 'BALANCE_FLOOR', 'APPROVAL_EXPIRED'].includes(a.result)) break;
    }
    const s = score(store.j, approval.attempts); store.j.score = s; store.save(); return s;
  } finally { store.release(); }
}

// Read-only confirmation that the endpoint serves every Soroban RPC method the probe needs (sendTransaction is
// confirmed only by the first real submission; it cannot be exercised without submitting).
async function methods(rpc) {
  const out = {};
  const check = async (name, f) => { const t0 = Date.now(); try { const v = await f(); out[name] = {ok: true, ms: Date.now() - t0, ...(v || {})}; } catch (_) { out[name] = {ok: false, ms: Date.now() - t0}; } };
  await check('getNetwork', async () => { const n = await rpc.getNetwork(); assert.equal(n.passphrase, PINS.network); return {protocolVersion: n.protocolVersion}; });
  await check('getVersionInfo', async () => { const v = await rpc.getVersionInfo(); return {version: v.version, protocolVersion: v.protocolVersion ?? v.protocol_version}; });
  let latest; await check('getLatestLedger', async () => { latest = await rpc.getLatestLedger(); return {sequence: latest.sequence, hasCloseTime: latest.closeTime !== undefined}; });
  await check('getLedgers', async () => { const r = await rpc.getLedgers({startLedger: latest.sequence, pagination: {limit: 1}}); assert.equal(r.ledgers.length, 1); return {}; });
  await check('getFeeStats', async () => { const f = await rpc.getFeeStats(); return {sorobanP95: f.sorobanInclusionFee.p95, sorobanP99: f.sorobanInclusionFee.p99}; });
  const chain = makeChain(rpc);
  await check('getLedgerEntries', async () => { const a = await chain.account(); return {keeperSubEntries: a.subEntries}; });
  await check('simulateTransaction', async () => { const b = await inBand(chain); return {observationInBand: b.inside, tick: b.tick}; });
  await check('getTransaction', async () => { const r = await rpc.getTransaction('0'.repeat(64)); assert.equal(r.status, 'NOT_FOUND'); return {status: r.status}; });
  return {allOk: Object.values(out).every(x => x.ok), methods: out, sendTransaction: 'not exercised (would submit); confirmed by the first probe transaction'};
}
async function latency(chain, seconds) {
  const seen = new Map(), t0 = chain.now(); let errors = 0;
  while (chain.now() - t0 < seconds * 1000) {
    try { const l = await chain.latest(); if (!seen.has(l.sequence)) seen.set(l.sequence, {seenMs: chain.now(), closeTime: Number(l.closeTime)}); } catch (_) { errors++; }
    await chain.sleep(400);
  }
  const lags = [...seen.values()].filter(x => x.closeTime).map(x => x.seenMs / 1000 - x.closeTime).sort((a, b) => a - b);
  const q = p => lags.length ? Math.round(lags[Math.min(lags.length - 1, Math.floor(p * lags.length))] * 100) / 100 : null;
  return {ledgers: seen.size, errors, lagSeconds: {median: q(0.5), p90: q(0.9), p99: q(0.99), max: lags.length ? Math.round(lags.at(-1) * 100) / 100 : null},
    note: 'lag = first time this endpoint reported the ledger minus its close time (whole seconds, up to 1 s overstatement)'};
}

module.exports = {sourceAuth, inBand, bandWait, methods, endpoint, journal, cliSigner, verifySigned, SCOPE, METHOD, WINDOW, PINS, validateApproval, build, prepareResources, follow, attempt, runProbe, score, latency, makeChain, sourceSha256};

if (require.main === module) {
  const [mode, ...args] = process.argv.slice(2), opt = n => { const i = args.indexOf(n); return i < 0 ? null : args[i + 1]; };
  (async () => {
    if (mode === 'score') { const j = JSON.parse(fs.readFileSync(path.join(opt('--state'), 'probe-journal.json'))); return score(j); }
    if (mode === 'source-hash') return {sourceSha256: sourceSha256()};
    const approval = opt('--approval') ? JSON.parse(fs.readFileSync(opt('--approval'))) : null;
    const {rpc, label} = endpoint(opt('--rpc-file') || approval?.rpcEndpointFile, opt('--rpc-host') || approval?.rpcHost);
    const chain = makeChain(rpc);
    if (mode === 'methods') return {endpoint: label, ...(await methods(rpc))};
    if (mode === 'latency') return {endpoint: label, ...(await latency(chain, Number(opt('--seconds') || 300)))};
    if (mode === 'dry-run') {
      validateApproval({...approval, status: 'OPERATOR_APPROVED'}, Math.floor(Date.now() / 1000));
      const n = await chain.network(), res = await prepareResources(chain, approval), acc = await chain.account(), latest = await chain.latest();
      const tx = build(acc.sequence + 1n, res.sorobanData, res.bid, latest.sequence + 1, Math.floor(Date.now() / 1000));
      const st = await chain.view(PINS.receipt, 'stage_state');
      return {endpoint: label, protocol: n.protocolVersion, signing: false, submission: false, keeperBalanceStroops: String(acc.balance), openJobs: Number(st.open_jobs),
        declaredInstructions: res.declaredInstructions, bidStroops: String(res.bid), maxFeePerTxStroops: String(res.maxFee), simulatedGuard: res.simulatedGuard, unsignedHash: tx.hash().toString('hex')};
    }
    if (mode === 'run') {
      assert(args.includes('--execute'), 'run requires --execute');
      return runProbe({approval, chain, sign: cliSigner(approval.signer, PINS.network)});
    }
    throw Error('usage: methods | latency | dry-run | run --execute | score | source-hash');
  })().then(r => console.log(stringify(r)), e => { console.error('probe refused/halted: ' + (e.message || 'error').split('\n')[0].slice(0, 200)); process.exitCode = 1; });
}
