// Read-only: walk Reflector price(asset, ts) back N rounds to find missing rounds (justifies the max-age multiplier k).
'use strict';
const {S, MAINNET, json, chain} = require('./lib.cjs');
const c = chain(MAINNET);
const asset = (feed, s, tok) => feed === 'external' ? S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Other'), S.xdr.ScVal.scvSymbol(s)])
  : S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('Stellar'), new S.Address(tok).toScVal()]);
(async () => {
  const N = Number(process.argv[2] || 144), out = {observedAt: new Date().toISOString(), rounds: N, feeds: {}};
  for (const feed of ['external', 'stellarDex']) {
    const id = MAINNET.oracles[feed], last = Number((await c.view(id, 'last_timestamp')).value);
    out.feeds[feed] = {last};
    for (const [s, m] of Object.entries(MAINNET.markets)) {
      if (feed === 'external' && s === 'PYUSD') continue;
      let missing = [], prices = [];
      for (let i = 0; i < N; i++) {
        const ts = last - i * 300; let v;
        try { v = (await c.view(id, 'price', [asset(feed, s, m.underlying), S.nativeToScVal(BigInt(ts), {type: 'u64'})])).value; } catch { v = null; }
        if (!v) missing.push(ts); else prices.push(Number(v.price) / 1e14);
      }
      const mn = Math.min(...prices), mx = Math.max(...prices);
      out.feeds[feed][s] = {present: prices.length, missing: missing.length, missingTs: missing.slice(0, 20), min: mn, max: mx, rangeBps: Math.round((mx - mn) / mn * 1e4)};
    }
  }
  console.log(json(out));
})().catch(e => { console.error(e.message); process.exit(1); });
