import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import * as S from '@stellar/stellar-sdk';
import {loadKeyFile, createKeyFile} from '../src/keyfile.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const tmpDir = () => { const d = fs.mkdtempSync(path.join(os.tmpdir(), 'cashliq-key-')); fs.chmodSync(d, 0o700); return d; };

test('keygen creates a 0600 key, prints only the public key, never overwrites', () => {
  const d = tmpDir(), f = path.join(d, 'k.secret');
  const out = execFileSync(process.execPath, [path.join(here, '..', 'src', 'keygen.mjs'), f]).toString().trim();
  assert(S.StrKey.isValidEd25519PublicKey(out));
  const secret = fs.readFileSync(f, 'utf8').trim();
  assert.equal(S.Keypair.fromSecret(secret).publicKey(), out);
  assert(!out.includes(secret) && !out.includes('S'.concat(secret.slice(1, 8))));
  assert.equal(fs.statSync(f).mode & 0o777, 0o600);
  assert.throws(() => execFileSync(process.execPath, [path.join(here, '..', 'src', 'keygen.mjs'), f], {stdio: 'pipe'}), e => !String(e.stderr).includes(secret) && /already exists/.test(String(e.stderr)));
  assert.equal(fs.readFileSync(f, 'utf8').trim(), secret);        // unchanged
  fs.rmSync(d, {recursive: true});
});

test('loadKeyFile refuses open modes, symlinks, mismatches and junk without echoing contents', () => {
  const d = tmpDir(), f = path.join(d, 'k.secret');
  const pub = createKeyFile(f), secret = fs.readFileSync(f, 'utf8').trim();
  assert.equal(loadKeyFile(f, {expectedPublicKey: pub}).publicKey(), pub);
  assert.throws(() => loadKeyFile(f, {expectedPublicKey: S.Keypair.random().publicKey()}), /does not match/);
  fs.chmodSync(f, 0o640); assert.throws(() => loadKeyFile(f), /too open/); fs.chmodSync(f, 0o600);
  const l = path.join(d, 'link'); fs.symlinkSync(f, l); assert.throws(() => loadKeyFile(l), /symlink/);
  const j = path.join(d, 'junk'); fs.writeFileSync(j, 'not-a-seed ' + secret.slice(0, 10), {mode: 0o600});
  assert.throws(() => loadKeyFile(j), e => /valid Stellar secret seed/.test(e.message) && !e.message.includes(secret.slice(0, 10)));
  assert.throws(() => loadKeyFile('relative/path'), /absolute/);
  const open = fs.mkdtempSync(path.join(os.tmpdir(), 'cashliq-open-')); fs.chmodSync(open, 0o755);
  assert.throws(() => createKeyFile(path.join(open, 'k')), /directory mode/);
  fs.rmSync(d, {recursive: true}); fs.rmSync(open, {recursive: true});
});
