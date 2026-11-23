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
    pruneEveryMs: int(env.PRUNE_EVERY_MS, 3_600_000),
    minProfitBps: int(env.MIN_PROFIT_BPS, 200),
    inclusionFee: int(env.INCLUSION_FEE_STROOPS, 100_000),
    maxFeeStroops: int(env.MAX_FEE_STROOPS, 20_000_000),
    withdrawSeized: bool(env.WITHDRAW_SEIZED, true),
    eventsLookbackLedgers: int(env.EVENTS_LOOKBACK_LEDGERS, 17_280),
    stateFile: env.STATE_FILE || path.join(here, '..', 'data', 'state.json'),
    journalFile: env.JOURNAL_FILE || path.join(here, '..', 'data', 'journal.jsonl'),
    minFeeXlmRaw: BigInt(env.MIN_FEE_XLM_RAW || '200000000'),
    // Signer, in order: LIQUIDATOR_KEY_FILE (Droplet: 0600 file created on the host by src/keygen.mjs), the legacy
    // LIQUIDATOR_SECRET run-time secret (App Platform), or, on Testnet only, SIGNER_CLI_ALIAS.
    keyFile: env.LIQUIDATOR_KEY_FILE || null,
    signerCliAlias: env.SIGNER_CLI_ALIAS,
    accountReserveXlmRaw: BigInt(env.ACCOUNT_RESERVE_XLM_RAW || '20000000'),
    // Permissionless cache_price refresher: every new Reflector round, own bid and caps, persisted per-UTC-day budget.
    refresh: {
      inclusionFee: int(env.REFRESH_INCLUSION_FEE_STROOPS, 1_000),
      maxFeeStroops: int(env.REFRESH_MAX_FEE_STROOPS, 100_000),
      dailyBudgetStroops: int(env.REFRESH_DAILY_BUDGET_STROOPS, 30_000_000),
      timeoutSec: int(env.REFRESH_TIMEOUT_SEC, 30),
      submitWaitMs: int(env.REFRESH_SUBMIT_WAIT_MS, 35_000),
    },
    cpuStop: int(env.CPU_STOP, 97_000_000),
  };
  if (!cfg.publicKey) throw Error('LIQUIDATOR_PUBLIC_KEY required');
  if (cfg.signerCliAlias && cfg.network.startsWith('Public')) throw Error('SIGNER_CLI_ALIAS is Testnet-only');
  if (cfg.network !== pins.network) throw Error('NETWORK_PASSPHRASE does not match pins');
  cfg.neverTouch = new Set(pins.neverTouch || []);
  cfg.margin = loadMarginConfig(pins, env);
  // Cash-pass exclusion: never liquidate these borrowers (alert only). The margin pooled account is always added when
  // a margin contract id is configured, whether or not the margin pass itself is enabled.
  cfg.neverLiquidateBorrowers = new Set([...(pins.neverLiquidateBorrowers || []), ...(cfg.margin.contract ? [cfg.margin.contract] : [])]);
  return cfg;
}

const STRKEY_C = /^C[A-Z2-7]{55}$/;
const STRKEY_G = /^G[A-Z2-7]{55}$/;

/**
 * Margin pass (pooled margin v2). Off unless `margin.enabled` is true AND `margin.contract` is set. Release defaults come
 * from pins.margin; an operator overlay MARGIN_CONFIG_FILE (JSON, e.g. /etc/peridot-cash-liquidator/margin.json) may set
 * enabled / contract / thresholds, but the contract's WASM must still be one of the release-pinned `wasmAllowed`.
 * Its own dry-run flag (MARGIN_DRY_RUN, default true) so a margin dry stage never switches off live cash liquidations;
 * the effective margin dry run is DRY_RUN || MARGIN_DRY_RUN.
 */
export function loadMarginConfig(pins, env = process.env) {
  const base = pins.margin || {};
  const overlay = env.MARGIN_CONFIG_FILE ? JSON.parse(fs.readFileSync(env.MARGIN_CONFIG_FILE, 'utf8')) : {};
  for (const k of ['wasmAllowed', 'controller']) if (k in overlay) throw Error(`MARGIN_CONFIG_FILE may not override release pin margin.${k}`);
  const m = {...base, ...overlay};
  const out = {
    enabled: m.enabled === true && !!m.contract,
    contract: m.contract || null,
    wasmAllowed: base.wasmAllowed || [],
    controller: base.controller || pins.controller,
    dryRun: bool(env.MARGIN_DRY_RUN, true),
    cpuStop: int(env.MARGIN_CPU_STOP, m.cpuStop ?? 289_000_000),          // 85 % of the 340 M liquidation-flow limit (SPEC §7)
    maxFeeStroops: int(env.MARGIN_MAX_FEE_STROOPS, m.maxFeeStroops ?? 30_000_000),
    minProfitBps: int(env.MARGIN_MIN_PROFIT_BPS, m.minProfitBps ?? 0),    // profit must be > 0 after the tx fee in any case
    payBufferBps: m.payBufferBps ?? 50,
    maxSettleIters: m.maxSettleIters ?? 8,
    hfDownMin: BigInt(m.hfDownMin ?? 1_100_000),
    liquidatableAlertMs: (m.liquidatableAlertSecs ?? 600) * 1000,
    realertMs: (m.realertSecs ?? 600) * 1000,
    backerLookbackLedgers: m.backerLookbackLedgers ?? 100_000,
    backersSeed: m.backersSeed || [],
    roundPollMs: int(env.MARGIN_ROUND_POLL_MS, m.roundPollMs ?? 5_000),
  };
  if (out.contract && !STRKEY_C.test(out.contract)) throw Error('margin.contract must be a C... contract id');
  if (out.enabled && !out.wasmAllowed.length) throw Error('margin.enabled requires release pin margin.wasmAllowed');
  for (const b of out.backersSeed) if (!STRKEY_G.test(b) && !STRKEY_C.test(b)) throw Error('margin.backersSeed entries must be addresses');
  return out;
}

// Pause watcher (src/pause-main.mjs): a separate process with a separate pause-only key. It never loads, and refuses to
// start next to, liquidator credentials. Dry-run by default.
export function loadPauseConfig(env = process.env) {
  const pinsFile = env.PINS_FILE || path.join(here, '..', 'config', 'mainnet.json');
  const pins = JSON.parse(fs.readFileSync(pinsFile, 'utf8'));
  const w = pins.pauseWatcher || {};
  const cfg = {
    pins,
    mode: 'pause-watcher',
    rpcUrl: env.RPC_URL || 'https://soroban-rpc.mainnet.stellar.gateway.fm',
    network: env.NETWORK_PASSPHRASE || pins.network,
    publicKey: env.PAUSE_PUBLIC_KEY,
    keyFile: env.PAUSE_KEY_FILE,
    allowEnvSecret: false,
    signerCliAlias: undefined,
    liquidatorPublicKey: env.LIQUIDATOR_PUBLIC_KEY || null,
    dryRun: bool(env.DRY_RUN, true),
    runOnce: bool(env.RUN_ONCE, false),
    pollMs: int(env.POLL_INTERVAL_MS, 30_000),
    inclusionFee: int(env.PAUSE_INCLUSION_FEE_STROOPS, 100_000),
    maxFeeStroops: int(env.PAUSE_MAX_FEE_STROOPS, 5_000_000),
    cpuStop: int(env.CPU_STOP, 97_000_000),
    stateFile: env.PAUSE_STATE_FILE || path.join(here, '..', 'data', 'pause-state.json'),
    journalFile: env.PAUSE_JOURNAL_FILE || path.join(here, '..', 'data', 'pause-journal.jsonl'),
    usdcDepegBps: int(env.USDC_DEPEG_BPS, w.usdcDepegBps ?? 50),            // alert only (inside the band)
    usdcBand: (w.usdcBand1e6 ?? [980_000, 1_020_000]).map(BigInt),          // pinned fallback; no env override
    eurcVsEurBps: int(env.EURC_VS_EUR_BPS, w.eurcVsEurBps ?? 100),
    staleAfterSeconds: int(env.STALE_AFTER_SECONDS, w.staleAfterSeconds ?? 900),
    confirmations: int(env.PAUSE_CONFIRMATIONS, w.confirmations ?? 2),
    clearCycles: int(env.PAUSE_CLEAR_CYCLES, w.clearCycles ?? 20),
  };
  if (!cfg.publicKey) throw Error('PAUSE_PUBLIC_KEY required');
  if (!cfg.keyFile || !path.isAbsolute(cfg.keyFile)) throw Error('PAUSE_KEY_FILE (absolute path) required');
  if (env.LIQUIDATOR_SECRET || env.LIQUIDATOR_KEY_FILE || env.SIGNER_CLI_ALIAS) throw Error('refused: liquidator credentials present in the pause watcher environment');
  if (cfg.network !== pins.network) throw Error('NETWORK_PASSPHRASE does not match pins');
  if (pins.admin && cfg.publicKey === pins.admin) throw Error('refused: pause key is the admin');
  if (cfg.liquidatorPublicKey && cfg.publicKey === cfg.liquidatorPublicKey) throw Error('refused: pause key is the liquidator key');
  if (cfg.confirmations < 1) throw Error('PAUSE_CONFIRMATIONS must be >= 1');
  if (!(cfg.usdcBand.length === 2 && cfg.usdcBand[0] > 0n && cfg.usdcBand[0] <= cfg.usdcBand[1])) throw Error('pauseWatcher.usdcBand1e6 must be [lo, hi] with 0 < lo <= hi');
  cfg.neverTouch = new Set(pins.neverTouch || []);
  return cfg;
}
