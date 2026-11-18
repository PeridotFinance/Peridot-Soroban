// Read-mostly: release ONE journal entry that was PREPARED but provably never applied, so the step can be prepared again.
// Proof required (all): entry is the journal tail with status PREPARED; closed-ledger number > its maxLedger and close time > maxTime;
// hash NOT_FOUND on RPC and 404 on Horizon; the Ledger account sequence is still below the entry's sequence.
// Effects: copies the journal to journal.before-release-<hash>.json, writes expired-<hash>.json, removes the tail entry
// (and the segment it opened, if that segment has no other entry), sets active=null. Never signs or sends.
'use strict';
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const stringify = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
(async () => {
  const [dir, rpcUrl] = process.argv.slice(2); assert(dir && rpcUrl, 'usage: release-expired-unsigned-v3.cjs JOURNAL_DIR RPC_URL');
  const file = path.join(dir, 'journal.json'), j = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert(!fs.existsSync(path.join(dir, 'lock')), 'journal locked by a running tool');
  const tail = j.entries.at(-1); assert(tail && tail.status === 'PREPARED' && j.active === tail.step, 'tail is not a PREPARED active entry');
  assert(!j.halted);
  const tx = S.TransactionBuilder.fromXDR(tail.unsignedXdr, S.Networks.PUBLIC);
  assert.equal(tx.hash().toString('hex'), tail.hash); assert.equal(tx.sequence, tail.sequence);
  const maxLedger = Number(tx.ledgerBounds.maxLedger), maxTime = Number(tx.timeBounds.maxTime);
  const rpc = new S.rpc.Server(rpcUrl, {timeout: 60});
  const latest = await rpc.getLatestLedger();
  assert(latest.sequence > maxLedger + 1, `not yet expired: latest ${latest.sequence} <= maxLedger ${maxLedger}+1`);
  const q = await rpc.getTransaction(tail.hash); assert.equal(q.status, 'NOT_FOUND', 'hash known to RPC; reconcile instead');
  const h = await fetch('https://horizon.stellar.org/transactions/' + tail.hash); assert.equal(h.status, 404, 'hash known to Horizon; reconcile instead');
  const acct = await (await fetch('https://horizon.stellar.org/accounts/' + tx.source)).json();
  assert(BigInt(acct.sequence) < BigInt(tail.sequence), 'account sequence reached the entry sequence; investigate');
  const ledger = await (await fetch('https://horizon.stellar.org/ledgers/' + latest.sequence)).json();
  assert(Math.floor(Date.parse(ledger.closed_at) / 1000) > maxTime, 'close time not past maxTime');
  const evidence = {result: 'EXPIRED_NEVER_APPLIED', step: tail.step, hash: tail.hash, sequence: tail.sequence, maxLedger, maxTime,
    latestLedger: latest.sequence, latestClosedAt: ledger.closed_at, accountSequence: acct.sequence, checkedAt: new Date().toISOString(), entry: tail};
  fs.writeFileSync(path.join(dir, `journal.before-release-${tail.hash}.json`), fs.readFileSync(file), {flag: 'wx', mode: 0o600});
  fs.writeFileSync(path.join(dir, `expired-${tail.hash}.json`), stringify(evidence), {flag: 'wx', mode: 0o600});
  const idx = j.entries.length - 1; j.entries.pop(); j.active = null;
  const seg = j.segments.at(-1); if (seg && seg.firstEntry === idx) j.segments.pop();
  const tmp = file + '.tmp-' + process.pid; fs.writeFileSync(tmp, stringify(j), {flag: 'wx', mode: 0o600}); fs.renameSync(tmp, file);
  console.log(stringify({result: evidence.result, step: tail.step, hash: tail.hash, entriesNow: j.entries.length, segmentsNow: j.segments.length}));
})().catch(e => { console.error(e.message); process.exitCode = 1; });
