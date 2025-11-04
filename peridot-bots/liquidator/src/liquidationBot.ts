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

interface EventCursor {
  cursor?: string;
  startLedger?: number;
}

const MAX_FAILURE_DELAY_MS = 60_000;

export class LiquidationBot {
  private readonly borrowerState = new Map<string, BorrowerState>();
  private readonly liquidator: Keypair;
  private readonly eventCursor: EventCursor = {};
  private readonly contractIds: string[];

  constructor(
    private readonly config: BotConfig,
    private readonly server: rpc.Server,
    private readonly contracts: SorobanClient,
  ) {
    this.liquidator = Keypair.fromSecret(config.liquidatorSecret);
    this.contractIds = [
      config.peridottrollerId,
      ...config.markets.map(market => market.vaultId),
    ];
  }

  async start(): Promise<void> {
    await this.bootstrapCursor();
    // eslint-disable-next-line no-constant-condition
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

      await sleep(this.config.pollIntervalMs);
    }
  }

  private async bootstrapCursor(): Promise<void> {
    const latest = await this.server.getLatestLedger();
    const start = Math.max(0, latest.sequence - this.config.eventBacklog);
    this.eventCursor.startLedger = start;
    console.info(
      `Starting liquidation bot at ledger ${latest.sequence} (backlog ${this.config.eventBacklog})`,
    );
  }

  private async pollEvents(): Promise<void> {
    const request = {
      cursor: this.eventCursor.cursor,
      startLedger: this.eventCursor.cursor ? undefined : this.eventCursor.startLedger,
      limit: this.config.eventPageSize,
      filters: [
        {
          type: 'contract',
          contractIds: this.contractIds,
        },
      ],
    } satisfies rpc.Api.GetEventsRequest;

    const res = await this.server.getEvents(request);
    if (res.events.length === 0) {
      return;
    }

    for (const eventInfo of res.events) {
      this.handleEvent(eventInfo);
    }
    this.eventCursor.cursor = res.cursor;
  }

  private handleEvent(event: rpc.Api.EventInfo): void {
    if (!Array.isArray(event.topic)) {
      return;
    }
    const topics = event.topic.map(topic => scValToNative(topic));
    if (topics.length === 0) {
      return;
    }
    const eventName = topics[0];
    if (typeof eventName !== 'string') {
      return;
    }

    const lower = eventName.toLowerCase();
    if (
      lower === 'market_entered' ||
      lower === 'borrow_event' ||
      lower === 'mint' ||
      lower === 'repayborrow'
    ) {
      const candidate = this.extractAddress(topics[1]);
      if (candidate) {
        this.trackBorrower(candidate);
      }
    } else if (lower === 'market_exited') {
      const candidate = this.extractAddress(topics[1]);
      if (candidate) {
        this.borrowerState.delete(candidate);
      }
    }
  }

  private extractAddress(value: unknown): string | undefined {
    if (typeof value === 'string') {
      return value;
    }
    if (value && typeof value === 'object' && 'address' in value) {
      return (value as { address: string }).address;
    }
    return undefined;
  }

  private trackBorrower(address: string): void {
    const now = Date.now();
    const state = this.borrowerState.get(address);
    if (state) {
      state.lastSeen = now;
    } else {
      this.borrowerState.set(address, { lastSeen: now, failures: 0 });
      console.info(`[events] tracking borrower ${address}`);
    }
  }

  private async scanBorrowers(): Promise<void> {
    const now = Date.now();
    for (const [borrower, state] of this.borrowerState) {
      if (state.lastEvaluated && now - state.lastEvaluated < this.config.borrowerRefreshMs) {
        continue;
      }

      // Back off on repeated failures.
      if (
        state.failures > 0 &&
        state.lastEvaluated &&
        now - state.lastEvaluated < Math.min(MAX_FAILURE_DELAY_MS, state.failures * 5000)
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
    if (!repayCandidate) {
      return null;
    }

    const collateralCandidate = await this.pickCollateralMarket(
      borrower,
      repayCandidate.market,
      repayCandidate.repayAmount,
    );
    if (!collateralCandidate) {
      return null;
    }

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
    let chosen: { market: MarketConfig; debt: bigint } | undefined;

    for (const market of this.config.markets) {
      const debt = await this.contracts.call<bigint>(market.vaultId, 'get_user_borrow_balance', [
        toAddress(borrower),
      ]);
      if (debt <= 0n) {
        continue;
      }
      if (!chosen || debt > chosen.debt) {
        chosen = { market, debt };
      }
    }

    if (!chosen) {
      return null;
    }

    const cap = await this.contracts.call<bigint>(this.config.peridottrollerId, 'preview_repay_cap', [
      toAddress(borrower),
      toAddress(chosen.market.vaultId),
    ]);

    const repayAmount = cap === 0n ? chosen.debt : cap < chosen.debt ? cap : chosen.debt;
    if (repayAmount === 0n) {
      return null;
    }

    return { market: chosen.market, repayAmount };
  }

  private async pickCollateralMarket(
    borrower: string,
    repayMarket: MarketConfig,
    repayAmount: bigint,
  ): Promise<{ market: MarketConfig; seizeAmount: bigint } | null> {
    let best: { market: MarketConfig; seizeAmount: bigint } | null = null;

    for (const market of this.config.markets) {
      const balance = await this.contracts.call<bigint>(market.vaultId, 'get_ptoken_balance', [
        toAddress(borrower),
      ]);
      if (balance <= 0n) {
        continue;
      }

      const seize = await this.contracts.call<bigint>(
        this.config.peridottrollerId,
        'preview_seize_ptokens',
        [toAddress(repayMarket.vaultId), toAddress(market.vaultId), toU128(repayAmount)],
      );

      if (seize <= 0n) {
        continue;
      }

      if (!best || seize > best.seizeAmount) {
        best = { market, seizeAmount: seize };
      }
    }

    return best;
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

    const result = response.result?.retval as xdr.ScVal | undefined;
    const nativeResult = result ? scValToNative(result) : null;
    console.info(
      `[success] borrower=${plan.borrower} repay=${plan.repayMarket.symbol} amount=${plan.repayAmount} collateral=${plan.collateralMarket.symbol} seize=${plan.seizeAmount} result=${describeNative(
        nativeResult,
      )}`,
    );
  }
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.message}${error.stack ? `\n${error.stack}` : ''}`;
  }
  return String(error);
}

function describeNative(value: unknown): string {
  if (value === null || value === undefined) {
    return 'null';
  }
  if (typeof value === 'bigint') {
    return value.toString();
  }
  if (Array.isArray(value)) {
    return `[${value.map(describeNative).join(', ')}]`;
  }
  if (typeof value === 'object') {
    try {
      return JSON.stringify(value);
    } catch {
      return value.toString();
    }
  }
  return String(value);
}
