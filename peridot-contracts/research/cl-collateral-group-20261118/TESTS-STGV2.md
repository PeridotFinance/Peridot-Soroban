# Native tests: receipt StgV2 and the group controller (21 Nov 2026)

Stream B of the CL collateral group launch. This report closes BUILD-REPORT open issue 7 ("no native
receipt test suite for StgV2"). Nothing was signed, sent, deployed or upgraded on any network.

## 1. What runs where

| Suite | Location (committed) | Runs on | Count | Result |
|---|---|---|---:|---|
| StgV2 receipt | `prototypes/cl-collateral-group/receipt-v2/tests/stgv2_test.rs` | disposable test tree, see below | 30 | pass (39 / 39 together with the controller suite, tree built from HEAD) |
| Group controller | `prototypes/cl-collateral-group/receipt-v2/tests/group_ctl_test.rs` | same tree | 9 | pass |
| Vault v2 seizure / boundary | `vault-v2/contracts/cl-share-vault/src/seize_tests.rs` (+2 tests) | vault-v2 workspace | 9 | pass |
| d3-chain unit tests (existing) | `d3-chain/src/d3_chain.rs` | d3-chain crate | 7 | pass (unchanged) |

Commands (from `peridot-contracts/`):

```bash
bash prototypes/cl-collateral-group/receipt-v2/tests/make-test-tree.sh        # SRC_REV=HEAD by default
cd target/clg/ws/stgv2-test/peridot-contracts
cargo test -p receipt-vault --lib -- stgv2_test group_ctl_test
cd prototypes/cl-collateral-group/vault-v2 && cargo test --locked -p cl-share-vault --features cl-pyusd-vault-b seize_tests
```

**The test tree.** `make-test-tree.sh` takes the overlay sources from a git revision (`git archive`,
default `HEAD`), so uncommitted work in the shared worktree never leaks in. It then does the following:

1. It runs the candidate's own `apply-stgv2.py` over the run-5 base.
2. It replaces the workspace's generic `simple-peridottroller` sources with the generated **group**
   controller sources (`apply-patches.py`) and enables `group`.
   - The controller is a **dependency** of the receipt test crate, so `cfg!(test)` is false inside it.
   - Its real 24 h admin timelock therefore applies. Inside the controller's own unit tests the
     timelock is 0, which would make timelock tests vacuous.
3. It copies the two test modules in.

**One test-tree-only source rewrite.**
- The problem: soroban-sdk 25.3.1 `testutils` cannot convert a contracttype field of type
  `Option<contracttype struct>` to `ScVal`. The xdr blanket impl needs `T: Into<ScVal>`, and
  contracttype derives only `TryFrom`.
- The consequence: `Stg2Status { nav: Option<Stg2Ck>, … }` makes the **candidate source fail to
  compile under `cargo test`**. This is also why no native suite existed.
- The rewrite: in the test tree only, `stage2_status` becomes a crate-internal accessor with an
  identical body.
- What it does not touch: no candidate WASM. The exported view is covered by the compiled-bytes rows.

**Stack under test.**
- The real StgV2 receipt (`staged_v2.rs` + `d3_chain.rs` over StgV1).
- The real group controller (G0–G11).
- A second, real `ReceiptVault` as the static-8 % USDC market.
- Test doubles for the CL vault (`MockClVault`: the StgV1 mock plus `total_supply`,
  `get_position_liquidity`, `stage_idle` and `stage_seize_reassign` with the vault's gating), for the
  Reflector oracle and for the pool `get_slot0`.
- These doubles prove receipt-side and controller-side logic. Valuation, rewards and footprints of the
  real vault stay with the compiled rows (BUILD-REPORT §2) and the vault-v2 tests.

## 2. Coverage by requirement

### Value checkpoint (L1a)
- `checkpoint_nav_is_permissionless_vault_wide_and_valid_for_exactly_60_s`:
  - no auth;
  - rate = NAV(total) × p / 1e14 × 1e6 / total, checked against an independent formula;
  - USDC-numeraire views;
  - a zero-unit owner reads 1e6;
  - valid at 60 s, invalid at 61 s.
- `checkpoint_nav_refusals_no_units_oracle_stale_and_inactive_record_nothing`:
  - #2108 when there are no units;
  - #2105 for an oracle 601 s old, timestamped in the future, or at price 0 or −1;
  - an oracle exactly 600 s old is accepted;
  - a missing price is #2105, and no D3 observation is recorded;
  - an inactive class gives #2114.
- `guards_depeg_100_bps_pool_band_50_bps_and_tick_range_edges` checks exact integer edges:
  - pool spot exactly 1.005 / 0.995 is OK, and the next representable spot is blocked;
  - tick −20 / 19 is OK, −21 / 20 is blocked;
  - p = 1.01 / 0.99 is OK, ±1 raw unit beyond is blocked;
  - an absurd sqrt price, a trapping pool, and a venue quote that reverts, is zero or is misshaped
    are all blocked (an observation, never a revert).
- `blocked_checkpoint_never_overwrites_or_refreshes_the_last_good_nav`.

### D3 heartbeat chain and the fallback (L1b), on the real clock (no `stg2-measure`)
- `d3_chain_real_clock_599_refused_600_admitted_and_gap_rules`:
  - #2111 with no chain;
  - ten 60 s observations;
  - #2112 at 599 s, admitted at 600 s;
  - admission extends the chain;
  - a 61 s lapse gives #2113;
  - a blocked observation after the gap restarts the chain.
- `d3_healthy_checkpoint_clears_and_two_pushes_ten_minutes_apart_are_refused`.
- `d3_fallback_requires_a_cause_now_and_is_refused_while_nav_is_refreshable`:
  - #2104 while the NAV can be refreshed;
  - admitted for the depeg, venue-failure and pool-unreachable causes.
- `fallback_value_is_the_lower_bound_formula_with_2_pct_haircut`. Expected values were computed
  outside Rust with exact integer floors:
  - 982,927 at p = 1 (priced at P_lo × 0.98);
  - 975,062 at p = 0.99;
  - unchanged when the pool is pushed further;
  - #2110 when the value is zero.
- `fallback_is_liquidation_only_valid_60_s_and_nav_takes_precedence`.
- `finding_f5_zero_or_misshaped_venue_answer_blocks_nav_and_fallback` (see §5).

### Borrow, exits, repay-only (D4)
- `borrow_needs_a_fresh_nav_and_respects_cf`:
  - no checkpoint means no borrow;
  - above CF is refused;
  - 60 s OK, 61 s refused;
  - a fallback alone never supports a borrow.
- `prepare_close_with_debt_needs_fresh_nav_and_remaining_cover`:
  - #2101 when stale;
  - #2106 when the close would leave a shortfall;
  - without debt, no checkpoint is needed, even with a stale oracle.
- `d4_oracle_stale_makes_the_account_repay_only`:
  - #2105 on both checkpoints;
  - borrow, close (#2101), emergency exit (#2116), `exit_market` and liquidation are all refused;
  - **repay works**;
  - once debt-free, everything that needs no price works again.

### Seizure (L3), queue, `sync_seized` (L5), fee (D5)
- `l3_seize_moves_the_mirror_queues_one_pending_and_l5_syncs_once_fee_off`:
  - seize = repay × 1.08 / rate;
  - one queue entry with the fee off;
  - units move and none are created;
  - the vault beneficiary does not move before L5;
  - neither the liquidator nor the borrower can exit seized units before L5;
  - L5 is permissionless and runs once; a replay or unknown id gives #2109.
- `l5_forwards_the_vault_boundary_flag_and_out_of_order_syncs_work`.
- `l3_fee_on_queues_a_second_entry_for_the_fee_recipient`.
- `l3_is_cache_only_and_needs_a_fresh_checkpoint_or_fallback`: an expired cached price is refused,
  works after L2, and works without cache-only as a control.
- `l3_on_the_fallback_after_a_continuous_600_s_block`.
- `seize_entrypoint_rejects_every_malformed_context_and_needs_the_controller`: nine malformed contexts
  give #2115, as do self-seizure and a call without controller auth.
- `controller_binding_is_frozen_and_activation_is_one_time` and `stage2_activate_guards`.

### Reward boundary (BUILD-REPORT issues 3/4), on the real vault v2 code
With the mock vault, the receipt side checks exact forwarding of the boundary flag, one-shot pending
entries and the fee-recipient entry. Conservation is a vault property, tested in `seize_tests.rs`. Two
holders sit outside the seizure (Bob and the liquidator); a stale L5 opens a boundary and the next
claim of 40 is quarantined.

- `route_a_per_boundary_release_can_only_credit_the_seized_from_holder`:
  - Route A (`propose_reward_attribution` / `attribute_rewards`, the route used in DESIGN §3.3 and the
    existing table test) credits **only the boundary holder** (the borrower).
  - Once it is used, the segment route is closed for that interval.
  - **Bob's and the liquidator's share of the quarantined claim is unreachable.**
  - The existing `design_table_3_3_on_real_ledger` attributes all 40 to the borrower, although Bob held
    50 % of the shares.
  - No double claim; the ledger is conserved.
- `route_b_segment_release_pays_every_holder_once_at_post_move_weights`:
  - Route B (`propose/attribute_reward_segment`, then per-holder `settle_reward_segment`) pays all 40,
    each holder once: Bob +20 (made whole), borrower +12, liquidator +8.
  - Route A is closed after settlement starts.
  - **Weights are post-move**: `seize_gap` records no `reward_history::observe` before the move, so
    the interval is one segment sealed at the claim. Accrual earned by the seized units between the
    last claim and L5 goes to the liquidator. It is bounded by the gap (≥ 61 s) × the seized share.

**Operational rule (proposed):** release seizure boundaries **only by route B**.
- Route A strands the other holders' share.
- A code fix (an `observe` before the move in `seize_gap`) would make route B exact. It costs
  footprint in L5 and is not made here.

### Emergency exit (user-required)
- **`user_required_debt_free_account_exits_market_in_one_tx_then_emergency_exits`: PASS.**
  - Setup: entered, CF 50 %, units held, no checkpoint, no cached price, PYUSD and USDC oracles stale.
  - One `exit_market` invocation under strict `mock_auths` succeeds; `env.auths()` shows exactly one
    authorization, the owner's.
  - Then `stage_emergency_exit` (all units) and a permissionless `stage_emergency_pay` pay the full
    mixture.
  - Compiled-bytes footprint of the same sequence: BUILD-REPORT §2 **row RD9** (58 + 89 entries).
- **`user_required_entered_account_is_refused_emergency_exit_with_and_without_debt`: PASS.**
  - #2116 with and without debt (rows RD8, RD5), and state is unchanged.
  - `exit_market` with debt is refused (row RD10).
- **`user_required_emergency_exit_works_while_paused_oracle_stale_pool_broken_or_cf_zero`: PASS.**
  - Conditions: the StgV1 deposit latch and every controller pause (redeem, deposit, borrow,
    liquidation, on both markets), a stale and trapping oracle, a trapping pool, CF 0.
  - Result: a not-entered holder exits and is paid.
- **`user_required_entered_debt_free_holder_exits_while_paused_oracle_broken_and_cf_zero`: PASS.**
  The same conditions for an entered debt-free holder: `exit_market`, then emergency exit, and again
  at CF 0.
- **`f1_emergency_exit_works_when_the_controller_traps`: PASS** (after fix F1). The controller is
  replaced by a contract that traps on every call. A never-entered holder and an entered borrower both
  emergency-exit.
- **`f1_paused_but_answering_controller_still_refuses_an_entered_owner`: PASS.** A controller that
  answers is never bypassed.

### Group controller rules (`group_ctl_test.rs`)
- `g6_only_the_sleeve_carries_a_cf_so_cash_collateral_never_mixes`:
  - the USDC market cannot get a CF;
  - a USDC supply gives no borrowing power;
  - liquidity equals sleeve units × rate × CF exactly;
  - with no sleeve bound, nothing can get a CF.
- `g1_sleeve_binding_is_one_time_and_needs_a_listed_market`.
- `g7_at_most_two_markets_and_g2_at_most_two_entered`.
- `g8_no_static_price_fallback_and_borrow_fails_closed_when_the_usdc_price_is_stale`.
- `g9_no_margin_liquidation_controller`.
- `g10_debt_free_account_can_always_exit_and_an_indebted_or_unreadable_one_cannot`. The exit is
  oracle-free; unreadable debt fails closed.
- `cf_changes_are_timelocked_both_ways_and_only_emergency_disable_is_immediate`:
  - loosening executes only after 24 h;
  - **tightening to a lower non-zero CF is also timelocked (24 h)**;
  - a new proposal restarts the clock;
  - the only immediate tightening is `emergency_disable_collateral` (CF 0, acknowledgement required),
    which also clears the pending change.
- `incentive_and_close_factor_are_timelocked_guardian_pauses_immediately_but_cannot_unpause`.
- `liquidation_requires_a_shortfall_respects_the_close_factor_and_the_pause`:
  - a healthy account is refused;
  - a request above 50 % is clamped to the close factor, not refused;
  - the liquidation pause is honoured.

## 3. Defects found and fixes

| ID | Severity | Defect | Fix | Test |
|---|---|---|---|---|
| F1 | High (user requirement) | `stg2_emergency_check` read the controller with a plain invoke, so a trapping or unreachable controller blocked **every** holder's emergency exit | `137de32`: `try_invoke_contract`. It fails **open** only when the call errors; an answer listing the market still refuses (#2116). **Trade-off:** an entered borrower can exit only once the controller is dead, when liquidation is impossible anyway | `f1_*` (2) |
| F2 | High (must-fix) | After `emergency_disable_collateral` (CF 0), an entered borrower **with debt** passed `exit_market`, because the check was `pbal > 0 && cf > 0` and the sleeve reports 0 debt. The borrower could then emergency-exit every unit and leave the debt unbacked. The Testnet rehearsal used exactly this switch to create a shortfall | G10 in `controller/apply-patches.py`: the sleeve is guarded whenever `group_no_debt` is false, whatever its CF. Committed inside `6e83fd7`: the upgradeability commit swept the uncommitted hunk in from the shared worktree | `f2_cf_zero_does_not_let_an_indebted_borrower_leave_the_sleeve` |
| F3 | Build | Uncommitted upgradeability WIP put `#[cfg(feature)]` on a fn inside `#[contractimpl]`, which broke compilation with the feature off | Reported; resolved by the upgradeability stream before `6e83fd7` | — |
| F4 | Medium (operations; admin-only) | Route A strands non-boundary holders' share of a quarantined claim; route B uses post-move weights. Both routes are **admin-only** (`storage::admin(e).require_auth()` in `rewards::propose_attribute` / `attribute` and in `reward_history::propose` / `attribute`), so no outsider or liquidator can trigger the stranding. Per-holder `settle_reward_segment` is permissionless but pays only the caller-named holder's own weight, once | Launch: an ops rule and a refusal in the CL ceremony tool (never route A on a boundary opened by `seizesync … opened=true`). Later, through the upgrade path: mark seizure boundaries and refuse route A (+1 write / +1 entry in a boundary-opening L5, about +100–200 B), or add `reward_history::observe` before the move in `seize_gap` (about +2–3 entries, about +100 B; makes route B exact). Both are analysis estimates, not built | `route_a_*`, `route_b_*` |
| F5 | Low (liveness, fails closed, broken venue only) | `checkpoint_nav` treats a venue answer of 0 or of the wrong shape as blocked; `checkpoint_fallback` treats any successful answer as refreshable (#2104). Neither checkpoint can then be written, so the account cannot be liquidated. **Not reachable by an outsider on the real vault:** the bound vault's `stage_share_value` always returns a one-element vector, and its value (booked idle plus the pool's `estimate_withdraw_position` of the whole position) cannot be driven to 0 by moving the pool or by donations. Only an empty vault or a pool returning zeros can produce it. A reverting quote is consistent: both checkpoints see an error | Documented as fail-closed; not fixed. Possible later fix through the 24 h upgrade path: reuse the NAV cause test in the fallback | `finding_f5_zero_or_misshaped_venue_answer_blocks_nav_and_fallback` |
| F6 | Info | The candidate source does not compile under `cargo test` (`Option<contracttype>` in `Stg2Status`) | Test-tree accessor rewrite (§1) | — |

Both fixes are in the release freeze `96e7b3c` (`MAINNET-SIMS.md` FINAL), which contains `137de32` and
`6e83fd7`.

## 4. Mutation testing

Tool: `cargo-mutants` 27.1.0, run against the disposable test tree with test filter
`stgv2_test group_ctl_test`, `-j 3`, and one scratch `TMPDIR` that was deleted afterwards.

**Scope.**
- Receipt: every mutant in `staged_v2.rs` and `d3_chain.rs`, except the measurement-only
  `stage2_measure_set_chain`, which is not compiled.
- Controller: every mutant in the new or patched functions: `group_*`, `try_cached_price`,
  `exit_market` (G10), `set_market_cf` (G6), `add_market` (G7), `set_price_fallback` (G8),
  `set_margin_liquidation_ctrl` (G9) and `get_collateral_excl_usd` (G5).
- Not mutated: `sum_positions_usd_for_markets_ctx` and `liquidate_internal`. They hold 182 mutants of
  mostly generic base code. The liquidation tests exercise them, but no mutation score is claimed.

| Run | Mutants | Caught | Missed | Unviable |
|---|---:|---:|---:|---:|
| Receipt, first run | 218 | 177 | 7 | 34 |
| Controller, first run | 57 | 35 | 22 | 0 |
| Re-run of the receipt functions with survivors, after the new tests | 31 | 27 | 4 | 0 |
| Re-run of controller `set_market_cf` and `exit_market` | 35 | 24 (+1 lost) | 6 | 4 |

The controller re-run lost one mutant to a full disk (another process filled the volume; the scratch
space was then freed). Two receipt survivors of the re-run, the `==` at 154 and the `*` → `+` at 194,
were killed by test fixes made after the re-run started. Each was confirmed by applying the mutant by
hand: the test fails on the mutant and passes on the original.

**Tests added to kill mutants:**
- `mutation_blocked_event_names_the_cause_and_huge_pool_price_never_traps`:
  - the `stg2_blocked` event carries cause 1, 2 or 3;
  - a sqrt price of 2^40 (spot above u128) is blocked, not a trap.
- `mutation_exit_cover_scales_with_the_usdc_price`: USDC at 2.0, with the cache refreshed.
- The CF range is accepted at both inclusive ends (1 % and 100 %).
- `exit_market` from the USDC market is refused while debt is outstanding.

**Remaining survivors (none meaningful):**

| Location | Mutant | Why it survives |
|---|---|---|
| receipt `band_ok` `s > 2Q` | `>` → `>=` | Equivalent: at s = 2 the spot is 4.0, which is out of band anyway |
| receipt `stg2_exit_check` `other + usd < borrows` | `<` → `<=` | Exact-equality boundary in USD; not constructible without a bespoke price/rate pair. The 1-unit sides are covered by the cover test |
| receipt `checkpoint_nav` `total > i128::MAX` | `>` → `==` / `>=` | Unreachable: units are bounded far below i128::MAX by the vault supply |
| controller `set_price_fallback` validation (11) | operator swaps | Unreachable in group mode: G8 refuses every `Some` before validation. This shows G8 dominates |
| controller `try_cached_price` `price > 0`, `scale > 0` | `>` → `>=` | Equivalent: `cache_price` never stores 0 |
| controller `exit_market` `market_cf > 0` (3) | swaps | Equivalent under G6/G10: the sleeve branch (`\|\| sleeve`) ignores CF, and for the USDC market (CF 0) the debt check that follows refuses anyway |
| controller `exit_market` `pbal > 0` | `>` → `>=` | Changes only a zero-unit indebted account exiting the sleeve (allowed versus refused). No safety effect |
| controller `exit_market` `current - 1` in `MarketUserCounts` (2) | `-` → `+` / `/` | Generic bookkeeping read only by delisting; not group code |


## 5. WASM builds (task 5)

Built with `stellar contract build --locked`, the public init-admin constant, and my own target dir.
Sources come from `git archive` (receipt and controller at `137de32`, unchanged through `0eefd81`;
vault at `15d9833`).

| Contract | Features | Bytes (< 131,072) | sha256 | Note |
|---|---|---:|---|---|
| Vault **base** (unmodified frozen Vault-B) | `cl-pyusd-vault-b` | 123,430 | `e66380303e50c617…` | **byte-identical to the deployed vault** e6638030… |
| Vault v2 | `cl-pyusd-vault-b` | 125,559 | `6fd8599a9b369ab8…` | = MAINNET-SIMS FINAL |
| Group controller | `group` (G0–G11) | 64,957 | `642421f151273a5a…` | = MAINNET-SIMS FINAL |
| Receipt StgV2 | `stg-mainnet-accepted,stg2-mainnet-accepted` | **125,585** | **`424454b94e455c92921d7f548be92ffbe0bc2bcf67dcb05eed1696dacad1d379`** | Canonical build (`build/canonical-build.sh 0c772d5`, canonical OZ path, OZ 3f81125b, receipt base digest bf6dad44…) **= MAINNET-SIMS FINAL**. My own non-canonical path gives 125,597 / d86acd2d…, which shows the known path dependency (BUILD-REPORT issue 8) |

The canonical build at `0c772d5` also reproduces vault v2 `6fd8599a…` and the controller `642421f1…`.
All three are under 131,072 B; the smallest headroom is the receipt's, 5,487 B.
