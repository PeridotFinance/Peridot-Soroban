# CL collateral group: Mainnet launch file (fresh stack, option A)

Prepared on 21 Nov 2026 for a target go-live within 3 days (Tue 24 Nov 2026).
Status: **PROPOSED, NOT APPROVED.** Nothing in this file has been signed, sent, uploaded, deployed or upgraded.
The only network use was read-only `simulateTransaction`, `getLedgerEntries` and Horizon reads.

Inputs:
- `DESIGN.md`, `D3-CONTINUITY.md`, `BUILD-REPORT.md`, `MAINNET-SIMS.md`
- `prototypes/cl-collateral-group/testnet/01-deploy.sh` … `04-d3-fallback.sh` (commit 65fc44b)
- `bots/cl-liquidator`
- `research/cash-borrowing-activation-20261117/tools/{ceremony.cjs, upgrade-run.sh}`, the pattern mirrored here

**Tooling added with this file** (`research/cl-collateral-group-20261118/tools/`):

| File | Purpose |
|---|---|
| `clg-ceremony.cjs` | Step model G1–G61, fresh live simulation per step, auth and resource checks, read-backs, gates, kill switches, upgrade commands. It reuses the cash tools' read-only `lib.cjs` and never signs, sends or reads keys |
| `clg-mainnet.json` | Every address, salt, artifact pin, cap and limit |
| `clg-run.sh` | `step <name> <args>`: fresh simulation → Ledger sign → verify → send → require SUCCESS. Same flow as `upgrade-run.sh`; the HD path comes from the prepared file |
| `clg-ceremony.test.cjs` | Offline tests, 5/5 pass: ids, caps, ordering, refusals, struct encoding |

Markers used below:
- **[LIVE-SIM]** marks a step that was already simulated against live Mainnet before this plan, with its ledger.
- **[EMU]** marks a step covered by the public-network emulation on live state (MAINNET-SIMS.md §3, 46/46 rows ok).
- **Every step** is in any case re-simulated live by `step` immediately before the Ledger signs it, and `prepare` refuses unless every earlier step reads back done.

---

## 0. One page

| Item | Value |
|---|---|
| What goes live | Vault v2 `CAGZA2TN…GVTB`, receipt StgV2 `CDOPEENZ…3UZO`, group controller `CDSJ3ACU…TEOA`, USDC lending market `CCDYSYV5…BS2N`. These are deterministic ids from the admin address and the salts in `clg-mainnet.json`; `node clg-ceremony.cjs addresses` prints them |
| Code | Candidates built at 96e7b3c (includes 6e83fd7 upgradeability and the 137de32 F1 emergency-exit fix): vault `6fd8599a…` 125,559 B, receipt `424454b9…` 125,585 B, controller `642421f1…` 64,957 B. USDC market from the on-chain cash code `5630a701` (no upload). Final freeze pending (HG1). Coordinator pin (21 Nov): exactly these three, pending the double canonical build confirmation in `build-hashes-cl-group.json`. **Never** pin `719914b1` (stale generated source, no F2) or `bf8864e1` (workspace-relative OZ path) |
| Caps | USDC borrow cap **200 USDC**; USDC supply cap 1,000 USDC with 400 USDC owner liquidity; CL backing cap **2,000 PYUSD**; CF on vault units **50 %**; close factor 50 %; incentive 8 %; liquidation fee 0; at most 2 markets (structural, G7). See §2 |
| CL liquidator | New dedicated key holding 200 USDC and 25 XLM. Covers two close-factor rounds on the full cap with a 10 % buffer (165 ≤ 200 USDC). Plus a separate fee-only **D3 heartbeat account** (12 XLM), §6.4 |
| Ledger signatures | **61** (G1–G61): **54** by the admin (index 0) and **7** by team account T3 (index 3), all in the Stellar app. Contingency fallbacks are listed separately |
| Admin XLM | Spends ≈ 416 XLM expected; ≤ 559 XLM if no refund is counted. 172.29 XLM today, so **G1 withdraws 500 XLM first**. See §3 |
| Days | Sat 21 gates and G1–G3 · Sun 22 deploy, wire, team borrow, first liquidation, CF-restore proposal · Mon 23 CF restore, pilot migration, pilot latches · Tue 24 public go-live |
| Hard gates before G4 | HG1–HG9 in §1. Three of them are open blockers in code or keys (HG4 bot Mainnet signer, HG5 keeper v6, HG6 keys) |

---

## 1. Hard gates (all must be green before G4, the first upload; uploads cost ≈ 370 XLM and cannot be undone)

| Gate | Requirement | Status 21 Nov |
|---|---|---|
| HG1 | **Release freeze.** Final bytes of the vault, the receipt (≥ 137de32, features `stg-mainnet-accepted,stg2-mainnet-accepted`) and the controller (`--features group`). Clean Almanax scan of the range. Final path, sha256 and bytes entered in `clg-mainnet.json`, then `artifacts.frozen=true` | **PLACEHOLDER.** Pinned now: the 96e7b3c candidates (`target/clg/freeze/final-96e7b3c/art`). Commits after 96e7b3c (for example d7cfc69, vault tests) must be checked for byte changes. The tool refuses uploads and deploys while `frozen=false` |
| HG2 | **Upload CPU.** The vault and receipt uploads need ≈ 106.7M instructions, above the 100M policy (network limit 400M). Precedent: the pilot vault upload ran at 104.87M. You decide by setting `limits.uploadCpuAcceptedByUser=true` | **USER DECISION.** The tool refuses until it is set |
| HG3 | **Emergency exit (condition 3)** passes on the frozen receipt bytes (§10): `user_required_emergency_exit_works_while_paused_oracle_stale_pool_broken_or_cf_zero`, `user_required_entered_debt_free_holder_exits_while_paused_oracle_broken_and_cf_zero`, `f1_emergency_exit_works_when_the_controller_traps`, `f1_paused_but_answering_controller_still_refuses_an_entered_owner`, `f2_cf_zero_does_not_let_an_indebted_borrower_leave_the_sleeve`, and `g10_debt_free_account_can_always_exit_and_an_indebted_or_unreadable_one_cannot` (all in `prototypes/cl-collateral-group/receipt-v2/tests/`, commit 137de32) | **PLACEHOLDER: pass result on the frozen bytes.** Not run by this stream |
| HG4 | **CL bot Mainnet signer.** `bots/cl-liquidator/src/chain.mjs:27` refuses `--live` on Mainnet. A reviewed change is needed: a key-file signer as in `bots/cash-liquidator` (`LIQUIDATOR_KEY_FILE`), with the Mainnet passphrase pinned and the same journal rules | **BLOCKER (owner: lead).** §6 cannot run live without it |
| HG5 | **Keeper v6.** Keeper v5 (`prototypes/cl-pyusd-vault-b-pilot-tools/keeper-*-v5.cjs`) is pinned to CCGK/CD6N. A copy pinned to the new receipt and vault v2 is needed for `stage_convert/mint/checkpoint_investment/invest/finalise` (G49, G59). It uses the same observation key GBHW…ZON4 (19.31 XLM), in sessions that never overlap a v5 session | **BLOCKER (owner: lead).** Fallback for T3 only: `stage_cancel` at MINTED leaves cash-backed units (Testnet flow), and every permissionless step has a Ledger fallback |
| HG6 | **Keys.** T3 public key (`stellar keys add peridot-team3 --ledger --hd-path 3`), the CL liquidator public key, and the **heartbeat** public key, both printed on their host. Neither may be the cash liquidator GDQA…5B5G, the pause key GBSK…PQX3, the admin or T3, and they must differ from each other. All three are entered in `clg-mainnet.json` (`signers.team3`, `liquidator`, `heartbeat`) | **PLACEHOLDER** |
| HG7 | **Re-quote and re-emulate on the frozen bytes.** `tools/mainnet-sims.cjs quote` and the `fresh_mainnet` emulation (MAINNET-SIMS.md §6) rerun on HG1 bytes, including the receipt with F1. Replace `params.capCoverage.worstStage` with the measured L3 row on those bytes, then run `node clg-ceremony.cjs caps` | **PLACEHOLDER** (current figures come from the 18 Nov candidates and 6e83fd7) |
| HG8 | **Approval.** You approve §2 (caps) and §3 (fees) and change `clg-mainnet.json` `status` | Pending |
| HG9 | **Cash-market interplay.** G2 takes 600 USDC from the cash USDC market. Together with COMMANDS-21NOV §3 (500 USDC to the cash liquidator), the cash USDC supply falls to ≈ 2,940 USDC. At its 70 % reserve that leaves ≈ 882 USDC borrowable, 18 below its 900 cap. Accept this, or lower G2 | Decision |

---

## 2. Caps and parameters (proposed; `node clg-ceremony.cjs caps` checks them)

| Parameter | Value | Reasoning |
|---|---|---|
| USDC borrow cap (`set_borrow_cap`) | **2,000,000,000 raw = 200 USDC** | Cash rule: cap × close factor × 1.10 ≤ liquidator inventory. Round 1 is 100 USDC and round 2 is 50; with the buffer that is 165 USDC ≤ **200 USDC** dedicated CL inventory. Two rounds are covered, not just one, because seized units only become cash after L5 + close + withdraw + swap. The cash liquidator's 500 USDC is already committed to the cash cap (900 × 0.5 × 1.1 = 495) and is not counted |
| Worst-case liquidation stage vs 85 % of the internal limits | L3 = **80 entries / 17 writes / 32.7M CPU** against 85 / 42 / 85M | Cash liquidator rule. The internal CL entry limit stays 100 and is never raised. A fee-on L3 is 84, so the **liquidation fee is 0** (D5). **PLACEHOLDER:** replace with the L3 row on the HG1 bytes (HG7) |
| Group USDC liquidity | Owner supplies **400 USDC** (G42); `min_cash_reserve_bps` **5,000** | 400 × 50 % = 200 = the borrow cap, so the cap and the reserve bind together. Suppliers other than the owner can always withdraw 50 % |
| USDC supply cap | **10,000,000,000 raw = 1,000 USDC** | Room for third-party supply 2.5 × the owner's. Bounds supplier exposure to CL bad debt |
| CL backing cap (`stage_activate`) | **20,000,000,000 raw = 2,000 PYUSD** cost basis | It must cover the 800 PYUSD migration plus the 50 PYUSD team position plus public headroom. It **can only be lowered** afterwards (`stage_lower_backing_cap`); raising it needs a 24 h receipt upgrade, so it is not set too tight. Units do not create USDC debt; the borrow cap bounds the debt |
| Receipt base supply cap / seed | 1 PYUSD / 1 PYUSD | The base pToken path is only a permanent floor (Testnet and pilot pattern) |
| Vault `max_deploy` | 10,000,000,000 raw = 1,000 PYUSD per command | Pilot value. The 809 PYUSD migration fits in one command |
| CF on the vault units | **500,000 = 50 %** | The collateral is a PYUSD/USDC pair inside ±20 ticks. The depeg guard is 1 %, the fallback haircut 2 % (97.9 % of NAV), the D3 griefing window about 2.1 %, and the incentive 8 %. At 50 % a 40 % collateral loss still covers debt × 1.08. Raising the CF later is the 24 h path, which is the safe direction |
| Close factor / incentive / fee | 50 % / 108 % / 0 | Controller defaults (`get_close_factor_scaled` 500,000; incentive 1,080,000; fee 0). No setter is called |
| USDC market rate / reserve factor / flash fee | Static 8 % APR / 10 % / 100 % | The cash-borrowing release choices (D8: no JRM, −7 entries per L3) |
| Markets | 2: the USDC market and the CL receipt | G7 refuses a third market and G6 refuses a CF on the USDC market (condition c) |
| Team position | 50 PYUSD in, **20 USDC** borrowed | 20 ≤ 98 % of 50 × 0.5. The first liquidation repays 10 USDC and seizes ≈ 10.8 USDC of units |

---

## 3. Fees: exact XLM per step, and who pays

**Payer.** Unless stated, the **admin Ledger GC4T5MP5…T7FP** pays and is the source. T3 pays its own 7 transactions from the 8 XLM it is funded with. The CL liquidator pays its bot transactions from its 25 XLM. The keeper GBHW…ZON4 pays its observations (19.31 XLM today).

**How the columns are derived.**
- "min" is the simulated minimum resource fee.
- "cap" is min × 1.2 + 0.001 + the 0.001 bid. It is the maximum fee in the signed envelope; the unused refundable part is returned.
- The pilot paid about **87 %** of the uploads' min.

| Sig | Step | min (XLM) | cap (XLM) | Basis |
|---|---|---:|---:|---|
| G1 | withdraw 500 XLM pTokens (cash XLM market) | 0.0040243 | 0.0068291 | [LIVE-SIM] 65592556 |
| G2 | withdraw 600 USDC pTokens | 0.0040157 | 0.0068188 | [LIVE-SIM] 65592556 |
| G3 | withdraw 60 PYUSD pTokens | 0.0053584 | 0.0084300 | [LIVE-SIM] 65592556 |
| G4 | upload vault v2 (6fd8599a, 125,559 B) | 167.6576637 | 201.1911964 | [LIVE-SIM] 65592616 (sims, 96e7b3c) and 65592640 (clg), exact bytes |
| G5 | upload receipt StgV2 (424454b9, 125,585 B, with F1) | 162.7072417 | 195.2506900 | [LIVE-SIM] 65592617 (sims, 96e7b3c) and 65592640 (clg) |
| G6 | upload controller (642421f1, 64,957 B) | 92.4333637 | 110.9220364 | [LIVE-SIM] 65592617 (sims, 96e7b3c) and 65592640 (clg) |
| G7–G9 | create vault / controller / receipt | 0.0209 each | 0.0271 each | [LIVE-SIM] proxies (MAINNET-SIMS §2) |
| G10 | create USDC market from 5630a701 | 0.0209286 | 0.0271143 | [LIVE-SIM] 65592556, real code |
| G11–G41 | 31 configuration transactions | 0.0005–0.47 each, ≈ 2.5 total | ≤ 5 total | [EMU] rows; pilot `fee_charged` analogues (initialize ≈ 0.27–0.47, setters ≤ 0.08) |
| G42 | owner deposit 400 USDC | ≈ 0.005 | ≤ 0.05 | Cash-market analogue |
| G43 / G44 | create the liquidator (+25 XLM) **and** the heartbeat account (+12 XLM): one transaction, two ops / pay 200 USDC | 0.002 / 0.001 | 0.002 / 0.001 | Classic, fixed 10,000 stroops per op |
| G45 | the borrow switch | ≈ 0.001 | ≤ 0.05 | [EMU] M2b |
| G46 / G48 | create T3 (+8 XLM transferred) / pay T3 51 PYUSD | 0.001 / 0.001 | 0.001 each | Classic |
| G47, G49–G53, G55 | T3's own transactions (trustlines, open, enter × 2, checkpoint, borrow, repay) | ≈ 0.1 total | ≤ 0.4 | [EMU] F1, B1–B6; **paid by T3** |
| G54 / G56 / G57 | disable CF / propose CF / execute CF | ≈ 0.001 each | ≤ 0.05 each | Controller setter analogue |
| G58 | pilot `stage_prepare_close` (ledger-owner-call) | ≈ 0.05 | ≤ 0.1 | Pilot A3 actual (RETRY-COMMANDS) |
| G59 | owner `stage_open` ≈ 809 PYUSD on v2 | ≈ 0.3 | ≤ 1 | [EMU] F1 |
| G60 / G61 | pilot latches CCGK / CD6N | 0.0008732 / 0.0009646 | 0.0030478 / 0.0031575 | [LIVE-SIM] 65592556 |

**Totals for the admin.**

| | Expected | Worst case (no refunds) |
|---|---:|---:|
| Uploads | ≈ 368 (87 % of 422.80) | 507.36 |
| Creates + configuration + other transactions | ≈ 4 | ≈ 7 |
| XLM transferred to the liquidator (25), heartbeat (12) and T3 (8) | 45 | 45 |
| **Total outflow** | **≈ 416 XLM** | **≈ 559 XLM** |

**Balance.** 172.29 + 500 (G1) = 672.29 XLM, which covers the worst case plus the 3 XLM reserve with margin.
MAINNET-SIMS §4 recommends ≥ 365 XLM; 500 also covers the two account fundings and growth of the bytes at HG1.

---

## 4. Timeline (Berlin time, CET = UTC+1)

| When | What | Signatures |
|---|---|---|
| Sat 21 Nov, by 20:00 | HG1–HG9 green. Optional: G1–G3 withdrawals | G1–G3 |
| Sun 22 Nov 09:00–09:45 | Uploads | G4–G6 |
| 09:45–10:15 | Creates | G7–G10 |
| 10:15–12:00 | Configuration and wiring, price gate | G11–G41 |
| 12:00–12:30 | Owner liquidity; liquidator account, trustlines (on its host), USDC | G42–G44 |
| 12:30–14:30 | CL bot dry-run soak (≥ 2 h), then the liquidator gate | — |
| 14:30 | Borrow switch | G45 |
| 14:45–15:45 | T3 account, trustlines, open (keeper v6 session), enter × 2, checkpoint + borrow | G46–G53 |
| 16:00–16:30 | CF → 0, bot `--once --live` (first real liquidation), gate, T3 repays the rest | G54, G55 |
| 16:30–16:45 | Propose the CF restore. Its ETA is ≈ Mon 23 Nov 16:45, printed by `gate cf-eta` | G56 |
| Mon 23 Nov, ETA + 30 s (≈ 16:46) | Execute the CF restore | G57 |
| 17:00–18:00 | Keeper v5 session, then pilot full close on CCGK/CD6N | G58 |
| 18:00–19:00 | Keeper v6 session, then the owner redeposits into v2; gate | G59 |
| 19:00–19:15 | Pilot deposit latches (one-way) | G60, G61 |
| Tue 24 Nov 10:00 | Public go-live (frontend). Bot `--watch --live` has been running since Sun 16:30 | — |

Each prepared transaction is valid for 40 ledgers (about 200 s). Sign within about 3 minutes of `step` printing the call.

---

## 5. Operator commands

**Setup (zsh, in its own terminal).** `clg-run.sh` redefines `step`, `check` and `U` from the cash `upgrade-run.sh`, so never source both in one shell.
```bash
T=/Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group/peridot-contracts
cd $T/research/cl-collateral-group-20261118/tools
source clg-run.sh
clg caps && clg addresses && check
```
Expected:
- `caps` prints four `"ok": true` rows.
- `addresses` prints the four ids from §0.
- `check` lists G1–G61; at the start only `done null` (manual read-back) or `false`.

**Ledger and app.**
- Every `step` uses the Stellar app.
- It signs with the account index printed in the line `Approve on the Ledger now (… account index N)`: 0 = admin GC4T…T7FP, 3 = T3.
- The device shows the contract (or the operation for classic steps) and the function. Compare both with the printed `call`.

**Expected output of every `step`:**
- `== <name>: "n": "Gxx" "call": "<fn>" "maxFeeXlm": <cap>`
- then `OK <name> "tx_hash": "<64 hex>"`
- then the read-back shown in the tables below. `rb <step-id>` must print `"done": true`, except for the manual read-backs marked "m".

**On failure (every step):**

| Failure | Action |
|---|---|
| `PREPARE FAILED` | Nothing was signed. Read the one-line reason: a refusal names the rule. Fix only what the reason names, then run the same line again |
| `SIGN FAILED`, or the window expired | Run the same line again. Stale files are kept, renamed `*-stale.*` |
| `VERIFY FAILED` | Nothing was sent (wrong account or HD path). Check the Ledger account index |
| `SEND FAILED` | Run `clg status <hash>`. If it is `FAILED` or `NOT_FOUND` after 60 s, run the line again; it re-simulates. Otherwise send me the 3 error lines |
| `PREPARE FAILED … already done` | The state already equals the target (for example a default). Skip the step; `rb <id>` confirms it |
| Any failure after a step that **pauses or latches** | Stop and send me the output. The latches are one-way |

Files are written to `/Users/joshua/Peridot/clg-launch-20261122/`, outside git. Never delete them.

### 5.1 Funding: G1–G3 (Sat 21 or Sun 22 09:00)
```bash
step g01-wd-xlm   prepare fund.withdraw.XLM
step g02-wd-usdc  prepare fund.withdraw.USDC
step g03-wd-pyusd prepare fund.withdraw.PYUSD
```

| Sig | Read-back (m) |
|---|---|
| G1 | Admin XLM ≈ 672.29 (`curl -s https://horizon.stellar.org/accounts/GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP`); XLM pTokens 287,584,313,671 → 282,584,313,671 |
| G2 | Admin USDC ≈ 617.7; USDC pTokens 40,400,000,000 → 34,400,000,000 |
| G3 | Admin PYUSD ≈ 66.83; PYUSD pTokens 1,062,250,859 → 1,002,250,859 |

These redeem the owner's own pTokens 1:1 (exchange rate exactly 1.0 at 21 Nov). They are only possible while the cash markets are unpaused and the amounts are available.

### 5.2 Uploads: G4–G6 (Sun 09:00–09:45)
```bash
step g04-up-vault   prepare upload.vault
step g05-up-receipt prepare upload.receipt
step g06-up-ctl     prepare upload.controller
```
- Read-back: `rb upload.vault` (and `upload.receipt`, `upload.controller`) prints `done true`, meaning the pinned code hash is on chain.
- The fee caps are 210 / 210 / 120 XLM, and the step refuses anything above them.
- **If G5 or G6 fails after G4 succeeded:** retry. An uploaded code entry is harmless; it is only a cost.

### 5.3 Creates: G7–G10
```bash
step g07-cr-vault   prepare deploy.vault
step g08-cr-ctl     prepare deploy.controller
step g09-cr-receipt prepare deploy.receipt
step g10-cr-usdcm   prepare deploy.usdcMarket
```
- Read-back: `rb deploy.vault` etc. prints `done true`, meaning the instance exists at the precomputed id and runs the pinned hash.
- **Id check.** `prepare` refuses unless the host's simulated return address equals the precomputed id. This was verified live for G10 at ledger 65592849.
- **If a create lands but its read-back hash differs:** stop. Do not initialize it.

### 5.4 Configuration: G11–G41, then the price gate (Sun 10:15–12:00)
```bash
step g11 prepare vault.initialize
step g12 prepare vault.set_primary_reward_token
step g13 prepare vault.set_max_deploy
step g14 prepare controller.initialize
step g15 prepare controller.set_oracle
step g16 prepare controller.set_oracle_asset_symbol.USDC
step g17 prepare receipt.initialize
step g18 prepare receipt.set_peridottroller
step g19 prepare receipt.set_min_cash_reserve_bps
step g20 prepare receipt.set_supply_cap
step g21 prepare receipt.seed_minimum_supply
step g22 prepare vault.stage_bind
step g23 prepare vault.stage_configure_guardian
step g24 prepare vault.stage_set_invest_keeper
step g25 prepare receipt.stage_activate
step g26 prepare receipt.stage2_activate
step g27 prepare usdcMarket.initialize
step g28 prepare usdcMarket.set_min_cash_reserve_bps
step g29 prepare usdcMarket.set_supply_cap
step g30 prepare usdcMarket.set_borrow_cap
step g31 prepare usdcMarket.set_reserve_factor
step g32 prepare usdcMarket.set_flash_loan_fee
step g33 prepare usdcMarket.set_borrow_rate
step g34 prepare usdcMarket.enable_static_rates
step g35 prepare usdcMarket.seed_minimum_supply
step g36 prepare usdcMarket.set_peridottroller
step g37 prepare controller.add_market.usdcMarket
step g38 prepare controller.add_market.receipt
step g39 prepare controller.group_set_sleeve
step g40 prepare controller.group_set_liq_cache_only
step g41 prepare controller.set_market_cf.receipt
gate prices
```

| Sig | Call (target) | Read-back (`rb <id>`, done = true when) | EMU |
|---|---|---|---|
| G11 | `initialize(admin, pool CAPI…DQYX, 0, CALI)` (vault) | instance `Initialized` true | V1 |
| G12 | `set_primary_reward_token(admin, AQUA CAUI…OJPK)` | `PrimaryRewardToken` = AQUA | V2 |
| G13 | `set_max_deploy(admin, 10,000,000,000)` | `Params.max_deploy` | V3 |
| G14 | `initialize(admin)` (controller) | `get_admin` = admin | G1 |
| G15 | `set_oracle(CAFJ)`; first set is immediate | `get_oracle` = CAFJ | G2 |
| G16 | `set_oracle_asset_symbol(USDC, "USDC")` | `OracleAssetSymbol(USDC)` | G3 |
| G17 | `initialize(PYUSD, 0, 0, admin)` (receipt) | `get_underlying_token` = PYUSD | R1 |
| G18 | `set_peridottroller(group ctrl)` | `Peridottroller` = ctrl | R2 |
| G19 | `set_min_cash_reserve_bps(admin, 10000)` | 10000 | R3 |
| G20 | `set_supply_cap(10,000,000)` | `SupplyCap` | R4 |
| G21 | `seed_minimum_supply(admin, 10,000,000)`; 1 PYUSD from the admin, nested transfer checked | `PermanentMinimumSupply` | R5 |
| G22 | `stage_bind(admin, receipt)` (vault) | `StgV1Receipt` = receipt | V4 |
| G23 | `stage_configure_guardian(admin, admin)` | `StgV1Guardian` | V5 |
| G24 | `stage_set_invest_keeper(admin, GBHW…ZON4)` | `StgV1InvestKeeper` | V6 |
| G25 | `stage_activate(admin, vault, admin, 20,000,000,000)` | `stage_config` vault + cap | R6 |
| G26 | `stage2_activate(admin, {ctrl, pool, CALI, USDC})` | `stage2_config.controller` | R7 |
| G27 | `initialize(USDC, 0, 0, admin)` (USDC market) | underlying = USDC | M1 |
| G28 | `set_min_cash_reserve_bps(admin, 5000)` | 5000 | M2 (row ran 1000) |
| G29 | `set_supply_cap(10,000,000,000)` | `SupplyCap` | not in EMU |
| G30 | `set_borrow_cap(2,000,000,000)` | `BorrowCap` | not in EMU |
| G31 | `set_reserve_factor(100,000)` | `ReserveFactorScaled` | not in EMU |
| G32 | `set_flash_loan_fee(1,000,000)` | `FlashLoanFeeScaled` | not in EMU |
| G33 | `set_borrow_rate(80,000)` | `BorrowYearlyRateScaled` | M3 |
| G34 | `enable_static_rates(admin)` | `RatesReady` true and no `InterestModel` | M4 |
| G35 | `seed_minimum_supply(admin, 10,000,000)`; 1 USDC | `PermanentMinimumSupply` | M5 (row ran 0.001 USDC) |
| G36 | `set_peridottroller(group ctrl)` | `Peridottroller` | M6 |
| G37 | `add_market(USDC market)` | `is_market_supported` | G4 |
| G38 | `add_market(receipt)` | `is_market_supported` | G5 |
| G39 | `group_set_sleeve(receipt)`; one-time | instance `Sleeve` | G7 |
| G40 | `group_set_liq_cache_only(true)` | instance `LiqCacheOnly` | G8 |
| G41 | `set_market_cf(receipt, 500,000)`; first set is immediate | `MarketCF(receipt)` = 500000 | G9 |
| gate | `gate prices`: controller USDC price equals CAFJ `Other(USDC)` within 5 bps; no fallback price | `"ok": true` | B4 |

Specific failure actions:
- **G18 or G36 against the wrong controller:** impossible, because the tool pins the derived id.
- **G25 or G26 refused on Mainnet:** the receipt was not built with the Mainnet features (HG1). Stop. Nothing is funded yet.
- **G41 shows a CF already set:** stop. The 24 h path applies, see §11.
- **`gate prices` false:** do not continue to G45. Run `gate prices` again after one oracle round (300 s).

### 5.5 Liquidity, liquidator and the switch: G42–G45 (Sun 12:00–14:30)
```bash
step g42-supply prepare usdcMarket.deposit.admin
step g43-liq-acct prepare fund.liquidator.xlm
node clg-ceremony.cjs trust-liquidator --out $U/liq-trust.unsigned.xdr   # sign + send ON THE HOST (§6.2): USDC + PYUSD
step g44-liq-usdc prepare fund.liquidator.usdc
# §6.3 dry-run soak >= 2 h, then:
gate liquidator
step g45-switch prepare usdcMarket.set_cash_borrowing_enabled
```

| Sig | Read-back |
|---|---|
| G42 | `rb usdcMarket.deposit.admin` true (admin pTokens ≥ 4,000,000,000) |
| G43 | m: the liquidator account exists with 25 XLM, and the heartbeat account with 12 XLM |
| G44 | m: liquidator USDC balance 200 |
| gate | `"ok": true`: balance ≥ 200 USDC, **both** trustlines USDC and PYUSD (exact issuers, authorized; a missing PYUSD trustline means SAC #13 on `stage_emergency_pay` after a seizure; test "liquidator gate needs BOTH trustlines"), no role overlap. The allowance is only reported; the bot's APPROVE stage sets it right before its first L3. Manual: dry-run log ≥ 2 h, zero errors |
| G45 | `rb usdcMarket.set_cash_borrowing_enabled` true |

- **If G45 must be undone:** `step k1-kill emergency kill-borrow` (§9).
- G45 is refused while a controller borrow pause is active.

### 5.6 Team position T3: G46–G53 (Sun 14:45–15:45)
```bash
step g46-t3-acct  prepare team.create
step g47-t3-trust prepare team.trust          # Ledger index 3
step g48-t3-pyusd prepare team.fund.pyusd
# keeper v6 session running (HG5), then:
step g49-t3-open  prepare team.open           # Ledger index 3; keeper runs convert/mint/invest/finalise
step g50-t3-enter-r prepare team.enter.receipt
step g51-t3-enter-m prepare team.enter.usdcMarket
step g52-t3-nav   prepare team.checkpoint_nav
step g53-t3-borrow prepare team.borrow        # sign within 60 s of G52
```

| Sig | Read-back |
|---|---|
| G46 | m: T3 exists with 8 XLM |
| G47 | m: T3 has PYUSD and USDC trustlines |
| G48 | m: T3 holds 51 PYUSD |
| G49 | m: `clg list` shows `stage_units(T3)` > 0 after the keeper's `stage_finalise` line, or after a cancel at MINTED (HG5 fallback) |
| G50, G51 | `rb team.enter.receipt` / `rb team.enter.usdcMarket` true. **Both are required**: a borrower must also enter the USDC market (MAINNET-SIMS §3) |
| G52 | none: it only refreshes the 60 s NAV checkpoint |
| G53 | `rb team.borrow` true (debt > 0, 20 USDC) |

- **G52 and G53 skip the read-back walk** (`fast`), because the 60 s window does not allow it. Instead, G53 refuses at prepare if the NAV checkpoint is older than 40 s. `clg list` takes about 4 s.
- **G53 refused or reverted (stale checkpoint):** rerun both with **new names**, because a name with a SUCCESS file is skipped: `step g52-$(date +%H%M%S) prepare team.checkpoint_nav`, then at once `step g53-$(date +%H%M%S) prepare team.borrow`.
- **G49 refused "open quote not admissible":** the pool is out of band or the oracle is stale. Wait 5 minutes and run it again.

### 5.7 First real liquidation: G54, bot, G55, G56 (Sun 16:00–16:45)
```bash
step g54-cf0 prepare liq.disable_cf
# on the liquidator host: one live cycle (§7)
gate first-liquidation
step g55-t3-repay prepare team.repay          # Ledger index 3
step g56-cf-propose prepare cf.propose
gate cf-eta                                   # prints etaUtc
```

| Sig | Read-back |
|---|---|
| G54 | `rb liq.disable_cf` true (CF 0). T3 is short: the bot plan shows a shortfall |
| gate | `"ok": true`: `next_pending` ≥ 1 and the liquidator is the vault beneficiary of the seized units (L5 landed) |
| G55 | m: `get_user_borrow_balance(T3)` = 0. The repay amount is debt + 0.01 % + 1,000 raw. `repay` caps it at the **pre-accrual** debt (`receipt-vault/src/contract.rs` `repay`, `planned_repay`), so a few raw units of interest can remain. If so, repeat: `step g55-$(date +%H%M%S) prepare team.repay`. The nested auth is a plain `transfer(T3 → market)` of at most the amount, as checked by the tool |
| G56 | `rb cf.propose` true (`PendingMarketCF` = 500000). `gate cf-eta` shows ETA = send time + 24 h |

- **Cancel G56 (proposal):** `step k-cancel-cf emergency disable-cf`. `emergency_disable_collateral` deletes the pending CF and keeps CF at 0. Re-propose later with `step g56b-cf-propose prepare cf.propose`.
- **If the bot does not liquidate within 10 minutes:** run its dry run (§6.3) and send me the plan. CF stays 0, which is safe: only T3 has debt.

### 5.8 CF restore: G57 (Mon 23 Nov, ETA + 30 s, ≈ 16:46)
```bash
gate cf-eta && step g57-cf-exec prepare cf.execute
```
- Read-back: `rb cf.execute` true (`MarketCF` = 500000, nothing pending).
- **If it fails "market cf timelocked":** the ETA has not passed. Wait, then run again.

### 5.9 Migration: G58–G61 (Mon 17:00–19:15), see §8

---

## 6. CL liquidator bot (dry run → gate → live)

### 6.1 Blocker
HG4: the bot cannot send on Mainnet today. Commands marked **[after HG4]** use the reviewed Mainnet signer. Until then only the dry run works.

### 6.2 Host and key
- **Host.** Run it as a second systemd service on Droplet `peridot-cashliq` (fra1, 165.245.249.83, context `peridot-liq2`), with its own data directory `/mnt/cashliq/cl-liquidator`, its own key file and its own journal. It never uses the cash liquidator key.
- **Key.** Generate the key on the host; it never leaves it. The command depends on HG4's signer design; mirror the cash `bootstrap-liquidator.sh`. Print only the public key and enter it in `clg-mainnet.json` `liquidator.publicKey` (HG6).
- **Heartbeat key.** A second key generated on the same host, for the fee-only D3 heartbeat (§6.4); see the commands there.
- **Trustlines (required, both).** The CL liquidator must hold **USDC** (GA5Z…KZVN) and **PYUSD** (GDQE…U2V5). A missing PYUSD trustline makes `stage_emergency_pay` (and the in-kind exit) of seized units revert with **SAC error #13** after a seizure. USDC is the repay asset.
  - **Mac:** `cd $T/research/cl-collateral-group-20261118/tools && node clg-ceremony.cjs trust-liquidator --out /Users/joshua/Peridot/clg-launch-20261122/liq-trust.unsigned.xdr`. This prepares one unsigned transaction with two `changeTrust` operations and the liquidator as source; it is never signed on the Mac or the Ledger.
  - **Host:** copy the file over with `scp`, then `stellar tx sign --sign-with-key <liquidator alias> --rpc-url … --network-passphrase "Public Global Stellar Network ; September 2015" liq-trust.unsigned.xdr > liq-trust.signed.xdr` and `stellar tx send … liq-trust.signed.xdr`. The fee is 0.002 XLM, from the liquidator's 25 XLM; each trustline adds 0.5 XLM of reserve.
  - **When:** after G43 and **before G44** (the USDC payment needs the trustline). It must be done before G45.
  - **Read-back:** `gate liquidator` shows `trustlines.USDC` and `trustlines.PYUSD` both `present: true, authorized: true` with the exact issuers. Otherwise the gate is false.

### 6.3 Config, dry run, gate, live
Copy `config/testnet.example.json` to `config/mainnet.local.json` (git-ignored) and set:
- `networkPassphrase` = "Public Global Stellar Network ; September 2015", `rpcUrl` = https://soroban-rpc.mainnet.stellar.gateway.fm
- `contracts`: receipt `CDOPEENZ…`, vault `CAGZA2TN…`, controller `CDSJ3ACU…`, usdcMarket `CCDYSYV5…`, usdc `CCW67TSZ…`, pyusd `CCCRWH6Q…`, oracle CALI `CALI2BYU…`
- `borrowers` = [T3]
- `liquidator` = the CL liquidator public key
- `repayCapRaw` = `1000000000` (100 USDC, the first round on the full cap)
- `submitMarginSec` 10, `watchIntervalSec` 10
- `heartbeatIdentity` = `cl-hb-main`, `heartbeatAccount` = the heartbeat public key, `heartbeatPeriodMs` 12000. `config.mjs` refuses a heartbeat identity equal to `executorIdentity` and a heartbeat account equal to the liquidator

```bash
cd /mnt/cashliq/cl-liquidator
node src/main.mjs --config config/mainnet.local.json --once       # dry run
node src/main.mjs --config config/mainnet.local.json --watch      # soak >= 2 h
```
- **Expected:** `start live=false network=Public…`, then a plan per cycle: `no borrower short`, the status of `stage2_status`, D3 `chain none`, and the watcher `feed age < 600 s`.
- **Gate:** zero errors for ≥ 2 h, then `gate liquidator` on the Mac (§5.5).
- **Live [after HG4]**, from Sun 16:00: first the single cycle of §7, then
  `node src/main.mjs --config config/mainnet.local.json --watch --live`
  as a systemd unit with restart-on-failure. It also provides the D3 heartbeat (≤ 20 s while a chain is open).
- **Stop (kill switch K7):** `sudo systemctl stop cl-liquidator`. A stopped bot is not a D3 cause and does not unlock the fallback (D3-CONTINUITY.md). The heartbeat runs inside the same process and stops with it.

### 6.4 D3 heartbeat account (fee-only)

**What it does.** `bots/cl-liquidator/src/heartbeat.mjs`, from stream A, is the D3 timer.
- While the normal checkpoint is blocked, it sends receipt `checkpoint_nav()` every **12 s** from the last submit, and nothing else.
- `checkpoint_nav` needs no authorization, so this account holds no power. It exists so the heartbeat never shares a sequence number with the liquidator's sends.
- It runs only with `--watch --live`.
- Every submit journals an intent with its hash, and inclusion is polled.

**Key generation**, on the CL bot Droplet, as the service user:
- The secret lives only in the CLI store or a chmod-600 file and is never printed.
- The exact form follows HG4's Mainnet signer: today the heartbeat signs with `stellar tx sign --sign-with-key <alias>`.

```bash
umask 077
stellar keys generate cl-hb-main --no-fund
stellar keys address cl-hb-main          # public key only -> clg-mainnet.json heartbeat.publicKey
chmod 600 ~/.config/stellar/identity/cl-hb-main.toml
ls -l ~/.config/stellar/identity/cl-hb-main.toml   # -rw-------
```
- Never run `stellar keys show` (that prints the secret).
- If HG4 moves the signer to a key file, generate the file the same way, `umask 077` with mode 600 and never printed. Use the `--sign-with-key` alias only on hosts where the CLI keystore is used.

**Funding, inside G43** (same transaction as the liquidator account; admin pays): **12 XLM**.

| Item | Value |
|---|---|
| L1a `checkpoint_nav` | 15.7M CPU, 18 entries, 1 write (DESIGN §5.2) |
| Cap per call | ≤ 0.007 XLM (min resource ≲ 0.004, × 1.2, plus 0.002 bid/pad), by analogy with the 18.5M-CPU / 7-write withdraw simulated at 0.0040 |
| Cap rate | 5 calls a minute × 0.007 = **2.1 XLM per hour** of open chain |
| 12 XLM | 1 XLM base reserve (0 subentries) + about **5 h** of continuous chain at the cap, about 9 h at the expected 0.004 |

**PLACEHOLDER:** replace the cap per call with the live `checkpoint_nav` simulation right after G26, using `clg-ceremony.cjs simulate-all`, step `team.checkpoint_nav`. Top up when the balance falls below **10 XLM**; `gate liquidator` refuses below that.

**Dry-run gate** (before `gate liquidator` and G45):
1. `node --test` in `bots/cl-liquidator` passes, including `test/heartbeat.test.mjs` (timer, gap statistics).
2. `node src/main.mjs --config config/mainnet.local.json --once` loads the config without a heartbeat or identity refusal. In dry run the `start` line shows `heartbeat: null`, because the heartbeat runs only live; that is expected.
3. A Testnet live run of the heartbeat (`config/testnet-real.local.json`, identity `clgr-hb`) over a ≥ 10-minute blocked chain. Its `gapStats` show p95 ≤ 20 s and **0 gaps over 60 s**, recorded as evidence. **PLACEHOLDER:** the stream A result.
4. `gate liquidator` shows `rows.heartbeat.xlm ≥ 10` and `roles: true`, meaning the heartbeat account is distinct from the liquidator, admin, T3, the cash liquidator and the pause key.
5. After going live: the first blocked period on Mainnet shows heartbeat intents and results in the journal about 12 s apart. Until then, the D3 residual of D3-CONTINUITY.md applies.

---

## 7. First real liquidation (Sun 16:00–16:30)

1. **G54** sets CF → 0, so T3's 20 USDC debt is fully short.
2. **[after HG4], on the host:**
   `node src/main.mjs --config config/mainnet.local.json --once --live`
   It sends at most 8 stages: L1a `checkpoint_nav`, L2 `cache_price(USDC)`, APPROVE, L3 `liquidate(T3, usdcMarket, receipt, ≈ 10 USDC, liquidator)`, L4 `checkpoint_rewards`, L5 `sync_seized(0)`.
   Expected journal results: L3 SUCCESS, then L5 SUCCESS within 60 s of L4.
3. **`gate first-liquidation`** on the Mac.
4. **G55:** T3 repays the remaining ≈ 10 USDC. This leaves no debt, so the bot has nothing more to do.
5. **G56:** propose the CF restore.
6. Start `--watch --live`.

The bot plans one L3 per cycle, for the largest shortfall (bots/cl-liquidator README). `--once --live` sends at most 8 stages, so there can be exactly one L3. Confirm this in the HG4 review of `main.mjs`.

Evidence to record: the hashes of L1a–L5, `stage2_pending(0)`, liquidator units, and T3 units before and after.

Notes:
- **Why CF → 0 is used.** A price-driven shortfall cannot be produced on a stable pair; the Testnet rehearsal used the same mechanism.
- **Effect on others.** Before go-live only T3 has debt. Anyone else who deposited cannot borrow at CF 0.

---

## 8. Migration: pilot ≈ 800 PYUSD from CD6N to v2, then close CD6N (Mon 17:00–19:15)

**Live position (read at ledger 65592394).**
- Owner units 7,996,501,607; principal 8,000,000,000 raw; NAV 799.65 USDC; invested; position liquidity 3,995,340,039,279.
- PYUSD has **7 decimals**, so this is ≈ **800 PYUSD, not 8,000**.
- The close quote gives total 7,992,711,897 raw (≈ 799.27 PYUSD out).
- CCGK `stage_deposits_paused` false; CD6N `stage_deposits_paused` false; no open command; both guardians are the admin.

**Order:** close fully, redeposit into v2, and only then apply the one-way latches. Neither latch blocks exits.

```bash
# keeper v5 session, as in research/cl-pyusd-tight-pilot-20261111/keeper-v4-review-20261116/REVIEW-v5.md §9
# steps 2-3 (new stateDirectory, fresh approval), then:
pstep g58-pilot-close prepare-close           # A3: stage_prepare_close(owner, ALL units)
# keeper v5: A4 stage_withdraw, A5 stage_close_convert (SUCCESS lines)
gate pilot-closed
# stop keeper v5; start the keeper v6 session (HG5)
step g59-migrate-open prepare migrate.open    # stage_open on the NEW receipt, amount = wallet PYUSD - 0.5 % - 1
# keeper v6: convert, mint, 3 observations, invest, finalise
gate migrated
step g60-ccgk-latch prepare pilot.receipt.stage_pause_deposits
step g61-cd6n-latch prepare pilot.vault.stage_pause_deposits
```

| Sig | Command / account | Read-back | If it fails |
|---|---|---|---|
| G58 | `ledger-owner-call.cjs prepare-close`, admin index 0. Its own checks: full position, 20 bps floor, deadline ≤ 3,600 s | `node ledger-owner-call.cjs read` shows the command at phase 10, then 11, then done; `gate pilot-closed` ok (units 0, no command); PYUSD wallet +≈ 799.2 | If the keeper stops at phase 10: `pstep g58-f1 prepare-step stage_withdraw <id>`. At phase 11: `pstep g58-f2 prepare-step stage_close_convert <id>`. **Never cancel on your own.** `<id>` is in `read` |
| G59 | clg `owner-open` (admin): fresh vault v2 quote (tolerance 20 bps, reserve + 50 %); auth checked as owner → vault `stage_accept_funds` → PYUSD transfer of principal + reserve only | `gate migrated` ok (v2 units > 0, no open command) | Quote not admissible: wait one oracle round. Keeper halt: send me the final JSON. Do not cancel unless I confirm |
| G60 | `stage_pause_deposits(admin)` on CCGK (one-way) [LIVE-SIM 65592556: 2 keys, 1 write] | `rb pilot.receipt.stage_pause_deposits` true | Nothing to undo: there is no unpause |
| G61 | `stage_pause_deposits(admin)` on CD6N (one-way) [LIVE-SIM 65592556] | `rb pilot.vault.stage_pause_deposits` true | Same as G60 |

Mechanism, checked read-only:
- **Backing cap 0 is impossible:** `stage_lower_backing_cap` asserts `cap > 0`.
- **The closing mechanism** is the two guardian latches. The receipt's `stage_pause_deposits` sets `StgV1Paused`; the vault's sets `StgV1DepositPause`, which blocks admission (#1/#2) only.
- **What stays possible:** exits, emergency exits and the permanent 1 PYUSD floor stay as they are.
- **The CCGK borrow-pause renewals** on the cash controller are unaffected.

Retired pre-incident contracts are never touched:
- `clg-ceremony.cjs` refuses the 20 retired ids from `addresses.md` as a target or argument.
- It refuses any target other than the four new contracts, except two cases: the two pilot latches, and the owner's own cash-market withdrawals.

---

## 9. Kill switches

Each is one command. All are admin, Ledger index 0. Durable switches stay until reversed; 72 h switches expire on their own and must be re-armed.

| # | Command | Effect | Kind |
|---|---|---|---|
| K1 | `step k1-kill emergency kill-borrow` | USDC market `set_cash_borrowing_enabled(false)`: no new borrows. Repay and liquidation unaffected | **Durable**. Undo: a fresh `set_cash_borrowing_enabled` true (rerun G45 with a new name) |
| K2 | `step k2-pb emergency pause-borrow on` / `… off` | Controller `set_pause_borrow(USDC market)` | **72 h** |
| K3 | `step k3-pl emergency pause-liquidation on` / `off` | Controller `set_pause_liquidation(receipt)`: stops seizure of CL units (bad oracle or pool) | **72 h** |
| K4 | `step k4-pr emergency pause-redeem on` / `off` | Controller `set_pause_redeem(USDC market)`: USDC suppliers | **72 h** |
| K5 | `step k5-cf0 emergency disable-cf` | `emergency_disable_collateral(receipt)`: CF 0 now. **Every CL borrower becomes liquidatable.** Also cancels a pending CF | **Durable**, last resort; restore = 24 h (G56/G57) |
| K6 | `step k6-lr emergency latch-receipt` | New receipt `stage_pause_deposits`: no new CL deposits | **Durable, one-way** |
| K7 | `sudo systemctl stop cl-liquidator` (host) | Stops the bot | Operational |
| K8 | `step k8-vp emergency vault-pause on` / `off` | Vault v2 `set_paused`. Reversible; required before a vault upgrade | **Durable**, reversible |
| K9 | `step k9-lv emergency latch-vault` | Vault v2 `stage_pause_deposits`: no new open | **Durable, one-way** |
| K10 | `step k10-bc emergency lower-backing-cap <raw>` | Lowers the CL backing cap | **Durable**, lower-only |

Order in an incident:
1. **Oracle or depeg:** K1 → K3 (and the watcher's recommended guardian calls).
2. **Broken valuation:** K1 → K6 → K5 only with my confirmation.

Repayment is never paused by any of these.

---

## 9b. Reward release after a seizure (ops rule, finding F4)

**Rule.** A boundary opened by a seizure sync is released **only by route B**:
```bash
clg reward-boundary <id>                                           # read-only: verdict + interval
step rb-prop-<seg>  reward-route-b propose <seg> <raw> <evidenceHex>
# 24 h later, same amount and evidence:
step rb-attr-<seg>  reward-route-b attribute <seg> <raw> <evidenceHex>
step rb-prep-<seg>  reward-prepare <seg> 4                         # repeat until it returns true
step rb-set-<seg>-<holder> reward-settle <G…> <seg> <checkpoint>   # once for EVERY holder of the segment
```
- **When it applies.** A seizure sync (L5) that runs more than 60 s after the last claim opens a reward boundary (D6). It publishes `seizesync (from, to, n, opened=true)`, and the boundary id is the vault's boundary Count just before that transaction.
- **Why route A is refused.** The admin-only route A (`propose_reward_attribution` / `attribute_rewards`) credits only that boundary's holder, the seized borrower. Used on such a boundary, it strands every other holder's share of the quarantined claim (`seize_tests.rs` `route_a_per_boundary_release_can_only_credit_the_seized_from_holder`).
- **Route B** (`route_b_segment_release_pays_every_holder_once_at_post_move_weights`) pays every holder once.
- **How the tool enforces it.** `clg-ceremony.cjs reward-route-a` reads the vault's `seizesync` events live and refuses three cases:
  - the boundary matches an `opened=true` event (same `from` holder and ledger close time);
  - the events cannot be read;
  - the boundary is older than the RPC event window (about 7 days, `oldestLedgerCloseTime`), so a seizure cannot be excluded. Such boundaries also go through route B.
- **Offline test:** `clg-ceremony.test.cjs`, "F4: route A is refused …".
- **Arguments.** The segment ids come from `reward_interval(boundary.interval)` (printed by `reward-boundary`). The per-holder `checkpoint` comes from `reward_history_count` / `reward_weight`. Both `reward-prepare` and `reward-settle` are permissionless: the admin only pays the fee.
- **Practice.** The bot runs L4 within 60 s before every L5 (BUILD-REPORT §4 item 4), so a boundary should be rare. When one opens, the bot alerts; record the boundary id from `reward-boundary` the same day.

## 10. Emergency exit for holders (condition 3)

| Holder state | Path | Dependencies | Test (`prototypes/cl-collateral-group/receipt-v2/tests`, commit 137de32) |
|---|---|---|---|
| Not entered (default for depositors) | `stage_emergency_exit(owner, shares, true)`, then `stage_emergency_pay(id)` | No oracle, pool, reward, keeper, deadline or pause. With a trapping controller it **fails open** (F1) | `user_required_emergency_exit_works_while_paused_oracle_stale_pool_broken_or_cf_zero`, `f1_emergency_exit_works_when_the_controller_traps` |
| Entered, no debt | `exit_market(owner, receipt)` (G10, no oracle), then the row above | Controller must answer; pauses do not block it | `user_required_entered_debt_free_holder_exits_while_paused_oracle_broken_and_cf_zero`, `g10_debt_free_account_can_always_exit_and_an_indebted_or_unreadable_one_cannot` |
| Entered, debt | `repay` (never depends on the sleeve), then the row above; or a health-checked `stage_prepare_close` with a fresh NAV | — | `f2_cf_zero_does_not_let_an_indebted_borrower_leave_the_sleeve`, `prepare_close_with_debt_needs_fresh_nav_and_remaining_cover` |
| Entered, controller paused but answering | Refused (#2116) by design: use the second row | — | `f1_paused_but_answering_controller_still_refuses_an_entered_owner` |

Holder command, from their own wallet; `stellar contract invoke` with `--send yes` after review:
```bash
stellar contract invoke --id CDOPEENZEYPSIAXZ645L2X3XN2TVLMS3SHC6FIBVPFKMP45TBAHX3UZO \
  --source-account <holder> --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015" -- \
  stage_emergency_exit --owner <G…> --shares <units> --accept_reward_waiver true
```

**PLACEHOLDER (HG3):** the pass result of these tests on the frozen receipt bytes.

**Residual.** If the receipt code itself is broken, holder recovery is a 24 h receipt upgrade (§11). There is no vault-side holder path.

---

## 11. Upgrades and timelocked changes (24 h; none needed at launch)

The new contracts carry `propose_upgrade_wasm`, `upgrade_wasm`, `cancel_upgrade` and `get_pending_upgrade` (commit 6e83fd7):
- **Vault:** takes `admin_addr` first, and `upgrade_wasm` needs `set_paused(admin, true)` first.
- **Receipt, controller and USDC market (5630a701):** take only the hash.

```bash
clg pending vault                                       # read-only
step u-prop-vault   propose-upgrade vault <sha256>      # ETA = now + 24 h
step u-cancel-vault cancel-upgrade vault                # CANCEL the pending proposal
step u-vp-on emergency vault-pause on                   # vault only, after the ETA
step u-exec-vault   execute-upgrade vault <sha256>
step u-vp-off emergency vault-pause off
```
Replace `vault` with `receipt`, `controller` or `usdcMarket` as needed; those need no pause.

**Timelocked CF changes.** The controller's CF changes after the first set are 24 h both ways (`set_market_cf` proposes, and the same call after the ETA executes). The cancel is `emergency disable-cf` (§5.7). Close factor and incentive are also 24 h. No change is planned.

---

## 12. Open placeholders

| # | Placeholder | Where |
|---|---|---|
| P1 | Final frozen hashes and sizes; the receipt rebuilt ≥ 137de32; Almanax scan; `artifacts.frozen=true` (HG1) | `clg-mainnet.json` `artifacts`, §3 G4–G6 |
| P2 | User acceptance of ≈ 106.7M upload CPU (HG2) | `limits.uploadCpuAcceptedByUser` |
| P3 | Pass result of the condition-3 tests on the frozen bytes (HG3) | §10 |
| P4 | CL bot Mainnet signer, key file and trustline helper (HG4) | §6 |
| P5 | Keeper v6 pinned to the new receipt and vault (HG5) | §5.6, §8 |
| P6 | T3 and CL liquidator public keys (HG6) | `clg-mainnet.json` |
| P7 | Re-quote, re-emulation and the L3 row on the frozen bytes (HG7) | `params.capCoverage.worstStage`, §3 |
| P8 | Exact `fee_charged` per configuration step: filled from `send.json` after each step | §3 |
| P9 | Liquidator in-kind exit of seized units: a manual tool for its host | §7 |
| P11 | Segment-id and checkpoint lookup for route B is manual (`reward-boundary`, `reward_weight`) | §9b |
| P10 | Heartbeat key and public key; live `checkpoint_nav` fee after G26; stream A's Testnet gap stats; the heartbeat's Mainnet signer form (HG4) | §6.4, `clg-mainnet.json` `heartbeat` |

## 13. Evidence (outside git)

| File | sha256 |
|---|---|
| `target/clg/evidence/launch-presims-20261121c.json` (`clg-ceremony.cjs simulate-all --allow-unfrozen` at ledger 65592640 on the 96e7b3c candidates: G1–G6, G10, G60, G61 live; all three upload fees reproduce the sims stream to the stroop) | `d71fb5af5730c09c1fde1d7f7ee713068e68d591b5ad3a4fa354fb6269ace13d` |
| `target/clg/evidence/launch-presims-20261121b.json` (`clg-ceremony.cjs simulate-all --allow-unfrozen` at ledger 65592625 on the 6e83fd7 candidates; superseded) | `f289dd816fb4a14f1fd1dd545fe7b909c9ae98b1f2cc54890b59c86fe534cd9e` |
| `target/clg/evidence/launch-presims-20261121.json` (same at 65592556 on the 18 Nov candidates; superseded) | `946707cc0783fa28e98040049094b88fc7477c68405b2f549224bdad9a91758c` |
| `sims/quote-final-20261121T081351Z.json`, `sims/quote-final-96e7b3c-20261121T081849Z.json` (from the sims stream) | — |
| Pilot read (`ledger-owner-call.cjs read`, ledger 65592394) | Reproduce with the same command (read-only) |
