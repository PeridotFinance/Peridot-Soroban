# yXLM observation risk review — September 19, 2026

Decision: **not approved for production borrowing activation**. The depth-based
reference remains read-only research, not a replacement for the implemented
trade-window observer. No risk parameters, reporter, keeper or contracts changed.
This is a scoped engineering review, not an independent economic audit.

September23 availability refresh: the always-on cloud run has0missed scheduler
slots but3018collection/validation failures in4267attempts through16:27:13UTC.
The latest24hours contain1086failures in1440attempts; the last2hours are healthy.
The longest consecutive failure streak is740minutes. Failure categories are now
available (spread/ratio guards), so laptop sleep does not explain these recorded
rejections. These are sample/window metrics, not observations of real loans or
realized losses. They fail to establish dependable liquidation pricing under the
uncapped target. See `DEPTH_LIVE_REHEARSAL.md` for exact counts and limitations.
The isolated Testnet positive/restart/recovery rehearsal is now running, not yet
complete. Reviewed governed recovery addresses resumption mechanics only; it
does not prevent losses while liquidations are price-gated during an outage.

September20 decision update: the user explicitly rejected supply and borrow
caps. Target raw caps are now zero (unlimited), not yet applied. References below
to approving numeric caps are superseded by this decision; they are not permission
to reintroduce caps. The oracle manipulation/availability and liquidation-loss
review must address uncapped public exposure. Small canary trades do not bound it.
The user then approved developing/reviewing the depth-TWAP replacement, explicitly
NOT Mainnet activation. Candidate construction/publication planning is documented
in `DEPTH_PRICING_REVIEW.md`; the economic findings below are not resolved by it.

## What the available evidence establishes

The earlier seven-day trade audit passed544/2011 candidate windows (27.05%).
High aggregate trading volume does not establish continuous pricing availability.
The bounded live depth/Aquarius run now samples once per minute, retaining failures
and enforcing contiguous30-minute windows. Results and final run status are recorded
in PRICING.md. The first completed run collected23/36 samples with23 agreements,
but missed13 slots during local scheduling pauses:0/6 healthy mature windows.
This establishes neither uninterrupted coverage nor market unavailability. An
always-on collection environment is needed; even a passing short run cannot
establish long-term availability.

A separate orderbook snapshot at14:34:58UTC, Horizon ledger64508543, fetched all
259 asks and402 bids through bounded pagination, verifying the exact pair and
rejecting duplicate offers. Relative to a0.979470436261XLM/yXLM classic execution
midpoint, the within1% subset was:

| Side | Near-price offers / accounts | Resting selling quantity | Largest account share |
| --- | --- | --- | --- |
| Sell yXLM (asks) | 7 / 5 | 22,483.1423064yXLM | 70.85% |
| Sell XLM (bids) | 13 / 8 | 44,475.2174265XLM | 71.86% |

Removing the largest ask account leaves6,552.2448714yXLM in that subset. This is
an orderbook-only arithmetic counterfactual, NOT a prediction that the full
classic quote fails: classic AMM liquidity is excluded. Accounts are not verified
independent owners; shared control can make concentration worse. No offers were
placed, cancelled or executed. Snapshot evidence and read-only reproduction script:
`/private/tmp/peridot-yxlm-offers-review-20260919.json` and
`/private/tmp/peridot-yxlm-offers-review.mjs`.

At14:53:33–36UTC, unsigned Aquarius simulations (ledgers64508766–67) and classic
direct path quotes returned the following indicative sell-side outputs:

| yXLM input | Classic XLM output | Aquarius XLM output |
| --- | --- | --- |
| 100 | 97.7928845 | 97.9728578 |
| 10000 | 9760.3291584 | 9785.6327249 |
| 50000 | 48673.7710519 | 48531.8717162 |
| 100000 | 95561.9749165 | 81505.3297774 |

The100000-unit Aquarius average sell rate is about16.8% below its100-unit rate.
This is evidence that small-probe agreement does not establish large-exit value,
not an estimate of attack cost. Both directions and the intermediate1000-unit
probe are preserved in `/private/tmp/peridot-yxlm-depth-curve-20260919.json`;
reproduction script `/private/tmp/peridot-yxlm-depth-curve.mjs`. Quotes were not
executed, reserved, or combined into a liquidation simulation; state can change.

## Threat model and conclusions

1. **Revocable liquidity and predictable sampling.** The1000/10000-unit probes
   check current quote depth and spread. They do not consume or reserve liquidity.
   An actor controlling offers could expose liquidity around known sample times
   and remove it before liquidators execute. Per-minute observations therefore do
   not prove liquidity persisted between samples. More frequent or jittered reads
   can improve observation coverage but do not create an economic guarantee.
2. **Correlated venues.** Aquarius is a second execution mechanism, not proof of
   independent ownership or price discovery. An actor influencing both venues, or
   correlated LP withdrawal, can satisfy two-way agreement while the reference is
   biased or both venues become unavailable. A synthetic regression deliberately
   shows coherent fabricated venue inputs passing consistency checks; it is NOT
   a demonstrated Mainnet exploit or an attack-cost estimate.
3. **Reporter and infrastructure trust.** On-chain bounds constrain a reporter,
   but cannot verify SDEX history. A compromised reporter can lie within allowed
   bounds or deny service; a shared data-provider outage can halt all three LP
   markets through their required-observation dependency. Two data providers for
   the same market improve operational resilience, not economic independence.
4. **No proven manipulation-cost lower bound.** Quoted notional is not irreversible
   attack cost. Capital may be recoverable and the attacker may already control
   the liquidity. No safe borrow cap can be derived merely as a percentage of the
  10000-unit probe. Neither this review nor an Almanax code scan establishes such
   a bound. Activation still requires review of losses under the approved uncapped
   exposure policy; this evidence does not establish safety for unlimited borrowing.
5. **Window lag, step limits and depegs.** A30-minute average responds slowly to a
   genuine move. A1% publication step limit is a rate bound, not an absolute bound
   against accumulated bias. A depeg or move outside configured bounds can halt
   pricing; an existing report may remain usable until expiry. Longer windows and
   stricter guards have real liveness costs. Never repair this by inventing prices,
   a1:1 yXLM alias, silently widening bounds or ramping through synthetic updates.
6. **Collateral is not guaranteed liquidation value.** For an illustrative
   two-asset NAV `V = X + Y*r`, a relative yXLM ratio error `e` inflates NAV by
   `w*e`, where `w = Y*r/V`. Concentrated liquidity can become heavily one-sided;
   do not assume `w = 50%`. Borrowing/debt, fees and realizable swap slippage require
   the actual complete receipt accounting, not this simplified identity. The user’s
  50% XLM and80% stablecoin collateral-factor targets are not permission to assume
   safe unlimited exposure or ignore correlated PYUSD/USDC pool risks.
7. **Outage liquidation remains unresolved.** Halting new borrowing prevents new
   exposure but does not liquidate existing debt. Repayment remains available;
   price-dependent liquidation, collateral transfer and withdrawal may be blocked.
   Need tested recovery, price-independent principal-exit boundaries and a governed
   depeg/outage policy that does not confiscate claims or restore unsafe borrowing.

## Release requirements

- Longer monitored runs across quiet and volatile periods, endpoint failures and
  real price changes; report ALL scheduled attempts and failed windows. A short
  successful soak is a connectivity/window-construction result only.
- Exposure-scaled two-way liquidation/slippage checks, including loss of dominant
  liquidity, out-of-range LP concentration and simultaneous market exits. Include
  account/control concentration and possible common control across venues.
- Record the approved no-cap policy, maximum acceptable loss, liquidation
  parameters, liquidity buffers and recovery authority. No numeric safe cap or
  safe unlimited exposure is established by this evidence alone.
- Full compiled router/controller/receipt/strategy integration with actual pool,
  reward routes/gauges, oracle expiry, restoration and liquidation. Keep current
  Mainnet fences until these checks and scoped final security review pass.
- Any switch from traded-window observations to executable-quote TWAP must be
  reviewed as a policy change; this research does not silently enable it.

References: Stellar’s [offer objects](https://developers.stellar.org/docs/data/apis/horizon/api-reference/resources/offers/object)
define amount as the asset being sold; [offer listing](https://developers.stellar.org/docs/data/apis/horizon/api-reference/get-all-offers)
provides current offers. [Strict-send paths](https://developers.stellar.org/docs/data/apis/horizon/api-reference/list-strict-send-payment-paths)
provide routes for a given input; [path payments](https://developers.stellar.org/docs/build/guides/transactions/path-payments)
can use orderbook and classic AMM liquidity. These APIs do not certify independence
of liquidity providers. Threat conclusions above are this review’s inferences.
