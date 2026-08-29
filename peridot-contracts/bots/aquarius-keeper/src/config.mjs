import { Keypair, Networks } from "@stellar/stellar-sdk";

export const MAINNET_TARGETS = Object.freeze([
  Object.freeze({
    label: "XLM",
    vaultId: "CB3WLG4QITFRELACDR74N63VEPICMQ35QW3DSAMF4KCFOITKOJSHH6RW",
    marketId: "CBRJTPI3327YPP57KGIZIU4Z6APBUN5F6LJ2Q3MPKCISUQJLAQFFZECZ",
  }),
  Object.freeze({
    label: "PYUSD",
    vaultId: "CANCOWOI6R2FZBDLZKUL6BUZJN3VONPZSUUSWFL3KF3MPG5INAF25EKY",
    marketId: "CBNVNCPEW2XXGBEGVMZQXBSODO5V2HMGPT5FFLVLT355SXJMYY53MLMA",
  }),
  Object.freeze({
    label: "USDC",
    vaultId: "CAQZ7XPUSOSBI66A4RPSNPEBI2EADBMBVUBSW6R2DYWC64QHDM3HKGIN",
    marketId: "CBIOHQFWKSYTRET3LJV4LTO3ZQWRQMQ7I2SJAZ62IZCQHDST4YO3AZP7",
  }),
]);

function integer(env, name, fallback, { min = 0, max = Number.MAX_SAFE_INTEGER } = {}) {
  const raw = env[name];
  const value = raw === undefined || raw === "" ? fallback : Number(raw);
  if (!Number.isInteger(value) || value < min || value > max) {
    throw new Error(`${name} must be an integer between ${min} and ${max}`);
  }
  return value;
}

function boolean(env, name, fallback = false) {
  const raw = env[name];
  if (raw === undefined || raw === "") return fallback;
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  throw new Error(`${name} must be true or false`);
}

function publicKey(value, name) {
  try {
    return Keypair.fromPublicKey(value).publicKey();
  } catch {
    throw new Error(`${name} must be a valid Stellar public key`);
  }
}

export function loadConfig(env = process.env) {
  const dryRun = boolean(env, "DRY_RUN", false);
  const secret = env.KEEPER_SECRET?.trim();
  const configuredPublicKey = env.KEEPER_PUBLIC_KEY?.trim();
  if (!secret && !dryRun) {
    throw new Error("KEEPER_SECRET is required unless DRY_RUN=true");
  }

  let keypair = null;
  if (secret) {
    try {
      keypair = Keypair.fromSecret(secret);
    } catch {
      throw new Error("KEEPER_SECRET is not a valid Stellar secret key");
    }
  }
  const resolvedPublicKey = keypair?.publicKey() ?? configuredPublicKey;
  if (!resolvedPublicKey) {
    throw new Error("KEEPER_PUBLIC_KEY is required for a dry run without a secret");
  }
  const validatedPublicKey = publicKey(resolvedPublicKey, "KEEPER_PUBLIC_KEY");
  if (configuredPublicKey && publicKey(configuredPublicKey, "KEEPER_PUBLIC_KEY") !== validatedPublicKey) {
    throw new Error("KEEPER_PUBLIC_KEY does not match KEEPER_SECRET");
  }

  const networkPassphrase = env.NETWORK_PASSPHRASE ?? Networks.PUBLIC;
  if (networkPassphrase !== Networks.PUBLIC) {
    throw new Error("Aquarius keeper is pinned to Stellar Mainnet");
  }

  return {
    rpcUrl: env.RPC_URL ?? "https://soroban-rpc.mainnet.stellar.gateway.fm",
    horizonUrl: env.HORIZON_URL ?? "https://horizon.stellar.org",
    networkPassphrase,
    keypair,
    publicKey: validatedPublicKey,
    targets: MAINNET_TARGETS,
    dryRun,
    runOnce: boolean(env, "RUN_ONCE", false),
    runHarvest: boolean(env, "RUN_HARVEST", true),
    harvestOnStart: boolean(env, "HARVEST_ON_START", true),
    pollIntervalMs: integer(env, "POLL_INTERVAL_MS", 300_000, {
      min: 30_000,
      max: 3_600_000,
    }),
    harvestIntervalMs: integer(env, "HARVEST_INTERVAL_MS", 3_600_000, {
      min: 300_000,
      max: 86_400_000,
    }),
    harvestRetryMs: integer(env, "HARVEST_RETRY_MS", 300_000, {
      min: 30_000,
      max: 3_600_000,
    }),
    inclusionFeeStroops: integer(env, "INCLUSION_FEE_STROOPS", 100_000, {
      min: 100,
      max: 10_000_000,
    }),
    rpcTimeoutMs: integer(env, "RPC_TIMEOUT_MS", 15_000, {
      min: 1_000,
      max: 120_000,
    }),
    confirmationTimeoutMs: integer(env, "CONFIRMATION_TIMEOUT_MS", 90_000, {
      min: 5_000,
      max: 300_000,
    }),
    maxConsecutiveFailedCycles: integer(env, "MAX_CONSECUTIVE_FAILED_CYCLES", 12, {
      min: 1,
      max: 1_000,
    }),
  };
}
