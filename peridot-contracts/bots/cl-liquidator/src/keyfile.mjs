// Key files for the CL liquidator and its fee-only heartbeat account (Mainnet, Droplet). Same rules as
// bots/cash-liquidator/src/keyfile.mjs: a key exists only on its own host, in a regular 0600 file owned by the service
// user, created there by src/keygen.mjs. Errors never include file contents; nothing here logs or returns a secret.
import fs from 'node:fs';
import path from 'node:path';
import {loadSdk} from './chain.mjs';

/** Load a Keypair from `file`, refusing symlinks, non-regular files, group/other permission bits and foreign owners. */
export function loadKeyFile(file, {expectedPublicKey} = {}, S = loadSdk()) {
  if (!file || !path.isAbsolute(file)) throw Error('key file path must be absolute');
  let st;
  try { st = fs.lstatSync(file); } catch { throw Error('key file missing or unreadable'); }
  if (st.isSymbolicLink()) throw Error('key file must not be a symlink');
  if (!st.isFile()) throw Error('key file is not a regular file');
  if ((st.mode & 0o077) !== 0) throw Error(`key file mode ${(st.mode & 0o777).toString(8)} too open (chmod 600)`);
  if (typeof process.getuid === 'function' && st.uid !== process.getuid()) throw Error('key file is not owned by this service user');
  let kp;
  try {
    const raw = fs.readFileSync(file, 'utf8').trim();
    if (!S.StrKey.isValidEd25519SecretSeed(raw)) throw Error('x');
    kp = S.Keypair.fromSecret(raw);
  } catch { throw Error('key file does not contain a valid Stellar secret seed'); }
  if (expectedPublicKey && kp.publicKey() !== expectedPublicKey) throw Error('key file does not match the configured public key');
  return kp;
}

/** Create a NEW key at `file` (never overwrites; directory must be 0700). Returns only the public key. */
export function createKeyFile(file, S = loadSdk()) {
  if (!file || !path.isAbsolute(file)) throw Error('key file path must be absolute');
  const ds = fs.statSync(path.dirname(file));
  if ((ds.mode & 0o077) !== 0) throw Error(`key directory mode ${(ds.mode & 0o777).toString(8)} too open (chmod 700)`);
  const kp = S.Keypair.random();
  const fd = fs.openSync(file, 'wx', 0o600);              // fails if the file exists
  try { fs.writeSync(fd, kp.secret() + '\n'); fs.fsyncSync(fd); } finally { fs.closeSync(fd); }
  fs.chmodSync(file, 0o600);
  return kp.publicKey();
}
