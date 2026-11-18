import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const bool = (v, d) => (v == null || v === '' ? d : /^(1|true|yes)$/i.test(v));
const int = (v, d) => (v == null || v === '' ? d : Number.parseInt(v, 10));

export function loadConfig(env = process.env) {
  const pinsFile = env.PINS_FILE || path.join(here, '..', 'config', 'mainnet.json');
  const pins = JSON.parse(fs.readFileSync(pinsFile, 'utf8'));
  const cfg = {
    pins,
    rpcUrl: env.RPC_URL || 'https://soroban-rpc.mainnet.stellar.gateway.fm',
    network: env.NETWORK_PASSPHRASE || pins.network,
    publicKey: env.LIQUIDATOR_PUBLIC_KEY,
    dryRun: bool(env.DRY_RUN, true),
    runOnce: bool(env.RUN_ONCE, false),
    pollMs: int(env.POLL_INTERVAL_MS, 30_000),
    minProfitBps: int(env.MIN_PROFIT_BPS, 200),
    inclusionFee: int(env.INCLUSION_FEE_STROOPS, 100_000),
    maxFeeStroops: int(env.MAX_FEE_STROOPS, 20_000_000),
    withdrawSeized: bool(env.WITHDRAW_SEIZED, true),
    eventsLookbackLedgers: int(env.EVENTS_LOOKBACK_LEDGERS, 17_280),
    stateFile: env.STATE_FILE || path.join(here, '..', 'data', 'state.json'),
    journalFile: env.JOURNAL_FILE || path.join(here, '..', 'data', 'journal.jsonl'),
    minFeeXlmRaw: BigInt(env.MIN_FEE_XLM_RAW || '200000000'),
    // Signer: LIQUIDATOR_SECRET (operator-provided encrypted secret on the host) or, on Testnet only, SIGNER_CLI_ALIAS.
    signerCliAlias: env.SIGNER_CLI_ALIAS,
    cpuStop: int(env.CPU_STOP, 97_000_000),
  };
  if (!cfg.publicKey) throw Error('LIQUIDATOR_PUBLIC_KEY required');
  if (cfg.signerCliAlias && cfg.network.startsWith('Public')) throw Error('SIGNER_CLI_ALIAS is Testnet-only');
  if (cfg.network !== pins.network) throw Error('NETWORK_PASSPHRASE does not match pins');
  cfg.neverTouch = new Set(pins.neverTouch || []);
  return cfg;
}
