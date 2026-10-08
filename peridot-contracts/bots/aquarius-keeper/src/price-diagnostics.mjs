// Read-only historical availability analysis. Never imported by the publisher.
import assert from 'node:assert/strict';
import { POLICY, raw7, windowPrice } from './observation.mjs';
import { jsonGet } from './price-collector.mjs';

export const YXLM_ISSUER = 'GARDNV3Q7YGT4AKSDF25LT32YSCCW4EV22Y2TV3I2PU2MMXJTEDL5T55';
const BUCKET_MS = POLICY.bucketSeconds * 1000;

export async function history({ root, networkPassphrase, startMs, endMs, get = jsonGet }) {
  const origin = new URL(root);
  assert(origin.protocol === 'https:' && !origin.username && !origin.password && !origin.search && !origin.hash);
  assert.equal((await get(origin)).network_passphrase, networkPassphrase, 'Horizon network mismatch');
  assert(Number.isSafeInteger(startMs) && Number.isSafeInteger(endMs));
  assert(startMs % BUCKET_MS === 0 && endMs % BUCKET_MS === 0 && startMs < endMs);
  assert(endMs - startMs <= 7 * 86400000, 'bounded to seven days');
  const rows = [];
  // At most144 buckets per request, below Horizon's200-record page limit.
  // Never silently truncate a page or deduplicate inconsistent overlapping data.
  for (let start = startMs; start < endMs; start += 43200000) {
    const end = Math.min(start + 43200000, endMs);
    const url = new URL('/trade_aggregations', origin);
    url.search = new URLSearchParams({
      base_asset_type: 'credit_alphanum4', base_asset_code: 'yXLM', base_asset_issuer: YXLM_ISSUER,
      counter_asset_type: 'native', resolution: String(BUCKET_MS),
      start_time: String(start), end_time: String(end), order: 'asc', limit: '200',
    });
    const page = await get(url);
    assert(Array.isArray(page?._embedded?.records), 'missing history page');
    assert(page._embedded.records.length < 200, 'possibly truncated page');
    for (const r of page._embedded.records) {
      const timestamp = Number(r.timestamp);
      assert(Number.isSafeInteger(timestamp) && timestamp >= start && timestamp < end, 'record outside requested page');
      rows.push(r);
    }
  }
  // Check all pages together, including duplicates at chunk boundaries.
  validateRows(rows, startMs, endMs);
  return rows;
}

function validateRows(rows, startMs, endMs) {
  let previous = -1;
  for (const r of rows) {
    const ts = Number(r.timestamp), trades = Number(r.trade_count);
    assert(Number.isSafeInteger(ts) && ts % BUCKET_MS === 0 && ts >= startMs && ts < endMs, 'invalid bucket timestamp');
    assert(ts > previous, 'duplicate or unordered bucket');
    assert(Number.isSafeInteger(trades) && trades >= 0, 'invalid trade count');
    raw7(r.base_volume); raw7(r.counter_volume);
    previous = ts;
  }
}

export function availability(rows, startMs, endMs) {
  assert(Number.isSafeInteger(startMs) && Number.isSafeInteger(endMs) && startMs < endMs);
  assert(startMs % BUCKET_MS === 0 && endMs % BUCKET_MS === 0);
  assert(endMs - startMs <= 7 * 86400000, 'bounded to seven days');
  validateRows(rows, startMs, endMs);
  const buckets = new Map(rows.map(r => [Number(r.timestamp), r]));
  let passed = 0, total = 0, failedRun = 0, longestFailedRun = 0;
  const failures = {}, daily = {};
  const volumes = rows.map(r => raw7(r.base_volume)).sort((a,b) => a < b ? -1 : a > b ? 1 : 0);
  const totalVolume = volumes.reduce((a,b) => a+b, 0n);
  const topCount = Math.max(1, Math.ceil(volumes.length / 10));
  const topVolume = volumes.slice(-topCount).reduce((a,b) => a+b, 0n);
  for (let end = startMs + POLICY.windowSeconds * 1000; end <= endMs; end += BUCKET_MS) {
    const day = new Date(end - 1).toISOString().slice(0,10);
    daily[day] ??= { passed:0, windows:0 };
    daily[day].windows++; total++;
    const window = [];
    for (let ts = end - POLICY.windowSeconds*1000; ts < end; ts += BUCKET_MS) {
      if (buckets.has(ts)) window.push(buckets.get(ts));
    }
    try {
      // Evaluated immediately after each historical window, not restamped today.
      windowPrice(window, end/1000, end/1000 + 10);
      passed++; daily[day].passed++; failedRun = 0;
    } catch (error) {
      failures[error.message] = (failures[error.message] ?? 0) + 1;
      failedRun++; longestFailedRun = Math.max(longestFailedRun, failedRun);
    }
  }
  return {
    start: new Date(startMs).toISOString(), end: new Date(endMs).toISOString(),
    expectedBuckets: (endMs-startMs)/BUCKET_MS, observedBuckets: rows.length,
    totalYxlmRaw: totalVolume.toString(), medianObservedBucketYxlmRaw: (volumes[Math.floor(volumes.length/2)] ?? 0n).toString(),
    topTenPercentBucketVolumeShareBps: totalVolume ? Number(topVolume*10000n/totalVolume) : 0,
    windows:total, passed, passRateBps:total ? Math.floor(passed*10000/total) : 0,
    longestFailedWindowRunSeconds:longestFailedRun*POLICY.bucketSeconds,
    failures,daily,
    limitations:'First rejection per window. Historical trade-volume gates only; no historical Aquarius quotes, keeper uptime, publication step checks or manipulation-resistance proof.',
  };
}
