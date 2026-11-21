// One-shot, run ON the Droplet as the service user: create a fresh Stellar key in a new 0600 file and print ONLY its
// public key. Refuses to overwrite. Usage: node src/keygen.mjs /abs/path/<name>.secret
// Used twice: the liquidator key and the fee-only heartbeat key (cl-hb-main). Never run it on the Mac for Mainnet keys.
import {createKeyFile} from './keyfile.mjs';

try {
  console.log(createKeyFile(process.argv[2]));
} catch (e) {
  console.error('keygen refused:', e.code === 'EEXIST' ? 'key file already exists (never overwritten)' : e.message);
  process.exit(1);
}
