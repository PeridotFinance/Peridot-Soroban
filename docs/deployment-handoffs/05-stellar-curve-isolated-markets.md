# Handoff 05: Curve on Stellar, prepare the integration, defer markets until liquidity exists

Audience: the implementation LLM working in `PeridotFinance/Peridot-Soroban`.
Written: 2026-10-30 (revised after design review). Read `00-shared-risk-model-and-rollout.md` first. **Off the critical path**: do this after handoffs 07 and 02 have shipped their first stages.

## 0. The ask and the answer

The operator wants Curve on Stellar: each Curve pool pair as its own isolated market group with leverage, and/or Curve pools as underlying yield.

As of 2026-10-30 that cannot go live in a useful form:

- Curve on Stellar is **StableSwap-NG only** (plain coins). Volatile and FX pairs need Curve's **FXSwap**, which the official launch says follows its audit, no date.
- There is **exactly one mainnet pool**: USDC / USDT0 / PYUSD (LP symbol USD3), about **$2.12 TVL**. No gauge, no CRV, no incentives.
- No XLM, EURC, yXLM or solvBTC pool.

**Scope now:** verify the interface and build a disabled **swap route**. **Deferred** until liquidity justifies them: dedicated lending groups per Curve pair and Curve LP vaults.

## 1. Facts (verified unless marked)

| Item | Value |
| --- | --- |
| Pool data API | `https://api2.curve.finance/get_pools/1500` (mainnet), `/get_pools/1501` (testnet), `/get_platforms`. Not `api.curve.finance` |
| Mainnet pool (also the LP token) | `CCCIPCJDDIGV7HAVUNAWHOLD3LGJ7N6OLS5RMGQXJFT2QBDNHMF2DKNI`, created 2026-10-22, WASM hash `4cbbec8e3604da9fb1115949ca737f5c26641a9b580974bd37c0980e7192b631`, A = 1000, LP 18 decimals |
| Coins | USDC `CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75`, USDT0 `CBSJZEIO5C7KC2SF3MKSNXXJSW5G3VTNBX4ATMKUI3B2MR4JKM4R26YF`, PYUSD `CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2` (all 7 decimals) |
| Mainnet factory | Not published; read it from the pool: `pool_info().factory` / `config().factory` |
| Testnet factory | `CANV76QKMKSQ26WZMLGVBGML5BCXKKZN2D2Q7HWFLKAEYZ6WWOJ6MHDS` (from a third-party `DEPLOYMENTS.md`; confirm), 10 test pools |
| Contracts source | Rust rewrite referenced as `curvefi/stableswap-rs` (not public); interface below from the official frontend `curvefi/curve-frontend`, `apps/stellar` |
| Audit / admin | No Stellar audit listed on docs.curve.finance. Pool resolves admin and fee receiver from its factory. Factory admin holder and upgradeability: unknown |

## 2. Interface (all amounts `i128` in native decimals, LP in 18)

- `exchange(caller, i, j, dx, min_dy, receiver) -> i128`; the pool pulls `dx` from `caller`, so `caller` authorizes. A contract can swap as itself.
- `get_dy(i, j, dx)`, `get_dx(i, j, dy)` (approximate).
- `add_liquidity(caller, amounts, min_mint, receiver) -> lp`; `remove_liquidity(caller, burn, min_amounts, receiver, sweep_admin_fees) -> Vec`; `remove_liquidity_imbalance(...)`; `remove_liquidity_one_coin(caller, burn, i, min_out, receiver)`.
- `calc_token_amount`, `calc_withdraw_one_coin`, `get_virtual_price()` (18 dec), `get_balances()`, `stored_rates()`, `total_supply()`, `balance(addr)`, `config()`, `pool_info()`, `pool_snapshot()`.
- EMA oracle state exists, but getter names are **not verified**. Never use the pool EMA as a price source on a pool this small.
- Only plain coins are implemented; yield-bearing or rate-oracle coins cannot be pool coins yet.

## 3. Build now

### 3a. Interface verification

Read the contract spec of the mainnet pool (`stellar contract info interface --id <pool>`) and the factory it reports. Record in `research/curve-stellar.md`: every function signature, EMA getter names, the factory id, admin address, whether the WASM is upgradeable, and the admin-fee settings. Re-run monthly while the deployment is young.

### 3b. Curve route in `swap-adapter`, disabled

Add a Curve route next to Aquarius and Soroswap: register a pool by id with its coin index map, verify `pool_info().coins` at registration, quote with `get_dy`, swap with `exchange(self, i, j, dx, min_dy, receiver)`, check the received amount by balance delta. Admin flag off on mainnet. Tests against a mock with the real names, then on testnet against the Curve testnet factory pools. Measure the footprint with `scripts/check_budget.sh`.

### 3c. Liquidity watcher

A keeper-repo job polls `get_pools/1500` and on-chain state daily and flags a pool as **ready for review** only when:

- TVL ≥ an operator threshold (proposal $1M) for 7 consecutive days with volume every day;
- every coin has a Reflector price (depeg-aware via `price-router`);
- liquidation capacity (shared risk model section 4) supports a meaningful cap;
- the factory admin and upgradeability are known and accepted by the operator.

## 4. Deferred (do not build yet)

- **Lending groups per Curve pair.** A pool like USD3 holds three coins. Any product that holds its LP token is exposed to **all three** (USDC, USDT0, PYUSD), including depeg of any one. A "two-asset group" around a three-coin pool does not isolate the third asset. When groups are built, define them by the **assets actually held**, not by pairs.
- **Curve LP vault.** When built: value LP as `lp × virtual_price` haircut by the worst coin's deviation from peg, using our own oracle prices; withdraw single-sided into the market asset; the shared risk model's share-vault proofs apply.
- **Leverage.** On stable pools leverage is a carry or looping trade, not directional. Directional leverage on Curve only becomes relevant after FXSwap brings FX or volatile pairs.

## 5. Open operator decisions

1. Watcher thresholds.
2. Accept unknown audit/admin status on Curve Stellar, or wait for Curve to publish both.
3. Whether to approach Curve/SDF about Peridot as a launch integrator for FXSwap pairs (BD).

Sources: api2.curve.finance get_pools/1500, get_pools/1501, get_platforms; github.com/curvefi/curve-frontend (apps/stellar, Cypress helpers, PRs #3503 and #3517); stellar.expert contract page for the mainnet pool; docs.curve.finance security/audits; launch coverage in The Block and Blockonomi (2026-10-29); github.com/kebkoether/ufama.
