import test from 'node:test';
import assert from 'node:assert/strict';
import {assessDepeg, fetchReference} from '../src/watcher.mjs';

const NOW = 1_800_000_000, cfg = {contracts: {receipt: 'CR', controller: 'CC', usdcMarket: 'CM'}, watcher: {depegBps: 100, feedMaxAgeSec: 600}};
const feed = (p, age = 0) => ({price: BigInt(Math.round(p * 1e8)) * 1_000_000n, timestamp: NOW - age});

test('par and exactly 100 bps: no alert; 101 bps: alert with both pause recommendations', () => {
  assert.equal(assessDepeg({feed: feed(0.9998), now: NOW, ref: null}, cfg).alerts.length, 0);
  assert.equal(assessDepeg({feed: feed(0.99), now: NOW, ref: null}, cfg).alerts.length, 0);
  const r = assessDepeg({feed: feed(0.9899), now: NOW, ref: null}, cfg);
  assert.equal(r.alerts.length, 1);
  assert.deepEqual(r.recommend.map(x => x.fn), ['stage_pause_deposits', 'pause_borrow_g']);
  assert.equal(r.recommend[1].args.market, 'CM');
});
test('feed age 600 s ok, 601 s alert; missing feed alert', () => {
  assert.equal(assessDepeg({feed: feed(1, 600), now: NOW, ref: null}, cfg).alerts.length, 0);
  assert.match(assessDepeg({feed: feed(1, 601), now: NOW, ref: null}, cfg).alerts[0].msg, /601 s old/);
  assert.equal(assessDepeg({feed: null, now: NOW, ref: null}, cfg).alerts.length, 1);
});
test('feed vs reference: flat feed at par against a depegged reference alerts', () => {
  assert.equal(assessDepeg({feed: feed(1), now: NOW, ref: 0.9905}, cfg).alerts.length, 0);
  const r = assessDepeg({feed: feed(1), now: NOW, ref: 0.985}, cfg);
  assert.match(r.alerts[0].msg, /reference/);
});
test('reference fetcher: disabled by default, injectable, never throws', async () => {
  assert.equal(await fetchReference({enabled: false, url: 'x'}, async () => ({})), null);
  const stub = async () => ({pyusd: {usd: 0.997}, usdc: {usd: 1.0}});
  assert.equal(await fetchReference({enabled: true, url: 'u', path: 'pyusd.usd', divPath: 'usdc.usd'}, stub), 0.997);
  assert.equal(await fetchReference({enabled: true, url: 'u', path: 'nope'}, stub), null);
  assert.equal(await fetchReference({enabled: true, url: 'u', path: 'a'}, async () => { throw Error('down'); }), null);
});
