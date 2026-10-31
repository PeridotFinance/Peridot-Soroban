# Handoff 03: SushiSwap on Stellar, which pools are worth an underlying vault strategy

Audience: the implementation LLM working in `PeridotFinance/Peridot-Soroban`.
Written: 2026-10-30. Read `00-shared-risk-model-and-rollout.md` first; this work is off the critical path and its backtest follows that file's single-ledger and profitable-capacity rules. This is an exploration and decision handoff. The contract work for a Sushi CL vault itself is in **handoff 04** (shared CL vault design for Sushi and Aquarius). Do this one first: it decides whether handoff 04 builds a Sushi venue at all.

## 1. Summary

Sushi on Stellar is **one product: a Uniswap-v3-style concentrated-liquidity AMM** (no v2 constant-product AMM on Stellar). Protocol TVL about **$15.3M** (DefiLlama, 2026-10-29), but it is almost all stablecoins and RWA tokens, and volume is thin (about **$134k/day** total). Only two pools carry real flow:

| Pool | 24h volume (CoinGecko) | ±2% depth | Lending-relevant? |
| --- | --- | --- | --- |
| **XLM/USDC** | ~$126k | ~$13k | **Yes.** The only Sushi pool worth a strategy |
| PYUSD/USDC | ~$8k | ~$13k | Stable-stable, low fee yield. Possible for the existing PYUSD/USDC LP group only |
| USDT0/USDC | ~$100 | ~$96k | Deep but idle. No |
| DeJTRSY/USDC, DeJAAA/USDC, USDY/USDC | ~$0 to ~$1 | deep | RWA, idle, outside our asset set. No |

Assets Peridot holds beyond XLM/USDC have essentially **no** Sushi liquidity (TVL by token, DefiLlama 2026-10-12): EURC $20, AQUA $43, BLND $7, solvBTC $18. XLM total on Sushi is about $336k.

For XLM/USDC, **Aquarius is far deeper**: its 0.10% XLM/USDC pool did about $1.16M/day, roughly 9x Sushi's. Soroswap is far thinner than both (about $10k/day total).

**Recommendation:** Sushi is a secondary venue. The only candidate is **XLM/USDC as a small, capped CL strategy** next to (not instead of) Aquarius, mainly as diversification against a single-venue failure and because its volume/TVL ratio is high. Do not build Sushi strategies for stable pairs or for EURC/AQUA/BLND/solvBTC.

## 2. Contracts (verified in the Sushi monorepo `sushi-labs/sushiswap`, `packages/stellar/contract-bindings` and `apps/web/.../stellar/_common/lib/soroban/contracts/index.ts`, and docs.sushi.com/contracts/clamm)

| Contract | ID |
| --- | --- |
| Factory | `CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF` |
| Position Manager (NFT) | `CARTUL5AWDZYBSN7HUUJZSKCAKCIAKM7M54Z76G6KRYCK4XPR3OHUQZ4` |
| Router (docs label it "Quoter") | `CDMIM23WOUL5CZBKX3GOA3V5R5AMVIMTCP52KCDQORWELAPLJ27WZCHL` |
| Pool Lens / Tick Lens | `CASKWJSINHFW7BF7RUOA4E2FP6B2TYRKFX2UOPWLCPOOPUR6UU3G2RWC` |
| Zap Router | `CAS3HJ4GNGJBRI2C4YY3JB6J7Q6RIIA56XHRAI2TWYV4GAROB6SZMLNG` |
| Flash Executor | `CDKKPFBNNQETVN4Y2CIU6CQMWCPIZDXDZE76IDSTF7HLQT6Q573PCDTH` |
| Legacy factory / legacy position manager (do not use) | `CCRSMJDITH3VK5QOGYCVZDAKIY5GL3RCG4TCVLIAVB662IW2V5KJGZGF` / `CC5CQHSGZEVKPDLMYTJYGUBDL5UW4NBMTRQ5Y43YDBJTJZKMZMKCEEDU` |

Fee tiers / tick spacing: **500/10, 3000/60, 10000/200**. At least 43 pools exist. Deployer of factory and position manager: `GC7RIP4D7UETKBZU7FNUYW5XIR2IPLUQDV2DUTZRKVF6C3P2A5QEDSZI`. Contract source is referenced as `github.com/hyplabs/sushi-stellar` (not publicly accessible when checked). stellar.expert shows both contracts as **unverified**; the position manager has had 2 WASM versions. **No public audit found.** The admin has already upgraded and migrated pools (new factory 2026-03-02, migration 2026-03-11), and the app still warns that unmigrated legacy positions risk restricted principal.

## 3. Discovery tasks (do these first; they were blocked from the research environment)

1. **Decode the pool list** from the factory's `pool_created` events (Soroban RPC `getEvents` on the factory id, or stellar.expert API). Produce `research/sushi-pools.json`: pool id, token0, token1 (contract ids), fee, tick spacing, created ledger.
2. **Per-pool state** for every pool: `slot0`-equivalent, liquidity, reserves, and 7 and 30 days of swap events. Compute volume, fees, fee APR on full TVL **and** on in-range liquidity for ranges of ±2%, ±5%, ±10%.
3. Try Sushi's data API (`data-api.sushi.com/api/graphql`, query `stellarPools` with `liquidityUSD`, `volumeUSD1d`, `feeUSD1d`, `totalApr1d`, `incentiveApr`) and cross-check against your own numbers. Treat API APRs as claims until reproduced.
4. Confirm the **XLM/USDC fee tier**. It decides everything: at about $126k/day, a 0.30% pool earns about $378/day gross, a 0.05% pool about $63/day. On a pool below roughly $0.7M that is the difference between a double-digit and a low single-digit fee APR before IL.
5. Check whether more than one XLM/USDC pool exists (different tiers) and where the volume actually goes.
6. Read the factory's upgrade-freeze state (`set_upgrade_frozen` exists; whether it is set was not found), the current admin/owner of factory, position manager and the target pool, and the protocol fee (`set_protocol_fee` / `collect_protocol`).
7. Ask Sushi (BD task for the operator) for the audit report and the upgrade policy. Without either, cap exposure hard.

## 4. Selection criteria

Score each candidate pool and record the result in `research/sushi-pools-decision.md`:

| Criterion | Threshold for "worth a strategy" |
| --- | --- |
| Asset fit | Both tokens are assets a Peridot market holds, or one is the market asset and the other is a stable we accept as IL exposure |
| Volume / TVL | Fee APR on the chosen range, net of protocol fee, beats Blend supply APY for the same asset by a margin that covers estimated IL at measured volatility |
| Depth | Our max position ≤ 10% of in-range liquidity, and a full exit moves price < 1% |
| Oracle | Both assets priced by Reflector (`CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN`) so the range and NAV are oracle-anchored. Pool `observe`/`observe_single` TWAP is a secondary check, not the anchor |
| Governance | Upgrade frozen or timelocked, or an explicit operator acceptance of upgrade risk with a cap |

Expected outcome from current data: **XLM/USDC passes asset fit and volume/TVL, fails depth for anything but a small allocation (±2% depth about $13k), and fails governance until Sushi answers.** Everything else fails asset fit or volume.

## 5. If XLM/USDC passes

- Implement it as the **sibling crate `sushi-cl-vault`** specified in handoff 04 section 4b: same external ABI and internal model as `aquarius-lp-vault`, with a Sushi venue module. This is the single chosen architecture; a shared `cl-vault-core` library is a later cleanup, not part of this work.
- Attach it to a market through the unmodified boosted seam (`set_boosted_vault` then `set_receipt_vault`), like `aquarius-lp-vault`.
- Cap: `max_deploy` such that our liquidity stays ≤ 10% of in-range liquidity at the time of deposit, re-checked on every deploy; start below $10k.
- Candidate homes: the XLM and USDC markets of a new isolated group, or a slice of the Upshift group from handoff 02 (one boosted strategy per market, so pick one per market; a split router is out of scope).
- Margin can also use Sushi as a **second swap route** (router `CDMIM23W...`). That needs a `swap-adapter` extension for the Sushi router call shape; scope it as its own small task with tests, after handoff 04.

## 6. Integration notes (verified from the TypeScript bindings)

- Position manager: `mint({token0, token1, fee, tick_lower, tick_upper, amount0_desired, amount1_desired, amount0_min, amount1_min, recipient, sender, deadline})` returns `(token_id: u32, liquidity: u128, amount0, amount1)`; `increase_liquidity`, `decrease_liquidity`, `collect`, `burn`; each also has a `*_with_hints(params, OracleHints{checkpoint, checkpoint_min, slot})` variant.
- **Token order is by raw contract-id bytes, not by the C... string.** Sort accordingly or `mint` reverts.
- Positions are transferable NFTs (`owner_of`, `transfer`, `approve`, `get_user_token_ids`). The strategy contract must own its NFT and never approve anyone.
- Valuation helpers: `position_principal(token_id, sqrt_price_x96)`, `position_fees`, `position_total`, `get_position_with_fees`. Value positions at the **oracle** sqrt price, not pool spot.
- Observed `mint` resource fee about 0.53 to 0.69 XLM; measure all calls with `scripts/check_budget.sh` against a mainnet-state simulation.
- No Sushi incentive program on Stellar was found, so there is no reward harvest leg; fees only.

## 7. Deliverables

1. `research/sushi-pools.json` and `research/sushi-pools-decision.md` (sections 3 and 4) with numbers you reproduced.
2. A one-paragraph go / no-go per pool for the operator.
3. If go for XLM/USDC: a ticket in handoff 04's plan for the Sushi venue adapter, with the fee tier, pool id, caps and governance acceptance filled in.

Sources: docs.sushi.com/contracts/clamm and /cpamm; github.com/sushi-labs/sushiswap (stellar contract bindings, web app stellar module, PR #2313); stellar.expert API pages for the factory and position manager; api.llama.fi/protocol/sushi-stellar, aquarius-stellar, soroswap, blend-pools-v2; coingecko.com exchanges sushiswap-v3-stellar and aquarius; amm-api.aqua.network external pools and totals.
