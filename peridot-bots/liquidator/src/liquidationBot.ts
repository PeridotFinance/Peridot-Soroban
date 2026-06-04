import { Keypair, rpc, scValToNative, xdr } from '@stellar/stellar-sdk';

import type { BotConfig, MarketConfig } from './config.js';
import { SorobanClient } from './contracts.js';
import { sleep, toAddress, toU128 } from './utils.js';

interface BorrowerState {
  lastSeen: number;
  lastEvaluated?: number;
  failures: number;
}

interface LiquidationPlan {
  borrower: string;
  repayMarket: MarketConfig;
  collateralMarket: MarketConfig;
  repayAmount: bigint;
  seizeAmount: bigint;
}

// Remove borrowers from the map after this long with no activity and no debt.
const BORROWER_STALE_MS = 24 * 60 * 60 * 1000; // 24 hours
const MAX_FAILURE_DELAY_MS = 60_000;

export class LiquidationBot {
  private readonly borrowerState = new Map<string, BorrowerState>();
  private readonly liquidator: Keypair;
  private cursor?: string;
  private startLedger?: number;
  private readonly contractIds: string[];

  constructor(
    private readonly config: BotConfig,
    private readonly server: rpc.Server,
    private readonly contracts: SorobanClient,
  ) {
    this.liquidator = Keypair.fromSecret(config.liquidatorSecret);
    this.contractIds = [
      config.peridottrollerId,
      ...config.markets.map(m => m.vaultId),
    ];
  }

  async start(): Promise<void> {
    await this.bootstrapCursor();
    while (true) {
      try {
        await this.pollEvents();
      } catch (error) {
        console.error(`[events] ${formatError(error)}`);
      }

      try {
        await this.scanBorrowers();
      } catch (error) {
        console.error(`[scan] ${formatError(error)}`);
      }

      this.cleanupStaleBorrowers();
      await sleep(this.config.pollIntervalMs);
    }
  }

  private async bootstrapCursor(): Promise<void> {
    const latest = await this.server.getLatestLedger();
    this.startLedger = Math.max(0, latest.sequence - this.config.eventBacklog);
    console.info(
      `[bot] starting at ledger ${latest.sequence} (backlog ${this.config.eventBacklog})`,
    );
  }

  private async pollEvents(): Promise<void> {
    const request: rpc.Server.GetEventsRequest = {
      filters: [{ type: 'contract', contractIds: this.contractIds }],
      ...(this.cursor ? { cursor: this.cursor } : { startLedger: this.startLedger }),
      limit: this.config.eventPageSize,
    };

    const res = await this.server.getEvents(request);

    // Always advance cursor even on empty pages so we don't re-scan old ledgers.
    if (res.cursor) {
      this.cursor = res.cursor;
    }

    for (const event of res.events) {
      this.handleEvent(event);
    }
  }

  private handleEvent(event: rpc.Api.EventResponse): void {
    if (!Array.isArray(event.topic) || event.topic.length === 0) {
      return;
    }

    const topics = event.topic.map(t => scValToNative(t));
    const eventName = topics[0];
    if (typeof eventName !== 'string') {
      return;
    }

    const name = eventName.toLowerCase();

    if (name === 'borrow_event') {
      // BorrowEvent: topics = ["borrow_event", borrower]
      const borrower = extractAddress(topics[1]);
      if (borrower) this.trackBorrower(borrower);
    } else if (name === 'repay_borrow') {
      // RepayBorrow: topics = ["repay_borrow", payer, borrower]
      // Track the borrower (topics[2]), not the payer (topics[1]).
      const borrower = extractAddress(topics[2]);
      if (borrower) this.refreshBorrower(borrower);
    } else if (name === 'market_entered') {
      // MarketEntered: topics = ["market_entered", account, market]
      const account = extractAddress(topics[1]);
      if (account) this.trackBorrower(account);
    } else if (name === 'market_exited') {
      // MarketExited: topics = ["market_exited", account, market]
      // Do NOT remove borrower here — they may still have debt in other markets.
      // Cleanup happens in cleanupStaleBorrowers() after verifying zero debt.
      const account = extractAddress(topics[1]);
      if (account) this.refreshBorrower(account);
    } else if (name === 'mint') {
      // Mint (deposit): topics = ["mint", minter]
      const minter = extractAddress(topics[1]);
      if (minter) this.trackBorrower(minter);
    }
  }

  private trackBorrower(address: string): void {
    const now = Date.now();
    const state = this.borrowerState.get(address);
    if (state) {
      state.lastSeen = now;
    } else {
      this.borrowerState.set(address, { lastSeen: now, failures: 0 });
      console.info(`[events] tracking ${address}`);
    }
  }

  private refreshBorrower(address: string): void {
    const state = this.borrowerState.get(address);
    if (state) {
      state.lastSeen = Date.now();
      // Force re-evaluation on next scan cycle.
      state.lastEvaluated = undefined;
    }
  }

  private async scanBorrowers(): Promise<void> {
    const now = Date.now();
    for (const [borrower, state] of this.borrowerState) {
      if (state.lastEvaluated && now - state.lastEvaluated < this.config.borrowerRefreshMs) {
        continue;
      }
      if (
        state.failures > 0 &&
        state.lastEvaluated &&
        now - state.lastEvaluated < Math.min(MAX_FAILURE_DELAY_MS, state.failures * 5_000)
      ) {
        continue;
      }

      state.lastEvaluated = now;
      try {
        const plan = await this.evaluateBorrower(borrower);
        if (!plan) {
          state.failures = 0;
          continue;
        }
        await this.executeLiquidation(plan);
        state.failures = 0;
      } catch (error) {
        state.failures += 1;
        console.error(`[liquidate] ${borrower} | ${formatError(error)}`);
      }
    }
  }

  private cleanupStaleBorrowers(): void {
    const now = Date.now();
    for (const [address, state] of this.borrowerState) {
      if (now - state.lastSeen < BORROWER_STALE_MS) continue;
      // Remove only if the borrower was recently evaluated and had no debt.
      // If never evaluated, keep them until the scanner confirms no shortfall.
      if (state.lastEvaluated && state.failures === 0) {
        this.borrowerState.delete(address);
        console.info(`[cleanup] removed stale borrower ${address}`);
      }
    }
  }

  private async evaluateBorrower(borrower: string): Promise<LiquidationPlan | null> {
    const [_liquidity, shortfall] = await this.contracts.call<[bigint, bigint]>(
      this.config.peridottrollerId,
      'account_liquidity',
      [toAddress(borrower)],
    );

    if (shortfall <= this.config.minShortfall) {
      return null;
    }

    const repayCandidate = await this.pickRepayMarket(borrower);
    if (!repayCandidate) return null;

    // Check liquidator has enough UNDERLYING token balance before going further.
    // repay_on_behalf calls transfer_from on the underlying token (not the pToken).
    const underlyingToken = await this.contracts
      .call<string>(repayCandidate.market.vaultId, 'get_underlying_token', [])
      .catch(() => null);
    if (!underlyingToken) {
      console.warn(`[plan] could not resolve underlying token for ${repayCandidate.market.symbol}`);
      return null;
    }
    const liquidatorBalance = await this.contracts
      .call<bigint>(underlyingToken, 'balance', [toAddress(this.liquidator.publicKey())])
      .catch(() => 0n);
    if (liquidatorBalance < repayCandidate.repayAmount) {
      console.warn(
        `[plan] insufficient underlying balance for ${repayCandidate.market.symbol}: have ${liquidatorBalance}, need ${repayCandidate.repayAmount}`,
      );
      return null;
    }

    const collateralCandidate = await this.pickCollateralMarket(
      borrower,
      repayCandidate.market,
      repayCandidate.repayAmount,
    );
    if (!collateralCandidate) return null;

    console.info(
      `[plan] borrower=${borrower} shortfall=${shortfall} repay=${repayCandidate.market.symbol} amount=${repayCandidate.repayAmount} collateral=${collateralCandidate.market.symbol} seize=${collateralCandidate.seizeAmount}`,
    );

    return {
      borrower,
      repayMarket: repayCandidate.market,
      collateralMarket: collateralCandidate.market,
      repayAmount: repayCandidate.repayAmount,
      seizeAmount: collateralCandidate.seizeAmount,
    };
  }

  private async pickRepayMarket(
    borrower: string,
  ): Promise<{ market: MarketConfig; repayAmount: bigint } | null> {
    // Query all markets in parallel.
    const debts = await Promise.all(
      this.config.markets.map(market =>
        this.contracts
          .call<bigint>(market.vaultId, 'get_user_borrow_balance', [toAddress(borrower)])
          .then(debt => ({ market, debt }))
          .catch(() => ({ market, debt: 0n })),
      ),
    );

    const chosen = debts.filter(d => d.debt > 0n).sort((a, b) => (b.debt > a.debt ? 1 : -1))[0];
    if (!chosen) return null;

    const cap = await this.contracts
      .call<bigint>(this.config.peridottrollerId, 'preview_repay_cap', [
        toAddress(borrower),
        toAddress(chosen.market.vaultId),
      ])
      // If the preview call fails, default to 50% (default close factor) so we don't
      // submit a repay_amount that exceeds the on-chain cap and get rejected.
      .catch(() => chosen.debt / 2n);

    const repayAmount = cap < chosen.debt ? cap : chosen.debt;
    if (repayAmount === 0n) return null;

    return { market: chosen.market, repayAmount };
  }

  private async pickCollateralMarket(
    borrower: string,
    repayMarket: MarketConfig,
    repayAmount: bigint,
  ): Promise<{ market: MarketConfig; seizeAmount: bigint } | null> {
    // Query all collateral markets in parallel.
    const candidates = await Promise.all(
      this.config.markets
        .filter(m => m.vaultId !== repayMarket.vaultId)
        .map(async market => {
          const balance = await this.contracts
            .call<bigint>(market.vaultId, 'get_ptoken_balance', [toAddress(borrower)])
            .catch(() => 0n);
          if (balance <= 0n) return null;

          const seize = await this.contracts
            .call<bigint>(this.config.peridottrollerId, 'preview_seize_ptokens', [
              toAddress(repayMarket.vaultId),
              toAddress(market.vaultId),
              toU128(repayAmount),
            ])
            .catch(() => 0n);

          if (seize <= 0n || seize > balance) return null;
          return { market, seizeAmount: seize };
        }),
    );

    const valid = candidates.filter((c): c is NonNullable<typeof c> => c !== null);
    if (valid.length === 0) return null;

    // Pick the collateral market that yields the highest seize amount.
    return valid.sort((a, b) => (b.seizeAmount > a.seizeAmount ? 1 : -1))[0];
  }

  private async executeLiquidation(plan: LiquidationPlan): Promise<void> {
    const response = await this.contracts.invoke(
      this.liquidator,
      this.config.peridottrollerId,
      'liquidate',
      [
        toAddress(plan.borrower),
        toAddress(plan.repayMarket.vaultId),
        toAddress(plan.collateralMarket.vaultId),
        toU128(plan.repayAmount),
        toAddress(this.liquidator.publicKey()),
      ],
    );

    const successResponse = response as rpc.Api.GetSuccessfulTransactionResponse;
    const returnValue = successResponse.returnValue
      ? scValToNative(successResponse.returnValue)
      : null;

    console.info(
      `[success] borrower=${plan.borrower} repay=${plan.repayMarket.symbol} amount=${plan.repayAmount} collateral=${plan.collateralMarket.symbol} seize=${plan.seizeAmount} result=${JSON.stringify(returnValue)}`,
    );
  }
}

function extractAddress(value: unknown): string | undefined {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && 'address' in value) {
    return (value as { address: string }).address;
  }
  return undefined;
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.message}${error.stack ? `\n${error.stack}` : ''}`;
  }
  return String(error);
}
