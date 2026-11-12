> For the complete documentation index, see [llms.txt](https://docs.aqua.network/llms.txt). Markdown versions of documentation pages are available by appending `.md` to page URLs; this page is available as [Markdown](https://docs.aqua.network/developers/concentrated-liquidity/contract-interface-reference.md).

# Contract interface reference

Full reference of the concentrated liquidity pool smart contract interface: position management, state getters, fees, swaps and rewards.

This page is the canonical public reference for the concentrated liquidity pool contract interface. Functions are grouped by where they live: the **router** (shared Aquarius entry point) and the **pool contract** (one per concentrated pool). Signatures are given in Rust/Soroban notation; the `e: Env` argument is implicit in every Soroban contract call and is not passed by the client.

Read-only getters and `estimate_*` functions don't change state — call them through transaction **simulation**, no signature or fees required. Estimates enforce the same validation as the real calls: on invalid input the simulation **fails with the corresponding error code** (for example, `PositionNotFound`, `AllCoinsRequired`, `OutMinNotSatisfied` when amounts round to zero liquidity) instead of returning a value — handle simulation errors accordingly.

## Router functions

The router address is listed in [Addresses & networks](/developers/reference/addresses-and-networks.md). Token vectors must always be [sorted by contract ID](/developers/reference/router-and-pool-contracts.md#token-ordering).

### init\_concentrated\_pool

Creates a concentrated pool for a token pair, or returns the existing one for that fee tier.

```rust
fn init_concentrated_pool(
    e: Env,
    user: Address,
    tokens: Vec<Address>,
    fee: u32,
) -> (BytesN<32>, Address);
```

* `user` — the account creating the pool (pays the pool creation fee in AQUA if one is configured).
* `tokens` — the two token addresses, sorted.
* `fee` — one of `10` (0.1%), `30` (0.3%), `100` (1.0%). Tick spacing is derived automatically: `20`, `60` or `200` respectively.

Returns the pool index hash and the pool contract address.

### Pool discovery

```rust
fn get_pools(e: Env, tokens: Vec<Address>) -> Map<BytesN<32>, Address>;
fn get_info(e: Env, tokens: Vec<Address>, pool_index: BytesN<32>) -> Map<Symbol, Val>;
fn get_pool(e: Env, tokens: Vec<Address>, pool_index: BytesN<32>) -> Address;
```

`get_pools` returns all pools for a pair (all types). Concentrated pools report `pool_type: "concentrated"` in `get_info`, along with `fee` and `tick_spacing`. See the [Get pools info example](/developers/code-examples/get-pools-info.md).

### Router-level deposit, withdraw, swaps, rewards

The generic router functions documented in [Router & pool contracts](/developers/reference/router-and-pool-contracts.md) — `deposit`, `withdraw`, `swap_chained`, `claim` — all work with concentrated pools. A router `deposit` into a concentrated pool opens a **full-range position** (see [position model](/developers/concentrated-liquidity.md#positions)). For ranged positions, call the pool contract directly.

## Pool contract: position management

### deposit\_position

Opens a new position or adds liquidity to an existing one with the same range.

```rust
fn deposit_position(
    e: Env,
    sender: Address,
    tick_lower: i32,
    tick_upper: i32,
    desired_amounts: Vec<u128>,
    min_liquidity: u128,
) -> (Vec<u128>, u128);
```

* `sender` — position owner; authorizes the token transfers.
* `tick_lower`, `tick_upper` — range boundaries; must be multiples of the pool's tick spacing, `tick_lower < tick_upper`, within `±887272`.
* `desired_amounts` — `[amount0, amount1]` in raw units (token order = sorted order). These are **maximums you authorize**, not exact spends: the contract computes the maximum liquidity purchasable at the current price, pulls what it needs and refunds the excess within the same transaction.
* `min_liquidity` — slippage guard on **liquidity units** (not token amounts): the transaction fails with `OutMinNotSatisfied` if the minted liquidity is below this value.

Returns `(actual_amounts, minted_liquidity)` — the amounts actually spent and the liquidity minted.

Special cases:

* If the current price is **outside the range**, only one token is required — pass `0` for the other. Which one depends on the side: a range entirely **above** the current price takes only `token0`, entirely **below** — only `token1`. Passing only the wrong-side token yields zero liquidity and fails with `OutMinNotSatisfied`.
* The **first deposit into an empty pool initializes the pool price** from `amount1 / amount0`. It must include both tokens (`AllCoinsRequired` otherwise), and the tick derived from that ratio must fall inside the deposit range (`TickOutOfBounds` otherwise).

### estimate\_deposit\_position

Simulation counterpart of `deposit_position` — same math, no transfers.

```rust
fn estimate_deposit_position(
    e: Env,
    tick_lower: i32,
    tick_upper: i32,
    desired_amounts: Vec<u128>,
) -> (Vec<u128>, u128);
```

### withdraw\_position

Burns liquidity from a position and transfers principal **plus all accrued swap fees** to the owner. This applies to **partial withdrawals too**: any `withdraw_position` call, regardless of `amount`, always sweeps the position's entire accrued fee balance along with the withdrawn principal — there is no way to withdraw principal while leaving fees unclaimed. Withdrawing the full liquidity closes the position.

```rust
fn withdraw_position(
    e: Env,
    owner: Address,
    tick_lower: i32,
    tick_upper: i32,
    amount: u128,
    min_amounts: Vec<u128>,
) -> Vec<u128>;
```

* `amount` — liquidity to burn (up to the position's total; see `get_position`).
* `min_amounts` — `[min0, min1]` slippage guards checked against the total payout (principal + fees).

Returns the paid-out token amounts. Withdrawals are always available — this function has no kill-switch.

### estimate\_withdraw\_position

```rust
fn estimate_withdraw_position(
    e: Env,
    owner: Address,
    tick_lower: i32,
    tick_upper: i32,
    amount: u128,
) -> Vec<u128>;
```

Like `withdraw_position`, the returned amounts **include the position's accrued fees** on top of the principal.

## Pool contract: swap fees

```rust
// Pending fees of one position (simulate, read-only)
fn get_position_fees(e: Env, owner: Address, tick_lower: i32, tick_upper: i32) -> Vec<u128>;
// Pending fees across all owner's positions
fn get_all_position_fees(e: Env, owner: Address) -> Vec<u128>;

// Collect fees of one position
fn claim_position_fees(e: Env, owner: Address, tick_lower: i32, tick_upper: i32) -> Vec<u128>;
// Collect fees of all owner's positions
fn claim_all_position_fees(e: Env, owner: Address) -> Vec<u128>;
```

All return `[amount0, amount1]`. Claiming emits a `claim_fees` event with the owner and token addresses as topics.

## Pool contract: state getters

```rust
// Current price state: sqrt_price_x96 (Q64.96) and current tick
fn get_slot0(e: Env) -> Slot0;
// Tick spacing of this pool (20 / 60 / 200)
fn get_tick_spacing(e: Env) -> i32;
// Liquidity currently active at the current tick
fn get_active_liquidity(e: Env) -> u128;

// Single position details
fn get_position(e: Env, recipient: Address, tick_lower: i32, tick_upper: i32) -> PositionData;
// All positions of a user: tick ranges + total raw liquidity
fn get_user_position_snapshot(e: Env, user: Address) -> UserPositionSnapshot;

// Tick-level data (advanced: building liquidity distribution charts)
fn get_tick(e: Env, tick: i32) -> TickInfo;
fn get_ticks_batch(e: Env, ticks: Vec<i32>) -> Vec<TickInfo>;
// Bounds of initialized ticks; min > max means no initialized ticks
fn get_tick_bounds(e: Env) -> (i32, i32);
// Tick corresponding to the price ratio amount1/amount0
fn tick_from_amounts(e: Env, amount0: u128, amount1: u128) -> i32;

// Cumulative swap fees per unit of liquidity since pool creation, Q128 fixed-point
fn get_fee_growth_global_0_x128(e: Env) -> U256;
fn get_fee_growth_global_1_x128(e: Env) -> U256;

// Bitmap index of initialized ticks (see below)
fn get_chunk_bitmap(e: Env, word_pos: i32) -> U256;
fn get_chunk_bitmap_batch(e: Env, start_word: i32, count: u32) -> Vec<U256>;

// Generic pool info (also available via router)
fn get_info(e: Env) -> Map<Symbol, Val>;   // pool_type, fee, tick_spacing
fn get_tokens(e: Env) -> Vec<Address>;     // [token0, token1] sorted
fn get_reserves(e: Env) -> Vec<u128>;      // LP reserves, excludes protocol fees
fn get_fee_fraction(e: Env) -> u32;
fn get_protocol_fee_fraction(e: Env) -> u32;
// Total liquidity across all positions; user's total across their positions
fn get_total_shares(e: Env) -> u128;
fn get_user_shares(e: Env, user: Address) -> u128;
```

Relevant types:

```rust
struct Slot0 {
    sqrt_price_x96: U256, // sqrt(token1/token0 price) * 2^96
    tick: i32,            // floor(log_1.0001(price))
}

struct PositionData {
    fee_growth_inside_0_last_x128: U256, // internal fee accounting
    fee_growth_inside_1_last_x128: U256,
    liquidity: u128,                     // position's liquidity
    tokens_owed_0: u128,                 // fees settled but not yet collected
    tokens_owed_1: u128,
}

struct UserPositionSnapshot {
    ranges: Vec<PositionRange>, // { tick_lower: i32, tick_upper: i32 }
    raw_liquidity: u128,        // sum of all position liquidity
    weighted_liquidity: u128,   // boosted working liquidity — the user's reward
                                // weight as of their last checkpoint (see rewards)
}
```

{% hint style="info" %}
`tokens_owed_*` in `get_position` only reflects fees settled during past interactions with the position. For the live pending total, use `get_position_fees`.
{% endhint %}

{% hint style="warning" %}
**Missing-position behavior differs between getters.** `get_position`, `get_position_fees` and `claim_position_fees` **fail with `PositionNotFound`** when called for a range the owner has no position in (including one you have already fully withdrawn). The aggregate views — `get_user_position_snapshot` and `get_all_position_fees` — safely return empty/zero results for a user with no positions.
{% endhint %}

#### Enumerating initialized ticks

To reconstruct the pool's liquidity distribution (for example, for a chart), you need every **initialized tick** — a tick referenced by at least one position boundary (`liquidity_gross > 0` in `TickInfo`). Two strategies:

* **Simple:** call `get_tick_bounds` and scan `get_ticks_batch` over multiples of the tick spacing between the bounds. Fine for pools with a modest active range.
* **Sparse (bitmap):** ticks are indexed in a two-level bitmap. Ticks are grouped into **chunks of 16**: `chunk_pos = floor((tick / tick_spacing) / 16)`. Each bit of a 256-bit **word** flags a chunk containing at least one initialized tick: `word_pos = chunk_pos >> 8`, bit index `chunk_pos & 255` (bit 0 = least significant). Scan words with `get_chunk_bitmap_batch`, then read only the flagged chunks' ticks via `get_ticks_batch`.

Given active liquidity at the current tick (`get_active_liquidity`) and the `liquidity_net` deltas of initialized ticks, liquidity at any tick is reconstructed by walking outward from the current tick and applying the deltas: add `liquidity_net` when crossing a tick left-to-right, subtract when right-to-left.

## Pool contract: swaps

Concentrated pools implement the standard Aquarius pool swap interface, so direct-pool swaps work exactly like the [Executing swaps through specific pool](/developers/code-examples/executing-swaps-through-specific-pool.md) example:

```rust
fn swap(e: Env, user: Address, in_idx: u32, out_idx: u32, in_amount: u128, out_min: u128) -> u128;
fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, in_amount: u128) -> u128;
fn swap_strict_receive(e: Env, user: Address, in_idx: u32, out_idx: u32, out_amount: u128, in_max: u128) -> u128;
fn estimate_swap_strict_receive(e: Env, in_idx: u32, out_idx: u32, out_amount: u128) -> u128;
```

`in_idx`/`out_idx` are the token indices in the sorted pair (0 or 1).

{% hint style="warning" %}
**Partial-fill behavior differs between the two swap modes.** An exact-input `swap` that runs out of initialized liquidity before consuming the whole input does **not refund the unswapped remainder**: the remainder is treated like a swap fee — the protocol share is diverted to the treasury and the rest is distributed to the liquidity positions that covered the last active tick of the swap path. This is intentional: a refund leg could itself fail mid-transaction (for example, a missing trustline on the refund path of a chained swap), so the contract guarantees a simple invariant instead — **exactly `in_amount` is consumed, and at least `out_min` is received**, or the transaction fails. Always set a meaningful `out_min` and check `estimate_swap` first. Edge cases: a swap that cannot move any liquidity at all reverts with `InsufficientLiquidity` rather than consuming the input, and an exact-output `swap_strict_receive` that cannot produce `out_amount` fails with `InsufficientLiquidity` as well.

Multi-hop routes (`swap_chained`) through concentrated pools are also heavier computationally than through other pool types and can hit Soroban per-transaction limits sooner.
{% endhint %}

## Pool contract: AQUA rewards

```rust
// Pending reward estimate
fn get_user_reward(e: Env, user: Address) -> u128;
// Detailed rewards state (keys include to_claim, tps, exp_at, boost, working_balance)
fn get_rewards_info(e: Env, user: Address) -> Map<Symbol, i128>;
// Claim accrued rewards
fn claim(e: Env, user: Address) -> u128;

// Preview the (unboosted, boosted) working balance a deposit would produce —
// useful for estimating the ICE boost effect before depositing
fn estimate_working_balance(
    e: Env, user: Address, tick_lower: i32, tick_upper: i32, new_liquidity: u128,
) -> (u128, u128);

// Rewards participation flag (opted in by default)
fn get_rewards_state(e: Env, user: Address) -> bool;
fn set_rewards_state(e: Env, user: Address, state: bool);
```

Remember: positions accrue rewards **only while in range** — `tick_lower <= current_tick < tick_upper` — and the in-range check is **checkpoint-based**: the reward weight is snapshotted when the owner interacts with the pool (deposit, withdraw, claim) and is not re-evaluated between interactions as the price moves. A position that left its range keeps accruing at the stale weight until the owner's next action, and one that re-entered its range earns nothing until then — perform an action (for example, `claim`) after the price crosses a range boundary to re-sync. Note: this checkpoint behavior is specific to the current contract version and may change in future versions.

The same operations are available through the router with `(tokens, pool_index)` arguments. Calling `claim` with nothing accrued succeeds and pays zero — it is a no-op, not an error. Reward claiming can be temporarily paused by protocol admins in emergencies; principal withdrawal can not.

## Events

Events emitted by the pool contract, for indexers and monitoring (topics listed first, then the data payload):

| Event                | Topics (after the event name)     | Data                                          |
| -------------------- | --------------------------------- | --------------------------------------------- |
| `deposit_liquidity`  | `token0`, `token1`                | `liquidity`, `amount0`, `amount1`             |
| `withdraw_liquidity` | `token0`, `token1`                | `liquidity`, `amount0`, `amount1`             |
| `position_update`    | `user`                            | `tick_lower`, `tick_upper`, `liquidity_delta` |
| `pool_state`         | —                                 | `sqrt_price_x96`, `tick`, `active_liquidity`  |
| `claim_fees`         | `owner`, `token0`, `token1`       | `amount0`, `amount1`                          |
| `claim_reward`       | `reward_token`, `user`            | `amount`                                      |
| `trade`              | `token_in`, `token_out`, `trader` | `sold_amount`, `bought_amount`, `fee`         |
| `update_reserves`    | —                                 | `reserve0`, `reserve1`                        |

Correlation notes for indexers: a swap emits `trade`, `update_reserves` and `pool_state` together; `withdraw_position` additionally emits `claim_fees` when it auto-sweeps the position's accrued fees.

## Common errors

| Code   | Error                     | Meaning                                                                                                                                               |
| ------ | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `201`  | PoolAlreadyInitialized    | Pool init attempted twice                                                                                                                             |
| `205`  | DepositKilled             | Deposits are paused by the protocol                                                                                                                   |
| `2002` | TokensNotSorted           | Token vector not sorted by contract ID                                                                                                                |
| `2101` | InvalidTickRange          | Malformed tick range                                                                                                                                  |
| `2107` | TickOutOfBounds           | Tick outside allowed bounds (for example, first-deposit price outside the deposit range)                                                              |
| `2109` | TickNotSpacedCorrectly    | Boundary is not a multiple of the tick spacing                                                                                                        |
| `2110` | TickLowerNotLessThanUpper | `tick_lower >= tick_upper`                                                                                                                            |
| `2111` | TickLowerTooLow           | Below `MIN_TICK`                                                                                                                                      |
| `2112` | TickUpperTooHigh          | Above `MAX_TICK`                                                                                                                                      |
| `2118` | PositionNotFound          | No position for (owner, range)                                                                                                                        |
| `2119` | TooManyPositions          | More than 20 positions per user per pool                                                                                                              |
| `2121` | InsufficientLiquidity     | Withdraw amount exceeds position liquidity; or the pool cannot fill a swap (exact-output cannot produce `out_amount`, exact-input cannot move at all) |

Generic validation errors (zero amounts, slippage guards like `OutMinNotSatisfied`) are shared with other Aquarius pool types.
