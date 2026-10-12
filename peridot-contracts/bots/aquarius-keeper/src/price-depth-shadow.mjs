// RESEARCH ONLY: no publisher imports, credentials or write methods. Classic
// venue execution quotes are not proof of fills, locked liquidity or independence
// of liquidity providers. They are NOT an approved lending oracle.
import assert from 'node:assert/strict';
import { raw7, SCALE, near } from './observation.mjs';
import { jsonGet } from './price-collector.mjs';
import { YXLM_ISSUER } from './price-diagnostics.mjs';
const NETWORK = 'Public Global Stellar Network ; September 2015';
export const SHADOW_POLICY = 'yxlm-direct-depth-v1-research';
const PROBES = ['1000', '10000'];

function yxlm(row, prefix) {
  return row[`${prefix}_asset_type`] === 'credit_alphanum4'
    && row[`${prefix}_asset_code`] === 'yXLM' && row[`${prefix}_asset_issuer`] === YXLM_ISSUER;
}
function output(page, sellingYxlm, amount) {
  assert(Array.isArray(page?._embedded?.records), 'missing path response');
  const direct = page._embedded.records.filter(r => Array.isArray(r.path) && r.path.length === 0);
  // Do not mix in an apparently better multihop route or guess asset orientation.
  assert.equal(direct.length, 1, 'one unambiguous direct route required');
  const row = direct[0];
  assert(sellingYxlm ? yxlm(row, 'source') && row.destination_asset_type === 'native'
    : row.source_asset_type === 'native' && yxlm(row, 'destination'), 'path asset mismatch');
  assert.equal(raw7(row.source_amount), raw7(amount), 'path amount mismatch');
  const received = raw7(row.destination_amount);
  assert(received > 0n, 'empty path output');
  return received;
}
function midpoint(probe, sold, bought) {
  const bid = sold * SCALE / probe, ask = probe * SCALE / bought;
  assert(bid > 0n && ask >= bid, 'crossed/empty bid-ask');
  const mid = (bid + ask) / 2n;
  assert(mid >= SCALE * 80n / 100n && mid <= SCALE * 105n / 100n, 'shadow ratio bounds');
  assert((ask - bid) * 10000n <= mid * 100n, 'shadow spread exceeds 1%');
  return { bid, ask, mid };
}
function checkPoint(point) {
  assert.equal(point.policy, SHADOW_POLICY, 'shadow policy mismatch');
  assert(Number.isSafeInteger(point.timestamp) && point.timestamp > 0, 'invalid point time');
  assert(Number.isSafeInteger(point.ledgerBefore) && point.ledgerBefore > 0 && Number.isSafeInteger(point.ledgerAfter)
    && point.ledgerAfter >= point.ledgerBefore && point.ledgerAfter - point.ledgerBefore <= 6, 'incoherent ledger sample');
  assert(Array.isArray(point.depth) && point.depth.length === 2, 'two depth probes required');
  const prices = point.depth.map((d,i) => {
    assert.equal(d.probeRaw, raw7(PROBES[i]).toString(), 'probe identity mismatch');
    for (const n of [d.soldForRaw,d.boughtRaw]) assert(typeof n === 'string' && /^[1-9][0-9]{0,38}$/.test(n), 'invalid output units');
    return midpoint(BigInt(d.probeRaw), BigInt(d.soldForRaw), BigInt(d.boughtRaw));
  });
  assert(near(prices[1].mid, prices[0].mid, 50n), 'depth impact exceeds 0.5%');
  // Use the larger probe; the smaller probe is a depth-impact check.
  return prices[1].mid;
}

export function compareAquarius(point, {probeRaw,soldForRaw,boughtRaw}) {
  const reference = checkPoint(point);
  // Match the deployed observer's100-unit probe, distinct from shadow depth.
  assert.equal(probeRaw, '1000000000', 'Aquarius probe mismatch');
  for (const v of [soldForRaw,boughtRaw]) assert(typeof v==='string' && /^[1-9][0-9]{0,38}$/.test(v), 'invalid Aquarius output');
  const bid=BigInt(soldForRaw)*SCALE/BigInt(probeRaw), ask=BigInt(probeRaw)*SCALE/BigInt(boughtRaw);
  return {referenceRatio:reference.toString(),aquariusBid:bid.toString(),aquariusAsk:ask.toString(),
    agrees:bid<=ask && near(bid,reference,100n) && near(ask,reference,100n),publicationEligible:false};
}

export async function depthPoint({ root='https://horizon.stellar.org', now, get=jsonGet }) {
  const origin = new URL(root);
  assert(origin.protocol === 'https:' && !origin.username && !origin.password && !origin.search && !origin.hash);
  assert.equal((await get(origin)).network_passphrase, NETWORK, 'Horizon network mismatch');
  const ledger = async () => {
    const url = new URL('/ledgers', origin); url.search = new URLSearchParams({order:'desc',limit:'1'});
    const r = (await get(url))?._embedded?.records?.[0];
    const timestamp = Date.parse(r?.closed_at)/1000, sequence = Number(r?.sequence);
    assert(Number.isSafeInteger(timestamp) && timestamp <= now() && now()-timestamp <= 60, 'stale/future ledger');
    assert(Number.isSafeInteger(sequence) && sequence > 0, 'invalid ledger');
    return {timestamp,sequence};
  };
  const before = await ledger(), began = now(), depth = [];
  for (const amount of PROBES) {
    const results = [];
    for (const sell of [true,false]) {
      const url = new URL('/paths/strict-send', origin);
      url.search = new URLSearchParams({source_asset_type:sell?'credit_alphanum4':'native',
        ...(sell?{source_asset_code:'yXLM',source_asset_issuer:YXLM_ISSUER}:{}),source_amount:amount,
        destination_assets:sell?'native':`yXLM:${YXLM_ISSUER}`});
      results.push(output(await get(url),sell,amount));
    }
    depth.push({probeRaw:raw7(amount).toString(),soldForRaw:results[0].toString(),boughtRaw:results[1].toString()});
  }
  const after = await ledger();
  assert(now() >= began && now()-began <= 30, 'quote collection too slow');
  // Oldest observed chain timestamp, never collection completion or local clock.
  const point = {policy:SHADOW_POLICY,timestamp:before.timestamp,ledgerBefore:before.sequence,
    ledgerAfter:after.sequence,depth};
  checkPoint(point);
  return point;
}

export function depthWindow(points, now) {
  assert(Number.isSafeInteger(now) && Array.isArray(points) && points.length >= 31 && points.length <= 64, '31–64 observations required');
  const prices = points.map(checkPoint), end = points.at(-1).timestamp, start = end-1800;
  assert(end <= now && now-end <= 60, 'stale/future shadow window');
  assert(points[0].timestamp <= start && start-points[0].timestamp <= 90, 'incomplete shadow window');
  let area = 0n;
  for (let i=1;i<points.length;i++) {
    const a=points[i-1].timestamp,b=points[i].timestamp;
    assert(b>a && b-a<=90, 'missing/duplicate shadow sample');
    assert(points[i].ledgerBefore > points[i-1].ledgerBefore, 'replayed ledger sample');
    if (b <= start) continue;
    // Clipped linear interpolation over adjacent observed prices; never bridge a
    // missing interval. Twice-integrated area avoids fractional arithmetic.
    const left=Math.max(a,start),delta=b-a;
    const leftPrice=prices[i-1]+(prices[i]-prices[i-1])*BigInt(left-a)/BigInt(delta);
    area+=(leftPrice+prices[i])*BigInt(b-left);
  }
  const ratio=area/3600n;
  assert(prices.every(p=>near(p,ratio,100n)), 'unstable shadow window');
  return {policy:SHADOW_POLICY,start,end,ratio:ratio.toString(),
    publicationEligible:false,limitation:'Shadow TWAP only; Aquarius cross-check, manipulation/exposure review and continuous availability validation still required.'};
}
