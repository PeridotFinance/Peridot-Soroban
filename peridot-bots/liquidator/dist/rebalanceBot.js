import { nativeToScVal } from '@stellar/stellar-sdk';
import { sleep } from './utils.js';
// Rebalance when idle cash exceeds this multiple of the configured target.
// 2.0 = rebalance when idle > 20% on a 10%-target vault.
const REBALANCE_TRIGGER_MULTIPLIER = 2.0;
export class RebalanceBot {
    client;
    signer;
    markets;
    underlyingTokens;
    pollIntervalMs;
    constructor(client, signer, markets, 
    // Maps vaultId -> underlying token contract ID (needed to read idle cash balance)
    underlyingTokens, pollIntervalMs) {
        this.client = client;
        this.signer = signer;
        this.markets = markets;
        this.underlyingTokens = underlyingTokens;
        this.pollIntervalMs = pollIntervalMs;
    }
    async run() {
        console.log('[rebalance] bot started');
        while (true) {
            for (const market of this.markets) {
                try {
                    await this.checkAndRebalance(market);
                }
                catch (err) {
                    console.error(`[rebalance] error on ${market.symbol}:`, err);
                }
            }
            await sleep(this.pollIntervalMs);
        }
    }
    async checkAndRebalance(market) {
        const tokenId = this.underlyingTokens[market.vaultId];
        if (!tokenId) {
            console.warn(`[rebalance] no underlying token configured for ${market.symbol}, skipping`);
            return;
        }
        // Idle cash = underlying token balance held directly by the vault contract
        const idleCash = await this.client.call(tokenId, 'balance', [
            nativeToScVal(market.vaultId, { type: 'address' }),
        ]);
        const totalUnderlying = await this.client.call(market.vaultId, 'get_total_underlying', []);
        if (totalUnderlying === 0n)
            return;
        const bufferBps = await this.client.call(market.vaultId, 'get_idle_cash_buffer_bps', []);
        const targetIdle = (totalUnderlying * BigInt(bufferBps)) / 10000n;
        const triggerThreshold = BigInt(Math.floor(Number(targetIdle) * REBALANCE_TRIGGER_MULTIPLIER));
        if (idleCash <= triggerThreshold)
            return;
        const idlePct = Number((idleCash * 10000n) / totalUnderlying) / 100;
        console.log(`[rebalance] ${market.symbol}: idle=${idleCash} (${idlePct.toFixed(1)}%) ` +
            `exceeds 2× target (${Number(bufferBps) / 100}%) — rebalancing`);
        await this.client.invoke(this.signer, market.vaultId, 'rebalance_idle_cash', [
            nativeToScVal(this.signer.publicKey(), { type: 'address' }),
        ]);
        console.log(`[rebalance] ${market.symbol}: done`);
    }
}
