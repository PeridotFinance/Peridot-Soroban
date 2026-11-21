// D3 heartbeat: while the normal checkpoint is BLOCKED, send receipt `checkpoint_nav()` on a fixed timer
// (default 12 s from the last SUBMIT), independent of the plan loop, without waiting for inclusion.
// It uses its own fee-only source account (checkpoint_nav needs no auth), so it never shares a sequence
// number with the liquidator's CLI sends. Keys stay in the Stellar CLI: the transaction is built and
// simulated here, signed by `stellar tx sign --sign-with-key <alias>`, and submitted by RPC.
// Every submission journals an intent WITH its hash; inclusion is polled and journalled as a result.

export const HEARTBEAT_PERIOD_MS = 12_000;
export const OBS_MAX_AGE_MS = 60_000;

/**
 * Pure timer logic (unit-tested). Due when the latest observation says the checkpoint is blocked (a
 * chain may or may not exist yet: the first blocked observation starts it), that observation is not
 * older than OBS_MAX_AGE_MS, nothing is mid-submit, and >= periodMs passed since the last SUBMIT.
 */
export function heartbeatDue({blocked, obsAtMs, nowMs, lastSubmitMs, submitting, periodMs = HEARTBEAT_PERIOD_MS}) {
  if (!blocked || submitting) return false;
  if (obsAtMs == null || nowMs - obsAtMs > OBS_MAX_AGE_MS) return false;
  return lastSubmitMs == null || nowMs - lastSubmitMs >= periodMs;
}

/** Gap statistics (s) over a sorted list of ledger close times of included heartbeats. */
export function gapStats(times) {
  const g = []; for (let i = 1; i < times.length; i++) g.push(times[i] - times[i - 1]);
  if (!g.length) return {n: 0};
  const s = [...g].sort((a, b) => a - b), q = p => s[Math.min(s.length - 1, Math.ceil(p * s.length) - 1)];
  return {n: g.length, min: s[0], p50: q(0.5), p95: q(0.95), max: s.at(-1), over20: g.filter(x => x > 20).length, over60: g.filter(x => x > 60).length};
}

/**
 * makeHeartbeat({S, rpc, cfg, journal, sign, out, clock}) -> {observe(blocked), tick(), stop(), stats()}
 * `sign(xdr) -> Promise<signedXdr>` (CLI). `observe` is called by the plan loop after each observation.
 */
export function makeHeartbeat({S, rpc, cfg, journal, sign, out, clock = () => Date.now()}) {
  const st = {blocked: false, obsAtMs: null, lastSubmitMs: null, submitting: false, seq: null, pending: new Map(), included: [], retryAtMs: null};
  const period = cfg.heartbeatPeriodMs ?? HEARTBEAT_PERIOD_MS, fee = String(cfg.heartbeatFeeStroops ?? 10_000), RETRY_MS = 2_000;
  async function submitOne() {
    st.submitting = true;
    const t0 = clock();
    try {
      if (st.seq == null) st.seq = BigInt((await rpc.getAccount(cfg.heartbeatAccount)).sequenceNumber());
      const acct = new S.Account(cfg.heartbeatAccount, String(st.seq));
      let tx = new S.TransactionBuilder(acct, {fee, networkPassphrase: cfg.networkPassphrase})
        .addOperation(new S.Contract(cfg.contracts.receipt).call('checkpoint_nav')).setTimeout(45).build();
      const sim = await rpc.simulateTransaction(tx);
      if (!S.rpc.Api.isSimulationSuccess(sim)) { out({level: 'warn', msg: 'heartbeat simulation failed', err: String(sim.error).split('\n')[0]}); return; }
      tx = S.rpc.assembleTransaction(tx, sim).build();
      const signed = S.TransactionBuilder.fromXDR(await sign(tx.toXDR()), cfg.networkPassphrase);
      const hash = signed.hash().toString('hex');
      const seq0 = journal.intent('HB', 'L1a', null, {call: {contract: cfg.contracts.receipt, fn: 'checkpoint_nav', args: {}}, hash, source: cfg.heartbeatAccount});
      const r = await rpc.sendTransaction(signed);
      if (r.status === 'ERROR' || r.status === 'TRY_AGAIN_LATER') {
        // Live Testnet soak: TRY_AGAIN_LATER (queue full / earlier tx still pending) followed by a sequence
        // resync produced txBAD_SEQ ERRORs and a 65-70 s gap. Keep the local sequence on TRY_AGAIN_LATER and
        // retry after RETRY_MS; resync only on ERROR.
        let code = null; try { code = r.errorResult?.result().switch().name ?? null; } catch {}
        if (r.status === 'ERROR') st.seq = null;
        st.retryAtMs = clock() + RETRY_MS;
        journal.result(seq0, 'HB', 'L1a', null, false, {hash, sendStatus: r.status, code});
        out({level: 'warn', msg: 'heartbeat send rejected', status: r.status, code, hash});
        return;
      }
      st.seq += 1n; st.lastSubmitMs = t0; st.pending.set(hash, {seq0, at: t0});
      out({level: 'info', msg: 'heartbeat submitted', hash, sendMs: clock() - t0});
    } catch (e) { st.seq = null; st.retryAtMs = clock() + RETRY_MS; out({level: 'warn', msg: 'heartbeat failed', err: e.message.slice(0, 200)}); }
    finally { st.submitting = false; }
  }
  async function poll() {
    for (const [hash, p] of st.pending) {
      try {
        const g = await rpc.getTransaction(hash);
        if (g.status === 'NOT_FOUND') { if (clock() - p.at > 90_000) { st.pending.delete(hash); journal.result(p.seq0, 'HB', 'L1a', null, false, {hash, status: 'NOT_FOUND_90s'}); } continue; }
        st.pending.delete(hash);
        const ok = g.status === 'SUCCESS', closeTime = Number(g.createdAt);
        if (ok) st.included.push(closeTime);
        journal.result(p.seq0, 'HB', 'L1a', closeTime, ok, {hash, ledger: g.ledger, retval: ok && g.returnValue ? String(S.scValToNative(g.returnValue)) : null});
      } catch (e) { out({level: 'warn', msg: 'heartbeat poll failed', err: e.message}); }
    }
  }
  let timer = null;
  return {
    observe(blocked) { st.blocked = !!blocked; st.obsAtMs = clock(); },
    async tick() {
      const now = clock();
      if ((st.retryAtMs == null || now >= st.retryAtMs) && heartbeatDue({blocked: st.blocked, obsAtMs: st.obsAtMs, nowMs: now, lastSubmitMs: st.lastSubmitMs, submitting: st.submitting, periodMs: period})) { st.retryAtMs = null; submitOne(); }
      if (st.pending.size) await poll();
    },
    start(ms = 500) { timer = setInterval(() => { this.tick().catch(e => out({level: 'warn', msg: 'heartbeat tick failed', err: e.message})); }, ms); return this; },
    stop() { if (timer) clearInterval(timer); },
    stats() { return gapStats(st.included); },
    state: st,
  };
}
