import { loadConfig } from "./config.mjs";
import { MarginLiquidationKeeper } from "./keeper.mjs";
import { loadState, saveState } from "./state.mjs";
import { StellarClient } from "./stellar.mjs";
import { openJournal } from "./journal.mjs";
import { requireSafe } from "./safety.mjs";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const logger = {
  info(message, fields = {}) {
    console.log(JSON.stringify({ level: "info", time: new Date().toISOString(), message, ...fields }));
  },
  warn(message, fields = {}) {
    console.warn(JSON.stringify({ level: "warn", time: new Date().toISOString(), message, ...fields }));
  },
  error(message, fields = {}) {
    console.error(JSON.stringify({ level: "error", time: new Date().toISOString(), message, ...fields }));
  },
};

const config = loadConfig();
const state = await loadState(config.stateFile);
// Rebuild on every restart: the local cursor is disposable, unlike the journal.
state.initialized = false;
const journal = config.dryRun ? null : await openJournal(config);
const client = new StellarClient(config, logger, journal);
const keeper = new MarginLiquidationKeeper(
  config,
  client,
  state,
  () => saveState(config.stateFile, state),
  logger,
);

let stopping = false;
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    stopping = true;
    client.stopped = true;
    logger.info("shutdown requested", { signal });
  });
}

logger.info("margin liquidation keeper started", {
  controllerId: config.controllerId,
  publicKey: config.publicKey,
  dryRun: config.dryRun,
});

while (!stopping) {
  const started = Date.now();
  try {
    requireSafe((await client.server.getNetwork()).passphrase === config.networkPassphrase, "RPC network mismatch");
    await client.reconcile();
    await keeper.runCycle();
    logger.info("keeper cycle complete", { dryRun: config.dryRun,
      activePositions: state.activePositionIds.length, lastEventLedger: state.lastEventLedger });
  } catch (error) {
    logger.error("keeper cycle failed", { error: error.fatal ? error.message : "read or simulation failed" });
    if (error.fatal) { process.exitCode = 1; break; }
  }
  if (config.runOnce) break;
  const remaining = Math.max(0, config.pollIntervalMs - (Date.now() - started));
  if (!stopping) await sleep(remaining);
}

try { await saveState(config.stateFile, state); }
finally { await journal?.close(); }
logger.info("margin liquidation keeper stopped");
