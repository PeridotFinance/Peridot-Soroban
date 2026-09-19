// Public data only. No keys, transaction building, signing or submission.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { history, availability } from './price-diagnostics.mjs';
const args=process.argv.slice(2);
assert(args.length <= 1 && (args.length===0 || /^[1-7]$/.test(args[0])), 'usage: node src/price-audit.mjs [days:1..7]');
const endMs=Math.floor(Date.now()/300000)*300000, startMs=endMs-Number(args[0]??7)*86400000;
const rows=await history({root:'https://horizon.stellar.org',networkPassphrase:'Public Global Stellar Network ; September 2015',startMs,endMs});
// Preserve the normalized public observations for repeatable independent analysis.
const observations=rows.map(({timestamp,trade_count,base_volume,counter_volume})=>({timestamp,trade_count,base_volume,counter_volume}));
console.log(JSON.stringify({observedAt:new Date().toISOString(),
  observationsSha256:createHash('sha256').update(JSON.stringify(observations)).digest('hex'),
  report:availability(observations,startMs,endMs),observations},null,2));
