import { Keypair, Networks, StrKey } from "@stellar/stellar-sdk";

const DEFAULT_TESTNET_CONTROLLER =
  "CAKKHUGHP67UA4F42QOYPKNGRSBJEOE62MGDXA2UURTEYFOQGSMIRUFO";

function integer(name, fallback, { min = 0, max = Number.MAX_SAFE_INTEGER } = {}) {
  const raw = process.env[name];
  const value = raw === undefined || raw === "" ? fallback : Number(raw);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new Error(`${name} must be an integer between ${min} and ${max}`);
  }
  return value;
}

function boolean(name, fallback = false) {
  const raw = process.env[name];
  if (raw === undefined || raw === "") return fallback;
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  throw new Error(`${name} must be true or false`);
}

export function loadConfig() {
  const dryRun = boolean("DRY_RUN", false);
  const secret = process.env.LIQUIDATOR_SECRET?.trim();
  const configuredPublicKey = process.env.LIQUIDATOR_PUBLIC_KEY?.trim();
  if (!secret && !dryRun) {
    throw new Error("LIQUIDATOR_SECRET is required unless DRY_RUN=true");
  }
  const keypair = secret ? Keypair.fromSecret(secret) : null;
  const publicKey = keypair?.publicKey() ?? configuredPublicKey;
  if (!publicKey) {
    throw new Error("LIQUIDATOR_PUBLIC_KEY is required for a dry run without a secret");
  }

  const feeDistributionVaults = [...new Set((process.env.FEE_DISTRIBUTION_VAULTS ?? "")
    .split(",").map((value) => value.trim()).filter(Boolean))];
  if (feeDistributionVaults.length > 16 || feeDistributionVaults.some((id) => !StrKey.isValidContract(id))) {
    throw new Error("FEE_DISTRIBUTION_VAULTS must contain at most 16 contract addresses");
  }

  return {
    rpcUrl: process.env.RPC_URL ?? "https://soroban-testnet.stellar.org",
    horizonUrl: process.env.HORIZON_URL ?? "https://horizon-testnet.stellar.org",
    networkPassphrase: process.env.NETWORK_PASSPHRASE ?? Networks.TESTNET,
    controllerId: process.env.MARGIN_CONTROLLER_ID ?? DEFAULT_TESTNET_CONTROLLER,
    keypair,
    publicKey,
    dryRun,
    runOnce: boolean("RUN_ONCE", false),
    feeDistributionVaults,
    feeDistributionIntervalMs: integer("FEE_DISTRIBUTION_INTERVAL_MS", 300_000, { min: 30_000 }),
    minFeeUnderlying: BigInt(integer("MIN_FEE_UNDERLYING", 10_000, { min: 1 })),
    minFeePtokens: BigInt(integer("MIN_FEE_PTOKENS", 10_000, { min: 1 })),
    rpcTimeoutMs: integer("RPC_TIMEOUT_MS", 15_000, { min: 1_000, max: 120_000 }),
    pollIntervalMs: integer("POLL_INTERVAL_MS", 5_000, { min: 1_000 }),
    confirmationTimeoutMs: integer("CONFIRMATION_TIMEOUT_MS", 60_000, { min: 5_000 }),
    slippageBps: integer("LIQUIDATION_SLIPPAGE_BPS", 100, { min: 0, max: 5_000 }),
    maxPositionsPerCycle: integer("MAX_POSITIONS_PER_CYCLE", 25, { min: 1, max: 1_000 }),
    bootstrapMaxPositionId: process.env.BOOTSTRAP_MAX_POSITION_ID
      ? BigInt(process.env.BOOTSTRAP_MAX_POSITION_ID)
      : null,
    eventStartLedger: process.env.EVENT_START_LEDGER
      ? integer("EVENT_START_LEDGER", 0, { min: 1 })
      : null,
    stateFile: process.env.STATE_FILE ?? "./data/state.json",
  };
}
