// SKELETON entrypoint: validates configuration and prints the job plan. It performs no network calls and never
// signs. The production implementation must follow DESIGN.md section 14 before any deployment.
import {loadConfig} from './config.mjs';
import {describe} from './jobs.mjs';

const cfg = loadConfig();
console.log(JSON.stringify({keeper: 'sentora-keeper', skeleton: true, dryRun: cfg.dryRun, plan: describe(cfg)}, null, 2));
if (!cfg.dryRun) {
  console.error('sentora-keeper skeleton refuses live mode: no signer implemented');
  process.exit(1);
}
