// JSON config (see config/testnet.example.json). Holds public addresses and a CLI identity ALIAS only, never a key.
import fs from 'node:fs';
import path from 'node:path';
import {TESTNET, MAINNET, assertIdentityName} from './chain.mjs';
import {validateMainnetConfig} from './mainnet.mjs';

const REQUIRED = ['receipt', 'vault', 'controller', 'usdcMarket', 'usdc', 'pyusd', 'oracle'];

export function loadConfig(file) {
  const cfg = JSON.parse(fs.readFileSync(file, 'utf8'));
  return normalize(cfg, path.dirname(path.resolve(file)));
}

export function normalize(cfg, baseDir = process.cwd()) {
  const out = {submitMarginSec: 10, watchIntervalSec: 10, networkPassphrase: TESTNET, dataDir: './data', borrowers: [], ...cfg};
  for (const k of REQUIRED) if (!out.contracts?.[k]) throw Error(`config.contracts.${k} required`);
  if (!/^G[A-Z2-7]{55}$/.test(out.liquidator ?? '')) throw Error('config.liquidator must be a public G... address');
  for (const k of Object.keys(out)) if (/secret|seed|private|mnemonic/i.test(k)) throw Error(`refused: config key "${k}" looks like key material`);
  if (out.submitMarginSec < 0 || out.submitMarginSec > 15) throw Error('submitMarginSec must be within 0..15 (checkpoints live 60 s)');
  if (!(out.watchIntervalSec >= 1 && out.watchIntervalSec <= 20)) throw Error('watchIntervalSec must be 1..20 (D3 heartbeat <= 20 s)');
  out.repayCapRaw = out.repayCapRaw == null ? null : BigInt(out.repayCapRaw);
  out.dataDir = path.resolve(baseDir, out.dataDir);
  // Mainnet (HG4): key files on the Droplet instead of CLI aliases; pins, fees and the dry-run gate (src/mainnet.mjs).
  if (out.networkPassphrase === MAINNET) return validateMainnetConfig(out);
  assertIdentityName(out.executorIdentity);
  if (out.heartbeatIdentity != null) {
    assertIdentityName(out.heartbeatIdentity);
    if (out.heartbeatIdentity === out.executorIdentity) throw Error('heartbeatIdentity must differ from executorIdentity (separate sequence numbers)');
    if (!/^G[A-Z2-7]{55}$/.test(out.heartbeatAccount ?? '') || out.heartbeatAccount === out.liquidator) throw Error('heartbeatAccount must be a public G... address distinct from the liquidator');
    if (out.heartbeatFeeStroops != null && !(Number.isInteger(out.heartbeatFeeStroops) && out.heartbeatFeeStroops >= 100 && out.heartbeatFeeStroops <= 1_000_000)) throw Error('heartbeatFeeStroops must be an integer 100..1000000');
    if (out.heartbeatPeriodMs != null && !(out.heartbeatPeriodMs >= 3000 && out.heartbeatPeriodMs <= 15000)) throw Error('heartbeatPeriodMs must be 3000..15000 (on-chain gap target <= 20 s)');
  }
  return out;
}
