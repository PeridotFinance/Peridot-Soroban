import { Keypair, scValToNative } from '@stellar/stellar-sdk';
import { sleep, toAddress, toU128 } from './utils.js';
// Remove borrowers from the map after this long with no activity and no debt.
const BORROWER_STALE_MS = 24 * 60 * 60 * 1000; // 24 hours
const MAX_FAILURE_DELAY_MS = 60_000;
export class LiquidationBot {
    config;
    server;
    contracts;
    borrowerState = new Map();
    liquidator;
    cursor;
    startLedger;
    contractIds;
    constructor(config, server, contracts) {
        this.config = config;
        this.server = server;
        this.contracts = contracts;
        this.liquidator = Keypair.fromSecret(config.liquidatorSecret);
        this.contractIds = [
            config.peridottrollerId,
            ...config.markets.map(m => m.vaultId),
        ];
    }
    async start() {
        await this.bootstrapCursor();
        while (true) {
            try {
                await this.pollEvents();
            }
            catch (error) {
                console.error(`[events] ${formatError(error)}`);
            }
            try {
                await this.scanBorrowers();
            }
            catch (error) {
                console.error(`[scan] ${formatError(error)}`);
            }
            this.cleanupStaleBorrowers();
            await sleep(this.config.pollIntervalMs);
        }
    }
    async bootstrapCursor() {
        const latest = await this.server.getLatestLedger();
        this.startLedger = Math.max(0, latest.sequence - this.config.eventBacklog);
        console.info(`[bot] starting at ledger ${latest.sequence} (backlog ${this.config.eventBacklog})`);
    }
    async pollEvents() {
        const request = {
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
    handleEvent(event) {
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
            if (borrower)
                this.trackBorrower(borrower);
        }
        else if (name === 'repay_borrow') {
            // RepayBorrow: topics = ["repay_borrow", payer, borrower]
            // Track the borrower (topics[2]), not the payer (topics[1]).
            const borrower = extractAddress(topics[2]);
            if (borrower)
                this.refreshBorrower(borrower);
        }
        else if (name === 'market_entered') {
            // MarketEntered: topics = ["market_entered", account, market]
            const account = extractAddress(topics[1]);
            if (account)
                this.trackBorrower(account);
        }
        else if (name === 'market_exited') {
            // MarketExited: topics = ["market_exited", account, market]
            // Do NOT remove borrower here — they may still have debt in other markets.
            // Cleanup happens in cleanupStaleBorrowers() after verifying zero debt.
            const account = extractAddress(topics[1]);
            if (account)
                this.refreshBorrower(account);
        }
        else if (name === 'mint') {
            // Mint (deposit): topics = ["mint", minter]
            const minter = extractAddress(topics[1]);
            if (minter)
                this.trackBorrower(minter);
        }
    }
    trackBorrower(address) {
        const now = Date.now();
        const state = this.borrowerState.get(address);
        if (state) {
            state.lastSeen = now;
        }
        else {
            this.borrowerState.set(address, { lastSeen: now, failures: 0 });
            console.info(`[events] tracking ${address}`);
        }
    }
    refreshBorrower(address) {
        const state = this.borrowerState.get(address);
        if (state) {
            state.lastSeen = Date.now();
            // Force re-evaluation on next scan cycle.
            state.lastEvaluated = undefined;
        }
    }
    async scanBorrowers() {
        const now = Date.now();
        for (const [borrower, state] of this.borrowerState) {
            if (state.lastEvaluated && now - state.lastEvaluated < this.config.borrowerRefreshMs) {
                continue;
            }
            if (state.failures > 0 &&
                state.lastEvaluated &&
                now - state.lastEvaluated < Math.min(MAX_FAILURE_DELAY_MS, state.failures * 5_000)) {
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
            }
            catch (error) {
                state.failures += 1;
                console.error(`[liquidate] ${borrower} | ${formatError(error)}`);
            }
        }
    }
    cleanupStaleBorrowers() {
        const now = Date.now();
        for (const [address, state] of this.borrowerState) {
            if (now - state.lastSeen < BORROWER_STALE_MS)
                continue;
            // Remove only if the borrower was recently evaluated and had no debt.
            // If never evaluated, keep them until the scanner confirms no shortfall.
            if (state.lastEvaluated && state.failures === 0) {
                this.borrowerState.delete(address);
                console.info(`[cleanup] removed stale borrower ${address}`);
            }
        }
    }
    async evaluateBorrower(borrower) {
        const [_liquidity, shortfall] = await this.contracts.call(this.config.peridottrollerId, 'account_liquidity', [toAddress(borrower)]);
        if (shortfall <= this.config.minShortfall) {
            return null;
        }
        const repayCandidate = await this.pickRepayMarket(borrower);
        if (!repayCandidate)
            return null;
        // Check liquidator has enough balance before going further.
        const liquidatorBalance = await this.contracts.call(repayCandidate.market.vaultId, 'balance', [toAddress(this.liquidator.publicKey())]).catch(() => 0n);
        if (liquidatorBalance < repayCandidate.repayAmount) {
            console.warn(`[plan] insufficient balance for ${repayCandidate.market.symbol}: have ${liquidatorBalance}, need ${repayCandidate.repayAmount}`);
            return null;
        }
        const collateralCandidate = await this.pickCollateralMarket(borrower, repayCandidate.market, repayCandidate.repayAmount);
        if (!collateralCandidate)
            return null;
        console.info(`[plan] borrower=${borrower} shortfall=${shortfall} repay=${repayCandidate.market.symbol} amount=${repayCandidate.repayAmount} collateral=${collateralCandidate.market.symbol} seize=${collateralCandidate.seizeAmount}`);
        return {
            borrower,
            repayMarket: repayCandidate.market,
            collateralMarket: collateralCandidate.market,
            repayAmount: repayCandidate.repayAmount,
            seizeAmount: collateralCandidate.seizeAmount,
        };
    }
    async pickRepayMarket(borrower) {
        // Query all markets in parallel.
        const debts = await Promise.all(this.config.markets.map(market => this.contracts
            .call(market.vaultId, 'get_user_borrow_balance', [toAddress(borrower)])
            .then(debt => ({ market, debt }))
            .catch(() => ({ market, debt: 0n }))));
        const chosen = debts.filter(d => d.debt > 0n).sort((a, b) => (b.debt > a.debt ? 1 : -1))[0];
        if (!chosen)
            return null;
        const cap = await this.contracts
            .call(this.config.peridottrollerId, 'preview_repay_cap', [
            toAddress(borrower),
            toAddress(chosen.market.vaultId),
        ])
            .catch(() => 0n);
        // cap === 0n means no cap enforced — repay the full debt.
        const repayAmount = cap === 0n ? chosen.debt : cap < chosen.debt ? cap : chosen.debt;
        if (repayAmount === 0n)
            return null;
        return { market: chosen.market, repayAmount };
    }
    async pickCollateralMarket(borrower, repayMarket, repayAmount) {
        // Query all collateral markets in parallel.
        const candidates = await Promise.all(this.config.markets
            .filter(m => m.vaultId !== repayMarket.vaultId)
            .map(async (market) => {
            const balance = await this.contracts
                .call(market.vaultId, 'get_ptoken_balance', [toAddress(borrower)])
                .catch(() => 0n);
            if (balance <= 0n)
                return null;
            const seize = await this.contracts
                .call(this.config.peridottrollerId, 'preview_seize_ptokens', [
                toAddress(repayMarket.vaultId),
                toAddress(market.vaultId),
                toU128(repayAmount),
            ])
                .catch(() => 0n);
            if (seize <= 0n || seize > balance)
                return null;
            return { market, seizeAmount: seize };
        }));
        const valid = candidates.filter((c) => c !== null);
        if (valid.length === 0)
            return null;
        // Pick the collateral market that yields the highest seize amount.
        return valid.sort((a, b) => (b.seizeAmount > a.seizeAmount ? 1 : -1))[0];
    }
    async executeLiquidation(plan) {
        const response = await this.contracts.invoke(this.liquidator, this.config.peridottrollerId, 'liquidate', [
            toAddress(plan.borrower),
            toAddress(plan.repayMarket.vaultId),
            toAddress(plan.collateralMarket.vaultId),
            toU128(plan.repayAmount),
            toAddress(this.liquidator.publicKey()),
        ]);
        const successResponse = response;
        const returnValue = successResponse.returnValue
            ? scValToNative(successResponse.returnValue)
            : null;
        console.info(`[success] borrower=${plan.borrower} repay=${plan.repayMarket.symbol} amount=${plan.repayAmount} collateral=${plan.collateralMarket.symbol} seize=${plan.seizeAmount} result=${JSON.stringify(returnValue)}`);
    }
}
function extractAddress(value) {
    if (typeof value === 'string')
        return value;
    if (value && typeof value === 'object' && 'address' in value) {
        return value.address;
    }
    return undefined;
}
function formatError(error) {
    if (error instanceof Error) {
        return `${error.message}${error.stack ? `\n${error.stack}` : ''}`;
    }
    return String(error);
}
