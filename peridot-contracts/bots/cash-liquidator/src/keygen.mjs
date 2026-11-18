// One-shot, run ON the host as the service user: create a fresh Stellar key in a new 0600 file and print ONLY its public
// key. Refuses to overwrite. Usage: node src/keygen.mjs /abs/path/key.secret
import {createKeyFile} from './keyfile.mjs';

const file = process.argv[2];
try {
  const pub = createKeyFile(file);
  console.log(pub);
} catch (e) {
  console.error('keygen refused:', e.code === 'EEXIST' ? 'key file already exists (never overwritten)' : e.message);
  process.exit(1);
}
