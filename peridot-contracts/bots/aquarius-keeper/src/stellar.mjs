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
      .setTimeout(60)
      .build();
  }

  async execute(contractId, method, args = []) {
    let harvestInputs;
    if (method === "harvest") {
      const lastHarvest = await this.read(contractId, "get_last_harvest");
      const params = await this.read(contractId, "get_params");
      if (typeof lastHarvest !== "bigint" || lastHarvest < 0n ||
          !Number.isSafeInteger(params.harvest_cooldown) || params.harvest_cooldown < 0) {
        throw new Error("invalid on-chain harvest cooldown");
      }
      if (lastHarvest > 0n && BigInt(Math.floor(Date.now() / 1000)) <
          lastHarvest + BigInt(params.harvest_cooldown) + 30n) {
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
    prepared.sign(this.config.keypair);
    const submitted = await this.server.sendTransaction(prepared);
    if (submitted.status === "ERROR" || submitted.status === "TRY_AGAIN_LATER") {
      throw new Error(`${method} submission failed: ${stringify(submitted)}`);
    }

    const result = await this.waitForTransaction(submitted.hash);
    if (result.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
      throw new Error(`${method} failed on-chain: ${stringify(result)}`);
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
