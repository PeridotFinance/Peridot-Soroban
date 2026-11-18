// Append-only JSONL journal of stage attempts, plus a small state file (pending-queue low-water mark).
// Every send writes an INTENT before the CLI runs and a RESULT after. A crash between the two leaves an unresolved
// intent; for L3 that blocks any further L3 for the same borrower until chain state proves nothing landed.
import fs from 'node:fs';
import path from 'node:path';

const enc = (_, v) => (typeof v === 'bigint' ? String(v) : v);
export const INTENT_TTL = 600; // s of ledger time; well above the transaction validity window

export function openJournal(dir) {
  const file = path.join(dir, 'journal.jsonl'), stateFile = path.join(dir, 'state.json');
  const read = () => { try { return fs.readFileSync(file, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l)); } catch { return []; } };
  const append = rec => { fs.mkdirSync(dir, {recursive: true}); fs.appendFileSync(file, JSON.stringify({wall: new Date().toISOString(), ...rec}, enc) + '\n'); };
  let seq = read().reduce((m, r) => Math.max(m, r.seq ?? 0), 0);
  return {
    file,
    records: read,
    intent(stage, key, now, extra = {}) { seq += 1; append({kind: 'intent', seq, stage, key, now, ...extra}); return seq; },
    result(seq0, stage, key, now, ok, extra = {}) { append({kind: 'result', of: seq0, stage, key, now, ok, ...extra}); },
    note(rec) { append({kind: 'note', ...rec}); },
    loadState() { try { return JSON.parse(fs.readFileSync(stateFile, 'utf8')); } catch { return {lowWater: 0}; } },
    saveState(s) { fs.mkdirSync(dir, {recursive: true}); fs.writeFileSync(stateFile + '.tmp', JSON.stringify(s, enc)); fs.renameSync(stateFile + '.tmp', stateFile); },
  };
}

/**
 * Borrowers for which L3 must not be (re)sent. An L3 intent without a result blocks its borrower until either
 * INTENT_TTL has passed with `next_pending` unchanged since the intent (no seizure landed anywhere), or an operator
 * appends a result. If `next_pending` moved, the bot cannot tell whose seizure landed: it stays blocked (alert).
 */
export function unresolvedL3(records, now, nextPending) {
  const done = new Set(records.filter(r => r.kind === 'result').map(r => r.of));
  const out = new Map();
  for (const r of records) {
    if (r.kind !== 'intent' || r.stage !== 'L3' || done.has(r.seq)) continue;
    const expired = now - r.now > INTENT_TTL && Number(nextPending) === Number(r.nextPending);
    if (!expired) out.set(r.key, {seq: r.seq, reviewRequired: Number(nextPending) !== Number(r.nextPending)});
  }
  return out;
}

/** Ledger time of the latest L4 whose simulated or sent result was `false` (claim unavailable). */
export function lastL4Failure(records) {
  let t = null;
  for (const r of records) if (r.kind === 'result' && r.stage === 'L4' && r.ok) t = r.retval === false ? r.now : null;
  return t;   // CLI/RPC errors say nothing about the claim: they do not unlock the boundary-opening L5
}

/** Cache timestamp at the latest L2 intent: L2 is preferred at most once per oracle round. */
export function lastL2CacheTs(records) {
  let t = null;
  for (const r of records) if (r.kind === 'intent' && r.stage === 'L2') t = r.cacheTs ?? null;
  return t;
}
