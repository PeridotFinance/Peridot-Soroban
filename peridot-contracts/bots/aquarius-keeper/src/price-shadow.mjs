// One public-data observation only. No keys, daemon, publisher or Mainnet writes.
import assert from 'node:assert/strict';
import {depthPoint,compareAquarius} from './price-depth-shadow.mjs';
import {aquariusSnapshot} from './price-shadow-rpc.mjs';
assert(process.argv.length === 2, 'usage: node src/price-shadow.mjs');
const point=await depthPoint({now:()=>Math.floor(Date.now()/1000)});
const aquarius=await aquariusSnapshot();
assert(Math.floor(Date.now()/1000)-point.timestamp<=60,'shadow sample expired during cross-check');
assert(aquarius.ledgerMin>=point.ledgerBefore&&aquarius.ledgerMax-point.ledgerBefore<=12,'Horizon/RPC ledger mismatch');
console.log(JSON.stringify({observedAt:new Date().toISOString(),publicationEligible:false,point,aquarius,
  comparison:compareAquarius(point,aquarius)},null,2));
