import { Networks } from "@stellar/stellar-sdk";
import { StellarClient, scU64 } from "./stellar.mjs";

const DEFAULT_CONTROLLER = "CAKKHUGHP67UA4F42QOYPKNGRSBJEOE62MGDXA2UURTEYFOQGSMIRUFO";
const DEFAULT_PUBLIC_KEY = "GBCSJA3NTK34D7NHBDHDQZE7G7BCBZLBFJN72YNS4G3K7ACF3NC4NEY4";

function enumTag(value) {
  if (Array.isArray(value)) return value[0];
  if (value && typeof value === "object") return Object.keys(value)[0];
  return String(value);
}

function stringify(value) {
  return JSON.stringify(value, (_, item) => (typeof item === "bigint" ? item.toString() : item), 2);
}

const config = {
  rpcUrl: process.env.RPC_URL ?? "https://soroban-testnet.stellar.org",
  horizonUrl: process.env.HORIZON_URL ?? "https://horizon-testnet.stellar.org",
  networkPassphrase: process.env.NETWORK_PASSPHRASE ?? Networks.TESTNET,
  controllerId: process.env.MARGIN_CONTROLLER_ID ?? DEFAULT_CONTROLLER,
  publicKey: process.env.INVENTORY_PUBLIC_KEY ?? DEFAULT_PUBLIC_KEY,
  rpcTimeoutMs: 30_000,
};

const client = new StellarClient(config, {
  warn(message, details) {
    process.stderr.write(`${message}: ${details?.error ?? "retrying"}\n`);
  },
});

const counter = BigInt(await client.read("get_position_counter"));
const active = [];
const failures = [];

for (let id = 1n; id <= counter; id += 1n) {
  try {
    const arg = [scU64(id)];
    const position = await client.read("get_position", arg);
    if (position === null || position === undefined) continue;

    const [
      perps,
      pendingPerpsOpen,
      pendingPerpsOpenExecution,
      pendingPerpsClose,
      pendingLiquidation,
    ] = await Promise.all([
      client.read("get_perps_position", arg),
      client.read("get_pending_perps_open", arg),
      client.read("get_pending_perps_open_execution", arg),
      client.read("get_pending_perps_close", arg),
      client.read("get_pending_liquidation", arg),
    ]);

    const isPerpsV3 =
      (perps !== null && perps !== undefined) ||
      (pendingPerpsOpen !== null && pendingPerpsOpen !== undefined);

    active.push({
      id: id.toString(),
      owner: String(position.owner),
      side: enumTag(position.side),
      status: enumTag(position.status),
      mode: isPerpsV3 ? "PerpsV3" : "LegacyOrMarginV2",
      pendingOpen: pendingPerpsOpen !== null && pendingPerpsOpen !== undefined,
      pendingOpenExecution:
        pendingPerpsOpenExecution !== null && pendingPerpsOpenExecution !== undefined,
      pendingOpenExpiresAt: pendingPerpsOpen?.expires_at ?? null,
      pendingClose: pendingPerpsClose !== null && pendingPerpsClose !== undefined,
      pendingLiquidation: pendingLiquidation !== null && pendingLiquidation !== undefined,
    });
  } catch (error) {
    failures.push({ id: id.toString(), error: error.message });
  }
}

const summary = {
  controllerId: config.controllerId,
  positionCounter: counter.toString(),
  activeCount: active.length,
  perpsV3Count: active.filter((position) => position.mode === "PerpsV3").length,
  legacyOrMarginV2Count: active.filter((position) => position.mode !== "PerpsV3").length,
  pendingOpenCount: active.filter((position) => position.pendingOpen).length,
  pendingOpenExecutionCount: active.filter((position) => position.pendingOpenExecution).length,
  pendingCloseCount: active.filter((position) => position.pendingClose).length,
  pendingLiquidationCount: active.filter((position) => position.pendingLiquidation).length,
  failures: failures.length,
};

process.stdout.write(`${stringify({ summary, active, failures })}\n`);

const strictUpgradeReady = process.env.STRICT_UPGRADE_READY === "true";
const hasPending =
  summary.pendingOpenCount > 0 ||
  summary.pendingCloseCount > 0 ||
  summary.pendingLiquidationCount > 0;
if (
  failures.length > 0 ||
  summary.legacyOrMarginV2Count > 0 ||
  (strictUpgradeReady && hasPending)
) {
  process.exitCode = 2;
}
