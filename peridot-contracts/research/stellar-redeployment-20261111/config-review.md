# Release checks 3–5: default-build configuration source review

2026-11-11. Read-only review of default `simple-peridottroller`, `receipt-vault`
and `jump-rate-model`; no network queries, build, signing or deployment performed
by this reviewer. Source findings below are not an on-chain or compiled-test pass.
The lead's flash-loan correction/tests and scanner result are separate evidence.

## Check 3 — Ledger-pinned initialization and guardian authority

All three production build paths bind an expected initialization admin and require
that address's authorization. Use the operator's approved **public** Ledger address
`GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP` consistently for:

- `SIMPLE_PERIDOTTROLLER_INIT_ADMIN`.
- `RECEIPT_VAULT_INIT_ADMIN`.
- `JUMP_RATE_MODEL_INIT_ADMIN`.

These are compile-time configuration inputs, not signing credentials. A build
success and source inspection do not prove the installed artifact contains the
intended value. Pin each final WASM hash and reproduce the wrong-admin, missing-auth,
correct-admin and repeat-initialization tests against that exact artifact. Native
`test-default-admin` behavior cannot replace a default compiled release check.

|Contract|Actual initialization ABI, excluding implicit Env|Readback|
|---|---|---|
|Controller|`initialize(admin: Address)`|`get_admin() -> Address`; instance `Initialized`; persistent `Admin`, `SupportedMarkets`|
|Receipt, once per underlying|`initialize(token_address: Address, supply_yearly_rate_scaled: u128, borrow_yearly_rate_scaled: u128, admin: Address)`|`get_admin()`, `get_underlying_token()`, supply/debt getters below; persistent initialization/rate keys|
|Rate model|`initialize(base: u128, multiplier: u128, jump: u128, kink: u128, admin: Address)`|**No get_admin/get_params ABI exists**. Decode persistent `Admin`, `BaseRatePerYear`, `MultiplierPerYear`, `JumpMultiplierPerYear`, `Kink`; verify rate outputs separately|

Relevant source: controller `contract.rs:188,395`; receipt `contract.rs:1187,4825`;
rate model `lib.rs:46,256`. Rates use1e6 scaling. No initialization parameter values
are approved by this review.

Controller and receipt admin transfer is two-step `set_admin(new_admin)` then
`accept_admin()`, authorized by current and pending admins respectively. Inspect
`PendingAdmin` directly to prove absence; there is no corresponding public getter.
The rate model has parameter/upgrade authority but no admin-transfer entrypoint.

The existing default controller provides:

- `set_pause_guardian(guardian: Address)` — current admin authorization.
- `pause_borrow_g(guardian,market,paused)`, `pause_redeem_g(...)`,
  `pause_liquidation_g(...)`, `pause_deposit_g(...)` — only `paused=true`, exact
  configured guardian authorization. Liquidation pause also pauses borrowing.
- Admin `set_pause_borrow(market,paused)`, `set_pause_redeem(...)`,
  `set_pause_liquidation(...)`, `set_pause_deposit(...)`.
- `is_borrow_paused(market)`, `is_redeem_paused(market)`,
  `is_liquidation_paused(market)`, `is_deposit_paused(market)` return bool.

There is **no get_pause_guardian ABI**: read persistent `PauseGuardian` directly.
Read the matching `Pause*Until` map as well as the boolean getter: existing
controller pauses expire after72hours (`constants.rs:17`; `contract.rs:1399`).
They are not a permanent latch or a durable borrowing prohibition. The proposed
CL deposit-only guardian latch is a different, still-unimplemented control.

The supply-only receipt's durable gate is separately represented by instance
`MinCashReserveBps` being present and `CashBorrowingEnabled=false`. Getter false
alone is insufficient: an unconfigured legacy receipt also returns false by
default. Verify both `get_min_cash_reserve_bps() -> Option<u32>` and
`get_cash_borrowing_enabled() -> bool`. Do not infer permission from
`get_borrowable_cash()`: it is a cash quantity, not the enabled flag.

## Check 4 — Fresh empty state, no strategy/oracle reuse

Source confirms that a freshly initialized controller does not bind an oracle,
PERI reward token, guardian, reserve recipient, margin controller or market.
It initializes empty supported-market/pause maps. A fresh receipt initializes
zero deposited/managed cash/debt/reserves/admin fees and `RatesReady=false`, with
no boosted vault, margin controller, interest model or peridottroller binding.
Those remain separate explicitly authorized configuration calls. A rate model
stores only its own admin/rate parameters and has no external strategy binding.

Default receipt initialization gives local CF500,000 and caps0 (unset), while
controller `get_market_cf(unconfigured_market)` returns0. Do not conflate these
defaults or describe an unset supply cap as a zero-deposit limit.

Exact readbacks for each new receipt:

|Getter|Expected fresh/empty result or purpose|
|---|---|
|`get_admin()`|Approved Ledger public address|
|`get_underlying_token()`|Exact intended USDC or native-XLM SAC|
|`get_total_deposited()`, `get_total_ptokens()`, `get_total_borrowed()`|0|
|`total_supply()`|0|
|`get_total_reserves()`, `get_total_admin_fees()`, `get_total_bad_debt()`|0|
|`get_boosted_vault() -> Option<Address>`|None|
|`get_boosted_asset_count() -> Option<u32>`|None|
|`get_margin_controller() -> Option<Address>`|None|
|`get_cl_custody() -> bool`|false for USDC/XLM cash markets|
|`get_min_cash_reserve_bps()`|Some(the explicitly approved setting), after supply-only configuration|
|`get_cash_borrowing_enabled()`|false|
|`get_exchange_rate()`|Initial1,000,000, subject to exact fresh-state check|
|`get_total_underlying()`, `get_available_liquidity()`|0 before funding|
|Underlying SAC `balance(receipt)`|0 before any operator funding; external donation may make a newly deployed address nonempty|

There are **no public receipt getters** named `get_peridottroller`,
`get_interest_model`, `get_supply_cap`, `get_borrow_cap` or `get_rates_ready`.
Use exact persistent keys `Peridottroller`, `InterestModel`, `SupplyCap`,
`BorrowCap`, `RatesReady`; also inspect `BoostedVault`, `MarginController`,
`PendingAdmin`, `PendingUpgradeHash`, `PendingUpgradeEta`. The optional local cash
policy keys are in the **instance**, not persistent storage.

Controller readbacks include `get_admin()`, `get_oracle() -> Option<Address>`,
`get_reserve_recipient() -> Option<Address>`, `is_market_supported(market)`,
`get_market_cf(market)` and the pause getters above. Read persistent
`SupportedMarkets`, `PeridotToken`, `MarginLiquidationControllers`,
`PendingOracle`, `PendingOracleEta`, pending admin/upgrade keys, plus any
`OracleAssetSymbol(token)`, `PriceCache(token)`, `FallbackPrice(token)` and
`FallbackPriceSetAt(token)` directly. Fresh-state provenance plus exact ledger
entry reads are required; a few absent known keys are not a proof of every
possible entry's absence.

### Supply-only without an oracle

**Source-feasible with no debt anywhere in this fresh group and durable borrowing
disabled. Not yet certified by this review.** Respect the user's no-reused-oracle
instruction: leave controller `get_oracle()==None`, do not call `set_oracle`, and
do not install static fallback prices as a disguised oracle replacement.

The relevant source paths are:

1. `deposit` uses cash/exchange-rate/reward/pause checks, with no price lookup.
   With `PeridotToken` absent, controller `accrue_user_market` returns before
   reward distribution (`controller contract.rs:3128`).
2. `withdraw` and pToken transfer request local debt and `get_borrows_excl` first.
   They perform valuation only when either is nonzero (`receipt contract.rs:1779,2140`).
   Controller `get_borrows_excl` skips a market with zero debt before requesting
   its asset price (`controller contract.rs:2198`).
3. `set_peridottroller` sanity-checks `get_price_usd(token)` as an Option; it does
   **not** require Some. With a fresh empty group the collateral/debt sanity reads
   are zero. `cache_price` returns None when Oracle is absent.
4. `set_interest_model(model)` checks the fresh model's rate ABI and sets
   RatesReady=true; it does not require an oracle. That does not itself authorize
   borrowing, which must remain disabled through the durable cash policy.

Required focused execution before marking this operational: two fresh compiled
USDC/XLM receipts bound to the fresh default controller, no Oracle/PERI/fallback,
durable borrowing disabled; deposit both markets, transfer pTokens, withdraw all
from each, advance beyond pause expiry, and repeat. Trace no oracle calls and
verify debt remains0. Real issuer restrictions/native-XLM account balance costs
and resource limits remain separate checks. Missing cross-market debt data must
still fail closed, not be rewritten to zero to make exits pass.

If debt is introduced later, missing-price liquidation/borrow/health checks are
expected blockers. Supply-only feasibility does not authorize borrowing or remove
the stale-price liquidation issue.

### External identity exception

Fresh Peridot contracts must not reuse old compromised controller/receipt/rate/
strategy/permission addresses. The canonical **USDC and native-XLM asset contracts**
are external asset identities: depositing USDC/XLM necessarily interacts with those
existing SACs. This is not reuse of a Peridot market or oracle. Replacing them with
newly issued lookalike tokens would change the product. Keep this exception explicit
in the deployment manifest, with asset issuer/native identity verification. No
oracle, Upshift, DeFindex, margin or legacy boosted-route exception is inferred.

## Check 5 — Later CL market without controller redeployment

Default controller `add_market(market: Address)` is admin-authorized and calls
the candidate receipt's `get_underlying_token()`. It updates SupportedMarkets,
SupportedToken(token) and MarketUnderlying(market); it does not require an oracle
or recreate/redeploy the controller (`contract.rs:1676`). An unconfigured market's
controller CF is0. `set_market_cf(market,0)` is **not** a valid normal setter:
its allowed range starts at10,000 (1%). Leave CF unconfigured for that default0;
do not invent a setter or collateral-activation approval.

Consequently the fresh default controller can administratively list a future CL
receipt later without redeployment. This does not prove that the future share
market is safe to activate. It needs a **new CL receipt and vault**, correct
underlying-share binding, custody/reward hooks, actual primary valuation, recovery
and resource proofs. Existing cash receipts cannot be retargeted: their underlying
is set at initialization and there is no underlying-token replacement setter.

The separate `lp-peridottroller` policy is not interchangeable with this default
controller. Its `lp-zero-peri` branch checks `lp_version`, limits the group and
uses a different live-price policy; the generic package explicitly refuses that
internal feature. Do not accidentally build/deploy it as the ordinary cash group.

For subsequent CL listing read back `is_market_supported(new_receipt)==true`,
receipt `get_underlying_token()==new_share_vault`, `get_market_cf()==0`, exact
MarketUnderlying/SupportedToken records, borrowing disabled and no unintended
margin binding. Administrative extensibility is **not** Stage1 accounting,
collateral activation, leveraged-lending or public-release approval.

## Bounded result

Checks3/4/5 have source-level support with the distinctions above. No installed
admin, empty-state, no-oracle roundtrip, guardian signature or future-market test
was executed here; they remain unverified execution gates. The lead's release
stop, build hashes, corrected borrow gate tests and Almanax availability determine
their own evidence status independently.
