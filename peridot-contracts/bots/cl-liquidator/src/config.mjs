// JSON config (see config/testnet.example.json). Holds public addresses and a CLI identity ALIAS only, never a key.
import fs from 'node:fs';
import path from 'node:path';
import {TESTNET, assertIdentityName} from './chain.mjs';

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
  assertIdentityName(out.executorIdentity);
  out.repayCapRaw = out.repayCapRaw == null ? null : BigInt(out.repayCapRaw);
  out.dataDir = path.resolve(baseDir, out.dataDir);
  return out;
}
