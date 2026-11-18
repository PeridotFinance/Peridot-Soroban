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
`settle_v1` repays the margin debt from that mixture **before** anything reaches the user. No health-checked redeem
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
keeper (anyone) ──fund_v1/sync_open_v1/arm_exit_v1/trigger_exit_v1/settle_v1/cancel_v1(after deadline)──▶ cl-leveraged
keeper (anyone) ──convert_open/mint_cash/invest_cash/withdraw_close/convert_close/cancel(after deadline)──▶ group receipt

cl-leveraged ──prepare_open_pair/fund_open/prepare_close/cancel/stage1_job──────────▶ group receipt (StgV2 sleeve class)
             ──nav_checkpoint/fallback_checkpoint/checkpoint_nav/checkpoint_fallback─▶ group receipt (DESIGN §2, §4)
             ──get_market_cf/get_liquidation_incentive_scaled/is_borrow_paused──────▶ group controller
             ──borrow_for_margin_to_controller/repay_for_margin/get_margin_borrow_balance─▶ group USDC market
             ──lastprice(Stellar(PYUSD))─────────────────────────────────────────────▶ Reflector CALI (USDC base)
             ──swap_pool (only when the USDC leg cannot cover debt)────────────────▶ swap adapter → Aquarius pool
             ──transfer──────────────────────────────────────────────────────────────▶ PYUSD / USDC SACs
```

`cl-leveraged` is **never** inside the heavy receipt rows (convert, mint, invest, withdraw). Its calls are separate,
light transactions that read the command record (`stage1_job(id).result`). That keeps the 95.1M-CPU invest row (doc 27
O5) and the 78-entry withdraw row (C2a) unchanged.

---

## 2. Refined ABI (prototype names; versioned `_v1`)

Types (prototype `src/types.rs`):

- `LevConfigV1`: bound addresses (receipt, group controller, USDC market, swap adapter, pool, CALI oracle, PYUSD, USDC,
  treasury) and `LevRiskV1`. Set once at `initialize`; risk changes go through a 24 h timelocked proposal
  (`propose_risk_v1` / `execute_risk_v1`) and bump `revision`. **No defaults in code.**
- `LevRiskV1`: `open_buffer_bps` (b), `max_ckpt_age_secs` (group DESIGN: 60), `max_oracle_age_secs` (600),
  `max_depeg_bps` (100), `max_slippage_bps`, `open_fee_bps`, `close_fee_bps`, `exit_incentive_bps`, `tp_min_gap_bps`,
  `tp_confirm_delay_secs`, `tp_confirm_window_secs`, `max_deadline_secs`, `new_risk_enabled`, `revision`.
- `OpenLevV1{equity0, borrow1, swap_in, swap_dir, min_swap, min_shares, reserve0, deadline, max_ltv_bps, exit}`.
- `ExitPolicyV1{take_profit: Option<TakeProfitV1>, stop_hf_bps: Option<u32>}`;
  `TakeProfitV1 = EquityAtLeast(u128 USDC) | PriceAtLeast(u128 PYUSD/USDC, 14 dp)`.
- `PositionV1{owner, phase, cmd, shares, escrow0, escrow1, reserve0, equity_in, exit, armed, reason, revision}`.
- `LevPhaseV1`: `PendingFund`, `Opening`, `Active`, `Closing`, `Settling`, `Closed`, `Refunded`, `Defaulted`.

Entry points (O = owner, P = permissionless, E = anyone strictly after the stored deadline, A = admin):

| Entry point | Caller | Required phase | Effect / next phase |
|---|---|---|---|
| `initialize(admin, cfg)` | A | — | Store config once |
| `open_v1(owner, terms) -> pid` | O | — | Pull `equity0 + reserve0` PYUSD; borrow `borrow1` USDC into escrow (`borrow_for_margin_to_controller(pid)`); admission (§4); charge open fee; → PendingFund |
| `add_v1(owner, pid, terms)` | O | Active | Same as open for the increment; health on fresh NAV checkpoint over existing shares + new escrow; → PendingFund |
| `fund_v1(pid) -> cmd` | P | PendingFund | `prepare_open_pair` + `fund_open` on the receipt, owner = `cl-leveraged`, exact nested transfer authorized as current contract; escrow → command; → Opening |
| (receipt) `convert_open`, `mint_cash`, `invest_cash` | P | — | Unchanged staged rows; `cl-leveraged` not invoked |
| `sync_open_v1(pid)` | P | Opening | Read `stage1_job(cmd)`: Invested → add shares, assign beneficiary, → Active. Cancelled before mint → mixture returned to escrow → Settling (repay, refund). Cancelled after mint → shares released → Active |
| `set_exit_v1(owner, pid, exit)` | O | PendingFund..Active | Replace TP / stop; TP must clear `tp_min_gap_bps` above current equity; disarms |
| `arm_exit_v1(pid)` | P | Active | TP condition true on a fresh checkpoint → record `armed{ts, ckpt_ts}` |
| `trigger_exit_v1(pid) -> reason` | P | Active | Reason `Health` (HF < 1, NAV else fallback), `Stop` (HF < stop), `TakeProfit` (armed ≥ delay, ≤ window, fresh later checkpoint, still true). `prepare_close(cl-leveraged, shares, InKind)` → Closing |
| `close_v1(owner, pid, deadline)` | O | Active | Voluntary close, same command → Closing |
| (receipt) `withdraw_close` | P | — | Unchanged C2a row: in-kind mixture paid to `cl-leveraged` |
| `settle_v1(pid)` | P | Closing / Settling | Read `stage1_job(cmd).result` amounts; repay debt from USDC leg; swap PYUSD→USDC only for a deficit (oracle min-out); fees, incentive; residual in kind to owner → Closed, or → Defaulted with recorded shortfall |
| `cancel_v1(pid)` | O, or E | PendingFund / Opening / Closing | PendingFund: repay from escrow, refund → Refunded. Opening/Closing: receipt `cancel(cmd)` (fixed owner = `cl-leveraged`) → Opening is then synced; Closing → back to Active |
| `repay_v1(payer, pid, amount)` | payer | any with debt | External top-up / deleverage; payer authorizes |
| `position_v1(pid)`, `health_v1(pid)`, `config_v1()` | read | — | Views |

Errors are a reserved block **1800–1829** (`src/types.rs`), checked against 1700–1718 of the stage-1 freeze.

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
                    Closed / Defaulted ◀── settle_v1 (P) ◀── cmd Paid ◀── withdraw_close (P) ◀── Closing[cmd CloseLocked]
                                                                                cancel_v1 → Active (cmd Cancelled, shares unlocked)
```

Doc 23 mapping: PendingFund = O0/O1 (cash and debt committed together, held by the margin escrow instead of the vault);
Opening = O1–O4 inside the receipt; `sync_open_v1` = O5 delivery + A activation (no separate deliver: shares are already
owned by `cl-leveraged`); Closing = C0; `withdraw_close` = C1; `settle_v1` = C2 + R0 + R1 in one light transaction.

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
`c = c_withdraw + c_swap + incentive + close_fee`, then loses up to `Δ` of value during the exit window (§6: 3 transactions
after trigger, ≈ 3 ledgers + keeper latency). The debt is covered iff `CF ≤ (1 − c)(1 − Δ)`. On the fallback path the
value is already `≈ 97.9%` of NAV (DESIGN §2.2), so the fallback trigger is conservative by `h`.

Illustration only (test arithmetic, **not** proposed values): with `b = 2%`, CF 50% → `x_open` 49%, `L_max` 1.96;
CF 70% → 68.6%, 3.18; CF 80% → 78.4%, 4.63. The group has not chosen a sleeve CF; the spike used 50% as a fixture value.
The spec COLLATERAL-BORROWING-SPEC §7 requires the CF to come from backtested worst moves and funded exit capacity.

**Swap-free unwind at L ≤ 2.** In the middle of the range the mixture is about half USDC, so the USDC leg covers debt iff
`D ≤ V/2`, i.e. `L ≤ 2`. At the lower band edge the position is all PYUSD and `settle_v1` must swap; the swap row is
measured separately. Leverage above 2 always needs the swap on exit.

---

## 5. Take-profit, stop and health exit

**Trigger must only change state.** `arm_exit_v1` / `trigger_exit_v1` read the stored NAV checkpoint (or refresh it
inline through `checkpoint_nav()`, no auth) and the oracle, never pool spot or a pool quote, and their only effect is to
lock the shares into a close command. Execution protection is separate: the in-kind withdrawal needs no price, and the
only swap (`settle_v1`, deficit only) is bounded by `p_CALI · (1 − max_slippage)`.

| Kind | Condition | Price source | Persistence |
|---|---|---|---|
| Health (protocol) | `HF < 1` | NAV checkpoint ≤ 60 s, else fallback checkpoint ≤ 60 s (cause-gated by the group) | None: immediate |
| Stop (owner) | `HF < stop_hf_bps / 1e4` (stop > 1) | NAV checkpoint ≤ 60 s only | None |
| TP `EquityAtLeast(T)` | `V − D ≥ T` | NAV checkpoint ≤ 60 s | Armed, then confirmed on a **later** checkpoint ≥ `tp_confirm_delay_secs` and ≤ `tp_confirm_window_secs` after arming |
| TP `PriceAtLeast(P)` | `p_CALI ≥ P` | CALI only, age ≤ `max_oracle_age_secs` | Same arm/confirm |

**Why a pool push cannot fire a TP.** Inside the guarded band (pool within 50 bps of oracle, tick in [−20, 20)) a push
inflates the NAV only to second order: DESIGN §1.3 bounds it at about 0.05% of one leg at δ = 0.2%. Equity amplifies it by
`L`. `set_exit_v1` therefore rejects an equity TP closer than `tp_min_gap_bps` to the current equity, and the gap must be
chosen ≥ `L × 5 bps` with margin (test value 50 bps). A push beyond the band makes `checkpoint_nav` revert, so no fresh
checkpoint exists and nothing triggers. The arm/confirm rule makes the attacker hold the push against arbitrage across
two checkpoints. `PriceAtLeast` is pool-independent; its residual risk is the CALI feed itself (DESIGN §1.3: flat,
re-timestamps; DEX-trade driven). The only harm of an early TP is an early, user-chosen exit at oracle-bounded prices.

**Granularity.** Equity TP thresholds are honoured to about `L · 5 bps` of position value plus the NAV checkpoint
rounding (rate is 1e6-scaled per unit: ≤ 1 µUSDC per unit). AQUA is not in NAV: a TP on "profit including AQUA" is not
expressible; AQUA keeps accruing to the owner as beneficiary and is claimed separately.

---

## 6. Transactions per journey, triggers and fees

| Journey | Transactions (who) | Count |
|---|---|---:|
| Open | `open_v1` (O) → `fund_v1` (P) → `convert_open` → `mint_cash` → `invest_cash` (P, receipt; pilot bytes add `stage_checkpoint_investment` + `stage_finalise`) → `sync_open_v1` (P) | 6 (8 on pilot bytes) |
| Add | `add_v1` (O, may refresh NAV inline) → same as open from `fund_v1` | 6 |
| Take-profit | `arm_exit_v1` (P, refresh inline) → ≥ delay → `trigger_exit_v1` (P, refresh inline) → `withdraw_close` (P) → `settle_v1` (P) | 4 |
| Health / stop exit | `trigger_exit_v1` (P; NAV, else `checkpoint_fallback` inline for Health) → `withdraw_close` → `settle_v1` | 3 |
| Voluntary close | `close_v1` (O) → `withdraw_close` → `settle_v1` | 3 |
| Refund, before fund | `cancel_v1` (O, or E) | 1 |
| Refund, failed stage | receipt rejects → `cancel_v1` (O / E: receipt `cancel`) → `sync_open_v1` → `settle_v1` | 3 |

**Fees** (all symbols; decision U5): open fee `open_fee_bps` on borrowed notional, taken from equity at `open_v1` to
the treasury; close fee `close_fee_bps` on repaid debt at `settle_v1`; exit incentive `exit_incentive_bps` of repaid debt
to the `settle_v1` caller on Health/Stop/TP exits only (voluntary and refund paths pay none); receipt execution reserve
`reserve0` (doc 27) prepaid by the owner and refunded through the command. Keeper XLM fees are an operating cost, as for
the 1× keeper. Interest is the USDC market's (STATIC 8% APR proposed by the group, D8).

**Failure / refund paths.**

- `fund_v1` rejected by the receipt (pause, cap, admission): the whole transaction reverts; escrow and debt unchanged;
  keeper retries or the owner (anytime) / anyone (after deadline) runs `cancel_v1`, which repays from escrowed USDC
  (swapping PYUSD for any accrued-interest deficit) and refunds the rest.
- A receipt stage fails or expires: only `cancel` on the receipt can move funds, always back to `cl-leveraged`.
  `sync_open_v1` reads the `Released` result: mixture → Settling → `settle_v1` repays and refunds; locked shares → Active.
- `settle_v1` swap fails its oracle minimum (pool pushed or illiquid): it reverts and is retried. The owner or anyone
  may `repay_v1` the deficit to settle without a swap. Debt is never forgiven by a failed swap.
- Mixture worth less than debt after swapping everything: repay all, record `shortfall`, → Defaulted (lender loss
  visible; no protocol backstop is assumed — decision U6).
- Oracle stale (> 600 s): health uses only the fallback (which also needs CALI) → no trigger; position is repay-only
  (group D4). Open/add/TP are blocked.

---

## 7. Resource budget per transaction

Limits: internal 100 SDK entries (memory reads + disk reads + writes), 100 unique keys, 50 writes, 100M CPU (keeper stop
97M), 41,943,040 memory. Live Mainnet (spike, ledger 65527300): 400M CPU, 41,943,040 mem, 400 footprint, 200 disk reads,
200 writes, 200,000 read bytes, 132,096 write bytes, 132,096 tx bytes.

The measured table is in §7.1 (filled from `results.json`). Rows that run unchanged receipt code cite the accepted rows.

### 7.1 Measured (local host, mocks) and cited rows

MEASURED_TABLE_PLACEHOLDER

---

## 8. Estimate and order of work

ESTIMATE_PLACEHOLDER

---

## 9. Decisions for the user

DECISIONS_PLACEHOLDER
