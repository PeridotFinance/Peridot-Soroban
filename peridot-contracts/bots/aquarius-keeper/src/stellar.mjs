import {
  Contract,
  Horizon,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
} from "@stellar/stellar-sdk";
import { harvestDecision } from "./harvest.mjs";
import { feeCoverage } from "./fees.mjs";
import { convertedRewards, harvestProfitability, FEE_ORACLE, NATIVE, USDC, PYUSD } from "./profitability.mjs";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export const scAddress = (value) => nativeToScVal(value, { type: "address" });

function stringify(value) {
  return JSON.stringify(value, (_, item) => (typeof item === "bigint" ? item.toString() : item));
}

// Never serialize SDK transaction objects: they contain full signed envelopes
// and can exceed the log transport limit, hiding subsequent failure records.
export function transactionFailure(result) {
  let code;
  try { code = (result.errorResult ?? result.resultXdr)?.result().switch().name; } catch { /* optional XDR */ }
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

  async canPayFee(prepared) {
    const [account, page] = await Promise.all([
      this.retryRead("fee balance read", () => this.horizon.loadAccount(this.config.publicKey)),
      this.retryRead("base reserve read", () => this.horizon.ledgers().order("desc").limit(1).call()),
    ]);
    if (account.account_id !== this.config.publicKey) throw new Error("fee account mismatch");
    const ledger = page.records[0];
    const closed = Date.parse(ledger?.closed_at);
    const now = Date.now();
    if (!Number.isFinite(closed) || now - closed > 60_000 || closed > now + 5_000) {
      throw new Error("fee reserve ledger is stale or invalid");
    }
    const coverage = feeCoverage(account, ledger.base_reserve_in_stroops, prepared.fee);
    if (!coverage.sufficient || coverage.available < 10_000_000n) {
      this.logger.warn("keeper fee balance low", {
        publicKey: this.config.publicKey,
        availableStroops: String(coverage.available),
        requiredStroops: String(coverage.required),
        reserveStroops: String(coverage.reserve),
        sufficient: coverage.sufficient,
      });
    }
    return coverage.sufficient;
  }

  async harvestPrices(underlying) {
    if (underlying === NATIVE) return null;
    if (underlying !== USDC && underlying !== PYUSD) throw new Error("unsupported settlement asset");
    const asset = id => nativeToScVal(["Stellar", id], { type: ["symbol", "address"] });
    const [base, decimals, xlm, price] = await Promise.all([
      this.read(FEE_ORACLE, "base"), this.read(FEE_ORACLE, "decimals"),
      this.read(FEE_ORACLE, "lastprice", [asset(NATIVE)]),
      underlying === PYUSD ? this.read(FEE_ORACLE, "lastprice", [asset(PYUSD)]) : null,
    ]);
    if (!Array.isArray(base) || base.length !== 2 || base[0] !== "Stellar" || base[1] !== USDC || decimals !== 14) {
      throw new Error("unexpected fee oracle denomination");
    }
    return { xlm, asset: price };
  }

  checkHarvestProfit(rewards, prepared, underlying, prices) {
    return harvestProfitability(rewards, prepared.fee, underlying, prices, Math.floor(Date.now() / 1000));
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
    let harvestPrepared, rewardProceeds, feePrices;
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
        // This gate applies ONLY to optional harvest. Cache maintenance and
        // user withdrawal calls never depend on these prices or economics.
        try {
          rewardProceeds = convertedRewards(simulation.events, contractId);
          harvestPrepared = rpc.assembleTransaction(transaction, simulation).build();
          feePrices = await this.harvestPrices(harvestInputs.underlying);
          const profit = this.checkHarvestProfit(rewardProceeds, harvestPrepared, harvestInputs.underlying, feePrices);
          this.logger.info("harvest profitability checked", {
            contractId, ready: profit.ready, convertedRewardRaw: String(rewardProceeds),
            conservativeXlmStroops: String(profit.conservativeXlm), requiredXlmStroops: String(profit.requiredXlm),
            preparedFeeStroops: harvestPrepared.fee,
          });
          if (!profit.ready) return { deferred: true, method, reason: "unprofitable_harvest" };
        } catch {
          this.logger.warn("harvest profitability unavailable", { contractId });
          return { deferred: true, method, reason: "harvest_economics_unavailable" };
        }
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
    const prepared = harvestSimulation ? harvestPrepared
      : await this.retryRead(`${method} preparation`, () =>
      this.server.prepareTransaction(transaction),
    );
    // Defer before signing if the native reserve leaves too little for this
    // exact prepared fee. No automatic funding or reduced-fee retry.
    if (!(await this.canPayFee(prepared))) {
      return { deferred: true, method, reason: "insufficient_fee_balance" };
    }
    // Preparation and other read-only probes can take time. Check again before
    // signing; retain 60s transaction lifetime plus 30s margin below cache expiry.
    if (navTarget && !(await this.freshNav(navTarget))) {
      return { deferred: true, method, reason: "stale_nav" };
    }
    const expires = Number(prepared.timeBounds?.maxTime);
    if (!Number.isSafeInteger(expires) || expires <= 0) {
      throw new Error("prepared transaction requires a finite expiry");
    }
    if (Date.now() + 10_000 >= expires * 1000) {
      return { deferred: true, method, reason: "transaction_expiring" };
    }
    if (harvestInputs) {
      try {
        if (!this.checkHarvestProfit(rewardProceeds, prepared, harvestInputs.underlying, feePrices).ready) {
          return { deferred: true, method, reason: "unprofitable_harvest" };
        }
      } catch {
        return { deferred: true, method, reason: "harvest_economics_unavailable" };
      }
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
