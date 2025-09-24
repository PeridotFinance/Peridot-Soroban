# Peridot Lending (Soroban)

Peridot lending protocol on Soroban. It consists of:

- `receipt-vault`: per-market vault holding the underlying token, minting/burning pTokens, and handling deposit/withdraw, borrow/repay, interest, reserves, and liquidation hooks.
- `simple-peridottroller`: cross-market risk manager handling supported markets, oracle pricing, account liquidity, liquidation, previews, pause flags, pause guardian, and optional liquidation fee.

## Key Concepts

- Fixed-point scaling: `SCALE_1E6 = 1_000_000` for rates and exchange rates; `BorrowIndex` uses `1e18`.
- Interest: supply and borrow interest accrue via `update_interest`; can use an external Jump Rate Model.
- Oracle: Reflector-based USD prices used in the peridottroller for risk checks.
- No re-entry: cross-contract checks avoid re-entering the same vault (exclusion parameters).

## Contracts and APIs

### ReceiptVault

- Initialization and admin
  - `initialize(token, supply_yearly_rate_scaled, borrow_yearly_rate_scaled, admin)`
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
- Flash loans
  - `flash_loan(receiver, amount, data)` → transfers underlying to `receiver`, then expects repayment of `amount + fee` (fee = `amount * flash_loan_fee_scaled / 1e6`).
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

  - The peridottroller can distribute Peridot Tokens to suppliers and borrowers per-market using per-second speeds.
  - Rewards accrue lazily on user actions (deposit/withdraw/borrow/repay) and are minted on `claim(user)`.
  - Speeds are set per market independently for supply and borrow sides and are denominated in Peridot base units (decimals typically 6).

- Deploy Peridot Token and wire rewards

```rust
// Deploy Peridot Token (symbol "P", 6 decimals) with admin = peridottroller
use peridot_token as pt;
let peri_id = env.register(pt::PeridotToken, ());
let peri = pt::PeridotTokenClient::new(&env, &peri_id);
peri.initialize(&String::from_str(&env, "Peridot"), &String::from_str(&env, "P"), &6u32, &peridottroller_id);

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
bash scripts/build_wasm.sh
bash scripts/deploy_sandbox.sh
```

The deploy script:

- Deploys `SimplePeridottroller`, `JumpRateModel`, `PeridotToken`, and two `ReceiptVault` markets
- Initializes PERI and wires it to the controller
- Adds markets to the controller, wires controller to vaults
- Configures CF and reward speeds

Update `TOKEN_A`/`TOKEN_B` placeholders in `scripts/deploy_sandbox.sh` with real asset contract addresses.

## Deployment (testnet)

Set up a testnet identity and deploy:

```bash
soroban config identity generate myadmin
export IDENTITY=myadmin
export TOKEN_A=<asset_contract_address_A_on_testnet>
export TOKEN_B=<asset_contract_address_B_on_testnet>
bash scripts/build_wasm.sh
bash scripts/deploy_testnet.sh
```

The script uses `IDENTITY`, `TOKEN_A`, and `TOKEN_B` environment variables. Replace placeholders with real contract addresses before running.

### Verify (testnet)

After deployment, verify controller and vault state:

```bash
export CTRL_ID=<controller_id>
export VA_ID=<vault_a_id>
export VB_ID=<vault_b_id>
bash scripts/verify_testnet.sh
```

### Teardown (testnet)

To pause markets and zero reward speeds (safe teardown/reset):

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
// Claim for a batch of users (permissionless)
peridottroller.claim_all(&vec![user1, user2, user3]);

// User claims their own rewards (auth required for user)
peridottroller.claim_self(&user);
```

- Portfolio view:

```rust
let (rows, (coll_usd, debt_usd)) = peridottroller.portfolio(&user);
// rows: Vec<(market, ptoken_balance, debt, collateral_usd, borrow_usd)>
```
