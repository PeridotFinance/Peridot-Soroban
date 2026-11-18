'use strict';
// 16 Nov 2026: the real deployment journal was continued by step 20 (activate-receipt a63f86af…, CONFIRMED). Tests that
// exercise the pre-activation tools (FULL-config generator, mark-reviewed, pilot loadInputs before activation) need the
// 19-step journal as it stood before that. It is DERIVED here from the live journal (segment 0 and entries 0..18,
// byte-identical rows; fees = total minus step 20's charged fee), written to a temp directory, and checked against the
// operator's own pre-release backup. The real journal is only read.
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../../../..');
const DIR = path.join(ROOT, 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/deploy/run-seed800-20261115T132829Z/journal');
const STEP20 = 'a63f86afca92b790211443e274f579bd5c9a220ee40b0c3cdbe4c78ac10f147b';
function live() { return JSON.parse(fs.readFileSync(path.join(DIR, 'journal.json'))); }
function preActivation() {
  const j = live();
  assert.equal(j.entries.length, 20); assert.equal(j.segments.length, 2); assert.equal(j.active, null); assert.equal(j.halted, false);
  const s20 = j.entries[19]; assert.equal(s20.step, 'activate-receipt'); assert.equal(s20.status, 'CONFIRMED'); assert.equal(s20.hash, STEP20);
  const pre = {...j, segments: [j.segments[0]], entries: j.entries.slice(0, 19), active: null,
    feesCharged: String(BigInt(j.feesCharged) - BigInt(s20.feeCharged))};
  // Cross-check with the operator's pre-release backup (taken while step 20 was still an unapplied active row).
  const backup = JSON.parse(fs.readFileSync(path.join(DIR, 'journal.before-release-cf71e4f4df2b555b8b733e41460c9ae2e5c322444c12a0b5baf91a68d8126172.json')));
  assert.deepEqual(backup.entries.slice(0, 19), pre.entries); assert.equal(backup.feesCharged, pre.feesCharged); assert.deepEqual(backup.segments[0], pre.segments[0]);
  return pre;
}
function writePreActivation(t) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'pre-activation-journal-'))); t.after(() => fs.rmSync(dir, {recursive: true, force: true}));
  fs.chmodSync(dir, 0o700); fs.writeFileSync(path.join(dir, 'journal.json'), JSON.stringify(preActivation(), null, 2) + '\n', {mode: 0o600});
  return dir;
}
module.exports = {DIR, STEP20, live, preActivation, writePreActivation};
