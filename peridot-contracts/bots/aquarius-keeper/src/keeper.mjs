import { scAddress } from "./stellar.mjs";

export class AquariusKeeper {
  constructor(config, client, logger = console, now = () => Date.now()) {
    this.config = config;
    this.client = client;
    this.logger = logger;
    this.now = now;
    const initialHarvest = this.now();
    this.nextHarvest = new Map(
      config.targets.map((target) => [
        target.vaultId,
        config.harvestOnStart ? initialHarvest : initialHarvest + config.harvestIntervalMs,
      ]),
    );
  }

  async step(target, action, contractId, method, args = []) {
    this.logger.info("keeper step started", { target: target.label, action, contractId });
    try {
      await this.client.execute(contractId, method, args);
      this.logger.info("keeper step completed", { target: target.label, action, contractId });
      return true;
    } catch (error) {
      this.logger.error("keeper step failed", {
        target: target.label,
        action,
        contractId,
        error: error.message,
      });
      return false;
    }
  }

  async runTarget(target) {
    let failures = 0;
    if (!(await this.step(target, "refresh_nav_root", target.vaultId, "refresh_nav_root"))) {
      failures += 1;
    }

    const now = this.now();
    const harvestDue = now >= this.nextHarvest.get(target.vaultId);
    if (this.config.runHarvest && harvestDue) {
      const succeeded = await this.step(
        target,
        "harvest",
        target.vaultId,
        "harvest",
        [scAddress(this.config.publicKey)],
      );
      if (succeeded) {
        this.nextHarvest.set(target.vaultId, now + this.config.harvestIntervalMs);
      } else {
        this.nextHarvest.set(target.vaultId, now + this.config.harvestRetryMs);
        failures += 1;
      }
    }

    if (
      !(await this.step(
        target,
        "refresh_boosted_underlying",
        target.marketId,
        "refresh_boosted_underlying",
      ))
    ) {
      failures += 1;
    }
    return failures;
  }

  async runCycle() {
    let failures = 0;
    for (const target of this.config.targets) {
      failures += await this.runTarget(target);
    }
    this.logger.info("keeper cycle completed", {
      targets: this.config.targets.length,
      failures,
      dryRun: this.config.dryRun,
    });
    return failures;
  }
}
