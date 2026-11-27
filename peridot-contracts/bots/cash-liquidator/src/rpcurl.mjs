// RPC endpoint selection. A private endpoint (e.g. one whose URL carries an API key) is read from RPC_URL_FILE: a regular
// file of mode 0600 owned by the service user, holding one https URL whose host must equal RPC_HOST. The URL is never logged
// and never appears in an error: every log line is scrubbed of it (and of its key-bearing path) before it is written or sent.
import fs from 'node:fs';
import path from 'node:path';

export const PUBLIC_RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const secrets = new Set();

/** Register a secret string; scrubText replaces it everywhere. Short strings are ignored (would over-match). */
export function registerSecret(s) { if (typeof s === 'string' && s.length >= 12) secrets.add(s); }
export function scrubText(s) {
  let out = String(s ?? '');
  for (const x of [...secrets].sort((a, b) => b.length - a.length)) out = out.split(x).join('<rpc-secret>');
  return out;
}

/** Read and validate RPC_URL_FILE. Errors never include the file contents. */
export function loadRpcUrlFile(file, expectedHost) {
  if (!file || !path.isAbsolute(file)) throw Error('RPC_URL_FILE must be an absolute path');
  if (!expectedHost) throw Error('RPC_URL_FILE requires RPC_HOST (the expected host name)');
  let st;
  try { st = fs.lstatSync(file); } catch { throw Error('RPC_URL_FILE missing or unreadable'); }
  if (st.isSymbolicLink()) throw Error('RPC_URL_FILE must not be a symlink');
  if (!st.isFile()) throw Error('RPC_URL_FILE is not a regular file');
  if ((st.mode & 0o077) !== 0) throw Error(`RPC_URL_FILE mode ${(st.mode & 0o777).toString(8)} too open (chmod 600)`);
  if (typeof process.getuid === 'function' && st.uid !== process.getuid()) throw Error('RPC_URL_FILE is not owned by this service user');
  let u, text;
  try { text = fs.readFileSync(file, 'utf8').trim(); u = new URL(text); } catch { throw Error('RPC_URL_FILE does not contain a valid URL'); }
  if (u.protocol !== 'https:') throw Error('RPC_URL_FILE must hold an https URL');
  if (u.username || u.password) throw Error('RPC_URL_FILE must not carry credentials in user:password form');
  if (u.hostname !== expectedHost) throw Error('RPC_URL_FILE host does not match RPC_HOST');
  registerSecret(text); registerSecret(u.href);
  const tail = u.pathname + u.search;
  if (tail.length > 1) registerSecret(tail);
  for (const seg of u.pathname.split('/')) registerSecret(seg);                       // the key segment itself
  for (const v of u.searchParams.values()) registerSecret(v);
  return u.href;
}

/** {url, host, source}: RPC_URL_FILE (private, scrubbed) | RPC_URL (public, plain) | the public default. Both set is refused. */
export function resolveRpc(env = process.env) {
  if (env.RPC_URL_FILE) {
    if (env.RPC_URL) throw Error('set RPC_URL_FILE or RPC_URL, not both');
    const url = loadRpcUrlFile(env.RPC_URL_FILE, env.RPC_HOST);
    return {url, host: env.RPC_HOST, source: 'file'};
  }
  const url = env.RPC_URL || PUBLIC_RPC;
  return {url, host: new URL(url).hostname, source: env.RPC_URL ? 'env' : 'default'};
}
