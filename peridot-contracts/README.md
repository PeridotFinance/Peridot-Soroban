# Peridot Lending (Soroban)

Peridot lending protocol on Soroban. It consists of:

- `receipt-vault`: per-market vault holding the underlying token, minting/burning pTokens, and handling deposit/withdraw, borrow/repay, interest, reserves, and liquidation hooks.
- `simple-peridottroller`: cross-market risk manager handling supported markets, oracle pricing, account liquidity, liquidation, previews, pause flags, pause guardian, and optional liquidation fee.

## Two Product Stacks

This repo contains two distinct but complementary stacks:

1) Lending/Borrowing (spot-style)
   - `receipt-vault`: core lending vault (deposit/borrow/repay/withdraw).
   - `simple-peridottroller`: risk manager (collateral factors, gating, liquidation).
   - `jump-rate-model`: interest rate model for dynamic borrow/supply rates.
   - `peridot-token`: governance/reward token (if used by your deployment).

2) True Margin Trading (DEX-based via Aquarius)
   - `margin-controller`: margin positions using real borrows + on-chain swaps.
   - `swap-adapter`: Aquarius router wrapper used by margin-controller.
   - Uses Reflector oracle prices via peridottroller for health factor checks.

3) Smart Accounts + Boosted Markets (optional)
   - `smart-account-basic`: contract account that intercepts `require_auth` and delegates risk checks to Peridot.
   - `smart-account-factory`: deploys Basic smart accounts and stores wasm hashes.
   - Boosted markets: ReceiptVault can forward deposits into DeFindex vaults (single-asset) for yield.

Mocks (for tests only) live under `contracts/mocks/`.

## Key Concepts

- Fixed-point scaling: `SCALE_1E6 = 1_000_000` for rates and exchange rates; `BorrowIndex` uses `1e18`.
- Interest: supply and borrow interest accrue via `update_interest`; can use an external Jump Rate Model.
- Oracle: Reflector-based USD prices used in the peridottroller for risk checks.
- No re-entry: cross-contract checks avoid re-entering the same vault (exclusion parameters).

## Contracts and APIs

### ReceiptVault

- Initialization and admin
  - `initialize(token, supply_yearly_rate_scaled, borrow_yearly_rate_scaled, admin)`
  - `set_boosted_vault(admin, defindex_vault)` (optional)
  - `get_boosted_vault()`
  - `set_admin(new_admin)` / `get_admin()`
  - `set_interest_rate(admin, yearly_rate_scaled)`
  - `set_borrow_rate(admin, yearly_rate_scaled)`
  - `set_collateral_factor(admin, factor_scaled)`
  - `set_interest_model(admin, model_addr)`
  - `set_reserve_factor(admin, factor_scaled)`
  - `set_flash_loan_fee(admin, fee_scaled)`
  - `set_supply_cap(admin, cap)`
  - `set_borrow_cap(admin, cap)`
  - `reduce_reserves(admin, amount)`
  - `set_peridottroller(admin, peridottroller_addr)`
- User operations
  - `deposit(user, amount)` → mints pTokens at `exchange_rate`
  - `withdraw(user, ptoken_amount)` → burns pTokens, returns underlying (USD-gated when peridottroller set)
  - `borrow(user, amount)` → USD risk check via peridottroller; liquidity-guarded
  - `repay(user, amount)`
  - `bump_user_borrow_ttl(user)` / `bump_margin_borrow_ttl(position_id)` (permissionless keepalive)
  - `recover_borrow_snapshot(user)` / `recover_margin_snapshot(position_id)` (permissionless snapshot rebuild when canonical principal mirror exists)
  - `migrate_borrow_state_batch(users)` / `migrate_margin_state_batch(position_ids)` (permissionless migration + TTL keepalive batch)
- Flash loans
  - `flash_loan(initiator, receiver, amount, data)` → requires `initiator` auth, transfers underlying to `receiver`, then expects repayment of `amount + fee` (fee uses ceil division: `ceil(amount * flash_loan_fee_scaled / 1e6)`).
  - `preview_flash_loan_fee(amount)` → deterministic fee preview using the exact same rounding as `flash_loan`.
  - `receiver` must implement `on_flash_loan(vault: Address, amount: u128, fee: u128, data: Bytes)`; the vault reverts if the callback fails or does not return the required funds.
  - Flash loan fees accrue to reserves after repayment and respect peridottroller pause checks and liquidity guards.
- pToken (ERC20-like)
  - `approve(owner, spender, amount)`
  - `allowance(owner, spender) -> u128`
  - `transfer(from, to, ptoken_amount)`
  - `transfer_from(spender, from, to, ptoken_amount)`
    - Transfers are liquidity-gated when a peridottroller is wired; failing transfers will revert.
- Liquidation hooks (called by peridottroller)
  - `repay_on_behalf(liquidator, borrower, amount)`
  - `seize(borrower, liquidator, ptoken_amount)`
- Interest and views
  - `update_interest()`
  - `get_exchange_rate()`
  - `get_user_balance(user)` / `get_ptoken_balance(user)`
  - `get_user_borrow_balance(user)`
  - `get_total_deposited()` / `get_total_ptokens()` / `get_total_underlying()`
  - `get_total_borrowed()` / `get_total_reserves()` / `get_available_liquidity()`

### Peridottroller

- Admin and markets
  - `initialize(admin)`
  - `set_admin(new_admin)` / `get_admin()`
  - `add_market(admin, market)` / `remove_market(admin, market)`
  - `enter_market(user, market)` / `exit_market(user, market)`
  - `set_oracle(admin, oracle_addr)`
  - `set_close_factor(admin, factor_scaled)`
  - `set_liquidation_incentive(admin, incentive_scaled)`
  - `set_liquidation_fee(admin, fee_scaled)`
  - `set_reserve_recipient(admin, recipient_addr)`
  - `set_pause_guardian(admin, guardian)`
- Pricing and liquidity
  - `get_price_usd(token_addr)`
  - `account_liquidity(user) -> (liquidity_usd, shortfall_usd)`
  - `hypothetical_liquidity(user, market, borrow_amount, underlying_token)`
- Liquidation
  - `liquidate(liquidator, borrower, repay_market, collateral_market, repay_amount)`
- Preview helpers
  - `preview_borrow_max(user, market) -> u128`
    - Returns the maximum additional underlying the user can borrow from `market` without shortfall, considering market liquidity and global collateral.
  - `preview_redeem_max(user, market) -> u128`
    - Returns the maximum pTokens the user can redeem from `market` without shortfall, considering market liquidity and cross-market borrows.
  - `preview_repay_cap(borrower, repay_market) -> u128`
    - Returns close-factor-capped maximum repay amount on `repay_market`.
  - `preview_seize_ptokens(repay_market, collateral_market, repay_amount) -> u128`
    - Returns expected pTokens seized given repay amount and liquidation incentive, using oracle prices and current exchange rate.
- Pause flags
  - Setters (admin/guardian):
    - `set_pause_borrow(admin/guardian, market, paused)`
    - `set_pause_redeem(admin/guardian, market, paused)`
    - `set_pause_liquidation(admin/guardian, market, paused)`
    - `set_pause_deposit(admin/guardian, market, paused)`
  - Getters:
    - `is_borrow_paused(market)`
    - `is_redeem_paused(market)`
    - `is_liquidation_paused(market)`
    - `is_deposit_paused(market)`

## Auth Model

- Admin setters require `admin.require_auth()`.
- User actions require `user.require_auth()`.
- Liquidation requires `liquidator.require_auth()` in the peridottroller; vault hooks `repay_on_behalf` and `seize` are callable only when the vault is wired to a Peridottroller.

## Boosted Markets (DeFindex)

ReceiptVaults can optionally forward deposits into a DeFindex vault (single-asset) to earn external yield.
This is opt‑in per vault. If you do nothing, the market remains standard.

Admin:
- `set_boosted_vault(admin, defindex_vault)`
- `get_boosted_vault()` (view)

Behavior:
- Deposits forward underlying into the DeFindex vault; ReceiptVault holds DeFindex shares.
- Withdraws pull from DeFindex if local cash is insufficient.
- Exchange rate includes DeFindex‑managed assets.

CLI example (enable / disable):
```bash
VAULT=<RECEIPT_VAULT_ID>
DEFINDEX_VAULT=<DEFINDEX_VAULT_ID>
ADMIN=<ADMIN_ADDRESS>

# Enable boosted vault
stellar contract invoke --id "$VAULT" --source-account dev --network testnet -- \
  set_boosted_vault --admin "$ADMIN" --boosted_vault "$DEFINDEX_VAULT"

# Disable (set to zero address)
stellar contract invoke --id "$VAULT" --source-account dev --network testnet -- \
  set_boosted_vault --admin "$ADMIN" --boosted_vault "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
```

## Boosted Markets (Aquarius LP)

`contracts/aquarius-lp-vault` is a second implementation of the same
boosted-vault interface, backed by an Aquarius concentrated-liquidity position
instead of a DeFindex strategy. It implements the DeFindex ABI verbatim, so it
attaches through the unmodified `set_boosted_vault` entrypoint and needs no
change to ReceiptVault or the peridottroller.

Shape:
- Holds one **full-range** position (`deposit_position` / `withdraw_position`),
  so it is always in range and never needs a rebalance keeper.
- Accepts and settles in a **single** asset. Deposits are split (half swapped
  into the paired token) on the way in and recombined on the way out, so the
  market only ever sees its own underlying.
- Accepts capital **only from its permanently bound ReceiptVault**. An
  unbound strategy fails closed, and `set_receipt_vault` verifies the market
  already points back to the strategy and uses the same underlying. Users
  deposit the market asset and receive pTokens; they cannot bypass its supply
  cap by minting internal LP-vault shares directly. The selected concentrated
  pools expose range positions rather than fungible LP tokens, so there is no
  LP-token deposit path to support; the ReceiptVault is the sole entry.
- Reports NAV as `2 * L * sqrt(other_price / underlying_price)` using
  **Reflector prices, never pool spot**. A full-range position satisfies
  `amount0 = L/sqrt(P)` and `amount1 = L*sqrt(P)`, so this closed form is exact,
  and it means swinging the pool price cannot move the market's exchange rate.
- `harvest()` is permissionless: claims the admin-configured primary reward
  token (AQUA at launch), third-party gauge incentives and accrued swap fees,
  sells them for underlying through per-token route pools, and redeploys.
  `set_primary_reward_token`, `set_reward_route`, and `set_reward_min_rate` let
  governance rotate the reward asset, conversion venue, and minimum acceptable
  raw-underlying/raw-reward rate independently if Aquarius changes either.
  A route without a non-zero 1e7-scaled minimum rate fails closed and leaves
  the reward idle; permissionless callers cannot force a sale from a
  temporarily manipulated route quote.
- The ReceiptVault-facing share quote is a conservative liquidation value. It
  discounts gross oracle NAV by the configured pool-divergence and execution-
  slippage bounds so a redemption does not come back a few basis points short
  solely because the paired leg had to be swapped.

Two guards protect the position entry and paired-token exit swaps:
- `set_slippage_bps` — movement between the pool's quote and execution.
- `set_max_pool_divergence_bps` — the pool being *mispriced against the oracle*
  to begin with, which the slippage floor cannot see because it is derived from
  the pool's own quote. Verified on testnet: a ~7% pool/oracle gap cost 4.65%
  of a deposit before this existed.

If an exit quote breaches that oracle floor, the transaction reverts atomically
and keeps the supplier's shares intact for a later retry. This deliberately
prefers temporary withdrawal unavailability over realizing a manipulated rate.

Capacity is the binding constraint. Realised APR scales with
`pool_tvl / (pool_tvl + deployed)`, so `set_max_deploy` is a yield control as
much as a risk control — an uncapped vault on a thin pool dilutes itself to
near-zero yield.

The two requested launch markets both use concentrated pools and settle in the
first named asset:

- `scripts/deploy_aquarius_xlm_yxlm_mainnet.sh` — XLM deposits into the
  XLM/yXLM concentrated pool.
- `scripts/deploy_aquarius_pyusd_usdc_mainnet.sh` — separate PYUSD and USDC
  ReceiptVault markets backed by separate settlement vaults that share the
  same PYUSD/USDC concentrated pool. The two contracts are
  necessary because a strategy has one underlying, one share price and one
  ReceiptVault binding. AQUA is converted to PYUSD for one and USDC for the
  other.

For the temporary supply-only rollout, `deploy_aquarius_lp_controller_mainnet.sh`
creates an isolated controller that points directly at Reflector. Symbol aliases
price XLM and yXLM from `Other("XLM")`, and PYUSD and USDC from
`Other("USDC")`. The strategy scripts install the same aliases. This is a
deliberate parity assumption, not a depeg-aware oracle: collateral factors must
remain 0 and borrowing must remain paused. The scripts still reject a bad live
pool quote before deployment, and the strategies block new LP entry when their
runtime pool-divergence bound is exceeded. Use PriceRouter's peg clamp before
these markets ever become collateral or borrowable.

Both scripts hard-fail unless the target reports `pool_type = concentrated`
with the exact expected token order. They create new supply-only markets,
leave collateral factors at 0, and pause borrowing. The generic
`scripts/deploy_aquarius_lp_market_mainnet.sh` remains the USDC/EURC deployment
path. Each script also probes its AQUA route and installs a governance reward
floor at 95% of that live quote unless an explicit reviewed floor is supplied.

Keepers to schedule:
- `refresh_nav_root()` — keeps the cached price ratio fresh so user
  transactions do not pay the oracle's footprint cost.
- `harvest(caller)` — compounds rewards. The cooldown starts only after the
  call actually claims, converts, or deploys value, so an empty/failed
  permissionless call cannot delay the keeper.
- `refresh_boosted_underlying()` on the market — existing DeFindex keeper.

All three are driven by `scripts/run_aquarius_vault_keeper.sh`. For example:

```bash
VAULT_ID=C... MARKET_ID=C... IDENTITY=keeper NETWORK=mainnet-public \
  bash scripts/run_aquarius_vault_keeper.sh
```

### Transaction footprint

The end-to-end path spans ReceiptVault -> vault -> Aquarius pool -> three token
contracts -> the oracle, against Soroban's 100-entry cap. Two design choices
exist purely to fit inside it, and both are pinned by tests:
- Vault config, params and global accounting live in **single instance storage
  entries**. A key-per-field layout put the market deposit at 113 entries.
- The NAV price ratio is **cached** (`nav_root_max_age`, default 300s) so the
  withdraw path does not read Reflector inline.
- Past `nav_root_max_stale`, public strategy quotes fail soft to zero. A
  ReceiptVault redemption then sizes from its cached/accounting value and
  supplies a nonzero underlying floor; only that protected exit may use the
  strategy's last ratio, and it still enforces the pool-divergence guard. When
  the live quote is unavailable, share sizing uses the lower nonzero cached or
  accounting estimate to avoid under-redemption from an inflated cache.

Measured: market deposit 90 entries / 5.0M instructions, market withdraw
79 entries / 6.0M instructions.

A controller-wired borrow only fits when the borrower is collateralized in
the boosted market itself. Cross-market shapes exceed the 100-entry limit:
one additional collateral market measures 130 entries with an atomic unwind
(106 even when liquidity is pre-funded), and two additional markets measure
167 entries (143 pre-funded). Until the controller/market footprint is
redesigned, launch a new Aquarius-backed market as supply-only: collateral
factor 0 and borrowing paused. This still delivers LP fees and AQUA emissions
to XLM, PYUSD and USDC suppliers without exposing an unexecutable borrow path.

For rollout, use a separate LP-market Peridottroller from the existing
DeFindex markets. The LP group has different execution and oracle failure
modes, and separating it prevents cross-group collateral positions that do not
fit Soroban's transaction footprint. The supply-only markets can reuse the
existing asset-appropriate JRM; while borrowing is paused the model is not an
active risk surface. Give the LP group its own JRM only when borrowing is later
enabled or its utilization curve needs to differ. Deployment scripts therefore
require `CONTROLLER` to be supplied explicitly instead of silently choosing the
currently deployed controller.

### Known limitations

Three findings from review are accepted rather than fixed:

- **A stale boosted valuation can outlive its bound in the market above.** This
  vault returns a fail-soft zero past `nav_root_max_stale`, while
  `receipt-vault` falls back to `max(cached, estimated)` with no further
  freshness cutoff. Supplier exits remain available when the Aquarius pool is
  still within the last ratio's divergence bound: ReceiptVault passes the cash
  requirement as a nonzero minimum and the vault uses its cached ratio only for
  that protected unwind. The market's cached value can nevertheless overstate
  collateral after an oracle outage plus an adverse price move. Keep these
  markets at CF=0 with borrowing paused until cache freshness is enforced for
  collateral and borrowing decisions.

- **Unclaimed swap fees are not in NAV.** `position_value` excludes fees the
  pool still owes the position, so a deposit landing just before a `harvest`
  is priced against a slightly understated NAV and captures a share of fees
  that accrued before it. Including them needs a `get_all_position_fees` call
  on the deposit path, which already sits at 90 of the 100-entry transaction
  cap through the backing market — there is no room. The exposure is bounded by
  fees accrued since the last harvest, and `harvest()` is permissionless and
  rate-limited to once an hour by default; run it on a keeper to keep the
  window small.
- **Reward swaps have no live oracle cross-check.** Reward tokens generally
  have no Reflector feed, so every route instead requires an independent
  governance minimum raw exchange rate. A missing or breached floor leaves the
  reward idle. Review the floor against external market data and prefer deep
  route pools; do not update it mechanically from the route quote it guards.

### Authorization shape

The Aquarius pool does **not** call `user.require_auth()` on `swap` or
`deposit_position` — it relies on the token transfers carrying the caller's
authorization. So the transfer entries passed to `authorize_as_current_contract`
must sit at the **root** of the authorization list; nested under a pool-call
entry they are unreachable and the transfer fails with `Auth(InvalidAction)`.
This matches the shape `receipt-vault` already uses for DeFindex
(contract.rs:311-327). `withdraw_position` and the claim entrypoints *do*
authorize the pool call itself.

This was only found by running against the deployed pool on testnet — a mock
that required auth where the real pool does not made the wrong tree look
correct. `mock-aquarius-pool` now mirrors the real behaviour.

## Oracle Behavior

- Prices are fetched from the Reflector oracle and normalized by `10^decimals` returned by `decimals()`.
- Staleness: a price is considered stale if `price.timestamp + k*resolution < now`, where `resolution()` is the oracle's reporting interval and `k` defaults to 2.
- Missing or stale prices return `None`. Risk aggregation skips assets with no price. Previews and hypothetical checks will ignore missing-priced assets (collateral contributes 0; additional borrow on a missing-priced asset contributes 0 to USD borrow).
- For production, ensure all market tokens have live oracle prices to avoid permissive paths on borrow of missing-priced assets.

## Liquidation Fee to Reserves

- The Peridottroller can route a portion of seized pTokens to protocol reserves:
  - `set_liquidation_fee(fee_scaled)` sets the fraction (scaled 1e6).
  - `set_reserve_recipient(address)` sets the recipient account for fee pTokens.
  - During `liquidate`, `fee_scaled` of seized pTokens goes to `reserve_recipient`, the remainder to the liquidator.

## Rewards Distribution (Peridot Token)

- Overview

  - Rewards are optional and are not wired by the current testnet/mainnet deployment scripts.
  - If a reward token is explicitly configured later, the peridottroller can distribute Peridot Tokens to suppliers and borrowers per-market using per-second speeds.
  - Rewards accrue lazily on user actions (deposit/withdraw/borrow/repay) and are minted on `claim(user)`.
  - Speeds are set per market independently for supply and borrow sides and are denominated in Peridot base units (decimals typically 6).

- Optional: deploy Peridot Token and wire rewards

```rust
// Deploy Peridot Token (symbol "P", 6 decimals) with admin = peridottroller
use peridot_token as pt;
let peri_id = env.register(pt::PeridotToken, ());
let peri = pt::PeridotTokenClient::new(&env, &peri_id);
peri.initialize(
    &String::from_str(&env, "Peridot"),
    &String::from_str(&env, "P"),
    &6u32,
    &peridottroller_id,
    &1_000_000_000i128, // max supply
);

// Tell peridottroller which token to mint for rewards
peridottroller.set_peridot_token(&peri_id);

// Configure per-market reward speeds (tokens/sec in base units)
peridottroller.set_supply_speed(&market_a_id, &5u128);
peridottroller.set_borrow_speed(&market_b_id, &3u128);

// After some time has elapsed, users can claim accrued rewards
peridottroller.claim(&user);

// Check Peridot Token balance
assert!(peri.balance_of(&user) > 0);
```

- Notes
  - Accrual indices are maintained per market for suppliers and borrowers; a user's accrued amount is tracked and minted on claim.
  - Multi-market rewards are additive across all markets the user has interacted with.
  - Speeds can be updated at any time; indices will advance relative to the last accrual timestamp.

## Upgrades

Both `ReceiptVault` and `SimplePeridottroller` support admin-only in-place WASM upgrades.

```rust
// Admin-only: upgrade contract code to a new wasm hash
let new_hash: BytesN<32> = /* uploaded wasm hash */;
vault.upgrade_wasm(&new_hash);
peridottroller.upgrade_wasm(&new_hash);
```

- Only the respective contract admin may call `upgrade_wasm(new_wasm_hash)`.
- Ensure storage layout compatibility and run migrations as needed on the first call after upgrade.

## Building and Testing

Run all tests:

```bash
cd /home/josh/soroban/peridot-lending/receipt-vault && cargo test
```

## Deployment (sandbox)

Build WASMs and deploy to Soroban sandbox:

```bash
export INIT_ADMIN=$(soroban keys address alice)
bash scripts/build_wasm.sh
bash scripts/deploy_sandbox.sh
```

The deploy script:

- Deploys `SimplePeridottroller`, `JumpRateModel`, a mock USDT token, and two `ReceiptVault` markets
- Leaves `$P` rewards unwired
- Adds markets to the controller, wires controller to vaults
- Configures collateral factors

Update `TOKEN_A`/`TOKEN_B` placeholders in `scripts/deploy_sandbox.sh` with real asset contract addresses.

## Deployment (testnet)

Set up a testnet identity and deploy:

```bash
stellar keys generate --global dev --network testnet --fund
export IDENTITY=dev
export INIT_ADMIN=$(stellar keys public-key "$IDENTITY")
bash scripts/build_wasm.sh
bash scripts/deploy_testnet.sh
```

The script uses `IDENTITY` and will auto-create an open-mint USDT mock and the native XLM asset contract for the two markets. You can override with `TOKEN_A`/`TOKEN_B` if needed.

### Leveraged Margin (testnet)

Deploy the margin manager after the lending markets are live:

```bash
export ORACLE_ID=<reflector_oracle_contract_id>
export CTRL_ID=<controller_id>
export BASE_TOKEN=<xlm_asset_contract_id>
export QUOTE_TOKEN=<usdt_mock_contract_id>
export BASE_VAULT=<vault_a_id>
export QUOTE_VAULT=<vault_b_id>
bash scripts/deploy_margin_testnet.sh
```

### Verify (testnet)

After deployment, verify controller and vault state:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/verify_testnet.sh
```

### Teardown (testnet)

To pause markets during teardown/reset:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/teardown_testnet.sh
```

## Notes

- Events use a single-topic tuple: `(Symbol("event_name"),)` per Soroban topics requirements.
- Re-entry is avoided by excluding the current market in peridottroller aggregation and passing the market’s underlying token where needed.

## Usage Examples

Preview helpers:

```rust
// Max additional borrow from a market
let max_borrow: u128 = peridottroller.preview_borrow_max(&user, &market_vault_id);

// Max redeemable pTokens from a market
let max_redeem_ptokens: u128 = peridottroller.preview_redeem_max(&user, &market_vault_id);
```

Pause controls:

```rust
// Admin pauses operations
peridottroller.set_pause_borrow(&market_vault_id, &true);
peridottroller.set_pause_redeem(&market_vault_id, &true);
peridottroller.set_pause_liquidation(&market_vault_id, &true);

// Optional: set a pause guardian and have it toggle pauses
peridottroller.set_pause_guardian(&guardian);
peridottroller.pause_borrow_g(&guardian, &market_vault_id, &true);
peridottroller.pause_redeem_g(&guardian, &market_vault_id, &true);
peridottroller.pause_liquidation_g(&guardian, &market_vault_id, &true);
```

Caps and wiring:

```rust
// Set caps on a vault (admin-only)
vault.set_supply_cap(&1_000_000u128); // total underlying cap
vault.set_borrow_cap(&500_000u128);   // total borrowed cap

// Wire vault to the peridottroller
vault.set_peridottroller(&peridottroller_id);
```

Additional previews:

```rust
// Max repay allowed by close factor on a market
let max_repay: u128 = peridottroller.preview_repay_cap(&borrower, &repay_market_id);

// Expected pTokens seized for a given repay
let seize_ptokens: u128 = peridottroller.preview_seize_ptokens(&repay_market_id, &collateral_market_id, &repay_amount);
```

Admin transfer:

```rust
// Vault admin transfer
vault.set_admin(&new_admin);

// Peridottroller admin transfer
peridottroller.set_admin(&new_admin);
```

Liquidation fee routing:

```rust
peridottroller.set_liquidation_fee(&200_000u128); // 20%
peridottroller.set_reserve_recipient(&reserve_addr);
```

### Wiring a Jump Rate Model (dynamic APR with kink)

```rust
// Deploy a JumpRateModel and wire to a vault
use jump_rate_model as jrm;
let model_id = env.register(jrm::JumpRateModel, ());
let model = jrm::JumpRateModelClient::new(&env, &model_id);
// base=2%, multiplier=18%, jump=400%, kink=80%
model.initialize(&20_000u128, &180_000u128, &4_000_000u128, &800_000u128);

// Point the vault at the model to enable dynamic rates
vault.set_interest_model(&model_id);

// Thereafter, each `update_interest()` computes supply/borrow APR from utilization and kink.
```

### Controller-managed market parameters

- Collateral factor is stored in the `SimplePeridottroller` per market and used for all USD risk checks.

```rust
// Set CF to 60% for a market (admin-only)
peridottroller.set_market_cf(&market_id, &600_000u128);

// Read CF used in risk checks
let cf = peridottroller.get_market_cf(&market_id);
```

### Admin fee and reserves

Borrow interest is split into reserves, admin fee, and supplier growth.

```rust
// Vault-side configuration (admin-only)
vault.set_reserve_factor(&200_000u128); // 20%
vault.set_admin_fee(&50_000u128);      // 5%

// Read totals
let reserves = vault.get_total_reserves();
let admin_fees = vault.get_total_admin_fees();

// Withdraw
vault.reduce_reserves(&amount);
vault.reduce_admin_fees(&amount);
```

### UX helpers

- Multi-claim and self-claim:

```rust
// Only relevant if a reward token is explicitly configured.
// Claim for a batch of users (permissionless).
// Third parties can trigger claim timing, but rewards are always minted to each user.
peridottroller.claim_all(&vec![user1, user2, user3]);

// User claims their own rewards (auth required for user)
peridottroller.claim_self(&user);
```

- Portfolio view:

```rust
let (rows, (coll_usd, debt_usd)) = peridottroller.portfolio(&user);
// rows: Vec<(market, ptoken_balance, debt, collateral_usd, borrow_usd)>
```
