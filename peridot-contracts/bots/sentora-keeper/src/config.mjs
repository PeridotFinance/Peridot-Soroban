// SKELETON. Configuration for the Sentora group keeper (separate DigitalOcean worker, separate signer).
// Never commit KEEPER_SECRET; it is a DigitalOcean encrypted RUN_TIME secret. DRY_RUN=true never loads it.
const req = (env, k) => { const v = env[k]?.trim(); if (!v) throw new Error(`${k} is required`); return v; };
const num = (env, k, d) => { const v = env[k] === undefined ? d : Number(env[k]); if (!Number.isFinite(v) || v <= 0) throw new Error(`${k} invalid`); return v; };
const list = (env, k) => req(env, k).split(',').map(s => s.trim()).filter(Boolean);

// Signers this keeper must NEVER share (existing keepers / reporters / admins). Extend when new ones are added.
export const FORBIDDEN_SIGNERS = new Set([
  'GBLVLGECHVZAKOJ6XLLF4YRNNBTAZBM4O6ZMIL336BYIHKUZLACLXSE3', // aquarius-keeper (fee-only refresh/harvest signer)
  'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP', // Peridot admin
]);

// Pinned Sentora facts (Mainnet, 2026-11-25). A change of any of these is a kill trigger, not a config change.
export const SENTORA = {
  vaultWasm: 'ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f',
  escrowWasm: 'f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299',
  escrow: 'CCZSNGVMEHRKMMMQGVDHAXT3KSWIMPEPRRTUE2GU3F6TW4XA2Y6EMHN3',
  admin: 'GCOB2OVZWDLW3GHWA3PAON5PTMKPMPV6236Z3DQEA2OABRCECCMQZ5DI',
  adminSigners: 7, adminWeight: 10, adminThresholds: {med: 40, high: 40},
};

export function loadConfig(env = process.env) {
  const dryRun = (env.DRY_RUN ?? 'true').trim() !== 'false';
  const cfg = {
    rpcUrl: req(env, 'RPC_URL'),
    networkPassphrase: req(env, 'NETWORK_PASSPHRASE'),
    publicKey: req(env, 'KEEPER_PUBLIC_KEY'),
    dryRun,
    controller: req(env, 'CONTROLLER'),
    pegOracle: req(env, 'PEG_ORACLE'),
    pyusd: req(env, 'PYUSD_TOKEN'),
    receipts: list(env, 'RECEIPTS'), // USDC,PYUSD,XLM receipt vaults
    adapters: list(env, 'ADAPTERS'), // matching sentora-strategy adapters (keeper = guardian on each)
    sentoraVaults: list(env, 'SENTORA_VAULTS'),
    refreshMs: num(env, 'REFRESH_INTERVAL_MS', 240_000), // < 300 s health-cache bound
    pegMs: num(env, 'PEG_INTERVAL_MS', 60_000),
    monitorMs: num(env, 'MONITOR_INTERVAL_MS', 60_000),
    claimMs: num(env, 'CLAIM_INTERVAL_MS', 3_600_000),
    ttlMs: num(env, 'TTL_INTERVAL_MS', 86_400_000),
    inclusionFeeStroops: num(env, 'INCLUSION_FEE_STROOPS', 10_000),
    maxFeePerTxStroops: num(env, 'MAX_FEE_PER_TX_STROOPS', 2_000_000), // 0.2 XLM incl. resources
    dailyFeeBudgetStroops: num(env, 'DAILY_FEE_BUDGET_STROOPS', 50_000_000), // 5 XLM/day hard stop (estimate ~3.5)
    minBalanceStroops: num(env, 'MIN_BALANCE_STROOPS', 100_000_000), // warn below 10 XLM spendable
    proofApi: env.SENTORA_PROOF_API?.trim() || null, // unknown until Sentora confirms contract eligibility
    alertWebhook: env.ALERT_WEBHOOK?.trim() || null,
  };
  if (cfg.receipts.length !== cfg.adapters.length || cfg.adapters.length !== cfg.sentoraVaults.length) {
    throw new Error('RECEIPTS, ADAPTERS and SENTORA_VAULTS must align');
  }
  if (FORBIDDEN_SIGNERS.has(cfg.publicKey)) throw new Error('KEEPER_PUBLIC_KEY must be a dedicated Sentora-keeper key');
  if (!dryRun) {
    const secret = req(env, 'KEEPER_SECRET');
    cfg.secret = secret; // validated against publicKey by the signer module (not part of the skeleton)
  }
  return cfg;
}
