// Candidate policy, not a production risk-parameter approval. No floating-point
// prices, parity fallback, missing-bucket filling or AMM-derived SDEX prices.
export const SCALE = 1_000_000_000_000n;
export const POLICY = Object.freeze({ bucketSeconds:300, windowSeconds:1800, maxAgeSeconds:300,
  minBucketTrades:3, minBucketBaseRaw:100_000_000n, minTotalBaseRaw:10_000_000_000n,
  maxDeviationBps:100n, minRatio:800_000_000_000n, maxRatio:1_050_000_000_000n });
const ensure = (condition, reason) => { if (!condition) throw new Error(reason); };
export function raw7(value) {
  ensure(typeof value === 'string' && /^(0|[1-9][0-9]*)(\.[0-9]{1,7})?$/.test(value) && value.length <= 40, 'invalid decimal amount');
  const [whole, fraction=''] = value.split('.');
  return BigInt(whole)*10_000_000n + BigInt(fraction.padEnd(7,'0'));
}
export function near(a,b,bps) { return a > 0n && b > 0n && (a>b?a-b:b-a)*10_000n <= b*bps; }
export function windowPrice(records, end, now, policy=POLICY) {
  ensure(Number.isSafeInteger(end) && Number.isSafeInteger(now) && end % policy.bucketSeconds === 0
    && end <= now && now-end <= policy.maxAgeSeconds, 'stale/future window');
  const start=end-policy.windowSeconds, count=policy.windowSeconds/policy.bucketSeconds;
  ensure(Array.isArray(records) && records.length === count, 'incomplete window');
  const ratios=[]; let volume=0n;
  for (let i=0;i<count;i++) {
    const r=records[i], ts=Number(r.timestamp), trades=Number(r.trade_count);
    ensure(Number.isSafeInteger(ts) && ts === (start+i*policy.bucketSeconds)*1000, 'duplicate/missing/misaligned bucket');
    ensure(Number.isSafeInteger(trades) && trades>=policy.minBucketTrades, 'insufficient trades');
    const base=raw7(r.base_volume), counter=raw7(r.counter_volume);
    ensure(base>=policy.minBucketBaseRaw && counter>0n, 'insufficient bucket volume');
    const ratio=counter*SCALE/base;
    ensure(ratio>=policy.minRatio && ratio<=policy.maxRatio, 'ratio outside policy');
    ratios.push(ratio); volume+=base;
  }
  ensure(volume>=policy.minTotalBaseRaw,'insufficient window volume');
  // Equal-duration bucket VWAPs form the time-windowed price; do not let one
  // high-volume instant dominate the complete half-hour observation.
  const ratio=ratios.reduce((a,b)=>a+b,0n)/BigInt(count);
  ensure(ratios.every(r=>near(r,ratio,policy.maxDeviationBps)),'unstable window');
  return {ratio,start:BigInt(start),end:BigInt(end),baseVolumeRaw:volume};
}
export function crossCheck(report, sell, buy, probe, policy=POLICY) {
  ensure(typeof sell==='bigint' && typeof buy==='bigint' && typeof probe==='bigint'
    && sell>0n && buy>0n && probe>0n,'missing executable quote');
  ensure(near(sell*SCALE/probe,report.ratio,policy.maxDeviationBps)
    && near(probe*SCALE/buy,report.ratio,policy.maxDeviationBps),'Aquarius divergence');
  return report;
}

// Contract state, not local cache, is the restart/replay authority.
export async function runObservationCycle({collect, quotes, readPrevious, publish, invalidate, now}) {
  const previous=await readPrevious();
  let report;
  try {
    report=await collect();
    const q=await quotes(); crossCheck(report,q.sell,q.buy,q.probe);
    ensure(report.end <= BigInt(now()) && BigInt(now())-report.end <= BigInt(POLICY.maxAgeSeconds), 'observation expired while collecting');
  } catch (error) {
    // Do not keep moving the invalidation watermark on every failed poll: a
    // later complete healthy window must be able to recover normally.
    if (previous?.valid) await invalidate();
    return {state:'halted',reason:error.message};
  }
  if (previous && (report.end<=previous.end || report.end<=previous.invalidated_at)) {
    return {state:'waiting',reason:'newer complete window required'};
  }
  // Submission exceptions are not swallowed or retried: unknown transaction
  // state must stop the process and be reconciled by its public hash.
  await publish(report);
  return {state:'published',...report};
}
