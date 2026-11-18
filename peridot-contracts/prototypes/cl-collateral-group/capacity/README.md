# Capacity tool (spec item 4, design 1-2)

`node capacity.mjs example.input.json` prints, offline:

- (a) the PYUSD/USDC mixture a liquidator receives when closing seized units in kind (`stage_withdraw_kind` with
  `min_swap = 0`): now, and in the worst case (pool at/below the lower edge, all PYUSD);
- (b) how much PYUSD converts to USDC within each slippage budget (default 1% and 2%), from a ladder of simulated
  PYUSD->USDC quotes or, without quotes, a single-range constant-liquidity pool model that removes our liquidity
  and 50% of third-party liquidity;
- (c) recommended caps: total USDC borrow cap and backing (collateral) cap such that liquidating all debt at the
  8% incentive, at the worst-case PYUSD share, fits the conversion budget divided by the safety factor 2. The
  one-round (50% close factor, 0.54 D seized) figure is reported next to the recommended full-debt (1.08 D) one.

Assumptions are listed in `ASSUMPTIONS` (printed with every run). `example.input.json` is ILLUSTRATIVE, not
measured: a real run needs vault views (`get_position_liquidity`, `stage_idle`, `total_supply`, pool `get_slot0`)
and quotes simulated after liquidity removal at a pinned ledger, archived with the output. `node --test` here runs
the arithmetic tests (no network).
