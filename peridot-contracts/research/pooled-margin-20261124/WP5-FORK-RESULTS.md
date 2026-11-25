# Margin v2 — WP5 real-pool Mainnet-fork results (25 Nov 2026), updated for WP6

**Current build: WP6, sha256 `b1cb1cb21f02de1b1a9047d31fa60ffc066c2ddedc94c4dff2fcbdb0a7b1766f` (67,488 B).** The current
numbers are in *WP6 changes* and the *Resource table (WP6 build)*. The WP5 sections further down are kept for history.
They describe build `957ad3ca…5846`, except where a section is marked WP6.

Harness: `harness/margin-fork/tests/margin.rs` (isolated crate, soroban-sdk 30.0.0 / soroban-env-host 30.0.1, protocol 30).
Fork: Mainnet ledger **65662216** (fetched 2026-11-25 08:59 UTC with `tools/fetch-fork.cjs`, read-only public RPC, ~620 KB,
git-ignored under `harness/margin-fork/.scratch/`). It contains the live controller f41e5260, the live receipts 5630a701, the
SACs, Reflector and the real Aquarius XLM/USDC concentrated pool CBBMQ…BUCV.
Margin WASM: `stellar contract build --package margin-controller` → WP5 sha256 **957ad3ca…5846** (62,533 B); WP6
sha256 **b1cb1cb2…766f** (67,488 B). The test imports it
with `contractimport!`, asserts the hash and registers it fresh on the fork with the 5-argument v2 constructor.
Run: `cd harness/margin-fork && cargo test -- --test-threads=1 --nocapture`. Per-test JSON goes to `.scratch/wp5-*.json`.
**5/5 tests pass** on both builds. WP5 had one measured state over a limit (see *Over the limit*). On the WP6 build
every row is within its limit.

Pilot parameters, all set explicitly through the v2 setters: `set_risk` 5× on both sides; `set_caps` $25 per position,
$100 side equity, $500 side OI, $1 minimum; `set_fees` 0.10 % / 0.10 %, funding 2 % APR, bonus 3 % + 1 %;
`set_max_slippage` 1.5 %; `set_headroom` 1.25 / 1.10 / ±20 %, derived close reserve, coefficients 3.0 / 2.1 / 4.6.
The admin seeds the backstop with **$750 USDC pTokens** via `backstop_deposit`; the fork mints the admin's USDC in a test.
Positions use $24.90 equity, just under the $25 cap.
P0 is Reflector at the fork ledger: XLM $0.12425, USDC $1.0005.

## Method

- **Auth.** Every margin entrypoint called by a trader, backer, liquidator or keeper runs with an exact `mock_auths` tree,
  never mock-all. The tree holds the root call plus the caller's own token or pToken `transfer` sub-invocation, with the
  exact amount: the debt plus fee plus funding for `close_with_repay`, and the payment replicated from the contract math
  for `liquidate`. The pool swap auth is the contract's own `authorize_as_current_contract`.
  Only these use mock-all: setup/admin setters, Reflector `set_price` (the oracle admin) and the external cash liquidator.
- **Pool auth fidelity.** Under enforcing auth, a contract caller swapping on the real pool needs the ROOT `transfer`
  entry. Root only succeeds, as does both (v2 `swap()` sends both). Nested `swap→transfer` alone fails, and so does none.
  The fork's `estimate_swap` equals the 4 recorded Mainnet quotes, and the probe swap equals the Mainnet simulated output.
- **Gaps** are oracle-only. A Reflector round is posted with XLM × k/100 (the other assets unchanged), then the controller
  runs `cache_price` for XLM and USDC. The real pool stays at the P0 price. No swap is made against an oracle that
  disagrees with the pool in the failing direction: liquidations do not swap, and the XLM settle buys XLM, which the
  pool sells cheaper than the oracle at +40 %.
- **72 h warp** moves the timestamp only (sequence +120), so snapshot entries keep their liveUntil. A fresh P0 round is then
  posted.
- **Own accrual.** Views at the same timestamp (`get_position_debt` and the like) already commit `update_interest` in the
  test host. Every open, swap-close, backstop_withdraw and the heavy-state settles are therefore measured after a +5 s /
  +1 ledger tick, so the call pays its own receipt accrual. The tick moved the numbers by under 1 M instructions.
  `liquidate`, `close_with_repay` and the other settles need exact amounts from views at the same timestamp; expect them
  to be under 1 M instructions per receipt higher on Mainnet.
- **Footprint keys** = disk-read + in-memory read entries (each read-write key counted once).
  Trader, backer and keeper addresses are test contract addresses. On Mainnet a G-account signer adds its account entry
  (a disk read) plus a nonce entry (read-write), so **add about 2 keys per signer**. An XLM/USDC transfer from a G-account
  touches its account or trustline instead of a contract balance entry, which does not change the count.

## WP6 changes (25 Nov)

Contract changes (details in INTERFACE.md "WP6 changes"):

1. **`open` is refused while any backstop bad debt is unsettled** (`BackstopDebtOutstanding` #25).
2. **Every duplicate receipt and oracle read is removed from `open` → `pool_check`.** One price read per asset and one
   USDC rate before the trade. One held rate after it. The position debt comes from the borrow's own balance. One held
   pToken balance read before and one after the deposits. No `update_interest` in the Open pool check. The NAV is read
   once per pool check.
3. **`settle_backstop_debt` sizes itself** to the pooled free liquidity. It fails with the new `PooledShortfall` (#35)
   when there is none. New keeper view: `settle_max_ptokens`.
4. **The XLM settle swap input comes from the pool's `estimate_swap_strict_receive`**, bounded by the oracle.
   - The leftover XLM is kept as tracked **backstop XLM pTokens** (`BsXlmPt`, in `Ledger(XLM)`).
   - The next XLM settle repays from them first.
5. **Close reserve multiplier 1.01 → 1.015.**

Harness changes:

- **`heaviest_open_…`**: opens now fail #25 while USDC and XLM bad debt are outstanding (asserted). Opens are measured
  in seven states, all with a backstop exit pending:
  - first long;
  - first short, after a long;
  - second long and second short;
  - long and short after the settles, with backstop XLM pTokens held;
  - long and short after a 72 h warp, with 3 longs and 3 shorts open.

  It then liquidates a fresh 5× short at +40 % and settles that XLM bad debt while backstop XLM pTokens are held, so
  the settle runs its XLM-pT-first path on the live receipt.
- **`pooled_account_seizure_…`**:
  - An L2 solvent-liquidation claim is kept unclaimed through the ×7 spike.
  - `claim`, swap-`close` and `close_with_repay` are tried with pooled HF < 1.
  - Then a liquidation, the self-sized settles and the blocked exits.
- **Fidelity**: strict-receive round trip at 10 / 100 / 1,000 / 5,000 XLM. `estimate_swap` of the quoted input
  returns the asked output +0 to +5 stroops. A real swap of the quote for 100 XLM delivered exactly 100.0000000 XLM.

### 1. `open` back under 170 M

Worst open measured: **153.3 M** (long 3× after the 72 h warp, 3 + 3 positions open, exit pending). That is 23 % below
the 200 M gate and under the 170 M target. Before WP6 the worst was 225.1 M.

| Open state (all with a backstop exit pending unless noted) | WP5 | WP6 |
|---|---|---|
| long 5×, first long (pilot / heavy) | 170.9 M | 141.7 / 141.9 M |
| short 5×, after a long | 163.2 M | 133.2 / 133.4 M |
| long 3×, second long, shorts open (pilot / heavy) | 182.1 M | 152.5 / 152.6 M |
| short 3×, second short | 164.7 M | 134.9 / 134.9 M |
| long 5× / short 5× with USDC + XLM bad debt unsettled | 225.1 / 207.1 M | **refused #25** |
| long 5× / short 5× after the settles (backstop XLM pT held) | – | 152.8 / 135.1 M |
| long 3× / short 3× after 72 h (3 longs + 3 shorts open) | – | **153.3** / 134.8 M |

- **The 11 M gap between longs is isolated.** It is not leverage. A one-off probe opened three longs in a row with no
  short open (3×, 5×, 3×): **141.3 / 142.3 / 142.4 M**.
- A long costs about +11 M whenever the pooled account already owes XLM, i.e. a short is open. That cost falls inside
  the live receipt/controller calls: the borrow health check and the accrual for an account with debt in the other
  market. This attribution is inferred, not profiled.
- Shorts cost about the same in every order.
- The saving per open is about 29–30 M. Of that, about 9 M is the two skipped `update_interest` calls in the pool check
  and about 8.6 M the second `update_interest` + borrow-balance pair. The rest comes from fewer pToken balance reads,
  rate reads and price reads.

### 2. Pooled controller HF < 1: what traps and why (traced)

State: the seizure env. The cash controller seizes 280.84 USDC pT at XLM ×7, then `reconcile` sets Impaired; pooled HF
is 0.98. Each failing call was traced with the host diagnostic log. Every one ends in the live receipt's controller
liquidity check: `get_collateral_excl_usd` and `get_borrows_excl`, then the XLM price and the CF, then
`UnreachableCodeReached` in the receipt.

| Call (pooled HF < 1) | Result | Exact failing call |
|---|---|---|
| `close_with_repay` long (L3) | traps | Runs in this order:<br>1. USDC `transfer` in from the owner<br>2. `repay` 49.54 USDC succeeds (USDC debt → 0)<br>3. the fee `deposit` succeeds<br>4. **XLM receipt `transfer(margin → owner, 598.70 XLM pT)`** traps |
| `claim` XLM (L2 remainder, 57.13 XLM pT) | traps | **XLM receipt `transfer(margin → owner)`** |
| swap-`close` long (L3) | traps | **XLM receipt `withdraw(margin, 57.94 XLM pT)`**: the first step, before any swap or repay |
| `close_with_repay` short (S3) | ok | pays in first, then sends USDC pT |
| `liquidate` short underwater (S5) | ok | pays in first |
| `open` | #24 Impaired | – |

**Diagnosis: not an ordering bug.** L3's `close_with_repay` already repays before it moves any pToken, like
`liquidate`. The trap is genuine. Once the backstop's XLM bad debt has eaten the pooled free liquidity, L3's own XLM
collateral is backing that debt: about $286 CF-weighted at ×7, against $49.5 of L3 debt. Swap-close cannot repay before
it withdraws, because it needs the proceeds. So the contract is left as it is. Bypassing the live controller check would
remove the lender protection.

**Fix shipped: settles size themselves.** Recovery on the fork:

1. Liquidate S5. It pays in first.
2. Run `settle_max_ptokens`. It shows **15.32 pT**, capped by liquidity.
3. Settle. It repays **123.40 XLM** and **does not trap**. In WP5 the full settle trapped at this point.
4. Close S3 with `close_with_repay`.
5. Run the keeper loop. One more self-sized settle of 41.85 pT repays the remaining 336.86 XLM, and the shares reach 0.
6. L3 `close_with_repay` and the XLM `claim` then **succeed**.

Under a real controller shortfall the settle fails with `PooledShortfall` (#35) and the view returns 0. The native test
`settle_self_sizes_to_pooled_liquidity_and_converges` covers both:

- An external seizure leaves the account short, and the settle fails #35 with the view at 0.
- After a 25 pT `admin_recapitalize`, settles capped by the health-checking mock receipt converge to zero. The view
  always equals what the settle withdrew.

Open owner decision, not implemented: when an exit's transfer would fail, park its collateral as a `Claim`.

### 3. XLM settle: no idle XLM

At the +40 % oracle gap (gaps test) the XLM settle repays 107.35 XLM.

| | WP5 | WP6 |
|---|---|---|
| Idle XLM left in the contract | 45.2 XLM, untracked | **0** |
| Leftover kept | – | **0.0537 XLM pT** as `BsXlmPt` (≈ the 0.05 % buffer), in `Ledger(XLM)`; Ledger == balance |
| Backstop end NAV | 724.2 pT | **729.85 pT** |

The NAV gain comes from the input size. The input now follows the pool's price. Before, it followed the oracle + 1.5 %,
and the oracle-only +40 % gap made that much larger.

Native tests cover four cases:

- quote-sized settle;
- `BsXlmPt` used first by the next settle;
- a 3-stroop dust tail settled to exactly zero shares (0.1 XLM minimum purchase);
- the no-quote fallback to the oracle bound.

## Resource table (WP6 build b1cb1cb2, real pool, protocol-30 host)

Same method and limits as the WP5 table below. Every WP5 row was re-measured on the WP6 build. This table keeps the
worst state per entrypoint; three non-worst liquidate rows are left out (liquidate long 3× underwater −40 % 95.5 M,
short 5× solvent +20 % 87.9 M, short 3× solvent +40 % 67.6 M), and all rows are in `.scratch/wp5-*.json`.

| Entrypoint / scenario | Instr | Mem MB | Keys | Disk rd | Writes | Write B | Events B | Within limit |
|---|---|---|---|---|---|---|---|---|
| backstop_deposit (admin seed $750 / second backer) | 21.9 M | 5.00 | 52 | 0 | 12 | 3,996 | 552 | yes |
| **open long 3× after 72 h (3 + 3 open, exit pending): worst open** | **153.3 M** | 15.27 | 112 | 0 | 35 | 13,140 | 4,868 | yes |
| open long 5× (first long; most keys) | 141.9 M | 14.38 | **113** | 1 | 35 | 12,688 | 4,648 | yes |
| open short 5× after settles (worst short) | 135.1 M | 13.82 | 109 | 0 | 32 | 12,404 | 4,172 | yes |
| open with backstop bad debt unsettled | refused #25 | | | | | | | – |
| backstop_request_withdraw | 1.4 M | 1.48 | 6 | 0 | 4 | 3,756 | 168 | yes |
| backstop_withdraw (72 h, book open, pool check) | 86.0 M | 9.84 | 98 | 0 | 16 | 4,896 | 1,212 | yes |
| close long 3× (swap XLM→USDC, 72 h funding) | 162.3 M | 16.39 | 107 | 0 | 37 | 13,188 | 4,728 | yes |
| close short 3× (swap USDC→XLM) | 137.1 M | 15.03 | 107 | 0 | 34 | 12,396 | 3,864 | yes |
| close_with_repay long 5× | 85.7 M | 9.83 | 98 | 0 | 27 | 7,704 | 2,564 | yes |
| close_with_repay short 5× | 60.7 M | 8.53 | 93 | 0 | 24 | 6,912 | 1,644 | yes |
| liquidate long 2× solvent −40 % (worst liquidate) | 108.5 M | 12.09 | 99 | 0 | 28 | 8,072 | 2,868 | yes |
| liquidate short 5× underwater +40 % | 100.4 M | 10.94 | 96 | 0 | 24 | 7,188 | 2,348 | yes |
| liquidate long 5× underwater −20 % | 96.3 M | 10.43 | 96 | 0 | 25 | 7,140 | 2,460 | yes |
| settle_backstop_debt USDC (ticked, heavy; chunk / full 88.4 / 88.6 M) | 89.6 M | 10.25 | 96 | 0 | 22 | 6,684 | 2,436 | yes |
| **settle_backstop_debt XLM, backstop XLM pT repaid first then swap (3 + 3 open, exit pending): worst settle** | **198.0 M** | **18.63** | **110** | 0 | **33** | 12,312 | **5,772** | yes |
| settle_backstop_debt XLM (ticked, heavy, book open, exit pending; no BsXlmPt yet) | 135.2 M | 13.87 | 109 | 0 | 33 | 12,368 | 4,092 | yes |
| settle_backstop_debt XLM (self-sized keeper loop, Impaired; gaps 132.8 M) | 134.1 M | 13.97 | 109 | 0 | 33 | 12,404 | 4,092 | yes |
| settle_backstop_debt XLM (liquidity-capped partial, right after S5 liquidation) | 112.0 M | 12.40 | 107 | 0 | 30 | 11,564 | 3,340 | yes |
| reconcile (no deficit / deficit → Impaired) | 8.6 / 16.6 M | 3.53 | 75 | 0 | 2 | 3,504 | 208 | yes |
| claim XLM pTokens | 46.5 M | 7.22 | 92 | 0 | 17 | 5,216 | 792 | yes |
| claim USDC pTokens | 26.6 M | 5.99 | 85 | 0 | 12 | 4,900 | 572 | yes |

Worst case against each gate:

- **Release:** 113 keys (87 % of 130; about 115 with a G-account signer). The release maximum is swap-close long at
  **162.3 M** (81 % of 200 M), a flow WP6 did not change. Open is at 153.3 M (77 %), so the < 170 M target is met.
- **Liquidation flows:** 110 keys (32 % of 340), 198.0 M instructions (58 % of 340 M), 33 writes (19 %), 18.6 MB memory
  (52 % of 35.65 MB), 12.5 kB write bytes (11 %), 5.8 kB events (41 %). All are on the XLM settle that first repays from
  the backstop XLM pTokens (heavy test, end).
  - That path withdraws `BsXlmPt` from the live XLM receipt and repays before the swap. It adds a health-checked XLM
    withdraw, a second repay and an XLM rate read.
  - It works on the live receipt: the shares reach 0 and `BsXlmPt` goes from 0.0537 to 0.0539 XLM pT (the new surplus).
- **XLM settle cost: +45 M** against WP5 (88.5 → 132.8 M). The additions:
  - the pool quote, about 10 M;
  - `account_liquidity` + CF + one `update_interest`, about 13.5 M;
  - the surplus deposit to the XLM receipt and its balance reads.

  It stays within the 85 % liquidation gates. The USDC settle also pays for the liquidity cap: +15 M (73.8 → 88.6 M).

## WP5 resource table (superseded; build 957ad3ca)

Limits: margin release (open / close / close_with_repay / backstop_*) ≤ 130 keys and ≤ 200 M instructions, with the network
limits otherwise. Liquidation flows (liquidate, settle, reconcile, claim) ≤ 85 % of the live limits: 340 footprint, 170
disk reads, 170 writes, 340 M instructions, 35.65 MB memory, 112,281 B write bytes, 13,926 B events.

| Entrypoint / scenario | Instr | Mem MB | Keys | Disk rd | Writes | Disk rd B | Write B | Events B | Within limit |
|---|---|---|---|---|---|---|---|---|---|
| backstop_deposit (admin seed $750, dead shares) | 21.9 M | 5.00 | 52 | 0 | 12 | 0 | 3,996 | 552 | yes |
| backstop_deposit (second backer $50) | 21.4 M | 4.94 | 51 | 0 | 10 | 0 | 3,716 | 552 | yes |
| open long 5× (real-pool swap) | 170.9 M | 16.37 | **113** | 1 | 35 | 0 | 12,688 | 5,308 | yes |
| open short 5× | 163.2 M | 15.50 | 109 | 0 | 32 | 0 | 12,172 | 4,832 | yes |
| open long 3× | 182.1 M | 16.91 | 112 | 0 | 35 | 0 | 12,964 | 5,528 | yes |
| open short 3× | 164.7 M | 15.56 | 109 | 0 | 32 | 0 | 12,172 | 4,832 | yes |
| **open long 5×, heavy state**: both sides open, exit pending, USDC + XLM bad debt unsettled | **225.1 M** | 20.55 | 112 | 0 | 35 | 0 | 13,148 | 6,408 | **NO (>200 M)** |
| **open short 5×, heavy state** | **207.1 M** | 19.16 | 109 | 0 | 32 | 0 | 12,356 | 5,712 | **NO (>200 M)** |
| backstop_request_withdraw | 1.4 M | 1.48 | 6 | 0 | 4 | 0 | 3,756 | 168 | yes |
| backstop_withdraw (72 h, book open, pool check) | 85.9 M | 9.83 | 98 | 0 | 16 | 0 | 4,896 | 1,212 | yes |
| close long 3× (swap XLM→USDC, 72 h funding) | 162.2 M | 16.38 | 107 | 0 | 37 | 0 | 13,188 | 4,728 | yes |
| close short 3× (swap USDC→XLM) | 137.0 M | 15.02 | 107 | 0 | 34 | 0 | 12,396 | 3,864 | yes |
| close_with_repay long 5× | 85.6 M | 9.83 | 98 | 0 | 27 | 0 | 7,704 | 2,564 | yes |
| close_with_repay short 5× | 60.7 M | 8.53 | 93 | 0 | 24 | 0 | 6,912 | 1,644 | yes |
| liquidate long 5× underwater, −20 % (bad debt + Impaired check) | 96.3 M | 10.42 | 96 | 0 | 25 | 0 | 7,140 | 2,460 | yes |
| liquidate long 3× underwater, −40 % | 95.4 M | 10.38 | 96 | 0 | 24 | 0 | 7,004 | 2,460 | yes |
| liquidate long 2× solvent, −40 % (USDC bad debt outstanding) | 108.5 M | 12.09 | 99 | 0 | 28 | 0 | 8,072 | 2,868 | yes |
| liquidate short 5× solvent, +20 % | 87.9 M | 10.93 | 96 | 0 | 25 | 0 | 7,232 | 1,948 | yes |
| liquidate short 5× underwater, +40 % (XLM bad debt, oracle NAV) | 100.4 M | 10.94 | 96 | 0 | 24 | 0 | 7,188 | 2,348 | yes |
| liquidate short 3× solvent, +40 % | 67.6 M | 9.64 | 94 | 0 | 25 | 0 | 7,464 | 1,728 | yes |
| settle_backstop_debt USDC (chunk 0.5 pT) | 73.7 M | 8.63 | 90 | 0 | 22 | 0 | 6,544 | 2,216 | yes |
| settle_backstop_debt USDC (full) | 73.8 M | 8.63 | 90 | 0 | 22 | 0 | 6,544 | 2,216 | yes |
| settle_backstop_debt XLM (withdraw, real-pool swap, repay) | 88.5 M | 10.82 | 101 | 0 | 30 | 0 | 11,620 | 3,120 | yes |
| settle_backstop_debt XLM (ticked, heavy state, book open) | 89.8 M | 10.89 | 101 | 0 | 30 | 0 | 11,528 | 3,120 | yes |
| settle_backstop_debt USDC (ticked, heavy state) | 74.4 M | 8.80 | 90 | 0 | 22 | 0 | 6,636 | 2,216 | yes |
| reconcile (no deficit → false) | 8.4 M | 3.04 | 72 | 0 | 1 | 0 | 72 | 0 | yes |
| reconcile (deficit → newly Impaired, opens paused, event) | 16.5 M | 3.52 | 75 | 0 | 2 | 0 | 3,412 | 208 | yes |
| claim XLM pTokens | 46.4 M | 7.21 | 92 | 0 | 17 | 0 | 5,216 | 792 | yes |
| claim USDC pTokens | 26.6 M | 5.98 | 85 | 0 | 12 | 0 | 4,852 | 572 | yes |

### Over the limit: `open` in the heavy backstop state (200 M instruction gate). Resolved in WP6 (remedies 1–3 applied)

The normal book runs at 163–182 M. In the worst state, open goes over the gate: **225.1 M long and 207.1 M short**.
That state is both sides open, a live backstop exit request, and unsettled USDC and XLM bad debt, measured in the test
`heaviest_open_with_bad_debt_and_pending_exit`. Keys (112), memory, writes and bytes stay well inside, and 225 M is
still 56 % of the 400 M network limit, so the transaction would land. It is over the **owner's 200 M margin gate**.

Measured: +54 M against the normal 5× long open (170.9 → 225.1 M). Cause, from reading the code (not profiled): `pool_check(Check::Open)` reads the backstop NAV twice, once in `pending_pt` and once in `value_usd`. With bad debt
outstanding, each `nav_pt(false)` runs `bs_debt` on both assets. Each `bs_debt` call is `pooled_debt` (`update_interest` +
`get_user_borrow_balance`), plus the USDC exchange rate and both oracle prices. That is about 4 extra receipt calls
and 4–6 extra price/rate calls per open.

Proposed remedy, in order of preference:

1. **Refuse opens while backstop bad debt is unsettled** (`BsDebtShares(USDC) > 0 || BsDebtShares(XLM) > 0` →
   `BackstopDebtOutstanding`).
   - It matches the existing freeze on backstop deposits for XLM bad debt.
   - New positions should not draw on a backstop whose loss is not yet booked.
   - The keeper settles within a cycle anyway (SPEC §6).
   - It removes every `bs_debt` call from open, which returns it to the normal 163–182 M.
2. **Read the NAV once per `pool_check`**: compute `nav_pt(false)` once and pass it to `pending_pt` and `value_usd`,
   saving about half the overhead.
3. The general trims below. They help every open, but alone are not enough to bring 225 M under 200 M.

Every other flow is within its limit. The worst case against each gate:

- **Release, normal state:** 113 keys (87 % of 130; about 115 with a G-account signer). 182.1 M instructions (91 % of
  200 M) on open long 3×.
- **Liquidation flows:** 101 keys (30 % of 340), 108.5 M instructions (32 % of 340 M), 30 writes (18 %), 12.1 MB memory
  (34 %), 11.6 kB write bytes (10 %), 3.1 kB events (22 %). Disk reads are 0–1 everywhere: live Soroban state is in
  memory.

The 3× long open costs about 11 M more than the 5× long open, even though its swap is smaller. The cause is not isolated:
the open order and receipt state differ between the two rows.

General trims, a few M instructions each:

- Drop the repeated `update_interest` + `get_user_borrow_balance` pairs (`pooled_debt` accrues per call; `position_debt`,
  open and `book` call it again).
- Read each receipt's `get_exchange_rate` once per call.
- Pass already-loaded prices into `oracle_min_out`.

## Scenario results (all pass)

**Pilot book, backstop exit, closes** (`pilot_book_backstop_exit_and_closes`)

- 5× long and 5× short open concurrently, then 3× long and 3× short. Leverage is 4.975 / 4.967 / 2.987 / 2.983.
  Open LTV is 0.800 / 0.801 / 0.666 / 0.667, and the stored liq LTV is 0.900 / 0.901 / 0.833 / 0.834
  (= (1 + LTV) / 2). After the 4 opens, pooled HF is 2.88 (1.25 required), stress −20 %/+20 % HF is 3.12 / 2.69, and the
  required backstop is $369 against a $750 seed.
- **Fees reach backstop NAV**: each open +0.074–0.124 USDC pT (0.10 % of notional). After 72 h, close/close_with_repay
  move +0.082–0.140 pT each (close fee + funding of 0.008–0.016 USDC).
- Backstop exit:
  - The second backer deposited $50 and requested its shares. Withdrawing before 72 h fails `Cooldown` (#26).
  - The admin requested the shares that would leave pooled HF ≈ 1.10. After the warp that withdraw fails
    **`BackstopHeadroom` (#22)** with the book open.
  - After the admin cancels, the backer's withdraw succeeds: 49.9998 pT out for 49.9750 pT in, the gain being its share
    of the open fees.
- After all 4 closes the pooled USDC and XLM debt are 0. Ledger == receipt balance after every phase.

**Gaps, liquidations, bad debt, settles** (`gaps_liquidations_and_settles`)

- Book: L5, L3, L2 longs and S5a, S5b, S3 shorts at P0, then +1 h.
- **XLM −20 %**:
  - L5 sits at LTV 1.0002, past its liq LTV of 0.900 and underwater. Liquidating it moves 2.90 USDC of residual debt
    shares into the backstop bucket (NAV −2.90 pT). `Impaired` stays false.
  - **L3/L2 debt is unchanged to the stroop.** L3 is at LTV 0.833 against a liq LTV of 0.833 and is not liquidatable.
  - USDC settle **chunked**: `max_ptokens` 0.5 pT repaid 0.50 USDC (shares 29.0 M → 24.0 M), then the rest. Other debts
    moved by −1 stroop (rounding against the backstop).
- **XLM −40 %**:
  - L3 underwater: 6.2 USDC of bad debt goes to the backstop, L2's debt is unchanged, then the USDC settle runs.
  - L2 (2×) is solvent: the backstop gets +0.25 pT and the owner a 57.1 XLM-pT claim, which `claim` pays.
  - Pooled HF stays 3.78.
- **XLM +20 %**: S5a is liquidated **solvent** at LTV 0.962 (branch asserted: no XLM bad-debt shares created). The
  backstop gets +1.17 pT. The owner remainder is 0: at 5×, +20 % uses up the 3 % + 1 % bonus.
- **XLM +40 %**:
  - S5b is underwater: 107.3 XLM of bad debt goes to the backstop (NAV −18.7 pT). **S3's debt is unchanged.**
  - `backstop_deposit` is frozen with `BackstopDebtOutstanding` (#25) until the XLM settle runs.
  - The XLM settle withdraws backstop USDC, swaps it on the **real pool** and repays all 107.35 XLM. S3 is unchanged.
  - S3 is liquidated solvent, and its 2.12 USDC-pT claim is paid.
- End state: pooled debt 0/0, `BsDebtShares` 0/0, NAV 724.2 pT (seed 749.6 pT, plus fees and bonuses, minus 27.8 of bad debt), Ledger == balance.

**Heavy open state** (`heaviest_open_with_bad_debt_and_pending_exit`)

- A backer exit request is pending. L5 is liquidated underwater at −20 % and S5 at +40 %, and both settles are left
  outstanding. The oracle then goes back to P0.
- A 5× long and a 5× short open still succeed: the pool check and the backstop size pass. They cost 225.1 M and 207.1 M
  instructions (see *Over the limit*).
- Ticked XLM and USDC settles then clear the debt, and Ledger == balance.

**Pooled-account seizure → reconcile** (`pooled_account_seizure_reconcile`)

- Book: S5, S3, L3. With the $750 seed, the live controller sees a shortfall only at **XLM × 7** (pooled HF 0.915). The
  keeper is offline in this scenario.
- A plain cash-controller `liquidate(margin, XLM receipt, USDC receipt, ¼ debt, ext)` repays 299 XLM and seizes
  **280.84 USDC pTokens**.
- Right after it, `reconcile` returns **true**:
  - Ledger(USDC) 947.73 vs a balance of 666.89 pT, a deficit of exactly the 280.84 seized. XLM is equal.
  - Pooled HF is 0.948.
  - `Impaired` is set, opens are paused and `margin_impaired` is emitted.
- A second `reconcile` returns false, and `open` fails `Impaired` (#24).
- The backstop NAV still reads 749.9 pT at this point: the seized pTokens are a Ledger deficit, not a NAV entry, until
  `admin_resolve_impairment` charges them (as designed).

## Known gaps (INTERFACE.md) exercised (WP5 build; WP6 resolution in *WP6 changes* §2–3)

1. **Pooled controller HF < 1 blocks some exits.** This is in the seizure env, after the reconcile.
   - `close_with_repay` **short** (S3) worked.
   - The underwater `liquidate` of S5 worked, since it pays in first. It moved 613.6 M XLM debt shares into the
     backstop bucket and took the NAV from 749.9 to **349.9 pT**, which is still positive, so not exhausted.
   - `close_with_repay` **long** (L3) failed with `Error(Context, InvalidAction)`. It failed both before and after S3/S5
     were gone. Pooled HF before the retry was 1.42 and the controller shortfall was 0.
   - The likely cause is the same controller health-check trap on L3's XLM pToken transfer out (598.7 XLM pT, worth about
     $286 CF-weighted at ×7). This trap is inferred, not traced; only the settle trap below was traced.
   - The full `settle_backstop_debt` XLM traps in the live controller's health check on the USDC `withdraw`
     (`UnreachableCodeReached`, traced). **A 10 pT chunk succeeds** (80.5 XLM repaid; XLM bad-debt shares 613.6 M →
     506.2 M), and the remaining full settle still traps.
   - Not exercised under pooled HF < 1: `claim`, and swap-`close`.

   Remedies:
   - Keeper: size the settle by simulation, searching for the largest `max_ptokens` that passes, and repeat.
   - Operator: `admin_recapitalize` (USDC pT), then settle, then let traders exit.
   - Do not try to make the contract bypass the controller check. The live controller check is the lender protection.
   - The pilot caps keep this out of reach. It needed an XLM ×7 oracle move with the keeper offline.
2. **XLM settle leaves an idle XLM surplus.** The input is sized at the oracle + 1.5 % and only `min(out, debt)` is
   repaid; the rest stays as untracked XLM underlying in margin. The oracle-only +40 % gap inflates it here
   (45.2 XLM). With the pool near the oracle it is about the 1.5 % buffer plus the pool's spread.
   Remedy (small): size the input with `estimate_swap`, or deposit the surplus to the XLM receipt and send it to the
   backstop through the next XLM settle. Not blocking for the pilot.

## Caveats

- Oracle-only gaps: in a real gap the pool also moves, and liquidity may thin. Liquidations do not swap, so their
  resources do not depend on this. Settle and close swap sizes stayed within the simulated footprint sizes
  (≤ 12,000 XLM / ≤ 5,000 USDC).
- Resources are host-model numbers from the protocol-30 host with all four contracts as WASM: margin, receipts,
  controller and pool. Re-check with a read-only Mainnet `simulateTransaction` once deployed (opens paused; CODE-PLAN §7).
- The receipt MarginController hook stays unset in every test. The harness asserts it, and nothing calls the hook
  functions.
