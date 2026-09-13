import {
  BASE_FEE,
  Contract,
  Horizon,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
} from "@stellar/stellar-sdk";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const scAddress = (value) => nativeToScVal(value, { type: "address" });
export const scU64 = (value) => nativeToScVal(BigInt(value), { type: "u64" });
export const scU128 = (value) => nativeToScVal(BigInt(value), { type: "u128" });

function stringify(value) {
  return JSON.stringify(value, (_, item) => (typeof item === "bigint" ? item.toString() : item));
}

export class StellarClient {
  constructor(config, logger = console) {
    this.server = new rpc.Server(config.rpcUrl, {
      allowHttp: config.rpcUrl.startsWith("http://"),
      timeout: config.rpcTimeoutMs,
    });
    this.horizon = new Horizon.Server(config.horizonUrl, {
      allowHttp: config.horizonUrl.startsWith("http://"),
      appName: "peridot-margin-liquidator",
    });
    this.contract = new Contract(config.controllerId);
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

  latestLedger() {
    return this.retryRead("getLatestLedger", () => this.server.getLatestLedger());
  }

  getEvents(request) {
    return this.retryRead("getEvents", () => this.server.getEvents(request));
  }

  async buildTransaction(method, args) {
    let account;
    try {
      account = await this.retryRead("getAccount", () =>
        this.server.getAccount(this.config.publicKey),
      );
    } catch (error) {
      this.logger.warn("RPC account read failed; using Horizon fallback", {
        error: error.message,
      });
      account = await this.retryRead("Horizon loadAccount", () =>
        this.horizon.loadAccount(this.config.publicKey),
      );
    }
    return new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: this.config.networkPassphrase,
    })
      .addOperation(this.contract.call(method, ...args))
      .setTimeout(60)
      .build();
  }

  async read(method, args = []) {
    const transaction = await this.buildTransaction(method, args);
    const simulation = await this.retryRead(`${method} simulation`, () =>
      this.server.simulateTransaction(transaction),
    );
    if (rpc.Api.isSimulationError(simulation)) {
      throw new Error(`${method} simulation failed: ${simulation.error}`);
    }
    if (!simulation.result) return null;
    return scValToNative(simulation.result.retval);
  }

  async submit(method, args = []) {
    const transaction = await this.buildTransaction(method, args);
    if (this.config.dryRun) {
      const simulation = await this.server.simulateTransaction(transaction);
      if (rpc.Api.isSimulationError(simulation)) {
        throw new Error(`${method} dry-run failed: ${simulation.error}`);
      }
      this.logger.info(`[dry-run] ${method} simulated`, {
        latestLedger: simulation.latestLedger,
        minResourceFee: simulation.minResourceFee,
      });
      return { dryRun: true, method };
    }

    const prepared = await this.retryRead(`${method} preparation`, () =>
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
    this.logger.info(`${method} confirmed`, { hash: submitted.hash, ledger: result.ledger });
    return result;
  }

  async waitForTransaction(hash) {
    const deadline = Date.now() + this.config.confirmationTimeoutMs;
    while (Date.now() < deadline) {
      const result = await this.server.getTransaction(hash);
      if (result.status !== rpc.Api.GetTransactionStatus.NOT_FOUND) return result;
      await sleep(1_000);
    }
    throw new Error(`transaction confirmation timed out: ${hash}`);
  }
}
