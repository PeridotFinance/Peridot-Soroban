# CL collateral group: design and measurements (18 November 2026)

## Scope

Concentrated-liquidity PYUSD vault units used as collateral to borrow USDC, in a **fresh, isolated
controller**. This is topology T2 of `prototypes/cl-staged-market-integration/COLLATERAL-BORROWING-SPEC.md` §1.

- **Accepted conditions:**
  - (a) the value checkpoint is ≤ 60 s old;
  - (b) the AQUA claim happens outside seizure;
  - (c) no account mixes CL collateral with cash collateral.
- **Refused:** raising the internal 100-entry limit.

Nothing was signed, sent, deployed or upgraded. Network use was read-only `simulateTransaction` and
`getLedgerEntries` on `https://soroban-rpc.mainnet.stellar.gateway.fm`. No key, keychain, secret or env
var was read.

Evidence labels:

- **[measured]**: local host rows in `rows-table.md` / `results.json` (this folder).
- **[src]**: source file and line.
- **[rpc]**: read-only RPC evidence file in this folder.
- **[analysis]**: my reasoning, not measured.

Two group stacks were measured:

- **JRM**: the USDC market uses the release jump-rate model, as the spike did.
- **STATIC**: a static 8% APR and no JRM. This is the cash-borrowing release choice
  (`research/cash-borrowing-upgrade-20261118/REPORT.md` §C, "Jump-rate model: rejected";
  `research/cash-borrowing-activation-20261117/params.proposed.json` USDC `borrowRateScaled` 80000).

**STATIC is the proposed configuration.**

---

## 0. Answers in one page

| # | Question | Answer |
|---|---|---|
| 1 | What prices PYUSD? | Reflector Stellar-DEX feed `CALI…PLE6M`, `lastprice(Stellar(PYUSD))`, quoted **in USDC** (its base). It is used **only inside the sleeve checkpoint**. The sleeve reports its value in **USDC**, the only debt asset, so the controller prices one token, USDC. It uses the external feed `CAFJ…34DLN` (`Other(USDC)`), as cash borrowing does. Guards: oracle age ≤ 600 s; \|PYUSD/USDC − 1\| ≤ 100 bps; pool spot within 50 bps of the oracle; pool tick inside the position's range [−20, 20). See §1. |
| 2 | Liquidation when the checkpoint cannot refresh | A liquidation-only **fallback checkpoint**. The position is valued as if it were entirely PYUSD (the cheaper asset) at the lower band-edge price, at min(oracle, edge price) × (1 − 2%), plus booked idle. No fees, no pool quote. It is written only for a named cause, lives ≤ 60 s, and is read only by liquidation. Stage L1b: **13 entries, 8.95M CPU, 3.99 MB, 1 write, 0 auth** (pool cause); 11 entries (depeg cause). Proven lower bound without the haircut. Fallback = **97.87% of NAV** at the capture. See §2. |
| 3 | Who accrues AQUA on collateral units | **The depositor (borrower)**, on all units including those securing the loan, until a seizure is synced. Entering a market and borrowing are controller-side flags; the vault beneficiary does not change. Seizure: L3 moves only the receipt-side unit mirror; L5 settles both holders at the stored index **without claiming**, then moves the beneficiary. Model test: borrower 69, other holder 75, liquidator 6 of 150 claimed. See §3. |
| 4 | Checkpoint age | `checkpoint_nav()` and `checkpoint_fallback()` take no arguments and require no auth (measured `auth` = 0). A checkpoint is valid iff `now − ts ≤ 60`. Borrow and redeem read the NAV checkpoint only. Liquidation reads NAV, else the fallback. Each 60 s ok / 61 s revert pair is measured. See §4. |
| 5 | Headroom | Liquidation is split into five stages. The largest, L3 (repay + seize), is **80 entries (61 + 2 + 17), 63 keys, 17 writes, 31.2M CPU, 4.34 MB** on STATIC, and 87 on JRM. It is identical in every variation row; liquidation fee on gives 84 (STATIC) or 91 (JRM). The cold path is unreachable: L3 reads only a cached price and otherwise reverts at 36 entries. **Every STATIC stage is ≤ 85 entries.** Without the split (inline) it is 100 warm / 105 cold. See §5. |
| 6 | Order and estimate | Vault v2 reassign first (the code-size kill point), then receipt StgV2, controller, bot, capacity, qualification. **37 / 59 / 93 engineer-days** (low / likely / high). See §6. |

---

## 1. Price path (question 1)

### 1.1 What the feeds contain [rpc]

Source: `evidence-oracle-probe.json`, ledger 65541689, 18 Nov 09:34 UTC, `simulateTransaction` only.

| Feed | Base | Assets | PYUSD | USDC |
|---|---|---:|---|---|
| Stellar DEX `CALI2BYU…PLE6M` | `Stellar(USDC SAC)` | 52 | `Stellar(CCCRWH6Q…HGU2)` is listed; `lastprice` 0.99979563 (ts 1794994200) | It is the base: `lastprice` returns exactly 1.0 |
| External CEX/DEX `CAFJZQWS…34DLN` | `Other(USD)` | 16 | **Absent**: `Other(PYUSD)` and `Stellar(PYUSD)` return `None` | `Other(USDC)` 1.00037564 |

- Both feeds have 14 decimals and 300 s resolution.
- There is no PYUSD/USD feed.
- Cash borrowing uses CAFJ and leaves PYUSD unmapped (`research/cash-borrowing-upgrade-20261118/REPORT.md`;
  `research/cash-borrowing-activation-20261117/PLAN.md` §2, option A).

### 1.2 Proposed path

```
sleeve checkpoint_nav():                                       group controller:
  nav_pyusd = vault.get_asset_amounts_per_shares(total_supply)   price(USDC) = CAFJ Other(USDC)
  p = CALI lastprice(Stellar(PYUSD))      (USDC per PYUSD)       (CALI in the measured rows: USDC is its
  guards (1.3)                                                    base; the footprint is identical [rpc])
  value_usdc = nav_pyusd × p                                      collateral token = debt token = USDC
  rate = value_usdc × 1e6 / total_supply   (vault-wide)
```

- **USDC numeraire.** The sleeve's `get_underlying_token()` returns USDC.
  - Collateral and debt are both USDC, so USDC/USD cancels in the health and seize maths.
  - Liquidation needs one price-cache entry, not two (the spike priced PYUSD and USDC).
  - A USDC depeg leaves USDC-denominated health unchanged.
- **Vault-wide rate.** The spike's `checkpoint_nav(reference_owner)` let the caller pick an owner whose
  rounding set everyone's rate. The group checkpoint takes no argument and divides whole-vault NAV by total
  supply (`prototypes/cl-collateral-group/sleeve/src/lib.rs:216`).
- **Feed footprint [rpc].** Source: `evidence-oracle-footprint.json`.
  - `lastprice`, `decimals` and `resolution` each read 2 keys (instance + code) on both feeds.
  - CAFJ costs about 0.3M CPU less per call.
  - Swapping the controller oracle from CALI (measured) to CAFJ (proposed) therefore changes **0 entries**.
    It changes only the CPU of the L2 and cold rows.

### 1.3 Guards

Each guard makes `checkpoint_nav` revert. The last good value is never reused for borrowing.

| Guard | Rule | Code | Row |
|---|---|---|---|
| Oracle age | CALI timestamp ≤ now, and now − ts ≤ 600 s (k = 2 × 300 s) | `sleeve/src/lib.rs:97` | — |
| Depeg | \|p − 1\| ≤ 100 bps | `:105` | GF1b: sleeve error #2 DepegGuard |
| Pool band | pool spot (`get_slot0`) within 50 bps of p, **and** pool tick ∈ [−20, 20) | `:107` | GF1: sleeve error #3 PoolOutOfBand |
| Venue | `get_asset_amounts_per_shares` traps on a failed `estimate_withdraw_position` or a stale primary price | vault `vault.rs:452–458` | — |

**The band check lives in the checkpoint.** The vault's own valuation view applies **no pool/oracle band**:
`prototypes/cl-pyusd-vault-b/vault.rs:452–458` calls `stage_primary` and `stage_live_nav_at`, and
`stage_band` (`:172`) is used only at admission.

[analysis] Inside the range, pushing the pool inflates the oracle-valued holdings only to second order:
about ½·x′·δ², or ≈ 0.05% of one leg at δ = 0.2%.

**CALI limitation [rpc + analysis].**
- PYUSD read 0.99981 on 17 Nov and 0.99980 on 18 Nov.
- The feed re-timestamps every round, so a fresh timestamp does not prove price discovery.
- The pool band is the second, independent source.
- An off-chain CEX watcher plus the guardian borrow pause should back it (D2).

---

## 2. Liquidation when the checkpoint cannot refresh (question 2)

### 2.1 Valuation (`checkpoint_fallback`, `sleeve/src/lib.rs:236`)

Inputs: position liquidity L over the vault's fixed band [s_lo, s_hi] = [√1.0001^−20, √1.0001^20]. These are
the vault's own `stage_fair_lp` constants.

```
a0   = L·(1/s_lo − 1/s_hi)                  position entirely PYUSD (price at or below the lower edge)
P_lo = s_lo² ≈ 0.998001 USDC per PYUSD       lower band-edge price
pf   = min(p_CALI, P_lo) × (1 − haircut)      haircut 200 bps (D3)
LP   = a0 × pf
idle = booked idle PYUSD × pf + booked idle USDC   (vault get_idle / get_other_idle_value)
rate_fb = (LP + idle) × 1e6 / total_supply
```

**Lower-bound proof [analysis, checked numerically].**
- An LP position's value is increasing in price.
- At any in-range sqrt price s, value(s) − a0·P_lo = L(s − s_lo)(2 − (s + s_lo)/s_hi) ≥ 0.
- Below the band, the position is a0 PYUSD worth a0·p.
- So a0·min(p, P_lo) never exceeds the true value, even with **no** haircut.

Measured at the capture: un-haircut 99.86% of NAV, haircut 97.87%.

An earlier draft valued a0 at p itself. That overstates an in-range position by L(s − s_lo)²/s_lo, and it
measured 4 bps *above* NAV before the haircut. It was replaced.

**Properties:**

- **No pool quote.** L, the ticks and the booked idle balances change only through vault operations, so
  pushing the pool cannot change the fallback **value**. The pool is read only to establish the cause.
- **Cause required.** The call reverts `NavRefreshable` (#4) unless one of these holds: the depeg guard
  trips, the pool is out of band or range, or the vault valuation fails (GF0).
  - A stopped keeper is not a cause.
  - Anyone, including the borrower, can refresh NAV, and a fresh NAV checkpoint always takes precedence
    (`liq_rate`, `:122`).
- **Residual griefing window [analysis] (D3).** Pushing the pool out of band does **switch** valuation from
  NAV to the fallback.
  - Borrowers whose health lies within the haircut (≈ 2.1%) of the liquidation threshold can then be
    liquidated and the incentive collected.
  - The attacker pays to hold the pool out of band against arbitrage, but only for the L1b + L2 + L3
    sequence.
  - Mitigations: a smaller haircut, a minimum out-of-band duration (two fallback checkpoints ≥ N ledgers
    apart), or a lower liquidation incentive on the fallback path.
- **Liquidation only.**
  - Borrow and redeem read `nav_rate` (`:120`) only. In GF2 a borrow with only a fallback checkpoint reverts.
  - The controller calls `get_liquidation_account_snapshot` and `get_liquidation_snapshot` only from
    `liquidate_internal` (patches G3/G4, §7).
- **Oracle also stale.** The fallback reverts `OracleStale` (#5), and the account is repay-only. A governed,
  timelocked recovery rate is the only further option (D4).
- **Price choice [analysis].**
  - min(CALI, pool spot) would detect a depeg on a flat CALI.
  - It would also let a pool-pusher liquidate healthy borrowers at a depressed value plus the 8% incentive.
  - CALI only is proposed (D2).

### 2.2 Measured [measured, STATIC; JRM in brackets]

| Row | Stage | CPU | Mem | Entries (m+d+w) | Keys | Writes | Auth |
|---|---|---:|---:|---|---:|---:|---:|
| GF3/GF4/GF7 | L1b, cause = pool out of band (reads slot0); cold = warm | 8,945,448 | 3,989,326 | **13** (12+0+1) | 12 | 1 | 0 |
| GF5 | L1b, cause = depeg (no pool read) | 7,355,663 | 3,669,045 | **11** (10+0+1) | 10 | 1 | 0 |
| GF0 | L1b refused while NAV is refreshable (#4) | 15,642,010 | 5,868,381 | 17 (17+0+0) | 17 | 0 | 0 |
| GF3 / GF4 | L3 on the fallback, warm / cold via L2 | 31,391,285 / 31,370,869 [33.28M] | 4,344,328 | **80** (61+2+17) [87] | 63 [70] | 17 | 1 |

Values at the capture:
- NAV rate 999,816; fallback 978,472 (1e6 = 1 USDC per unit).
- The same 100 USDC repay seized 1,103,761,783 units on the fallback and 1,080,198,756 on NAV.

The causes were emulated by setting `band_bps` / `depeg_bps` to 0. No real out-of-band pool or depeg was
produced.

---

## 3. AQUA on collateral units and at seizure (question 3)

### 3.1 Who accrues [src]

- The vault index advances **only on a successful claim**. `rewards::checkpoint`
  (`contracts/cl-share-vault/src/rewards.rs:134–195`) adds `delta × SCALE / total_shares` at `:170–176`.
- Each holder's share is computed by `settle` (`rewards.rs:108–122`) on `custody::effective(a)`
  (`custody.rs:76–81`): direct shares, which are 0 after binding, plus `Beneficial(a)`. The staged vault
  labels custody shares by beneficiary (`prototypes/cl-pyusd-vault-b/base-custody-append.rs`).
- `enter_market` writes only the controller's `UserMarkets(user)` (`simple-peridottroller/src/contract.rs:221`).
  A borrow touches only the lending market and the controller. Neither moves a unit or changes `Beneficial`.
- **Therefore the depositor accrues AQUA on every unit, including units securing a loan, until a seizure is
  synced (L5).**
- Between L3 and L5, claims still credit the borrower for the seized units. That favours the borrower.

**Invariant required between L3 and L5.** The vault still names the borrower as beneficiary of the seized
units. The receipt's unit mirror must therefore gate every burn, lock, close command and emergency exit, so
the borrower cannot move seized units. The liquidator cannot exit them before L5 either, because vault-side
availability is required.

### 3.2 Seizure without a claim

- **L3**, inside `liquidate`. The receipt moves its mirror borrower → liquidator and queues
  `Pending{from, to, n}` (`sleeve/src/lib.rs:308`). There is no vault call and no reward work.
- **L4**, a separate permissionless transaction. `vault.checkpoint_rewards()`, the real AQUA claim, indexes
  venue accrual at the **old** balances, so it goes to the borrower.
- **L5**, permissionless. `sync_seized(id)` (`:330`) runs the vault-side **settle-only reassign** `do_sync`
  (`:152`):
  1. check that the borrower has n unlocked units;
  2. `settle(from)` and `settle(to)` at the stored index;
  3. move `Beneficial`;
  4. `reset_debt` on both;
  5. `record_weight` on both.

  It makes **no `checkpoint()` call**.
- **Why a new vault entrypoint is needed.** The existing `custody::reassign(recovery=true)` cannot be used:
  it calls `rewards::recovery`, which claims first (`rewards.rs:203–206`), and that is the excluded 114–122
  path. The new vault entrypoint is `stage_seize_reassign`.
- **Gap rule (D6).** If L5 runs more than 60 s after the last successful claim:
  - it opens a reward boundary (`Boundary`, `Open`);
  - the next claim then goes to quarantine (`rewards.rs:159–168`);
  - release is by governed attribution (`rewards.rs:276–340`);
  - the vault needs a new instance field, `LastClaim`.

### 3.3 Model test [measured, model]

`harness/src/group.rs` `reward_semantics_model` passes. It runs on the group sleeve WASM, whose emulated
vault-side ledger copies the `settle`, `reset_debt` and `effective` arithmetic.

| Step | Borrower | Other holder | Liquidator | Quarantine |
|---|---:|---:|---:|---:|
| Claim 100 on 1000 / 1000 | 50 | 50 | 0 | 0 |
| L3: seize 400 (queued) | 50 | 50 | 0 | 0 |
| L4: claim 20 (old balances) | 60 | 60 | 0 | 0 |
| L5: fresh claim, no claim inside | 60 | 60 | 0 | 0 |
| Claim 30 on 600 / 1000 / 400 | **69** | **75** | **6** | 0 |
| Seize 100; L5 61 s after the last claim; claim 40 | 69 | 75 | 6 | **40** |

69 + 75 + 6 = 150 claimed, so rewards are conserved. The real `stage_seize_reassign` must reproduce this
table on compiled bytes.

---

## 4. Checkpoint age policy (question 4)

**Specification.**
- `MAX_AGE = 60` s is a receipt instance parameter.
- A checkpoint `{rate, ts, ledger}` is valid iff `ledger_timestamp − ts ≤ 60`.
- `checkpoint_nav()` and `checkpoint_fallback()` take no arguments and call no `require_auth`.
  - Measured: every L1a, L1b, L2, L4 and L5 row records 0 required authorizations (`auth` column).
  - L3 requires exactly 1: the liquidator.

| Action | Reads | When stale | 60 s | 61 s |
|---|---|---|---|---|
| Borrow: market → controller `hypothetical_liquidity_with_hint` → sleeve `get_account_snapshot` | NAV checkpoint only | Rate 0 on a CF > 0 market with units. The controller marks the account `indeterminate` (patched copy `contract.rs:3465–3482`) and returns (0, MAX); the market panics | GB4 ok | GB5 reverts (trap in the USDC market). GB0 (no checkpoint) and GF2 (fallback only) also revert |
| Redeem / prepare-close of an entered owner with debt | NAV checkpoint, then the controller's `get_borrows_excl`, `get_collateral_excl_usd`, CF and price | Sleeve error #1 `NavStale` | GR3 ok | GR4 #1. GR5 (would leave a shortfall) #6 |
| Liquidate | `get_liquidation_*`: NAV if ≤ 60 s, else fallback if ≤ 60 s, else rate 0 | Controller "health indeterminate" (`contract.rs:2739–2742`) | GL10 ok | GL11 reverts (no fallback). GF6 reverts (fallback at 61 s) |
| Repay | Nothing from the sleeve | Always works | — | — |

Controller and market reverts appear as traps, because panic strings are stripped from WASM. In each pair
the only difference is one extra second.

`no_checkpoint_views_model` covers only the never-written case.

No continuous 60 s keeper is needed:
- a borrower's frontend sends `checkpoint_nav` and then `borrow` or `redeem`;
- the liquidation bot sends its own L1a.

---

## 5. Headroom (question 5)

### 5.1 Where the 71–99 figures came from

- **Host:** local soroban-env-host / SDK **29.0.1** testutils. Pinned in
  `prototypes/cl-collateral-spike/harness/Cargo.toml`; `Cargo.lock` = 29.0.1.
- **State:** a Mainnet snapshot captured at **ledger 65527300** (close time 1794922542, protocol 29),
  refreshed at 65527301.
  - File: `.scratch/cl-collateral-spike/state/state.json`, sha256 `70cdf251…5a0b`.
  - Recorded in `research/cl-collateral-spike-20261117/results.json` (the `state` and `evidence_class` keys).
- **Evidence class:** not Core, not signed, `mock_all_auths`. CPU is re-priced with the live cost model:
  9 CPU coefficients differ.
- **Reproduced exactly here**, using the deployed controller bytes `847894a6…`:

  | Row | Entries |
  |---|---:|
  | REG S4a | 85 |
  | REG S4b | 71 |
  | REG S3g | 93 |
  | REG S3h | 85 |
  | REG S3g-t | 97 |
  | REG S3h-t | 89 |

  CPU is identical to the instruction.
- The "~91 / ~99" figures were S3h-t / S3g-t plus an *estimated* 2 writes. They were never measured.

### 5.2 Every stage, cold and warm [measured]

Definitions:
- "cold" = the controller `PriceCache` entries removed;
- "warm" = a cached price ≤ 600 s old;
- columns are CPU / mem / entries (m+d+w) / keys / writes / auth.

Stages are separate transactions: **L1a or L1b → L2 → L3 → L4 → L5**.

**Liquidation, STATIC (proposed)**

| Stage | Cold | Warm | Rows |
|---|---|---|---|
| L1a `checkpoint_nav` | 15,714,882 / 5,365,604 / **18** (17+0+1) / 17 / 1 / 0 | identical | STATIC GL6 / GL6w |
| L1b `checkpoint_fallback` | 8,945,448 / 3,989,326 / **13** (12+0+1) / 12 / 1 / 0 | identical | STATIC GF4 / GF3 |
| L2 `cache_price(USDC)`, always live | 4,267,930 / 2,004,198 / **17** (16+0+1) / 16 / 1 / 0 | 4,285,823 / 17 | STATIC GL6 / GL6w |
| L3 `liquidate` (cache-only) | **reverts by design**: 6,709,167 / 2,885,537 / 36 (34+1+1) / 35 / 1 | 31,221,985 / 4,337,412 / **80** (61+2+17) / 63 / 17 / 1 | STATIC GL2 / GL1, GL3, GL6 |
| L3, cache-only off (comparison) | 34,603,787 / 4,996,955 / **85** (65+2+18) / 67 / 18 / 1 | 80 | STATIC GL4 |
| L4 `vault.checkpoint_rewards` (real claim) | 9,857,079 / 4,663,119 / **29** (21+1+7) / 22 / 7 / 0 | identical | STATIC GL6 / GL6w |
| L5 `sync_seized`, fresh claim | 3,095,298 / 2,646,764 / **25** (15+0+10) / 15 / 10 / 0 | identical | STATIC GL6 / GL6w |
| L5, stale claim (opens a boundary) | 3,205,131 / **27** (16+0+11) / 16 / 11 / 0 | — | STATIC GL7 |
| L3 + L5 inline (no split) | 37,453,821 / 6,356,075 / **105** (77+2+26) / 79 / 26 **OVER** | 34,060,067 / 5,697,004 / **100** (73+2+25) / 75 / 25 | STATIC GL9 / GL8 |

**Liquidation, JRM stack.** L3 is **87** (68+2+17) / 70 keys / 33.1M CPU warm; cache-only off cold is 92;
inline is 107 warm / 112 cold. L1a, L1b, L2, L4 and L5 are identical to STATIC. The JRM adds 7 L3 entries,
matching the cash-borrowing finding.

**Borrow and redeem, STATIC [JRM in brackets]**

| Stage | Cold | Warm | Rows |
|---|---|---|---|
| B borrow | 24,710,697 / 4,524,865 / **75** (62+2+11) / 64 / 11 / 1 [83] | 21,152,899 / **64** (52+2+10) / 54 / 10 / 1 [72] | STATIC GB2 / GB1 |
| B repeat borrow | 25,952,918 / **77** / 64 / 13 [84] | 22,394,849 / **66** / 54 / 12 [73] | STATIC GB8 / GB7 |
| D redeem 10% with debt | 26,896,052 / 5,847,713 / **71** (60+1+10) / 61 / 10 / 1 [80] | 23,350,348 / 5,158,538 / **59** (49+1+9) / 50 / 9 / 1 [68] | STATIC GR2 / GR1 |

**Against the limits.**
- Project policy (100M CPU, keeper stop 97M; 41,943,040 memory; 100 entries; 100 keys; 50 writes): every
  stage is within it, and every STATIC stage is ≤ 85 entries.
- The largest stage (STATIC L3) uses 31% of 100M CPU, 10% of memory, 80% of entries, 63% of keys and 34% of
  writes.
- Live network (`evidence-network-settings-mainnet-20261118.json`): about 8% of 400M instructions; ≤ 2 of
  200 disk reads; ≤ 26 of 200 writes; 3,364 of 132,096 write bytes (L3).

### 5.3 Why L3 is fixed [measured + src]

**STATIC L3 keys by owner (63).**
- USDC lending market 35, controller 15, sleeve 4, contract code 3–4, USDC SAC 3.
- Liquidator account 1, trustline 1, nonce 1.
- It touches no vault, pool, oracle, reward contract or JRM.

| Growth source | Bound | Evidence |
|---|---|---|
| Entered markets | Group cap **2** (patch G2) | X5: a third market cannot be entered. X5b: the same market entered by an account with 0 markets succeeds |
| Cash collateral in the same account | Forbidden by (c); the cap of 2 leaves no slot | Spike S3x: +13 entries per extra market |
| Price cache (cold / warm) | L3 reads only a cached price (G4) and otherwise reverts; L2 always precedes it | STATIC GL2: revert at 36 entries. GL3: 80 |
| Interest model | Static rates (no JRM) | STATIC 80 vs JRM 87 |
| Reward tokens | None in L3. L4 and L5 carry the vault's single primary-reward index (`rewards.rs` `reward()`) | L5 = 25–27 |
| Positions | One fixed-range position per vault; not read in L3 | — |
| First vs repeat seizure | L3 80; L5 26 (+1 history read) | STATIC GL12 |
| Borrower also supplies the lending market | L3 80 | STATIC X2 |
| Borrower with locked units | L3 80; L5 25 | STATIC X3 |
| Interest accrued over 240 s | L3 80 | STATIC X6 |
| Fallback instead of NAV | L3 80 | STATIC GF3 / GF4 |
| **Liquidation fee on** | L3 **84** (+4: the fee recipient's mirror and a second queue entry), plus one more L5 | STATIC X4 |

Each seizure creates new keys (`Pend(id)`, weight-history `n`), but one per operation. The per-transaction
count is therefore constant as state grows. Byte sizes grow only in the controller's `SupportedMarkets`
map and in per-user vectors, both capped here.

### 5.4 Calibration, emulation and what is not measured

- **Sleeve double vs production receipt.**
  - The double is 25,080 bytes; the staged receipt is 112,328.
  - C1 vs C2: +0.43M CPU and +79 KB memory per load, 0 entries.
  - Expect about +0.5–1M CPU per sleeve-touching stage.
- **L5 emulation.** The vault-side keys (Beneficial, Holder, weight history, Locked, Boundary) are emulated
  in the sleeve's storage, with the same key-per-type layout as `custody.rs`, `rewards.rs` and
  `reward_history.rs`.
  - The real vault's code and instance are loaded through a real view (`stage_receipt`).
  - Expect ±1 entry in production: the vault instance write replaces the sleeve instance write.
- **Redeem is a stand-in.** It is a health-checked unit removal. The real encumbrance-aware
  `stage_prepare_close` and the emergency exit with a health check (spec item 2) are **not measured**. Their
  frozen-pair rows are 16 and 85 entries before the health call.
- **Also not measured:**
  - signing, fees, transaction size, Core application, real authorization trees;
  - CAFJ in the controller (0-entry delta from the RPC footprints);
  - patch G5 (other collateral cannot exist under condition c).

---

## 6. Order of work and estimate (question 6)

Assumptions:
- one engineer who knows the codebase, with AI assistance;
- T2, with the same CAFJ oracle, static rates and liquidator infrastructure as cash borrowing;
- the existing harnesses are reused;
- audit calendar, approval waits and on-chain fees are excluded.

These are estimates, not measurements.

| # | Work | Low | Likely | High | Notes |
|---|---|---:|---:|---:|---|
| 1 | **Vault v2**: `stage_seize_reassign` (settle-only), `LastClaim`, boundary on a stale claim, views used by the fallback | 6 | 10 | 18 | **Do first.** Kill point if it does not fit the 7,642-byte headroom; the high case splits out a reward-ledger contract |
| 2 | **Receipt StgV2** sleeve class: USDC-numeraire views; `checkpoint_nav` / `checkpoint_fallback` with guards; unit mirror gating every burn and lock; seize and queue; `sync_seized`; encumbrance-aware close and emergency exit | 8 | 12 | 18 | Reuses the StgV1 suite and key tooling |
| 3 | **Group controller** = `simple-peridottroller` + patches G0–G5 (`controller-group.patch`), tests, mutation | 3 | 5 | 8 | Fresh instance; no live upgrade |
| 4 | Oracle and depeg: CAFJ for USDC, CALI inside the sleeve, parameters, off-chain CEX watcher, guardian pause | 2 | 3 | 5 | Same infrastructure as cash borrowing |
| 5 | Liquidator bot: staged L1a/L1b → L2 → L3 → L4 → L5, off-chain health from simulated checkpoints, resumable journal | 4 | 6 | 9 | Extends the cash-borrowing liquidator |
| 6 | Capacity tool and funded in-kind exit rehearsal (spec 4a/4b) | 5 | 8 | 12 | Sets the caps |
| 7 | Qualification: compiled full stack, signed / Core rows with real auth, live-state rerun, security scan, adversarial review | 6 | 10 | 15 | |
| 8 | Approval package, runbook, upload simulation | 3 | 5 | 8 | |
| | **Total** | **37** | **59** | **93** | With two engineers on parallel tracks (1+2 ∥ 3+4+5), about 6–9 calendar weeks before audit |

**Dependencies.**
- 1 gates 2's seize and sync.
- 3 is independent once the view ABI is frozen.
- 4 and 6 need no contract code.
- 5 needs the ABI from 2 and 3.
- 7 needs everything.

**Why this is lower than the specification's 51 / 79 / 123.**
- The spike is done.
- Seizure is seizure-only.
- The controller diff is known.

---

## 7. Prototype artefacts

**`prototypes/cl-collateral-group/controller/apply-patches.py`.** Copies
`contracts/simple-peridottroller/src` (= build 4c98bdd3) and applies these patches, behind
`cfg!(feature = "group")`:

| Patch | Change |
|---|---|
| G0 | Instance keys |
| G1 | `group_set_sleeve`, `group_set_liq_cache_only` |
| G2 | Entered-market cap of 2 |
| G3 | The liquidation-context health sum calls `get_liquidation_account_snapshot` for the sleeve |
| G4 | `get_liquidation_snapshot` for the seize rate; cache-only prices in liquidation; **the advisory `liquidation_redeem_max_ptokens` preview is skipped for the sleeve**, as the LP build does. This removes work that the deployed-bytes regression rows still perform |
| G5 | `get_collateral_excl_usd` reverts on indeterminate |

- The `--features group` build is 64,594 bytes, sha256 `3e9cd1a9…fb74f`.
- Diff: `controller-group.patch`.
- A build without the feature is **not** the deployed controller: it is 63,529 bytes and still exports the
  group setters. The regression rows therefore use the deployed bytes `847894a6…`.
- An earlier build of the unmodified copied source was 63,285 bytes with a different hash, because
  string-constant order differs. `run.sh` does not rebuild it.

**Other artefacts.**

| Path | Contents |
|---|---|
| `prototypes/cl-collateral-group/sleeve/` | Group sleeve double (25,080 bytes, `9006cfc7…62a4`) |
| `prototypes/cl-collateral-group/harness/` | `common.rs`: the spike infrastructure, plus time advance, expected reverts and per-row auth recording. `group.rs`: both stacks, the rows and the model tests |
| `prototypes/cl-collateral-group/{run.sh, loop.sh, state-driver.cjs, summarize.py}` | Rerun pipeline |
| `prototypes/cl-collateral-group/{oracle-probe.cjs, oracle-footprint.cjs}` | Read-only RPC probes |

**Rerun.** From `peridot-contracts/`, run `bash prototypes/cl-collateral-group/run.sh`.
- It uses the archived capture copied to `.scratch/cl-collateral-group/state/`; the spike's copy is
  untouched.
- Missing keys were filled from RPC, and all came back absent. Max `lastModified` is 65527296, so no entry
  from 18 Nov entered the state.

---

## 8. Open decisions for the user

| ID | Decision | Proposal |
|---|---|---|
| D1 | Controller oracle for USDC: CAFJ (as in cash borrowing) or CALI (USDC is its base, giving one oracle dependency for the group) | CAFJ, as the brief asks; 0-entry difference. CALI would remove one liveness dependency |
| D2 | PYUSD source: CALI only (flat-feed risk) or min(CALI, pool) (griefable) | CALI only, plus an off-chain CEX watcher, the guardian borrow pause and small caps |
| D3 | Depeg band, pool band, fallback haircut, and the out-of-band griefing window (§2.1) | 100 bps / 50 bps + tick in range / 200 bps (fallback = 97.9% of NAV). Consider a minimum out-of-band duration or a reduced incentive on the fallback path |
| D4 | CALI also stale: repay-only, or a governed timelocked recovery rate | Repay-only for the first release |
| D5 | Liquidation fee: 0 (L3 = 80) or on (L3 = 84) | Either fits on STATIC; 0 proposed |
| D6 | Seizure gap rule: boundary / quarantine if L5 runs > 60 s after the last claim, or accept the split at new balances | Boundary (+2 entries) |
| D7 | Liquidation reads only a cached price | Yes: 80 instead of 85 cold |
| D8 | Static rates in the group lending market (as in cash borrowing) | Yes: −7 to −9 entries on every borrow and liquidation row |
| D9 | The liquidator exits seized units only after the permissionless L5 | Accept |
