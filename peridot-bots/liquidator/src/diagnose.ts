/**
 * Diagnostic script — connects to the configured network and dry-runs the full
 * liquidation bot logic without submitting any transactions.
 *
 * Run with:
 *   npx tsx src/diagnose.ts
 *
 * Set the same env vars as the main bot (.env file).
 */

import { Keypair, nativeToScVal, rpc, scValToNative } from '@stellar/stellar-sdk';
import { loadConfig } from './config.js';
import { SorobanClient } from './contracts.js';
import { toAddress, toU128 } from './utils.js';

const MAX_FAILURE_DELAY_MS = 60_000;

async function main(): Promise<void> {
  const config = loadConfig();
  const server = new rpc.Server(config.rpcUrl, {
    allowHttp: config.rpcUrl.startsWith('http://'),
  });
  const contracts = new SorobanClient(server, config.networkPassphrase);
  const liquidator = Keypair.fromSecret(config.liquidatorSecret);

  console.log('=== Peridot Liquidation Bot Diagnostics ===\n');
  console.log(`Network:      ${config.rpcUrl}`);
  console.log(`Controller:   ${config.peridottrollerId}`);
  console.log(`Liquidator:   ${liquidator.publicKey()}`);
  console.log(`Markets:      ${config.markets.map(m => m.symbol).join(', ')}\n`);

  // ─── 1. Latest ledger ─────────────────────────────────────────────────────
  const latest = await server.getLatestLedger();
  console.log(`Latest ledger: ${latest.sequence}`);
  const startLedger = Math.max(0, latest.sequence - config.eventBacklog);
  console.log(`Scanning events from ledger ${startLedger} (backlog=${config.eventBacklog})\n`);

  // ─── 2. Liquidator underlying balances ────────────────────────────────────
  console.log('--- Liquidator balances ---');
  for (const market of config.markets) {
    let underlying = '<unknown>';
    let balance: bigint | string = '<error>';
    try {
      underlying = await contracts.call<string>(market.vaultId, 'get_underlying_token', []);
      const raw = await contracts.call<bigint>(underlying, 'balance', [
        toAddress(liquidator.publicKey()),
      ]);
      balance = raw;
    } catch (err) {
      balance = `error: ${err}`;
    }
    console.log(
      `  ${market.symbol}: ${balance} (underlying: ${underlying})`,
    );
  }
  console.log();

  // ─── 3. Fetch recent events to discover borrowers ─────────────────────────
  console.log('--- Recent events (discovering borrowers) ---');
  const contractIds = [config.peridottrollerId, ...config.markets.map(m => m.vaultId)];
  const eventsRes = await server.getEvents({
    filters: [{ type: 'contract', contractIds }],
    startLedger,
    limit: config.eventPageSize,
  });

  const tracked = new Set<string>();
  for (const event of eventsRes.events) {
    if (!Array.isArray(event.topic) || event.topic.length === 0) continue;
    const topics = event.topic.map((t: any) => scValToNative(t));
    const name = String(topics[0]).toLowerCase();

    let borrower: string | undefined;
    if (name === 'borrow_event') {
      borrower = extractAddress(topics[1]);
    } else if (name === 'repay_borrow') {
      borrower = extractAddress(topics[2]);
    } else if (name === 'market_entered') {
      borrower = extractAddress(topics[1]);
    } else if (name === 'mint') {
      borrower = extractAddress(topics[1]);
    }

    if (borrower) {
      if (!tracked.has(borrower)) {
        console.log(`  [${name}] → tracking ${borrower}`);
      }
      tracked.add(borrower);
    }
  }
  console.log(`\nTotal unique addresses tracked: ${tracked.size}\n`);

  if (tracked.size === 0) {
    console.log('No borrowers found in recent event backlog. Try increasing EVENT_BACKLOG.');
    return;
  }

  // ─── 4. Evaluate each borrower ────────────────────────────────────────────
  console.log('--- Evaluating borrowers ---');
  for (const borrower of tracked) {
    console.log(`\nBorrower: ${borrower}`);

    // account_liquidity
    let liquidity = 0n;
    let shortfall = 0n;
    try {
      [liquidity, shortfall] = await contracts.call<[bigint, bigint]>(
        config.peridottrollerId,
        'account_liquidity',
        [toAddress(borrower)],
      );
    } catch (err) {
      console.log(`  account_liquidity error: ${err}`);
      continue;
    }
    console.log(`  liquidity=${liquidity}  shortfall=${shortfall}`);

    if (shortfall <= config.minShortfall) {
      console.log('  → healthy (no shortfall), skip');
      continue;
    }

    // per-market debt
    const debts = await Promise.all(
      config.markets.map(market =>
        contracts
          .call<bigint>(market.vaultId, 'get_user_borrow_balance', [toAddress(borrower)])
          .then(debt => ({ market, debt }))
          .catch(() => ({ market, debt: 0n })),
      ),
    );
    for (const { market, debt } of debts) {
      if (debt > 0n) console.log(`  debt [${market.symbol}]: ${debt}`);
    }

    const chosen = debts.filter(d => d.debt > 0n).sort((a, b) => (b.debt > a.debt ? 1 : -1))[0];
    if (!chosen) {
      console.log('  → no debt found (may have been repaid)');
      continue;
    }

    // repay cap
    const cap = await contracts
      .call<bigint>(config.peridottrollerId, 'preview_repay_cap', [
        toAddress(borrower),
        toAddress(chosen.market.vaultId),
      ])
      .catch(() => chosen.debt / 2n);
    const repayAmount = cap < chosen.debt ? cap : chosen.debt;
    console.log(`  repay market: ${chosen.market.symbol}  cap=${cap}  repayAmount=${repayAmount}`);

    // liquidator underlying balance
    const underlyingToken = await contracts
      .call<string>(chosen.market.vaultId, 'get_underlying_token', [])
      .catch(() => null);
    if (underlyingToken) {
      const liqBal = await contracts
        .call<bigint>(underlyingToken, 'balance', [toAddress(liquidator.publicKey())])
        .catch(() => 0n);
      const canLiquidate = liqBal >= repayAmount;
      console.log(
        `  liquidator balance (${chosen.market.symbol} underlying): ${liqBal} — ${canLiquidate ? 'SUFFICIENT ✓' : 'INSUFFICIENT ✗'}`,
      );
    }

    // collateral markets
    for (const market of config.markets.filter(m => m.vaultId !== chosen.market.vaultId)) {
      const pbal = await contracts
        .call<bigint>(market.vaultId, 'get_ptoken_balance', [toAddress(borrower)])
        .catch(() => 0n);
      if (pbal <= 0n) continue;

      const seize = await contracts
        .call<bigint>(config.peridottrollerId, 'preview_seize_ptokens', [
          toAddress(chosen.market.vaultId),
          toAddress(market.vaultId),
          toU128(repayAmount),
        ])
        .catch(() => 0n);

      const feasible = seize > 0n && seize <= pbal;
      console.log(
        `  collateral [${market.symbol}]: pbal=${pbal}  seize=${seize}  ${feasible ? '→ viable ✓' : '→ not viable (seize>balance or 0)'}`,
      );
    }

    console.log(`  *** WOULD LIQUIDATE ${borrower} ***`);
  }

  console.log('\nDiagnostics complete.');
}

function extractAddress(value: unknown): string | undefined {
  if (typeof value === 'string') return value;
  if (value && typeof value === 'object' && 'address' in value) {
    return (value as { address: string }).address;
  }
  return undefined;
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
