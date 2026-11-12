> For the complete documentation index, see [llms.txt](https://docs.aqua.network/llms.txt). Markdown versions of documentation pages are available by appending `.md` to page URLs; this page is available as [Markdown](https://docs.aqua.network/developers/concentrated-liquidity.md).

# Concentrated liquidity

How Aquarius concentrated liquidity pools work under the hood — ticks, prices, positions, fees and rewards. A guide for developers managing positions with code instead of the UI.

Aquarius concentrated liquidity pools are inspired by the Uniswap V3 primitive: instead of spreading liquidity across the whole price curve, each liquidity provider chooses a **price range** where their capital is active. This section explains the on-chain model and the smart contract interface so you can create, monitor and manage positions programmatically.

If you are looking for the web interface guide instead, see [Manage concentrated liquidity pool positions](/user-guides/pools/manage-concentrated-liquidity-pool-positions.md).

### Architecture at a glance

Two contracts matter for a position manager:

* **The AMM router** — the same entry point used by all Aquarius pools ([addresses here](/developers/reference/addresses-and-networks.md)). Use it to discover pools (`get_pools`, `get_info`), create new concentrated pools (`init_concentrated_pool`) and execute swaps (`swap`, `swap_chained`).
* **The pool contract itself** — each concentrated pool is a separate contract. All position management (`deposit_position`, `withdraw_position`, fee claiming, state getters) is done by calling the pool contract **directly**, not through the router.

A concentrated pool always holds exactly **two tokens**. Token order matters everywhere in the interface: `token0` and `token1` are the pool tokens sorted by contract ID, the same ordering used across Aquarius ([token ordering](/developers/reference/router-and-pool-contracts.md#token-ordering)). `get_tokens` on the pool returns the sorted pair.

### Prices and ticks

The pool tracks its current price as a square root in Q64.96 fixed-point format, exactly like Uniswap V3:

* `sqrt_price_x96 = sqrt(price) * 2^96`, where **price is the amount of `token1` per one unit of `token0`, in raw (stroop) units**. Since Stellar assets use 7 decimals on both sides, the raw ratio equals the human-readable price for classic asset pairs.
* Every price maps to a **tick**: `price(tick) = 1.0001^tick`. Tick `0` means a price of exactly 1.0, positive ticks mean `token0` is worth more `token1`, negative ticks less. One tick is a 0.01% price step.
* Ticks are bounded by `MIN_TICK = -887272` and `MAX_TICK = 887272`, covering the price range `[1.0001^-887272, 1.0001^887272]` — effectively any price.

The current price state is returned by `get_slot0` as `{ sqrt_price_x96, tick }`. To convert between prices and ticks off-chain:

```
tick  = floor( log(price) / log(1.0001) )
price = 1.0001 ^ tick
```

### Fee tiers and tick spacing

Positions cannot start or end at arbitrary ticks — the boundaries must be multiples of the pool's **tick spacing**, which is fixed at pool creation and derived from the fee tier:

| Fee tier | `fee` value | Tick spacing | Boundary granularity |
| -------- | ----------- | ------------ | -------------------- |
| 0.1%     | `10`        | `20`         | \~0.2% price steps   |
| 0.3%     | `30`        | `60`         | \~0.6% price steps   |
| 1.0%     | `100`       | `200`        | \~2% price steps     |

`fee` is expressed in basis points of 1/10000: `30` means 0.3% charged on every swap. Query a pool's parameters with `get_fee_fraction` and `get_tick_spacing`, or both at once via `get_info` (returns `pool_type: "concentrated"`, `fee` and `tick_spacing`).

Up to three concentrated pools (one per fee tier) can exist for the same token pair. New pools are created through the router's `init_concentrated_pool(user, tokens, fee)` — tick spacing is set automatically from the fee tier, and a pool creation payment may be charged in AQUA, same as for other pool types.

### Positions

A position is identified by the triple **(owner address, `tick_lower`, `tick_upper`)** — there are no position NFTs or numeric position IDs. Consequences of this model:

* Depositing again into the same range **adds liquidity to the existing position** rather than creating a new one.
* One account can hold up to **20 positions per pool**. Ranges must satisfy `tick_lower < tick_upper`, both multiples of the tick spacing, within the `MIN_TICK`/`MAX_TICK` bounds.
* All positions of an account can be listed on-chain with `get_user_position_snapshot(user)`, which returns the tick ranges and the total liquidity. Details of a single position are available via `get_position(owner, tick_lower, tick_upper)`.

A position holds an abstract **liquidity** amount (the `L` of the constant-product formula, not a token amount). How much of each token a given liquidity amount represents depends on where the current price sits relative to the range:

* **Price below the range** — the position is 100% `token0` (waiting for the price to rise into the range).
* **Price inside the range** — the position holds both tokens; the ratio shifts continuously as the price moves.
* **Price above the range** — the position is 100% `token1`.

You can therefore open one-sided positions deliberately: a range entirely above the current price requires only `token0`, entirely below — only `token1`.

When depositing, the amounts you pass are **maximums you authorize**, not exact spends: the contract computes the largest liquidity purchasable at the current price, takes what it needs and refunds the remainder within the same transaction. The actual amounts spent are returned by the call.

The **router-compatible `deposit`/`withdraw`** functions also work on concentrated pools: they open and close a **full-range position** (the widest range allowed by the tick spacing), making the pool behave like a classic volatile pool. This is what keeps concentrated pools compatible with generic Aquarius integrations.

{% hint style="warning" %}
The **first deposit into an empty pool sets the initial price** from the ratio of the two deposited amounts, and must therefore include both tokens. Double-check your amounts: a wrong initial price is an immediate arbitrage gift to the next trader.
{% endhint %}

### Swap fees

Swap fees accrue to positions whose range contains the trade's price path, proportionally to their share of active liquidity. Fees are tracked per position and do not compound. A protocol share of the swap fee (query `get_protocol_fee_fraction`) is diverted to the protocol treasury before LP distribution.

One extra source of LP income is specific to concentrated pools: if an exact-input swap exhausts the pool's initialized liquidity before consuming its whole input, the unswapped remainder is not refunded to the trader — it is distributed as additional fees to the positions covering the last active tick (see the [swaps section of the reference](/developers/concentrated-liquidity/contract-interface-reference.md#pool-contract-swaps)). This rewards LPs who extend liquidity coverage to the edges.

Collecting fees:

* `claim_position_fees(owner, tick_lower, tick_upper)` — collects accrued fees of one position.
* `claim_all_position_fees(owner)` — collects across all your positions in the pool.
* `withdraw_position` automatically pays out the position's **entire accrued fee balance** together with the withdrawn principal — even on a partial withdrawal. There is no way to withdraw principal while leaving fees unclaimed.

Pending amounts can be checked without a transaction via `get_position_fees` / `get_all_position_fees` (simulation-only calls).

### AQUA rewards

Concentrated pools participate in the standard Aquarius [AMM rewards](/voting-and-rewards/aquarius-amm-rewards.md) system, with one crucial difference: **a position earns rewards only while the current tick is inside its range** — the check is `tick_lower <= current_tick < tick_upper`, a binary in-range test. An out-of-range position earns nothing until the price returns.

Rewards accrue to the owner across all their in-range positions and are claimed per pool with `claim(user)` (also available through the router as `claim(user, tokens, pool_index)`). Estimate pending rewards with `get_user_reward(user)`. ICE boosts apply to concentrated positions the same way they do for other pools.

{% hint style="warning" %}
Reward accounting is **checkpoint-based**. Your reward weight — including the in-range check — is evaluated at the moment you interact with the pool (deposit, withdraw, claim) and stays fixed until your next interaction; the pool does not re-evaluate it as the price moves. In practice: a position that drifted out of range keeps accruing rewards at its last-checkpointed weight until your next action, and a position that came back into range earns nothing until an action re-snapshots it. If you manage positions actively, call `claim` (or perform any position action) after the price crosses your range boundary to bring your reward weight up to date.

This checkpoint behavior is a property of the current contract version and **may change in future versions** — don't build long-term assumptions on it.
{% endhint %}

### Typical position lifecycle

1. Find the pool: router `get_pools(tokens)` → filter by `get_info` `pool_type == "concentrated"`.
2. Read the current state: `get_slot0`, `get_tick_spacing`.
3. Choose a price range, convert to ticks, align to tick spacing.
4. Simulate `estimate_deposit_position(tick_lower, tick_upper, desired_amounts)` to see the actual amounts and liquidity.
5. Call `deposit_position(...)` with a `min_liquidity` slippage guard.
6. Monitor: `get_slot0().tick` vs your range; rebalance by withdrawing and re-depositing when the price escapes.
7. Collect: `claim_position_fees` for swap fees, `claim` for AQUA rewards.
8. Exit: `withdraw_position(...)` with `min_amounts` slippage guards.

Continue with the [Contract interface reference](/developers/concentrated-liquidity/contract-interface-reference.md) for exact signatures, or jump straight to the [code examples](/developers/concentrated-liquidity/managing-positions-code-examples.md).
