import { loadConfig } from "./config.mjs";
import { AquariusKeeper } from "./keeper.mjs";
import { StellarClient } from "./stellar.mjs";

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

let stopping = false;
let wake = null;
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    stopping = true;
    wake?.();
    logger.info("shutdown requested", { signal });
  });
}

async function wait(ms) {
  await new Promise((resolve) => {
    const timer = setTimeout(resolve, ms);
    wake = () => {
      clearTimeout(timer);
      resolve();
    };
  });
  wake = null;
}

try {
  const config = loadConfig();
  const client = new StellarClient(config, logger);
  const keeper = new AquariusKeeper(config, client, logger);
  let consecutiveFailedCycles = 0;

  logger.info("Aquarius keeper started", {
    publicKey: config.publicKey,
    dryRun: config.dryRun,
    targets: config.targets.map((target) => ({
      label: target.label,
      vaultId: target.vaultId,
      marketId: target.marketId,
    })),
  });

  while (!stopping) {
    const started = Date.now();
    const failures = await keeper.runCycle();
    if (failures > 0) {
      consecutiveFailedCycles += 1;
    } else {
      consecutiveFailedCycles = 0;
    }

    if (config.runOnce) {
      if (failures > 0) process.exitCode = 1;
      break;
    }
    if (consecutiveFailedCycles >= config.maxConsecutiveFailedCycles) {
      throw new Error(
        `keeper failed ${consecutiveFailedCycles} consecutive cycles; exiting for operator alert`,
      );
    }

    const remaining = Math.max(0, config.pollIntervalMs - (Date.now() - started));
    if (!stopping) await wait(remaining);
  }
  logger.info("Aquarius keeper stopped");
} catch (error) {
  logger.error("Aquarius keeper terminated", { error: error.message });
  process.exitCode = 1;
}
