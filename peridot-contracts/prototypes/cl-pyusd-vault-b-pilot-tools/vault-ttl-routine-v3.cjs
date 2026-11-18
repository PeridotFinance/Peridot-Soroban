// Recurring TTL (storage-rent) routine for the PYUSD Vault-B pilot AND every dependency the vault reads
// (pool, pool vault, plane, router, config storage, oracle, reward feed, AQUA reward token, boost token, funding/other tokens).
// Added 16 Nov 2026 at the user's request. READ-ONLY by default; never signs, never sends, no key access.
//
//   check [--rpc URL]                       fresh inventory -> state/status-latest.json; ALERT-*.json when an entry is
//                                           due (below --due-ledgers) or a previously seen entry has disappeared
//                                           (archived: restore needed). Exit code 2 on alert. Run daily by launchd.
//   prepare --source G... [--rpc URL]       unsigned ExtendFootprintTTL transactions for every due key (<=40 keys each),
//                                           written to state/prepared-<ts>/. The operator signs with the Ledger
//                                           (stellar tx sign --sign-with-ledger) within the 20-ledger window and sends.
//
// The cash TTL keeper's fee-only deployer signing exception does NOT cover these keys; this routine does not change it.
// Extra keys (for example footprints of exit / investment simulations) can be added with state/extra-keys.json
// ({"keys":[{"keyXdr":"...","source":"..."}]}); they are inventoried like the others.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const T = require('./ttl-inventory-v3.cjs');
const ROOT = path.resolve(__dirname, '../..');
const REL = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/';
const DEPS = path.resolve(ROOT, REL, 'activate/deploy-config-full-release-20261115T204252Z-L65497866.json');
const STATE = path.resolve(ROOT, REL, 'ttl-routine/state');
const EXTEND_TO = 2592000;      // ~150 days after extension (same horizon as the cash keeper)
const DUE_LEDGERS = 1036800;    // extend when fewer than ~60 days remain
const URGENT_LEDGERS = 207360;  // ~14 days: urgent alert
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const save = (p, v) => { fs.mkdirSync(path.dirname(p), {recursive: true}); fs.writeFileSync(p + '.tmp', json(v)); fs.renameSync(p + '.tmp', p); };
const deps = () => JSON.parse(fs.readFileSync(DEPS)).dependencies;

async function check(rpc, {dueLedgers = DUE_LEDGERS} = {}) {
  const inv = await T.inventory(rpc, {deps: deps()});
  // Extra keys (exit/invest footprints) are read directly.
  const extraFile = path.join(STATE, 'extra-keys.json');
  const extra = fs.existsSync(extraFile) ? JSON.parse(fs.readFileSync(extraFile)).keys : [];
  const known = new Set(inv.rows.map(r => r.keyXdr)), add = extra.filter(e => !known.has(e.keyXdr));
  for (let i = 0; i < add.length; i += 100) {
    const chunk = add.slice(i, i + 100), got = await rpc.getLedgerEntries(...chunk.map(e => S.xdr.LedgerKey.fromXDR(e.keyXdr, 'base64')));
    const by = new Map(got.entries.map(e => [e.key.toXDR('base64'), e]));
    for (const e of chunk) inv.rows.push(T.row(by.get(e.keyXdr), {key: S.xdr.LedgerKey.fromXDR(e.keyXdr, 'base64'), sources: new Set([e.source || 'extra'])}, inv.latestLedger, {}));
  }
  // Registry of keys ever seen present: a later absence means archival (restore needed), not "not created yet".
  const regFile = path.join(STATE, 'seen-registry.json'), reg = fs.existsSync(regFile) ? JSON.parse(fs.readFileSync(regFile)) : {keys: {}};
  for (const r of inv.rows) if (r.present) reg.keys[r.keyXdr] = {label: r.label, ownerRole: r.ownerRole, contract: r.contract || r.hash, lastSeenLedger: inv.latestLedger, liveUntilLedger: r.liveUntilLedger};
  const disappeared = Object.entries(reg.keys).filter(([k]) => !inv.rows.some(r => r.keyXdr === k && r.present)).map(([keyXdr, v]) => ({keyXdr, ...v}));
  const present = inv.rows.filter(r => r.present), due = present.filter(r => r.ledgersLeft < dueLedgers).sort((a, b) => a.ledgersLeft - b.ledgersLeft);
  const urgent = due.filter(r => r.ledgersLeft < URGENT_LEDGERS);
  const status = {status: due.length || disappeared.length ? 'ALERT' : 'OK', at: new Date().toISOString(), latestLedger: inv.latestLedger,
    dueBelowLedgers: dueLedgers, counts: {keys: inv.rows.length, present: present.length, due: due.length, urgent: urgent.length, disappeared: disappeared.length},
    earliest: present.sort((a, b) => a.ledgersLeft - b.ledgersLeft).slice(0, 5).map(r => ({ownerRole: r.ownerRole, label: r.label, contract: r.contract || r.hash, liveUntilLedger: r.liveUntilLedger, approxDaysLeft: r.approxDaysLeft})),
    due: due.map(r => ({ownerRole: r.ownerRole, label: r.label, contract: r.contract || r.hash, liveUntilLedger: r.liveUntilLedger, approxDaysLeft: r.approxDaysLeft, keyXdr: r.keyXdr})),
    disappeared, failedViews: inv.failedViews, signing: false, submission: false};
  save(path.join(STATE, 'inventory-latest.json'), inv); save(regFile, reg); save(path.join(STATE, 'status-latest.json'), status);
  if (status.status === 'ALERT') save(path.join(STATE, `ALERT-${status.at.replace(/[:.]/g, '')}.json`), status);
  // Desktop notification so an alert is seen without opening the state folder (macOS; failure is ignored).
  if (status.status === 'ALERT') { try { require('node:child_process').execFileSync('/usr/bin/osascript', ['-e', `display notification "${status.counts.due} due, ${status.counts.disappeared} archived. See ttl-routine/state/status-latest.json" with title "Peridot vault TTL ALERT"`], {timeout: 10000}); } catch (_) { /* notification is best effort */ } }
  return status;
}

async function prepare(rpc, {source}) {
  const st = await check(rpc); assert(!st.disappeared.length, 'an entry disappeared (archived): RestoreFootprint needed first; not prepared here');
  if (!st.due.length) return {status: 'NOTHING_DUE', latestLedger: st.latestLedger};
  const dir = path.join(STATE, `prepared-${new Date().toISOString().replace(/[:.]/g, '')}`), out = [];
  const keys = st.due.map(r => r.keyXdr);
  for (let i = 0; i < keys.length; i += 40) {
    const r = await T.prepareExtension(rpc, {source, keysXdr: keys.slice(i, i + 40), extendTo: EXTEND_TO});
    fs.mkdirSync(dir, {recursive: true}); fs.writeFileSync(path.join(dir, `ext-${i / 40}.unsigned.xdr`), r.unsignedXdr + '\n');
    out.push({file: path.join(dir, `ext-${i / 40}.unsigned.xdr`), hash: r.hash, keys: r.keys, maxFeeStroops: r.maxFeeStroops, validLedgers: r.validLedgers, sequence: r.sequence});
    // Ledger-sourced: each later transaction needs the next sequence, so only one is prepared per run.
    if (source === 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP') break;
  }
  const res = {status: 'UNSIGNED_PREPARED', signing: false, submission: false, extendTo: EXTEND_TO, due: st.due.length, transactions: out,
    next: 'stellar tx sign --sign-with-ledger --network mainnet "$(cat FILE)" > FILE.signed; then stellar tx send; rerun prepare for any remaining keys.'};
  save(path.join(dir, 'prepared.json'), res); return res;
}
module.exports = {check, prepare, EXTEND_TO, DUE_LEDGERS, STATE};
if (require.main === module) {
  const [mode, ...args] = process.argv.slice(2), opt = n => { const i = args.indexOf(n); return i < 0 ? null : args[i + 1]; };
  const rpc = new S.rpc.Server(opt('--rpc') || 'https://soroban-rpc.mainnet.stellar.gateway.fm', {timeout: 60});
  const due = opt('--due-ledgers') ? Number(opt('--due-ledgers')) : undefined;
  (mode === 'check' ? check(rpc, {dueLedgers: due}) : mode === 'prepare' ? prepare(rpc, {source: opt('--source')}) : Promise.reject(new Error('usage: check | prepare --source G...')))
    .then(r => { const {due: d, ...brief} = r; console.log(json({...brief, dueCount: d?.length})); if (r.status === 'ALERT') process.exitCode = 2; })
    .catch(e => { console.error(e.message); process.exitCode = 1; });
}
