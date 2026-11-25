# Margin v2 — WP5 real-pool Mainnet-fork results (25 Nov 2026)

Harness: `harness/margin-fork/tests/margin.rs` (isolated crate, soroban-sdk 30.0.0 / soroban-env-host 30.0.1, protocol 30).
Fork: Mainnet ledger **65662216** (fetched 2026-11-25 08:59 UTC with `tools/fetch-fork.cjs`, read-only public RPC, ~620 KB,
git-ignored under `harness/margin-fork/.scratch/`). It contains the live controller f41e5260, the live receipts 5630a701, the
SACs, Reflector and the real Aquarius XLM/USDC concentrated pool CBBMQ…BUCV.
Margin WASM: `stellar contract build --package margin-controller` → sha256 **957ad3ca…5846** (62,533 B). The test imports it
with `contractimport!`, asserts the hash and registers it fresh on the fork with the 5-argument v2 constructor.
Run: `cd harness/margin-fork && cargo test -- --test-threads=1 --nocapture`. Per-test JSON goes to `.scratch/wp5-*.json`.
**4/4 tests pass.**

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
- **Footprint keys** = disk-read + in-memory read entries (each read-write key counted once).
  Trader, backer and keeper addresses are test contract addresses. On Mainnet a G-account signer adds its account entry
  (a disk read) plus a nonce entry (read-write), so **add about 2 keys per signer**. An XLM/USDC transfer from a G-account
  touches its account or trustline instead of a contract balance entry, which does not change the count.

## Resource table (real pool, protocol-30 host)

Limits: margin release (open / close / close_with_repay / backstop_*) ≤ 130 keys and ≤ 200 M instructions, with the network
limits otherwise. Liquidation flows (liquidate, settle, reconcile, claim) ≤ 85 % of the live limits: 340 footprint, 170
disk reads, 170 writes, 340 M instructions, 35.65 MB memory, 112,281 B write bytes, 13,926 B events.

| Entrypoint / scenario | Instr | Mem MB | Keys | Disk rd | Writes | Disk rd B | Write B | Events B | Within limit |
|---|---|---|---|---|---|---|---|---|---|
| backstop_deposit (admin seed $750, dead shares) | 21.9 M | 5.00 | 52 | 0 | 12 | 0 | 3,996 | 552 | yes |
| backstop_deposit (second backer $50) | 21.4 M | 4.94 | 51 | 0 | 10 | 0 | 3,716 | 552 | yes |
| open long 5× (real-pool swap) | 170.9 M | 16.36 | **113** | 1 | 35 | 0 | 12,688 | 5,308 | yes |
| open short 5× | 162.9 M | 15.45 | 109 | 0 | 32 | 0 | 12,172 | 4,832 | yes |
| open long 3× | **181.7 M** | 16.84 | 112 | 0 | 35 | 0 | 12,964 | 5,528 | yes |
| open short 3× | 163.9 M | 15.49 | 109 | 0 | 32 | 0 | 12,172 | 4,832 | yes |
| backstop_request_withdraw | 1.4 M | 1.48 | 6 | 0 | 4 | 0 | 3,756 | 168 | yes |
| backstop_withdraw (72 h, book open, pool check) | 86.0 M | 9.83 | 98 | 0 | 16 | 0 | 4,896 | 1,212 | yes |
| close long 3× (swap XLM→USDC, 72 h funding) | 161.8 M | 16.31 | 107 | 0 | 37 | 0 | 13,188 | 4,728 | yes |
| close short 3× (swap USDC→XLM) | 136.1 M | 14.94 | 107 | 0 | 34 | 0 | 12,396 | 3,864 | yes |
| close_with_repay long 5× | 85.4 M | 9.83 | 98 | 0 | 27 | 0 | 7,704 | 2,564 | yes |
| close_with_repay short 5× | 60.6 M | 8.53 | 93 | 0 | 24 | 0 | 6,912 | 1,644 | yes |
| liquidate long 5× underwater, −20 % (bad debt + Impaired check) | 96.3 M | 10.42 | 96 | 0 | 25 | 0 | 7,140 | 2,460 | yes |
| liquidate long 3× underwater, −40 % | 95.4 M | 10.38 | 96 | 0 | 24 | 0 | 7,004 | 2,460 | yes |
| liquidate long 2× solvent, −40 % (USDC bad debt outstanding) | 108.5 M | 12.09 | 99 | 0 | 28 | 0 | 8,072 | 2,868 | yes |
| liquidate short 5× solvent, +20 % | 87.9 M | 10.93 | 96 | 0 | 25 | 0 | 7,232 | 1,948 | yes |
| liquidate short 5× underwater, +40 % (XLM bad debt, oracle NAV) | 100.4 M | 10.94 | 96 | 0 | 24 | 0 | 7,188 | 2,348 | yes |
| liquidate short 3× solvent, +40 % | 67.6 M | 9.64 | 94 | 0 | 25 | 0 | 7,464 | 1,728 | yes |
| settle_backstop_debt USDC (chunk 0.5 pT) | 73.7 M | 8.63 | 90 | 0 | 22 | 0 | 6,544 | 2,216 | yes |
| settle_backstop_debt USDC (full) | 73.8 M | 8.63 | 90 | 0 | 22 | 0 | 6,544 | 2,216 | yes |
| settle_backstop_debt XLM (withdraw, real-pool swap, repay) | 88.5 M | 10.82 | 101 | 0 | 30 | 0 | 11,620 | 3,120 | yes |
| reconcile (no deficit → false) | 8.4 M | 3.04 | 72 | 0 | 1 | 0 | 72 | 0 | yes |
| reconcile (deficit → newly Impaired, opens paused, event) | 16.5 M | 3.52 | 75 | 0 | 2 | 0 | 3,412 | 208 | yes |
| claim XLM pTokens | 46.4 M | 7.21 | 92 | 0 | 17 | 0 | 5,216 | 792 | yes |
| claim USDC pTokens | 26.6 M | 5.98 | 85 | 0 | 12 | 0 | 4,852 | 572 | yes |

**No entrypoint exceeds its limit.** Worst cases against each gate:

- **Release:** 113 keys (87 % of 130; about 115 with a G-account signer). 181.7 M instructions (91 % of 200 M) on open
  long 3×. Memory, writes, bytes and events are all ≤ 40 % of the network limits.
- **Liquidation flows:** 101 keys (30 % of 340), 108.5 M instructions (32 % of 340 M), 30 writes (18 %), 12.1 MB memory
  (34 %), 11.6 kB write bytes (10 %), 3.1 kB events (22 %). Disk reads are 0–1 everywhere: all live Soroban state is
  in memory.

The thin margin is CPU on opens and swap-closes (136–182 M). Opens vary by ±10 M with the pool state the swap crosses.
Proposed remedy if a later tier or pool shape pushes open above 200 M:

1. Drop the repeated `update_interest` + `get_user_borrow_balance` pairs. `pooled_debt` accrues per call, and
   `position_debt`, open and `pool_check`/`book` each call it again.
2. Read each receipt's `get_exchange_rate` once per call.
3. Skip the second `price()` fetches inside `oracle_min_out` by passing the prices already loaded.

Each is a few M instructions. None is needed for the pilot.

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

**Pooled-account seizure → reconcile** (`pooled_account_seizure_reconcile`)

- Book: S5, S3, L3. With the $750 seed, the live controller only sees a shortfall at **XLM × 7** (pooled HF 0.915). The
  keeper is offline in this scenario.
- A plain cash-controller `liquidate(margin, XLM receipt, USDC receipt, ¼ debt, ext)` repays 299 XLM and seizes 280.8 USDC
  pTokens. `reconcile` then returns **true**: Ledger(USDC) 739.97 vs a balance of 459.13. It sets `Impaired`, pauses opens
  and emits `margin_impaired`. A second `reconcile` returns false. `open` fails `Impaired` (#24).

## Known gaps (INTERFACE.md) exercised

1. **Pooled controller HF < 1 blocks some exits.** This is in the seizure env, after the backstop is exhausted.
   - `close_with_repay` short worked. So did the underwater `liquidate` of S5, which pays in first; it then exhausted
     the backstop with 506 XLM of bad debt.
   - `close_with_repay` **long** traps in the live controller's health check on the XLM pToken transfer
     (`UnreachableCodeReached` → `Error(Context, InvalidAction)`). It still fails after the pooled shortfall is back to
     0, because the long's XLM pTokens are what keep the pooled account solvent.
   - A full `settle_backstop_debt` XLM traps on the USDC withdraw. **A 10 pT chunk succeeds** (80.5 XLM repaid); the
     rest still traps.

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
