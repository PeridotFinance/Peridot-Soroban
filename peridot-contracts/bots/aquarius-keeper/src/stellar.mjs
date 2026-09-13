import {
  Contract,
  Horizon,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
} from "@stellar/stellar-sdk";
import { harvestDecision } from "./harvest.mjs";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const scAddress = (value) => nativeToScVal(value, { type: "address" });

function stringify(value) {
  return JSON.stringify(value, (_, item) => (typeof item === "bigint" ? item.toString() : item));
}

// Never serialize SDK transaction objects: they contain full signed envelopes
// and can exceed the log transport limit, hiding subsequent failure records.
export function transactionFailure(result) {
  let code;
  try { code = result.resultXdr?.result().switch().name; } catch { /* optional XDR */ }
  return stringify({ status: result.status, hash: result.txHash ?? result.hash, ledger: result.ledger, code });
}

const TX_LIFETIME_SECONDS = 60;
const NAV_SAFETY_SECONDS = 30;

export function navIsFresh(timestamp, maxAge, nowSeconds) {
  return typeof timestamp === "bigint" && timestamp > 0n &&
    typeof maxAge === "bigint" && maxAge > 0n &&
    timestamp <= nowSeconds &&
    nowSeconds - timestamp + BigInt(TX_LIFETIME_SECONDS + NAV_SAFETY_SECONDS) < maxAge;
}

export class StellarClient {
  constructor(config, logger = console) {
    this.server = new rpc.Server(config.rpcUrl, {
      allowHttp: false,
      timeout: config.rpcTimeoutMs,
    });
    this.horizon = new Horizon.Server(config.horizonUrl, {
      allowHttp: false,
      appName: "peridot-aquarius-keeper",
    });
    this.config = config;
    this.logger = logger;
  }

  async retryRead(label, operation) {
    let delayMs = 500;
    for (let attempt = 1; attempt <= 4; attempt += 1) {
      try {
        return await operation();
      } catch (error) {
        if (attempt === 4) throw error;
        this.logger.warn(`${label} failed; retrying`, { attempt, error: error.message });
        await sleep(delayMs);
        delayMs *= 2;
      }
    }
    throw new Error(`${label} retry loop exhausted`);
  }

  async sourceAccount() {
    try {
      return await this.retryRead("RPC account read", () =>
        this.server.getAccount(this.config.publicKey),
      );
    } catch (error) {
      this.logger.warn("RPC account read failed; using Horizon fallback", {
        error: error.message,
      });
      return this.retryRead("Horizon account read", () =>
        this.horizon.loadAccount(this.config.publicKey),
      );
    }
  }

  async buildTransaction(contractId, method, args) {
    const account = await this.sourceAccount();
    const contract = new Contract(contractId);
    return new TransactionBuilder(account, {
      fee: String(this.config.inclusionFeeStroops),
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(contract.call(method, ...args))
      .setTimeout(TX_LIFETIME_SECONDS)
      .build();
  }

  async freshNav(vaultId) {
    const params = await this.read(vaultId, "get_params");
    const timestamp = await this.read(vaultId, "get_last_nav_root_at");
    const now = BigInt(Math.floor(Date.now() / 1000));
    if (navIsFresh(timestamp, params?.nav_root_max_age, now)) return true;
    this.logger.warn("NAV freshness guard deferred dependent transactions", {
      vaultId, timestamp: String(timestamp), maxAge: String(params?.nav_root_max_age),
    });
    return false;
  }

  navTarget(contractId, method) {
    if (!["harvest", "rebalance", "refresh_boosted_underlying"].includes(method)) return null;
    const target = this.config.targets.find(t => method === "refresh_boosted_underlying"
      ? t.marketId === contractId : t.vaultId === contractId);
    if (!target) throw new Error("dependent transaction is not a configured keeper target");
    return target.vaultId;
  }

  async execute(contractId, method, args = []) {
    const navTarget = this.navTarget(contractId, method);
    if (navTarget && !(await this.freshNav(navTarget))) {
      return { deferred: true, method, reason: "stale_nav" };
    }
    let harvestInputs;
    if (method === "harvest") {
      const lastHarvest = await this.read(contractId, "get_last_harvest");
      const params = await this.read(contractId, "get_params");
      if (typeof lastHarvest !== "bigint" || lastHarvest < 0n ||
          typeof params.harvest_cooldown !== "bigint" || params.harvest_cooldown < 0n) {
        throw new Error("invalid on-chain harvest cooldown");
      }
      if (lastHarvest > 0n && BigInt(Math.floor(Date.now() / 1000)) <
          lastHarvest + params.harvest_cooldown + 30n) {
        return { deferred: true, method, reason: "cooldown" };
      }
      const underlying = await this.read(contractId, "get_underlying");
      const idle = await this.read(underlying, "balance", [scAddress(contractId)]);
      harvestInputs = { underlying, idle };
    }
    const transaction = await this.buildTransaction(contractId, method, args);
    let harvestSimulation;
    if (this.config.dryRun || harvestInputs) {
      const simulation = await this.retryRead(`${method} simulation`, () =>
        this.server.simulateTransaction(transaction),
      );
      if (rpc.Api.isSimulationError(simulation)) {
        throw new Error(`${method} simulation failed: ${simulation.error}`);
      }
      if (harvestInputs) {
        if (!simulation.result) throw new Error("harvest simulation returned no result");
        const decision = harvestDecision(
          simulation.events, contractId, harvestInputs.underlying, harvestInputs.idle,
          this.config.harvestMinUnderlyingRaw,
        );
        for (const skipped of decision.skips) {
          this.logger.warn("reward conversion blocked in simulation", {
            contractId, rewardToken: skipped.reward_token,
            rewardAmount: String(skipped.reward_amount), reason: skipped.reason,
          });
        }
        this.logger.info("harvest threshold checked", {
          contractId, ready: decision.ready, expectedSettlementRaw: String(decision.peak),
          minimumSettlementRaw: String(decision.minimum),
        });
        if (!decision.ready) return { deferred: true, method, reason: "below_threshold" };
        harvestSimulation = simulation;
      }
      if (this.config.dryRun) {
        this.logger.info("transaction simulated", {
          contractId,
          method,
          latestLedger: simulation.latestLedger,
          minResourceFee: simulation.minResourceFee,
        });
        return { dryRun: true, method };
      }
    }

    // Sign the exact simulation that passed the gate, without a second preparation.
    const prepared = harvestSimulation ? rpc.assembleTransaction(transaction, harvestSimulation).build()
      : await this.retryRead(`${method} preparation`, () =>
      this.server.prepareTransaction(transaction),
    );
    // Preparation and other read-only probes can take time. Check again before
    // signing; retain 60s transaction lifetime plus 30s margin below cache expiry.
    if (navTarget && !(await this.freshNav(navTarget))) {
      return { deferred: true, method, reason: "stale_nav" };
    }
    prepared.sign(this.config.keypair);
    const submitted = await this.server.sendTransaction(prepared);
    if (submitted.status === "ERROR" || submitted.status === "TRY_AGAIN_LATER") {
      throw new Error(`${method} submission failed: ${transactionFailure(submitted)}`);
    }

    const result = await this.waitForTransaction(submitted.hash);
    if (result.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
      throw new Error(`${method} failed on-chain: ${transactionFailure(result)}`);
    }
    if (method === "harvest") {
      for (const event of result.events?.contractEventsXdr?.flat() ?? []) {
        const topics = event.body().v0().topics().map(scValToNative);
        if (topics[0] === "harvest_skipped") {
          this.logger.warn("reward conversion skipped on-chain", {
            contractId, hash: submitted.hash,
            outcome: stringify(scValToNative(event.body().v0().data())),
          });
        }
      }
    }
    this.logger.info("transaction confirmed", {
      contractId,
      method,
      hash: submitted.hash,
      ledger: result.ledger,
    });
    if (method === "refresh_nav_root" && !(await this.freshNav(contractId))) {
      return { deferred: true, method, reason: "stale_nav", hash: submitted.hash, ledger: result.ledger };
    }
    return { hash: submitted.hash, ledger: result.ledger };
  }

  async read(contractId, method, args = []) {
    const transaction = await this.buildTransaction(contractId, method, args);
    const simulation = await this.retryRead(`${method} simulation`, () =>
      this.server.simulateTransaction(transaction),
    );
    if (rpc.Api.isSimulationError(simulation)) {
      throw new Error(`${method} simulation failed: ${simulation.error}`);
    }
    if (!simulation.result) {
      throw new Error(`${method} simulation returned no result`);
    }
    return scValToNative(simulation.result.retval);
  }

  async waitForTransaction(hash) {
    const deadline = Date.now() + this.config.confirmationTimeoutMs;
    while (Date.now() < deadline) {
      try {
        const result = await this.server.getTransaction(hash);
        if (result.status !== rpc.Api.GetTransactionStatus.NOT_FOUND) return result;
      } catch (error) {
        this.logger.warn("transaction status read failed; retrying", {
          hash,
          error: error.message,
        });
      }
      await sleep(1_000);
    }
    throw new Error(`transaction confirmation timed out; inspect hash before retrying: ${hash}`);
  }
}
