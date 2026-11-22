# Leveraged PYUSD/USDC CL position with take-profit: specification (18 November 2026)

**Status: proposal plus a local prototype, for review. Nothing is approved for deployment.** Nothing was signed,
sent, deployed or upgraded, on Mainnet or Testnet. No key, keychain, secret or environment variable was read. Option1
stays closed. No production maintenance margin, CF, incentive or fee value is set or assumed: every risk number below
is either a symbol or an explicitly test-only value.

Sources refined here:

- **Narrow typed command ABI**: `docs/deployment-handoffs/27-cl-stage1-build-plan-20261111.md` §2 and §9, as frozen in
  `interfaces/cl-stage1-interface/src/lib.rs` (package 1, `28-cl-stage1-package1-abi-freeze-20261111.md`). Doc 27 is
  the 1× build; it explicitly defers the 2× execution ABI to a separate document-only milestone.
- **2× committed-state machine**: `docs/deployment-handoffs/23-cl-aquarius-call-boundary-and-staged-design-20261110.md`
  "Committed states" (O0–O5, A, C0–C2, R0–R1). This spec is that milestone, refined for the CL collateral group.
- **CL collateral group** (being built on `feat/cl-collateral-group`): `research/cl-collateral-group-20261118/DESIGN.md`.
  Only the interfaces named there are relied on; everything else is mocked and listed in §3.
- **Spike**: `research/cl-collateral-spike-20261117/REPORT.md` (live network limits, pilot-byte setup rows).
- **Margin V2**: `contracts/margin-controller` (V3-suffixed entrypoints; legacy V1 exports disabled). Conventions reused:
  position ids, `borrow_for_margin_to_controller` / `repay_for_margin` debt namespace on the lending market, 1e6 scaling,
  24 h upgrade timelock, pending-command TTLs. None of its perps code is reused.
- **Staged vault operations**: `prototypes/cl-pyusd-vault-b-pilot-tools/ledger-owner-call.cjs` and
  `keeper-worker-v5.cjs` headers (owner-signed `stage_open` / `stage_prepare_close`; keeper-run `stage_convert`,
  `stage_mint`, `stage_checkpoint_investment`, `stage_invest`, `stage_finalise`, `stage_withdraw`, `stage_close_convert`,
  `stage_cancel`; 97M keeper CPU stop; deadline ≤ 3600 s).

Prototype: `prototypes/cl-leveraged/` (contract `cl-leveraged`, mocks, unit tests). Harness:
`prototypes/cl-leveraged/harness/`. Measured rows: `research/cl-leveraged-20261118/rows.md` and `results.json`.

---

## 0. The three questions that fix the architecture

### Q1. Who is the borrowing account? → the margin contract, per-position debt namespace (topology M)

| Option | Health / liquidation | Keeper-triggered stages | Verdict |
|---|---|---|---|
| **U**: the user is the group account (enter sleeve market, borrow USDC on the group controller) | Group per-account health and group L1–L5 liquidation | Every leveraged round needs a fresh owner-signed borrow (`receiver.require_auth()`), and take-profit needs an owner-signed redeem/close. A permissionless TP or stop cannot run | Rejected: TP cannot be permissionless |
| **P**: the margin contract is a single group account for all users | One shared health figure for every position | Possible | Rejected: one bad position contaminates every other; group seizure cannot be attributed |
| **M**: the margin contract (`cl-leveraged`) custodies each position's shares and borrows USDC in the lending market's **margin namespace keyed by position id** | Per position, computed by `cl-leveraged` from the group's NAV checkpoint and the group's CF | Permissionless keeper stages, since `cl-leveraged` is the command owner and authorizes its own calls | **Proposed** |

Under M:

- Debt lives in the group USDC lending market as `MarginBorrowSnapshots(position_id)`, using the existing receipt-vault
  `borrow_for_margin_to_controller(position_id, amount)` / `repay_for_margin(position_id, payer, amount)` /
  `get_margin_borrow_balance(position_id)` (`contracts/receipt-vault/src/contract.rs:4103,4230,4373,3496`). The
  market's `set_margin_controller` must name `cl-leveraged`. The `assert!(!Self::cl_custody(&env))` at `:4171` checks
  the **lending market's own** `ClCustody` flag (`:1572`), which is false for the USDC market, so the margin path is
  available. Borrow cap, pause (`is_borrow_paused` on the group controller), cash reserve and interest still apply.
- The group controller's account health is **not** consulted for these borrows (as for existing Margin V2). The
  controller remains the source of the sleeve CF, the liquidation incentive and the pause flags; `cl-leveraged` enforces
  per-position health with those parameters. This is a deliberate trust shift: see decision **U1**.
- Shares are owned by `cl-leveraged` on the vault and labelled with the user as beneficiary, so the user accrues AQUA on
  leveraged shares exactly as a direct holder does (group DESIGN §3.1). Needs a beneficiary-assign hook (§3, dep X3).
- `cl-leveraged` never enters a market on the group controller, so group L1–L5 never touch these shares. Forced exits
  are `cl-leveraged`'s own health exit (§5), funded by the position's own assets: no third-party liquidator capital.

### Q2. How does a position unwind under debt? → proceeds bound to repayment

The close command's owner and fixed payout recipient is `cl-leveraged` itself (doc 27's fixed-owner rule holds: the
owner of the receipt command is the margin escrow). `withdraw_close` pays the in-kind mixture to `cl-leveraged`, and
`collect_v1` books that mixture into the position's escrow and `settle_v1` repays the margin debt from it **before**
anything reaches the user. No health-checked redeem
(group GR5, error #6) is involved, so a position at the exit threshold, with no free collateral, still unwinds in one
pass. There is no iterative deleverage. Health exit is therefore the same transaction count as a normal close.

### Q3. How does borrowed USDC enter the vault? → a pair-funded open (dependency X1), single pass

The frozen `OpenTermsV1` funds token0 (PYUSD) only and converts `swap_in` PYUSD→USDC. Funding a leveraged open with
`E` PYUSD equity plus `D` borrowed USDC through it would need an extra USDC→PYUSD swap of `D` first, then the vault swaps
about `(E+D)/2` back. A pair-funded open (`prepare_open_pair`, both tokens, one guarded swap of the excess leg only)
swaps `|D − (E+D)/2| = |D − E|/2`, which is **zero at 2×**. Decision **U2** chooses; the prototype implements the pair
open against a mock, and the fallback (frozen ABI + one adapter swap in `fund_v1`) is costed in §7.

**Single pass, not a borrow loop.** The brief's "repeat to target leverage" is the manual loop. With per-position custody
the borrowed USDC is itself escrowed collateral from the moment it is drawn (doc 23 O1: "Borrowed cash and debt record
commit together"), so one borrow reaches the same bound the infinite loop converges to:

```
manual loop, k rounds at LTV x:   L_k = (1 − x^(k+1)) / (1 − x)   →   1 / (1 − x)
single pass:                      D ≤ x · (E + D)   ⇔   L = (E + D)/E ≤ 1 / (1 − x)
```

`add_v1` repeats an open on an active position (more equity and/or more debt), one command at a time, re-proving health
on a fresh NAV checkpoint. That is the "repeat" path, used when a single command is capped (backing cap, admission).

---

## 1. Actors and contracts

```
user (owner) ──open_v1/add_v1/set_exit_v1/close_v1/cancel_v1/repay_v1──▶ cl-leveraged (margin escrow, position ledger)
keeper (anyone) ──fund_v1/sync_open_v1/arm_exit_v1/trigger_exit_v1/collect_v1/swap_deficit_v1/settle_v1/cancel_v1(after deadline)──▶ cl-leveraged
keeper (anyone) ──convert_open/mint_cash/invest_cash/withdraw_close/convert_close/cancel(after deadline)──▶ group receipt

cl-leveraged ──prepare_open_pair/fund_open/prepare_close/cancel/stage1_job──────────▶ group receipt (StgV2 sleeve class)
             ──nav_checkpoint/fallback_checkpoint/checkpoint_nav/checkpoint_fallback─▶ group receipt (DESIGN §2, §4)
             ──get_market_cf/get_liquidation_incentive_scaled/is_borrow_paused──────▶ group controller
             ──borrow_for_margin_to_controller/repay_for_margin/get_margin_borrow_balance─▶ group USDC market
             ──lastprice(Stellar(PYUSD))─────────────────────────────────────────────▶ Reflector CALI (USDC base)
             ──swap_pool (swap_deficit_v1 only: USDC leg cannot cover debt)──────▶ swap adapter → Aquarius pool
             ──transfer──────────────────────────────────────────────────────────────▶ PYUSD / USDC SACs
```

`cl-leveraged` is **never** inside the heavy receipt rows (convert, mint, invest, withdraw). Its calls are separate,
light transactions that read the command record (`stage1_job(id).result`). That keeps the 95.1M-CPU invest row (doc 27
O5) and the 78-entry withdraw row (C2a) unchanged.

---

## 2. Refined ABI (prototype names; versioned `_v1`)

Types (prototype `leverage/src/types.rs`):

- `LevConfigV1`: bound addresses (receipt, group controller, USDC market, swap adapter, pool, CALI oracle, PYUSD, USDC,
  treasury, admin). Set once at `initialize`. `LevRiskV1` changes go through a 24 h timelocked proposal
  (`propose_risk_v1` / `execute_risk_v1`) and bump `revision`; `pause_new_risk_v1` stops opens/adds immediately.
  **No defaults in code.** The prototype does not yet stamp the risk revision on positions (production item 3).
- `LevRiskV1`: `open_buffer_bps` (b), `max_ckpt_age_secs` (group DESIGN: 60), `max_oracle_age_secs` (600),
  `max_depeg_bps` (100), `max_slippage_bps`, `open_fee_bps`, `close_fee_bps`, `exit_incentive_bps`, `tp_min_gap_bps`,
  `tp_confirm_delay_secs`, `tp_confirm_window_secs`, `max_deadline_secs`, `new_risk_enabled`, `revision`.
- `OpenLevV1{equity0, borrow1, swap_in, swap_dir, min_swap, min_shares, reserve0, deadline, max_ltv_bps, exit}`.
- `ExitPolicyV1{take_profit: TakeProfitV1, stop_hf_bps: Option<u32>}`;
  `TakeProfitV1 = None | EquityAtLeast(u128 USDC) | PriceAtLeast(u128 PYUSD/USDC, 14 dp)`.
- `PositionV1{owner, phase, cmd, shares, escrow0, escrow1, reserve0, pending, exit, armed, reason, equity_in, shortfall}`.
- `LevPhaseV1`: `PendingFund`, `Opening`, `Active`, `Closing`, `Settling`, `Closed`, `Refunded`, `Defaulted`.

Entry points (O = owner, P = permissionless, E = anyone strictly after the stored deadline, A = admin):

| Entry point | Caller | Required phase | Effect / next phase |
|---|---|---|---|
| `initialize(cfg, risk)` | A (`cfg.admin`) | — | Store config and risk once |
| `open_v1(owner, terms) -> pid` | O | — | Pull `equity0 + reserve0` PYUSD; borrow `borrow1` USDC into escrow (`borrow_for_margin_to_controller(pid)`); open fee to treasury out of the borrowed USDC (financed by the debt); admission (§4); → PendingFund |
| `add_v1(owner, pid, terms)` | O | Active | Same as open for the increment; health on fresh NAV checkpoint over existing shares + new escrow; → PendingFund |
| `fund_v1(pid) -> cmd` | P | PendingFund | `prepare_open_pair` + `fund_open` on the receipt, owner = `cl-leveraged`, exact nested transfer authorized as current contract; escrow → command; → Opening |
| (receipt) `convert_open`, `mint_cash`, `invest_cash` | P | — | Unchanged staged rows; `cl-leveraged` not invoked |
| `sync_open_v1(pid)` | P | Opening | Read `stage1_job(cmd)`: Invested → add shares, assign beneficiary, → Active. Cancelled before mint → mixture returned to escrow → Settling (repay, refund). Cancelled after mint → shares released → Active |
| `set_exit_v1(owner, pid, exit)` | O | PendingFund..Active | Replace TP / stop; TP must clear `tp_min_gap_bps` above current equity; disarms |
| `arm_exit_v1(pid)` | P | Active | TP condition true on a fresh checkpoint → record `armed{ts, ckpt_ts}` |
| `trigger_exit_v1(pid) -> reason` | P | Active | Reason `Health` (HF < 1 on NAV, else on the fallback; or CF = 0 with debt), `Stop` (HF < stop, NAV only), `TakeProfit` (armed ≥ delay, ≤ window, strictly newer checkpoint/oracle round, still true; evaluated independently of the NAV for `PriceAtLeast`). `prepare_close(cl-leveraged, shares, InKind)` → Closing |
| `close_v1(owner, pid)` | O | Active | Voluntary close, same command → Closing |
| (receipt) `withdraw_close` | P | — | Unchanged C2a row: in-kind mixture paid to `cl-leveraged` |
| `collect_v1(pid)` | P | Closing | Read `stage1_job(cmd).result` (`Exit` amounts), release the beneficiary label, shares → escrow; → Settling |
| `swap_deficit_v1(pid)` | P | Settling | Only if escrowed USDC < debt (+10 bps interest buffer) + fee + incentive: swap PYUSD→USDC for the deficit, min-out `p_CALI · (1 − max_slippage)`; `NoDeficit` otherwise |
| `settle_v1(keeper, pid)` | P (keeper signs: incentive recipient) | Settling | Repay lender first, then close fee, then exit incentive, residual in kind to owner → Closed / Refunded; USDC < debt with PYUSD left → `SwapRequired`; nothing left → Defaulted with recorded shortfall |
| `cancel_v1(pid)` | O, or E | PendingFund / Opening / Closing | PendingFund: reserve back, → Settling (refund via swap/settle); add-path with shares repays inline → Active. Opening: receipt `cancel(cmd)` (fixed owner = `cl-leveraged`) then synced. Closing: receipt `cancel` → Active; a Health exit only after its deadline |
| `repay_v1(payer, pid, amount)` | payer | any with debt | External top-up / deleverage; payer authorizes |
| `position_v1(pid)`, `health_v1(pid)`, `config_v1()` | read | — | Views |

Errors are a reserved block **1800–1829** (1800–1826 used, `leverage/src/types.rs`), disjoint from 1700–1718 of the stage-1 freeze.
Optional records are enums (`TakeProfitV1::None`, `ArmV1::Off`, `PendingV1::None`): `Option<UDT>` struct fields do not
convert under SDK 25.3.1 testutils. For the same reason the frozen crate itself (`TransitionV1.before: Option<PhaseV1>`) does
not compile with `testutils`, so the prototype carries a wire-identical copy of `PhaseV1..CommandV1` (`interface/src/stage1.rs`).

### State machine over the staged vault commands

```
             open_v1 (O)            fund_v1 (P)            convert_open → mint_cash → invest_cash (P, receipt)
   ──────▶ PendingFund ───────────▶ Opening[cmd: Prepared→Funded] ─────────────────────────────▶ cmd Invested
              │ cancel_v1 (O / E)        │ receipt cancel (O via cancel_v1 / E)                       │ sync_open_v1 (P)
              ▼                          ▼                                                           ▼
           Refunded ◀── settle ── Settling (mixture back)   ◀── cmd Cancelled before mint       Active ◀── add_v1 (O)
                                         Active (shares)    ◀── cmd Cancelled after mint          │  │ set_exit_v1 (O)
                                                                                                   │  │ arm_exit_v1 (P)
                                     trigger_exit_v1 (P: Health / Stop / TakeProfit) or close_v1 (O)│
                                                                                                   ▼
  Closed / Defaulted ◀── settle_v1 ◀── [swap_deficit_v1] ◀── Settling ◀── collect_v1 ◀── cmd Paid ◀── withdraw_close ◀── Closing[cmd CloseLocked]
                                                                                cancel_v1 → Active (cmd Cancelled, shares unlocked)
```

Doc 23 mapping: PendingFund = O0/O1 (cash and debt committed together, held by the margin escrow instead of the vault);
Opening = O1–O4 inside the receipt; `sync_open_v1` = O5 delivery + A activation (no separate deliver: shares are already
owned by `cl-leveraged`); Closing = C0; `withdraw_close` = C1; `collect_v1` + `swap_deficit_v1` = C2; `settle_v1` = R0 + R1.
Settlement was first prototyped as one transaction; projected on real dependencies it is ~89 entries without and ~122 with the
swap (§7.2), so it is split into three light stages, each projected ≤ 63 entries.

---

## 3. Dependencies on the CL group and the vault (not all in DESIGN.md)

| # | Interface | Source | Status |
|---|---|---|---|
| G1 | receipt `nav_checkpoint() -> Option<Ck{rate, ts, ledger}>`, `checkpoint_nav() -> rate` (no args, no auth, ≤ 60 s) | DESIGN §4, sleeve `lib.rs:216,265` | In DESIGN |
| G2 | receipt `fallback_checkpoint()`, `checkpoint_fallback()` (cause-gated, liquidation-only) | DESIGN §2 | In DESIGN. **Use by a non-controller caller is new** (D-G2) |
| G3 | controller `get_market_cf(sleeve)`, `get_liquidation_incentive_scaled()`, `is_borrow_paused(market)` | `simple-peridottroller/src/contract.rs:904,936,1480` | Exists in source; names not in DESIGN |
| X1 | receipt `prepare_open_pair(owner, PairOpenTermsV1)` (two-token funding) | — | **Not in DESIGN or the freeze**; mocked |
| X2 | receipt `stage1_job(id).result` carries actual payout / release amounts (`ExitV1`, `ReleasedV1`) | freeze `lib.rs:95–135` | In the freeze; StgV2 must keep it |
| X3 | vault beneficiary label for custodian-owned shares (`custody_assign(custodian, beneficiary, n)` pattern, `margin-controller/src/cl_custody.rs`) | existing custody hooks | **Not in DESIGN for StgV2**; mocked as receipt `assign_beneficiary` |
| X4 | USDC market margin namespace (`borrow_for_margin_to_controller`, `repay_for_margin`, `get_margin_borrow_balance`) with `set_margin_controller(cl-leveraged)` | receipt-vault source | Exists; group market configuration not in DESIGN |
| X5 | receipt unit mirror must treat `cl-leveraged`-owned units as ordinary owner units (no controller market entered) | DESIGN §3.1 | Assumed |

Everything marked X is mocked in `prototypes/cl-leveraged/mocks/` under explicit names and is a dependency on the group
engineer.

---

## 4. Valuation, health and maximum leverage

**Numeraire: USDC.** Position value `V = shares · rate / 1e6` where `rate` is the group's vault-wide NAV checkpoint rate
(DESIGN §1.2: CALI PYUSD/USDC × vault amounts, guards: oracle age ≤ 600 s, depeg ≤ 100 bps, pool spot within 50 bps of
oracle, tick in range). Escrowed tokens (PendingFund, Settling) are valued at `escrow1 + escrow0 · p_CALI` with the same
age and depeg guards. Debt `D` is `get_margin_borrow_balance(pid)` (accrued). USDC/USD (CAFJ) cancels because collateral
and debt are both USDC-denominated; it is read only if a USD-denominated TP is ever added (+2 keys; not prototyped).

```
CF      = controller.get_market_cf(sleeve)              (group parameter; the liquidation threshold LT = CF)
x_open  = CF · (1 − b)                                  (b = open_buffer_bps, ≥ fallback haircut h + drift allowance)
open:   D_after ≤ x_open · V_after   and   D_after ≤ owner's max_ltv
HF      = V · CF / D                                     (NAV checkpoint ≤ 60 s; for Health only, else fallback ≤ 60 s)
L_max   = 1 / (1 − x_open)
```

**Lender solvency condition.** A forced exit starts at `V = D/CF` and recovers `V · (1 − c)` where
`c = c_withdraw + c_swap + incentive + close_fee`, then loses up to `Δ` of value during the exit window. After the trigger
only `withdraw_close` is price-exposed for the position's shares (in-kind payout); `collect_v1`/`swap_deficit_v1`/`settle_v1`
follow, so Δ covers ≥ 1 ledger to the withdrawal plus the PYUSD leg's exposure until the deficit swap (≥ 3 more ledgers +
keeper latency). The debt is covered iff `CF ≤ (1 − c)(1 − Δ)`. On the fallback path the
value is already `≈ 97.9%` of NAV (DESIGN §2.2), so the fallback trigger is conservative by `h`.

Illustration only (test arithmetic, **not** proposed values): with `b = 2%`, CF 50% → `x_open` 49%, `L_max` 1.96;
CF 70% → 68.6%, 3.18; CF 80% → 78.4%, 4.63. The group has not chosen a sleeve CF; the spike used 50% as a fixture value.
The spec COLLATERAL-BORROWING-SPEC §7 requires the CF to come from backtested worst moves and funded exit capacity.

**Swap-free unwind at L ≤ 2.** In the middle of the range the mixture is about half USDC, so the USDC leg covers debt iff
`D ≤ V/2`, i.e. `L ≤ 2`. At the lower band edge the position is all PYUSD and `swap_deficit_v1` must run before `settle_v1`;
the swap row is measured separately. Leverage above 2 always needs the swap on exit.

---

## 5. Take-profit, stop and health exit

**Trigger must only change state.** `arm_exit_v1` / `trigger_exit_v1` read the stored NAV checkpoint (or refresh it
inline through `checkpoint_nav()`, no auth) and the oracle, never pool spot or a pool quote, and their only effect is to
lock the shares into a close command. Execution protection is separate: the in-kind withdrawal needs no price, and the
only swap (`swap_deficit_v1`, deficit only) is bounded by `p_CALI · (1 − max_slippage)`.

| Kind | Condition | Price source | Persistence |
|---|---|---|---|
| Health (protocol) | `HF < 1` | NAV checkpoint ≤ 60 s, else fallback checkpoint ≤ 60 s (cause-gated by the group) | None: immediate |
| Stop (owner) | `HF < stop_hf_bps / 1e4` (stop > 1) | NAV checkpoint ≤ 60 s only | None |
| TP `EquityAtLeast(T)` | `V − D ≥ T` | NAV checkpoint ≤ 60 s | Armed, then confirmed on a **later** checkpoint ≥ `tp_confirm_delay_secs` and ≤ `tp_confirm_window_secs` after arming |
| TP `PriceAtLeast(P)` | `p_CALI ≥ P` | CALI only, age ≤ `max_oracle_age_secs`; does not need the NAV checkpoint | Same arm/confirm; the confirm needs a strictly newer CALI round, so the window must exceed the 300 s resolution (enforced in `check_risk`) |
| Collateral disabled | group CF = 0 (`emergency_disable_collateral`) with debt | none | Immediate Health exit for every indebted position; opens/adds refused (decision U13) |

**Why a pool push cannot fire a TP.** Inside the guarded band (pool within 50 bps of oracle, tick in [−20, 20)) a push
inflates the NAV only to second order: DESIGN §1.3 bounds it at about 0.05% of one leg at δ = 0.2%. Equity amplifies it by
`L`. `set_exit_v1` therefore rejects an equity TP closer than `tp_min_gap_bps` to the current equity, and the gap must be
chosen ≥ `L × 5 bps` with margin (test value 50 bps). A push beyond the band makes `checkpoint_nav` revert, so no fresh
checkpoint exists and nothing triggers. The arm/confirm rule makes the attacker hold the push against arbitrage across
two checkpoints. `PriceAtLeast` is pool-independent; its residual risk is the CALI feed itself (DESIGN §1.3: flat,
re-timestamps; DEX-trade driven). The only harm of an early TP is an early, user-chosen exit at oracle-bounded prices.

**A pool push can block a trigger, though.** Out of band means no NAV checkpoint, so an owner Stop and an equity TP
cannot be evaluated while the push lasts (test `pool_push_blocks_stop_but_health_survives_on_fallback`). Health still
works through the cause-gated fallback, and a price TP still works on CALI alone (`price_take_profit_uses_oracle_only`, run
with the NAV refused). The real `checkpoint_nav` also refuses when the pool tick leaves the position's range (about
±0.2%), so an equity TP set above that move can only fire after the pool returns into range; a `PriceAtLeast` TP has no such
limit. The mock has no tick guard, so this range limit is stated, not tested.

**Granularity.** Equity TP thresholds are honoured to about `L · 5 bps` of position value plus the NAV checkpoint
rounding (rate is 1e6-scaled per unit: ≤ 1 µUSDC per unit). AQUA is not in NAV: a TP on "profit including AQUA" is not
expressible; AQUA keeps accruing to the owner as beneficiary and is claimed separately.

---

## 6. Transactions per journey, triggers and fees

| Journey | Transactions (who) | Count |
|---|---|---:|
| Open | `open_v1` (O) → `fund_v1` (P) → `convert_open` → `mint_cash` → `invest_cash` (P, receipt; pilot bytes add `stage_checkpoint_investment` + `stage_finalise`) → `sync_open_v1` (P) | 6 (8 on pilot bytes) |
| Add | `add_v1` (O, may refresh NAV inline) → same as open from `fund_v1` | 6 |
| Take-profit | `arm_exit_v1` (P, refresh inline) → ≥ delay → `trigger_exit_v1` (P, refresh inline) → `withdraw_close` (P) → `collect_v1` → [`swap_deficit_v1`] → `settle_v1` | 5–6 |
| Health / stop exit | `trigger_exit_v1` (P; NAV, else `checkpoint_fallback` inline for Health) → `withdraw_close` → `collect_v1` → [`swap_deficit_v1`] → `settle_v1` | 4–5 |
| Voluntary close | `close_v1` (O) → `withdraw_close` → `collect_v1` → [`swap_deficit_v1`] → `settle_v1` | 4–5 |
| Refund, before fund | `cancel_v1` (O, or E) → `swap_deficit_v1` (fee + interest) → `settle_v1` | 3 |
| Refund, failed stage | `cancel_v1` (O / E: receipt `cancel` + sync in one tx) → [`swap_deficit_v1`] → `settle_v1` | 2–3 |

**Fees** (all symbols; decision U5): open fee `open_fee_bps` on borrowed notional, paid to the treasury out of the
borrowed USDC at `open_v1` (so it is financed by the debt and counted in the LTV check); close fee `close_fee_bps` on repaid debt at `settle_v1`; exit incentive `exit_incentive_bps` of repaid debt
to the `settle_v1` caller on Health/Stop/TP exits only (voluntary and refund paths pay none); receipt execution reserve
`reserve0` (doc 27) prepaid by the owner and refunded through the command. Keeper XLM fees are an operating cost, as for
the 1× keeper. Interest is the USDC market's (STATIC 8% APR proposed by the group, D8).

**Failure / refund paths.**

- `fund_v1` rejected by the receipt (pause, cap, admission): the whole transaction reverts; escrow and debt unchanged;
  keeper retries, or the owner (anytime) / anyone (after deadline) runs `cancel_v1`: the reserve goes back and the
  position moves to Settling; `swap_deficit_v1` covers the open fee + accrued interest from PYUSD; `settle_v1` repays
  and refunds the rest (→ Refunded).
- A receipt stage fails or expires: only `cancel` on the receipt can move funds, always back to `cl-leveraged`.
  `sync_open_v1` (or `cancel_v1` in the same transaction) reads the `Released` result: mixture → Settling →
  [`swap_deficit_v1`] → `settle_v1` repays and refunds; locked (minted) shares → Active, never a second cash refund.
- `swap_deficit_v1` fails its oracle minimum (pool pushed or illiquid): it reverts and is retried; `settle_v1` refuses
  (`SwapRequired`) while PYUSD remains and USDC cannot cover the debt. Anyone may `repay_v1` the deficit instead.
  Debt is never forgiven by a failed swap. Interest keeps accruing in Settling; the swap adds a 10 bps buffer.
- Add path: while an `add_v1` command is pending, the position (which already has shares) is not health-exitable until
  the command finishes or is cancelled (≤ the open deadline). Decision U9.
- Mixture worth less than debt after swapping everything: repay all, record `shortfall`, → Defaulted (lender loss
  visible; no protocol backstop is assumed — decision U6).
- Oracle stale (> 600 s): health uses only the fallback (which also needs CALI) → no trigger; position is repay-only
  (group D4). Open/add/TP are blocked.

---

## 7. Resource budget per transaction

Limits: internal 100 SDK entries (memory reads + disk reads + writes), 100 unique keys, 50 writes, 100M CPU (keeper stop
97M), 41,943,040 memory. Live Mainnet (spike, ledger 65527300): 400M CPU, 41,943,040 mem, 400 footprint, 200 disk reads,
200 writes, 200,000 read bytes, 132,096 write bytes, 132,096 tx bytes.

**Evidence class.** Local host only: soroban-env-host / SDK **29.0.1** testutils, default cost model (the spike found
the live model adds +0.2M..+0.6M CPU per row; not applied here). `cl-leveraged` and all mocks are the compiled WASMs from
`stellar contract build` (contracts on SDK 25.3.1; `cl_leveraged.wasm` 43,304 bytes ≤ 131,072). One `Env` per transaction,
chained by ledger snapshot (+1 ledger, +6 s). Entries = memory reads + disk reads + writes; keys = unique ledger keys
requested from a recording snapshot source. `mock_all_auths` (recording; nonces created and counted). Users are
C-addresses: a G-account user adds about 1 account + 1 trustline disk read (spike). Not Core, not signed, no fees or tx
size. Reproduce: `bash prototypes/cl-leveraged/run.sh` (offline). Full rows: `rows.md`, `results.json`.

**Cold / warm.** *Inline* = the stored NAV checkpoint is older than 60 s and `cl-leveraged` refreshes it inside the same
transaction (`checkpoint_nav()`, no auth); *fresh* = a separate `checkpoint_nav` transaction ran ≤ 60 s earlier. For
`open_v1`, *first* = first position in the contract (counter and issuer accounts first touched, 2 disk reads), *second* =
same owner again. Archived (TTL-expired) entries are not measured: restoration stays native RestoreFootprint batches run
by the keeper (doc 27 §5).

#### 7.1 Measured: `cl-leveraged` stages on mocks

| Stage (tx) | Caller | CPU | Mem | Entries (m+d+w) | Keys | Writes | Group-dependent subtree |
|---|---|---:|---:|---|---:|---:|---|
| `open_v1` first | O | 3,128,900 | 3,892,525 | **32** (21+2+9) | 23 | 9 | market borrow X4, controller pause G3 |
| `open_v1` second | O | 3,188,563 | 3,893,666 | **30** (21+0+9) | 21 | 9 | same |
| `fund_v1` (pair open + fund, 2 nested transfers) | P | 3,194,889 | 2,836,543 | **21** (13+0+8) | 13 | 8 | receipt X1 + `fund_open` |
| `sync_open_v1` | P | 2,242,098 | 2,633,245 | **14** (10+0+4) | 10 | 4 | job result X2, beneficiary X3 |
| `set_exit_v1` (fresh NAV) | O | 2,138,397 | 2,705,228 | **14** (12+0+2) | 12 | 2 | NAV view G1, market view X4 |
| `arm_exit_v1` inline / fresh | P | 2,758,228 / 1,757,640 | 3,836,178 / 2,604,522 | **12** (10+0+2) / **9** (8+0+1) | 10 / 8 | 2 / 1 | inline L1a G1 |
| `trigger_exit_v1` TP inline / fresh | P | 5,228,349 / 4,221,997 | 4,032,058 / 3,192,333 | **20** (16+0+4) / **18** (14+0+4) | 16 / 14 | 4 / 4 | L1a G1, receipt `prepare_close` |
| `trigger_exit_v1` Health inline / fresh | P | 4,286,841 / 3,317,028 | 4,028,087 / 2,969,946 | **20** (16+0+4) / **18** (14+0+4) | 16 / 14 | 4 / 4 | L1a G1, CF G3 |
| `trigger_exit_v1` Health on fallback (NAV try fails) inline / stored | P | 5,649,574 / 4,680,828 | 4,369,493 / 4,032,057 | **20** (16+0+4) / **20** (16+0+4) | 16 / 16 | 4 / 4 | L1a try G1 + L1b G2 |
| `close_v1` | O | 2,074,988 | 2,646,883 | **15** (10+0+5) | 10 | 5 | receipt `prepare_close` |
| `collect_v1` | P | 2,026,175 | 2,635,191 | **9** (7+0+2) | 7 | 2 | job result X2, beneficiary X3 |
| `swap_deficit_v1` | P | 2,449,023 | 2,736,572 | **21** (16+0+5) | 16 | 5 | swap adapter + Aquarius pool |
| `settle_v1` (repay, fee, incentive, payout) | P (signs) | 2,684,825 | 2,613,558 | **28** (17+1+10) | 18 | 10 | market repay X4 |
| `add_v1` (fresh NAV) | O | 4,176,924 | 4,027,061 | **31** (23+0+8) | 23 | 8 | market borrow X4, NAV G1 |
| `repay_v1` | payer | 1,432,771 | 2,516,001 | **15** (11+0+4) | 11 | 4 | market repay X4 |
| `cancel_v1` PendingFund (owner) | O | 1,043,395 | 1,308,023 | **12** (8+0+4) | 8 | 4 | — |
| `cancel_v1` Opening after expiry (receipt cancel + sync) | E | 3,296,916 | 2,830,163 | **18** (12+0+6) | 12 | 6 | receipt `cancel` |
| `settle_v1` refund | P (signs) | 2,298,380 | 2,613,309 | **23** (15+0+8) | 15 | 8 | market repay X4 |
| `health_v1` (view) | — | 1,963,642 | 2,695,811 | **11** (11+0+0) | 11 | 0 | G1, X4 |

Mock receipt stages were also run (convert 7, mint 11, invest 11, checkpoint_nav 5, withdraw 19 entries); they are **not
representative** and are replaced by the cited rows below. Every measured row is ≤ 32 entries, ≤ 23 keys, ≤ 10 writes,
≤ 5.7M CPU and ≤ 4.4 MB memory, i.e. far inside both the internal and the live limits **on mocks**.

#### 7.2 Projected per transaction on real dependencies (estimate; replace when the group contracts exist)

Projection = measured row + Δ for each real subtree, without subtracting key overlap (conservative). Δ sources:
L1a real 18 entries / 17 keys / 15.71M vs mock 5 / 4 / 1.00M → **+13 / +13 / +14.7M** (DESIGN §5.2);
L1b real 13 / 12 / 8.95M vs mock 5 / 4 / 1.03M → **+8 / +8 / +7.9M**; failed NAV attempt inside `try` real GF0 17 / 17 / 15.64M
→ **+14 / +14 / +14.6M**; `prepare_close` doc 27 C1 13 / 10 / 2.72M → **+8 / +6 / +1.9M**; pair open + fund doc 27 O1 + O2
(16 + 17 entries, 7.40M + 3.75M) → **+12 / +10 / +10M**; beneficiary assign/release with reward settle ≈ group L5 25 / 15 / 3.1M
→ **+20 / +13 / +3M**; real receipt code load **+0.43M CPU, +79 KB** per load (DESIGN §5.4). **Not measured anywhere yet
(estimates):** margin-namespace borrow on the real USDC market (existing `receipt-vault` code, so measurable now and not
group-dependent; skipped here because the worktree lacks the OpenZeppelin submodule it builds against) ≈ **+40 / +33 / +15M** (the market contributes 36 keys to
the spike's S1a borrow and 35 to group L3); repay ≈ **+35 / +30 / +12M**; margin debt view ≈ **+6 / +6 / +2M**; adapter +
Aquarius pool swap ≈ **+25 / +20 / +12M** (doc 27 C3 is 32 entries / 23.8M including its receipt work).

| Transaction | Measured entries (mocks) | Projected entries | Projected keys | Projected CPU | vs 100 / 100 / 50 / 97M | Basis |
|---|---:|---:|---:|---:|---|---|
| `open_v1` | 32 | ~72 | ~56 | ~19M | fits; **kill line 85** | measured 32 + borrow |
| `fund_v1` | 21 | ~33 | ~23 | ~15M | fits | measured 21 + O1/O2 |
| receipt `convert_open` (O3) | cited | 30 (pilot 28) | 22 (19) | 31.2M (36.1M) | fits | doc 27 accepted / spike pilot bytes |
| receipt `mint_cash` (O4) | cited | 69 (pilot 44) | 50 (32) | 72.1M (18.4M) | fits | same |
| receipt `invest_cash` (O5) | cited | 61 (pilot 56 + finalise 26) | 43 (39) | **95.1M, cold 95.2M** (57.7M) | **tightest: 1.8M under the 97M keeper stop** | same; unchanged by leverage |
| `sync_open_v1` | 14 | ~34 | ~23 | ~6M | fits | measured 14 + X2/X3 |
| `set_exit_v1` | 14 | ~20 | ~18 | ~4M | fits | + market view |
| `arm_exit_v1` inline | 12 | ~31 | ~29 | ~20M | fits | + L1a + view |
| `trigger_exit_v1` TP / Health inline | 20 | ~47 | ~41 | ~24M | fits | + L1a + C1 + view |
| `trigger_exit_v1` Health on fallback inline | 20 | ~56 | ~50 | ~33M | fits | + failed L1a + L1b + C1 + view |
| receipt `withdraw_close` (C2a) | cited | 78 | 52 | 78.7M | fits (accepted) | doc 27; unchanged |
| `collect_v1` | 9 | ~29 | ~20 | ~6M | fits | + X2/X3 release |
| `swap_deficit_v1` | 21 | ~46 | ~36 | ~15M | fits | + adapter/pool swap |
| `settle_v1` | 28 | ~63 | ~48 | ~15M | fits | + repay |
| `add_v1` | 31 | ~77 | ~62 | ~22M | fits; near kill line | + borrow + view |
| `repay_v1` | 15 | ~50 | ~41 | ~14M | fits | + repay |
| `cancel_v1` Opening (receipt cancel + sync) | 18 | ~38 | ~28 | ~8M | fits; doc 27 has no accepted cancel bound | + cancel + X2/X3 |
| **Rejected single-tx settle** (collect + swap + repay + payout) | 33 / 41 (pre-split run) | ~89 no swap / **~122 with swap** | — | ~35M | **OVER 100 with swap** | why settlement is split |

Against the live Mainnet limits every row is at most ~24% of 400M CPU, ~10% of memory, well under 400 footprint entries
and 200 writes; measured write bytes ≤ 3,676 of 132,096 and events ≤ 1 KB. The binding constraint is the internal 100-entry
limit, and only through real-dependency subtrees that do not exist yet. **Gate:** re-measure open, add and settle on the
real group USDC market margin path before anything else; stop if any exceeds 85 entries.


---

## 8. Estimate and order of work

Assumptions as in DESIGN §6: one engineer who knows the codebase, with AI assistance; existing harnesses reused; audit
calendar, approval waits and on-chain fees excluded. Estimates, not measurements. **Excludes** the CL collateral group
itself (37 / 59 / 93, DESIGN §6) and the stage-1 1× receipt/vault build (doc 27: 31–48 + 6–10 contingency), both of which
this depends on.

| # | Work | Low | Likely | High | Depends on |
|---|---|---:|---:|---:|---|
| 1 | Freeze the leverage ABI and the X1–X5 interfaces with the group engineer (names, result records, custodian beneficiary, margin namespace config) | 2 | 3 | 5 | user decisions §9 |
| 2 | Vault/receipt: X1 pair-funded open (either swap direction), X2 typed results kept in StgV2, X3 custodian-labelled beneficiary; re-measure O2/O3. **Kill point:** the vault's 7,642-byte code headroom (DESIGN §6 item 1) | 4 | 7 | 12 | 1, group vault v2 |
| 3 | `cl-leveraged` production contract from the prototype: reentry guard, upgrade timelock/freeze, terms hashing, typed events, TTL policy, full error map | 4 | 6 | 9 | 1 |
| 4 | Real-dependency measurement: compiled real USDC market margin path, real StgV2 receipt, Aquarius swap, live-state rerun (gate: ≤ 85 entries) | 3 | 5 | 8 | 2, 3, group 2–3 |
| 5 | Keeper: open stages, TP arm/confirm loop, health monitor from simulated checkpoints, collect/swap/settle, resumable journal | 4 | 6 | 9 | 3; extends the group liquidator bot |
| 6 | Adversarial and mutation tests: pool push vs TP, CALI flat/stale, fallback griefing window, interest in Settling, bad debt, add-cancel, interrupted/expired stages, restoration | 4 | 6 | 10 | 3, 4 |
| 7 | Parameters: leverage LT vs sleeve CF, open buffer, incentive, fees, TP gap/delay, backtest worst moves, net return with AQUA −50% / −80% at L | 3 | 5 | 8 | group capacity tool |
| 8 | Qualification: signed / Core rows with real auth, security scan, review, runbook, upload simulation | 4 | 7 | 11 | all |
| | **Total** | **28** | **45** | **72** | |

Order of work: 0 user decisions → 1 → (2 ∥ 3) → 4 (stop if a row > 85 entries) → (5 ∥ 6 ∥ 7) → 8. With the group's
own work running in parallel on another engineer, the critical path is group vault v2 + receipt StgV2 → item 2 → item 4.


---

## 9. Decisions for the user

| ID | Decision | Proposal |
|---|---|---|
| U1 | Topology: **M** (`cl-leveraged` custodies shares and borrows in the USDC market's per-position margin namespace; it, not the group controller, enforces per-position health) vs **U** (user is the group account; no permissionless TP/stop) | M. It needs `set_margin_controller(cl-leveraged)` on the group USDC market, a review of that trust grant, and the market borrow cap as the aggregate bound |
| U2 | Pair-funded open (vault change X1) vs the frozen single-token open plus an extra USDC→PYUSD swap of the whole borrow | Pair-funded: zero swap at 2×, smaller exposure to the pool |
| U3 | Leverage liquidation threshold: the sleeve CF itself, or a separate lower LT for leveraged positions; open buffer b | Separate, ≤ sleeve CF, from backtests; b ≥ fallback haircut. No values proposed here |
| U4 | Take-profit kinds: `EquityAtLeast` (NAV checkpoint), `PriceAtLeast` (CALI), both; arm/confirm delay and window; minimum TP gap; USD-denominated TP (needs CAFJ, +2 keys) | Both kinds, USDC only; gap ≥ L × 5 bps with margin |
| U5 | Fees: open fee on borrowed notional (financed by the debt), close fee on repaid debt, exit incentive on Health/Stop/TP only, priority lender → fee → incentive; keeper XLM budget | As prototyped; values open |
| U6 | Bad debt: Defaulted with a recorded shortfall borne by USDC lenders, or a funded reserve/backstop | Recorded shortfall, no backstop, small caps first |
| U7 | Allow health exits on the cause-gated fallback checkpoint (inherits the group's out-of-band griefing window, DESIGN D3), or require a minimum out-of-band duration for leveraged positions | Allow, with the group's D3 mitigation applied identically |
| U8 | Owner may cancel own Stop/TP/voluntary closes before execution; Health exits only after expiry | Accept |
| U9 | Positions with a pending `add_v1` are not health-exitable until it finishes or expires (≤ deadline) | Accept with a short add deadline, or forbid adds below an HF floor |
| U10 | Command deadline horizon (receipt test prototype: 3,600 s) and TP confirm window | ≤ 1,800 s open, 600 s confirm window |
| U11 | Single-pass leverage vs explicit k-round borrow loops | Single pass (same bound, fewer transactions); `add_v1` for later rounds |
| U12 | Whether leveraged borrowing shares the group market's borrow cap with plain group borrowers or gets its own market | Own cap accounting: add an aggregate leveraged-debt cap in `cl-leveraged` (not prototyped) |
| U13 | Behaviour when the group sets the sleeve CF to 0 (`emergency_disable_collateral`): every indebted leveraged position becomes Health-exitable (prototyped), or freeze to repay-only | Exit-eligible: positions self-unwind in kind with their own assets; no seizure discount is involved |


## 10. User decisions recorded (22 Nov 2026)

Owner, in chat: "Leveraged U1 to U13: all as recommended. Nothing deploys without a ceremony list."

- U1–U13 are accepted as the **Recommendation** column of the §9 table, unchanged.
- No leveraged contract, upgrade, wiring call or parameter goes to Mainnet without its own numbered ceremony list. Each step in that list carries its command, Ledger account, expected output, read-back, failure action and cancel path, and is simulated against live Mainnet first.
- Values left open by the recommendations still need their own decision before that ceremony list: U3 thresholds from backtests, U5 fee values, U12 aggregate cap.
