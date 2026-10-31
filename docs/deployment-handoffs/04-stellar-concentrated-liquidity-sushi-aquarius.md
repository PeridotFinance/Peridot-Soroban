# Handoff 04: Concentrated liquidity strategies on Stellar (Aquarius CL and Sushi CL)

Audience: the implementation LLM working in `PeridotFinance/Peridot-Soroban`.
Written: 2026-10-30 (revised after design review). Read `00-shared-risk-model-and-rollout.md` first (it overrides this file where they differ), then handoff 03; it decides whether the Sushi half of this file gets built. Per the rollout order, this work comes **after** handoff 07's first stable-pair share vault.

## 0. Goal

Run an efficient keeper-managed concentrated-liquidity strategy behind Peridot lending markets on Stellar: range around an oracle price, recenter, collect fees, harvest rewards, all through the existing boosted-vault seam of `ReceiptVault`. Two venues: **Aquarius CL** (primary) and **Sushi CL** (secondary, capped).

## 1. What already exists (do not rebuild it)

Peridot already runs **Aquarius CL strategies on mainnet**. Before writing anything, read:

- `peridot-contracts/contracts/aquarius-lp-vault/` (`contract.rs`, `pool.rs`, `oracle.rs`, `math.rs`, `storage.rs`, `REWARD_SETTLEMENT_DESIGN.md`). It already: owns a single concentrated position, keeps an oracle-valued NAV (`nav_root`, pool/oracle divergence guard), recenters via `set_range_policy` / `needs_rebalance` / `rebalance` (half-width, rebalance margin, cooldown, tighter rebalance-divergence guard), harvests AQUA (`harvest`, `set_reward_route`, `set_reward_min_rate`, hybrid reward mode), and implements the boosted ABI so it attaches through unmodified `set_boosted_vault` + `set_receipt_vault`.
- `peridot-contracts/bots/aquarius-keeper/` on branch **`aquarius-keeper-v0.4.3`** (newest keeper, 2026-09-29: harvests gated on fee economics). One sequence-safe worker, NAV root refresh every 20 minutes, simulated `needs_rebalance`, harvest after 6h when worth it, refresh receipt-vault boosted cache. Dedicated signer with no protocol role.
- Branches `aquarius-price-observer-v0.1.x` (keyless cloud LP price observer, depth-TWAP candidate) and `feat/aquarius-lp-vault`.
- `addresses.md`: live strategies (XLM on XLM/yXLM CL `CADMDTCQ...HFS7F`; PYUSD and USDC on PYUSD/USDC CL `CAPIOQNU...HIDQYX`), range half-width 40 ticks, LP pilot controller `CCZKDMAP23ZFL55RVITKSW4LAONQABGPSK2Y77RS64GPHOVDDFF5ENGC`.
- The single-asset design: a market deposits one token; the vault swaps the paired leg at deposit and values everything in the market's underlying with the manipulation-resistant `nav_root`. Keep that model.

**Rule:** `aquarius-lp-vault` is live. Do not change its code for this work. New venues and pairs are new deployments (same WASM, new init params) or a sibling crate.

## 2. Current venue data (2026-10-30; reproduce before deciding)

### Aquarius CL (router `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK`, 45 concentrated pools per `amm-api.aqua.network/pools/?pool_type=concentrated`)

| Pair (fee) | Pool | TVL | Volume field | Fee APY | Reward APY (base) |
| --- | --- | --- | --- | --- | --- |
| **XLM/USDC 0.1%** | `CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV` | ~$1.24M | ~$1.16M | ~7.9% | ~3.7% |
| XLM/AQUA 0.3% | `CA4HTZNY...VID2S` (resolve full id via `get_pools`) | ~$212k | ~$149k | ~12.6% | ~20.9% |
| AQUA/USDC 0.3% | `CBRUQ7I6...YYZ7` (resolve) | ~$69k | ~$35k | ~9.5% | ~38.1% |
| USDC/sUSD 0.1% | `CBV6LZAB...WGWQ` (resolve) | ~$213k | ~$138k | ~6.0% | ~7.6% |
| XLM/yXLM 0.1% (live Peridot target) | `CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F` | ~$313k | ~$1.5k | ~0% | ~4.8% |
| PYUSD/USDC 0.1% (live Peridot target) | `CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX` | ~$276k | ~$158 | ~0% | ~8.3% |
| USDC/EURC 0.3% | `CDTSE6RLRI7ZO25JSER6E4SQR4PHJJNONEGS5HDJ3Y6LAKECRZKYN5CA` | ~$111k | ~$8.5k | ~1.1% | ~3.7% |

API APYs are claims with an undocumented volume window. The XLM/USDC CL pool currently shows `tick_spacing` 20, `current_tick` -18218. For comparison, the classic XLM/USDC constant-product pool `CA6PUJLB...CJBE` holds ~$3.2M at ~1.4% fee APY. Protocol fee share looks like 50% (inferred from `protocol_fees == lp_fees` in daily totals; read `get_protocol_fee_fraction()` to confirm).

**What changed versus our earlier "Aquarius CL ruled out" conclusion:** the ICE-boost part still holds (a contract cannot hold ICE, so the vault earns base rewards only, boost 1x). The economics part needs a fresh look: a real XLM/USDC CL pool now exists with ~$1.24M TVL and roughly 11.6% fee+base reward APY unboosted, about 5x the classic pool's fee APY. Whether a narrow range beats IL on XLM is still open and must be settled by the backtest in section 5, not assumed either way. USDC/EURC is still effectively dead.

### Sushi CL (see handoff 03 for full context)

Uniswap-v3 port, NFT positions, fee tiers 500/10, 3000/60, 10000/200, on-chain TWAP (`observe`, `observe_single`), no incentives, no public audit, admin-upgradeable pools (`upgrade`, `migrate`, `pause`; factory `upgrade_pool`, `set_upgrade_frozen`, `disable_upgrades_permanently`). Only XLM/USDC has meaningful flow (~$126k/day, ~$13k depth at ±2%). Position manager `CARTUL5AWDZYBSN7HUUJZSKCAKCIAKM7M54Z76G6KRYCK4XPR3OHUQZ4`, factory `CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF`, router `CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL`.

## 3. Venue mechanics that the vault and keeper must respect

### Aquarius CL (docs.aqua.network, contract interface reference)

- **No NFT, no salt.** A position is keyed by `(owner, tick_lower, tick_upper)`. Depositing into the same range increases it. **Max 20 positions per owner per pool.** Every recenter is a new key; withdraw the old range completely so stale keys do not pile up toward the limit.
- `deposit_position(sender, tick_lower, tick_upper, desired_amounts, min_liquidity) -> (Vec<u128>, u128)` refunds unused amounts in the same transaction. `withdraw_position(owner, tick_lower, tick_upper, amount, min_amounts) -> Vec<u128>` pays out **all** accrued fees of the position, **even on a partial withdrawal**. Any pooled vault must book the fee portion separately and keep the remaining holders' share; never pass the full fee payout to the withdrawing holder.
- Principal withdrawal has **no kill switch**; swaps and reward claims can be killed. Design exits so principal recovery never depends on a swap or a claim succeeding. `claim_position_fees`, `claim_all_position_fees(owner)`, `get_position_fees`, `get_position`, `get_user_position_snapshot`.
- **Rewards are binary in range and checkpointed only on deposit, withdraw or claim.** The stored weight is not updated when price moves. Aquarius recommends calling `claim` after price crosses a range boundary. The keeper must call `claim` (through the vault's harvest path) whenever the tick leaves or re-enters the range, or the vault keeps earning on a stale weight in one direction and missing it in the other.
- **Exact-in swap edge case:** if a swap runs out of liquidity, the unswapped remainder is given to positions at the last active tick instead of being refunded. Any swap the vault does (deposit-leg swap, reward conversion) must use `estimate_swap` first and a strict `out_min`, and keep sizes far from available depth.
- **Price source:** only spot `get_slot0`; no TWAP. Anchor everything to Reflector.
- Admin kill switches: `deposit_killed`, `swap_killed`, `claim_killed`. The vault must treat each as a named, expected failure (pause deploys, keep withdrawals working where the pool allows).
- Audits: CoinFabrik 2024, Certora Dec 2024, Cantina 2025, **Halborn Jun to Aug 2026 explicitly covering CL** (36 findings, none critical or high).

### Sushi CL (from `sushi-labs/sushiswap` contract bindings)

- Position manager `mint({token0, token1, fee, tick_lower, tick_upper, amount0_desired, amount1_desired, amount0_min, amount1_min, recipient, sender, deadline}) -> (token_id u32, liquidity, a0, a1)`; `increase_liquidity`, `decrease_liquidity`, `collect`, `burn`; each with a `*_with_hints(params, OracleHints{checkpoint, checkpoint_min, slot})` variant.
- Token order by raw contract-id bytes.
- Pool: `slot0`, `liquidity`, `tick_spacing`, `fee`, `observe(seconds_agos, hints)`, `observe_single`, `get_oracle_status`; error 62 `ObservationTooOld`; no cardinality increase function.
- Valuation helpers on the position manager: `position_principal(token_id, sqrt_price_x96)`, `position_fees`, `get_position_with_fees`.
- Fees only, no reward leg.

## 4. Design

### 4a. Aquarius: new deployments of the existing vault

New deployments of the existing WASM with new `initialize` parameters, attached with `set_boosted_vault` + `set_receipt_vault`, **only if** a test against the deployed WASM proves it already supports every required path: reward checkpoint (`claim`) independent of AQUA conversion, conversion failure not blocking principal exits, correct fee handling when Aquarius pays all accrued fees on a partial withdrawal, and an exit that works with swaps and claims killed. Any missing path means a code change (new WASM, reviewed) or, for leveraged use, the `cl-share-vault` from handoff 07. Record the result before deploying new instances.

Candidates:

| Candidate | Market underlying | Notes |
| --- | --- | --- |
| **XLM/USDC 0.1% CL** | USDC market and/or XLM market | Primary new candidate. Wider range than the stable pilots; the pilot's 40-tick half-width is for correlated pairs and would recenter constantly on XLM/USDC |
| XLM/AQUA 0.3%, AQUA/USDC 0.3% | XLM or USDC market | Reward-driven, small TVL, AQUA price risk on the paired leg. Only with tight `max_deploy` and an explicit operator yes |
| USDC/sUSD 0.1% | USDC market | Stable-ish. Needs a decision on accepting sUSD risk and a price for sUSD |

Check one constraint first: the receipt vault enforces one-to-one boosted-vault ownership across markets on the same controller. Two markets cannot share one strategy instance. Two strategies on the same pool from the same contract owner are separate positions only if they are separate contracts (positions key on owner).

### 4b. Sushi: sibling crate `sushi-cl-vault`

Only if handoff 03 says go. Same external ABI and the same internal model as `aquarius-lp-vault` (single underlying, paired leg swapped at deposit, oracle `nav_root`, range policy, keeper-only rebalance), with a Sushi venue module instead of `pool.rs`:

- The vault owns one NFT, never approves anyone, burns the old NFT on recenter after `decrease_liquidity` + `collect`.
- Valuation: `position_principal(token_id, oracle_sqrt_price)` plus collected-but-idle balances. Never value at pool spot. Use `observe` TWAP only as a second guard next to the Reflector divergence check.
- Pass `OracleHints` where the bindings require them; measure footprint with and without hints.
- Governance guard: store the expected WASM hash of the pool and position manager; refuse deploys if either changes or if the pool is paused; keep withdrawals possible where the venue allows.
- Copy `oracle.rs`, `math.rs`, range logic and NAV code into the new crate rather than refactoring the live crate into a shared library. A shared `cl-vault-core` crate is a good later cleanup, done with a fresh review and without changing the live WASM hash. Handoff 03 points here: the sibling crate is the chosen architecture.

### 4c. Oracle anchoring

| Asset | Reflector feed |
| --- | --- |
| XLM, BTC | External CEX/DEX, USD-quoted: `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN` (already used by Peridot) |
| AQUA, EURC, PYUSD, yUSDC | Stellar DEX feed, USDC-quoted: `CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M` (AQUA is often missing on the external feed) |
| EUR | FX feed: `CBKGPWGKSKZF52CFHMTRR23TBWTPMRDIYZ4O2P5VS65BMHYH4DXMCJZC` |

Verify each id and asset coverage on chain before use. A DEX-sourced AQUA price is partly derived from the same pools we LP in; cap AQUA-paired strategies accordingly and use the existing `max_pool_divergence_bps` and staleness `k` settings.

## 5. Range economics: decide from a backtest, not from defaults

Deliver `research/cl-backtest/` with a reproducible script that, per candidate pool:

1. Pulls 30 to 90 days of swaps (and liquidity changes) from Soroban events or the venue API, plus Reflector prices.
2. Simulates our position for half-widths of, for XLM/USDC, roughly ±2%, ±5%, ±10%, ±20% (convert to ticks with spacing 20 on Aquarius, 10/60/200 on Sushi), with the vault's real trigger (`rebalance_margin_ticks`), cooldown and swap cost on recenter.
3. Keeps **one portfolio P&L ledger** (shared risk model section 3): fees at the measured protocol share, base AQUA rewards only while in range (no ICE boost) and valued at the price the deployed conversion path would actually get, recenter swaps, Soroban fees, all marked at the oracle. Do not subtract IL, LVR and realized rebalance losses separately; report net return against holding the underlying and against a 50/50 rebalanced portfolio.
4. Recomputes with our own capital added at several sizes (dilution of fees and rewards).
5. Outputs net APY in the market's underlying, compares it to the realistic alternative yield for that asset (local lending rate, Upshift range), and states **profitable capacity**: "supports about $X at about Y% net". A candidate ships only if it beats the alternative by a margin the operator sets, over at least two volatility regimes in the window.

Starting points to test, not to ship: XLM/USDC half-width ±8% with recenter margin 25% of half-width and 6h cooldown; XLM/AQUA ±10%; stable pairs keep the 40-tick pilot policy.

## 6. Keeper changes (extend `bots/aquarius-keeper` from `aquarius-keeper-v0.4.3`)

- Add strategies from a config file, one serial worker, one signer, no protocol role (keep the sequence-safety rule: never two live copies on one key).
- **Range-cross claim for Aquarius:** track last known in-range state per strategy; on a cross, run the vault's harvest/claim path so the reward weight is re-checkpointed. Gate on simulated value as the current harvest gate does, but always re-checkpoint on the cross even when the AQUA amount is small, because a stale weight costs more over time than the fee.
- **Recenter:** simulate `needs_rebalance`, submit `rebalance` when true; respect cooldown; on Aquarius make sure the old range is fully withdrawn (20-position cap).
- **Sushi:** `collect` cadence by fee economics; no reward leg.
- **Fail-closed reads:** stale Reflector, pool/oracle divergence, killed flags, WASM-hash change on Sushi. Each is logged with a named reason; none triggers a paid retry loop.
- **Checkpoint is separate from conversion:** the range-cross reward checkpoint (`claim`) must succeed even if the AQUA→underlying swap would fail (swap killed, price guard). Claimed AQUA waits in the vault for a later conversion; it never blocks checkpoints or principal exits.
- Budget: measure resource fees per action like the README does (refresh-only vs harvest cycles) and update the signer funding note.

## 7. Tests and gates

| Gate | Evidence |
| --- | --- |
| Backtest (section 5) | Net APY tables per pool and range; operator approves pools and ranges |
| Unit | Sushi venue module against a faithful mock (NFT lifecycle, hints, observe, paused pool, upgraded WASM hash); Aquarius new-pair init params with a mock that reproduces the 20-position cap, the binary reward checkpoint and the exact-in remainder behavior |
| Integration | `receipt-vault` + strategy: idle buffer respected, deposit/withdraw/borrow/liquidation paths fit the 100-entry footprint (`scripts/check_budget.sh`), withdraw pulling from the position in one transaction |
| Mainnet-state simulation | Every keeper action simulated against current mainnet state before signing |
| Rollout | Same pattern as `rollout_aquarius_xlm40_mainnet.sh`: deploy paused, verify bindings both ways, tiny first deposit, keeper monitor-only 24h, then enable |

## 8. Open operator decisions

1. Which Aquarius pools (XLM/USDC only, or also AQUA pairs and USDC/sUSD) after the backtest.
2. Which markets and groups they attach to (core, LP pilot, Upshift group, a new isolated group). One boosted strategy per market.
3. Go or no-go on Sushi (from handoff 03) and the cap if go.
4. Reward handling: keep AQUA conversion into the underlying (current hybrid design) or split part to reserves.

Sources: docs.aqua.network (integrating with Aquarius, concentrated liquidity, contract interface reference, position code examples, ICE boosts, audits, backend API); amm-api.aqua.network pool and totals endpoints; Sushi contract bindings in github.com/sushi-labs/sushiswap; docs.sushi.com/contracts/clamm; developers.stellar.org oracle providers and reflector.network docs; existing Peridot code and `addresses.md`.
