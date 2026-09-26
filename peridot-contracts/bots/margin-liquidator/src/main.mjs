import { loadConfig } from "./config.mjs";
import { MarginLiquidationKeeper } from "./keeper.mjs";
import { loadState, saveState } from "./state.mjs";
import { StellarClient } from "./stellar.mjs";

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
const client = new StellarClient(config, logger);
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
    await keeper.runCycle();
  } catch (error) {
    logger.error("keeper cycle failed", { error: error.message });
  }
  if (config.runOnce) break;
  const remaining = Math.max(0, config.pollIntervalMs - (Date.now() - started));
  if (!stopping) await sleep(remaining);
}

await saveState(config.stateFile, state);
logger.info("margin liquidation keeper stopped");
