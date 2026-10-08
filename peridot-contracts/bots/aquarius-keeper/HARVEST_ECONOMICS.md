# Optional harvest economics and withdrawal independence

The September 29 change applies only to the background keeper's `harvest`
submission. It does not change contracts, withdrawal entrypoints, collateral
checks, reward routes, swap floors, NAV refreshes or receipt-cache maintenance.

After the existing cooldown and 0.001-settlement-asset dust check, the keeper:

1. Sums successful `harvested.underlying_out` events emitted by the target strategy.
   Idle underlying, recycled principal, paired-token residue and LP trading fees
   are excluded. This deliberately undercounts income: LP fees alone will not
   justify a harvest. Existing accumulated reward tokens sold by this harvest do
   count, including reward-token changes supported by the existing strategy.
2. Values those proceeds in XLM. Native XLM needs no oracle; USDC and PYUSD use
   the pinned Reflector Stellar-asset oracle `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M`.
   Its base must be Circle USDC and decimals 14. PYUSD uses its own feed, not
   assumed parity. Supported settlement SACs are pinned in `profitability.mjs`.
3. Requires proceeds discounted by 5% to exceed the **entire prepared maximum
   transaction fee plus 25%**, rounded conservatively with BigInt. Resource-fee
   refunds are not assumed. This is an estimate, not guaranteed realized profit.
4. Rejects missing/invalid/future prices, samples exceeding 600 seconds of age
   including 90 seconds of headroom, and cross-feed timestamps over 300 seconds
   apart. Rechecks price age after fee/NAV probes, just before signing.

Unprofitable or unavailable-price decisions defer **only harvest**, without a
signature, submission or failed-cycle restart. Rechecks use ordinary maintenance
cycles. The same economics applies in dry-run mode. No new collateral oracle or
on-chain pricing configuration is installed by this change.

## Withdrawals

The deployed legacy ReceiptVault withdrawal redeems strategy shares and the LP
position; the strategy's `withdraw` does not invoke its `harvest`. A profitable
keeper transaction is not a withdrawal prerequisite. Essential cache refreshes
continue even when every harvest is deferred. No minimum-return or collateral
safety check is weakened to make an exit succeed.

This is not a promise of unconditional withdrawals: liquidity, network fees,
collateral requirements, pool failures, oracle/slippage guards and cache health
still apply. It also does not redesign ownership or settlement of unharvested
legacy rewards; the separate receipt-based reward/lending migration stays gated.

Validation includes native integration tests of partial/full receipt withdrawals
with tiny pending unsold rewards and no completed harvest, keeper tests proving
all three markets still receive maintenance after economic deferrals, and fresh
unsigned Mainnet withdrawal previews. No test withdrawal moves user funds.

Release status is recorded in the repository's `CLAUDE.md` and local `Agents.md`;
implementation or a clean scan alone does not mean the worker is deployed.
