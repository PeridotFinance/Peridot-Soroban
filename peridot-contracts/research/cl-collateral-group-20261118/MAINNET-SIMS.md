# CL collateral group, fresh stack (option A): Mainnet simulations and fee quote (21 Nov 2026)

Stream 2. Read-only throughout: nothing was signed, sent, uploaded, deployed or upgraded. The network was
used only for `getLatestLedger`, `getAccount`, `getLedgerEntries` and `simulateTransaction` on
`https://soroban-rpc.mainnet.stellar.gateway.fm`, plus Horizon reads of pilot transactions.

## FINAL: release bytes from `git archive` of 96e7b3c (this section supersedes §1, §2 and §4)

**How the bytes were built.**
- **Source.** `git archive 96e7b3cfeaa6cf403b80f2f751b5e9aa018f7342`, which equals origin/feat/cl-collateral-group, unchanged through the end of the run. It covers `prototypes/cl-collateral-group` and `contracts/simple-peridottroller`. None of the shared worktree was used.
- **Changes included.** Vault upgrade gate (6e83fd7), controller G10/F2 and G11 (6e83fd7), receipt F1 (137de32).
- **Build.** `stellar contract build --locked`, stellar 26.1.0 and rustc 1.96.0, with the Mainnet init-admin constant GC4T…7FP.
- **Receipt inputs.** Base tree `target/clg/ws/receipt-base`, pinned by `apply-stgv2.py`'s staged.rs SHA. The OpenZeppelin dependency is rewritten to the **canonical absolute path** `/Users/joshua/Peridot/Peridot-Soroban/openzeppelin-stellar-contracts` (3f81125).
- **Artifacts.** `target/clg/freeze/final-96e7b3c/art/`.

| Contract | Features | Bytes (< 131,072) | sha256 |
|---|---|---:|---|
| Vault v2 | `cl-pyusd-vault-b` | 125,559 | `6fd8599a9b369ab84ba9ee8fd95642cde50621c07d5cc37cd278e8f225a4870e` |
| Receipt StgV2 | `stg-mainnet-accepted,stg2-mainnet-accepted` | 125,585 | `424454b94e455c92921d7f548be92ffbe0bc2bcf67dcb05eed1696dacad1d379` |
| Group controller | `group` (G0–G11) | 64,957 | `642421f151273a5a9e89523b5969fdcef89d22f0388c79289c5461cf0dc7f208` |
| USDC group market | — | 89,808 | `5630a701…e452`, already on chain (§3b) |

**Differences from the interim `target/clg/upg` bytes.**
- **Controller.** 719914b1 was built from a worktree `controller/src` generated *before* the committed G10/F2 edit. It lacks the sleeve exit check that applies while CF is 0 (`market_cf > 0 || sleeve`). 642421f1 is the HEAD build. **Do not upload 719914b1.**
- **Receipt.** bf8864e1 was built with the ws-relative OZ path. 424454b9 is the canonical-path build of the same source.
- **Vault.** The hash is identical.

### Live RPC quote

Ledgers 65592616–65592617: `sims/quote-final-96e7b3c-20261121T081849Z.json`, manifest `tools/manifest-final.json`.
- **Payer** for every step is the source account, the admin Ledger **GC4T…7FP**.
- **Uploads** need no authorization entries.
- **Creates** need source-account auth only.

| Step | min resource (XLM) | cap = min × 1.2 + 0.002 (XLM) | CPU | write B |
|---|---:|---:|---:|---:|
| U1 upload vault v2 6fd8599a | 167.6576637 | 201.1911964 | 106,660,682 | 125,660 |
| U2 upload receipt 424454b9 | 162.7072417 | 195.2506900 | 106,682,591 | 125,688 |
| U3 upload controller 642421f1 | 92.4333637 | 110.9220364 | 55,714,793 | 65,060 |
| C1–C3 create (on-chain proxies), C4 create USDC market (real 5630a701) | 4 × 0.0209 | 4 × 0.0271 | ≤ 1,935,720 | 104 |

**Upload allowance needed.** U1 and U2 run at about 106.7M CPU, above the 100M financial-transaction
policy. The network limit is 400M. The launch tool's upload mode must pin an upload allowance for these
exact hashes, as the pilot tool did per step.

### Configuration fees, step by step (each re-simulated live before signing)

Basis "pilot" is the actual `fee_charged` (Horizon) of the identical-shape step in the pilot deployment of
15 Nov. Basis "estimate" means there is no pilot analogue.

| Step | XLM | Basis |
|---|---:|---|
| V1 vault.initialize | 0.2687 | pilot |
| V2 set_primary_reward_token | 0.0138 | pilot |
| V3 set_max_deploy | 0.0005 | pilot |
| R1 receipt.initialize | 0.4688 | pilot |
| G1 ctrl.initialize | 0.47 | estimate (= R1) |
| G2 set_oracle | 0.05 | estimate |
| G3 set_oracle_asset_symbol | 0.05 | estimate |
| M1 usdc.initialize | 0.4688 | pilot (= R1) |
| M2, M2b, M3, M4 market setters | 4 × 0.0010 | pilot (cash-reserve / supply-cap analogue) |
| M5 seed_minimum_supply | 0.0831 | pilot |
| M6 usdc.set_peridottroller | 0.0238 | pilot |
| G4, G5 add_market | 2 × 0.0403 | pilot |
| G6 set_pause_borrow | 0.0212 | pilot |
| R2 receipt.set_peridottroller | 0.0238 | pilot |
| R3 cash reserve, R4 supply cap | 2 × 0.0010 | pilot |
| R5 seed_minimum_supply | 0.0831 | pilot |
| V4 stage_bind | 0.0361 | pilot |
| V5 guardian | 0.0218 | pilot |
| V6 invest keeper | 0.0145 | pilot |
| R6 stage_activate | 0.1331 | pilot |
| R7 stage2_activate | 0.05 | estimate |
| G7 sleeve, G8 cache-only | 2 × 0.03 | estimate |
| G9 set_market_cf | 0.05 | estimate |
| G10 set_pause_guardian | 0.03 | estimate |
| **Sum** | **≈ 2.51** | cap budget 5 |

### Total and withdrawal

| | XLM |
|---|---:|
| Uploads, expected (about 87% of min resource, as in the pilot) | ≈ 368 |
| Creates, configuration, first command | ≈ 3.6 |
| **Expected spend** | **≈ 372** |
| Caps if reserved without counting refunds: 507.47 + 0.11 + 5 + 2 | ≈ 514.6 |

- **Admin balance** at 08:19 UTC: 172.29 XLM. With 4 subentries the reserve is 3 XLM.
- **Recommendation: withdraw 400 XLM** (4,000,000,000 raw pTokens, rate 1.0) from XLM market CBHMG4CN….
- **Strict minimum** with sequential uploads and refunds: about 230 XLM.
- **The withdraw was simulated live** at ledger 65592624 and succeeded: min fee 0.0040252 XLM, 18.5M CPU, 38 + 7 footprint (`sims/withdraw-xlm-400-20261121T081928Z.json`).
- **XLM market pauses today:** redeem, deposit and borrow are all false.
- **Pending cash upgrade.** The cash upgrade 5630a701 for that market is still pending (ETA passed 19 Nov). Its execution requires deposit, redeem and borrow paused, so **the withdraw must be sent before that pause window or after it closes.**

### Emulation on these exact bytes

Public network id, live state at ledger 65592376: `target/clg/fresh/state/fresh-result-96e7b3c.json`,
sha256 `152d35d37079b715c28e9fad2d98e70fd7a71328129dd24ee064d936df5f5a8a`.

- **Every intended outcome is met** (10 sequences): the 30-step ceremony, the first command, borrow and
  repay, cancel, the timelock, and the refusals for unpaused execution and a wrong hash.
- **Upgrade.** `upgrade_wasm` to the d3ea4f91 marker succeeds, the marker returns 2, and **state is
  preserved**. The post-upgrade second command passes P1–P8. P9 is the frozen-oracle artefact.
- **Maxima:** 74 entries, 58 keys, 59.6M CPU.
- **The 74-entry, 59.6M second `stage_mint` is not an upgrade effect.** The same second command on the
  pre-upgrade vault (control K1–K3) measures identically: 74 entries / 59.57M. It is the cost of minting into
  a vault that already holds a position. That is within 100 entries and the 97M stop, but it is the largest
  staged row.

### Emergency exit (user-required), F1 and F2 on the compiled release bytes

Same emulation (public network id, live state, bytes 6fd8599a / 424454b9 / 642421f1 / 5630a701), with
`fresh_exit_rows` added. Evidence: `target/clg/fresh/state/fresh-result-96e7b3c-exits.json`, sha256
`98a14c72…b7fd`. All 27 sequences behave as intended. The only `ok: false` sequence is the known P9
frozen-oracle artefact. Expected refusals are counted as passes only when the call actually reverts.

"Trapped" means the dependency's code is swapped, in the local snapshot, to the cash-market WASM, so every
call to it fails with a recoverable VM error.

| Test (native counterpart in `receipt-v2/tests/stgv2_test.rs`) | Compiled result |
|---|---|
| EX1 `user_required_debt_free_account_exits_market…`. Entered debt-free owner. | Emergency exit refused **#2116**; `exit_market` passes 3,600 s later with the oracle stale (56 entries); then a full emergency exit (91 entries, 60.7M) and a permissionless pay (21) |
| EX2 `user_required_entered_account_is_refused…`. Entered with debt. | Emergency exit **#2116**; `exit_market` reverts |
| EX3 `user_required_emergency_exit_works_while_paused…`. Not-entered holder; StgV1 deposit latch, every controller pause on both markets, CF 0, CALI trapped and +10,000 s. | Half exit (89 entries, 62.0M), then pay, then the rest without waiver (91) — all ok |
| EX4 `user_required_entered_debt_free_holder_exits…`. Entered debt-free; pauses, CALI trapped; then re-enter and CF 0. | `exit_market`, then half exit, then `enter_market`, then `emergency_disable_collateral`, then `exit_market` at CF 0, then the rest — all ok |
| F1a `f1_emergency_exit_works_when_the_controller_traps`. Group controller trapped; entered borrower with debt. | Emergency exit **succeeds** (90 entries, 60.6M). This is the documented fail-open trade-off |
| F1b `f1_paused_but_answering_controller…`. Every controller pause on, controller answering. | Entered owner refused **#2116** |
| F2 `f2_cf_zero_does_not_let_an_indebted_borrower_leave…`. CF 0 with debt. | `stage_prepare_close` **#2106**; `exit_market` reverts; emergency exit **#2116**; after `repay_max`: `exit_market` and full emergency exit ok |

**Findings from these rows:**
- **Emergency exit headroom.** The emergency exit peaks at **91 entries** (limit 100) and 62.0M CPU on these bytes.
- **Repay the full debt with `repay_max`.** A fixed-amount `repay` of 10.01 USDC against a 10 USDC loan left
  **3 raw units** of interest debt. `group_no_debt` then (correctly) refused `exit_market`. Frontend and bot
  "repay all" flows must use `repay_max`, or the owner cannot leave the market for an emergency exit.
- **Pool-trap scope.** A completely dead *pool* (all calls trapping) makes extraction impossible: the vault's
  `get_all_position_fees` traps. This is inherent. The native "pool trap" mocks only quote functions, so
  EX3/EX4 here trap the oracle only.
- **F1 and footprint omission.** Fail-open is not reachable by footprint omission. Host 29
  `HostError::is_recoverable` treats `Storage/ExceededLimit` (access outside the footprint) and budget
  exhaustion as non-recoverable, so `try_invoke_contract` cannot be forced to `Err` by a borrower trimming
  the footprint. An archived controller entry cannot be skipped either: the transaction fails before
  execution.

### Coverage limit

These rows do **not** exercise, under the public network id on Mainnet-feature bytes: liquidation (L1a–L5),
seizure, `sync_seized` and `checkpoint_fallback`/D3. `stage_prepare_close` with debt is covered only as the F2a refusal. The emergency exit is now covered above.
Those were measured only on Testnet-id and default builds (BUILD-REPORT §2). The manual review did not
re-read the receipt or controller parts of 6e83fd7 and 137de32 in depth. Almanax has not scanned anything,
because the subscription has expired.

## 0. Why the plan is a fresh stack

The live vault CD6N cannot be upgraded.
- **Code hash.** It runs e66380…e3e9 (123,430 B), the frozen Vault-B base; checked with `stellar contract fetch`.
- **Mainnet gate.** `propose_upgrade_wasm` and `upgrade_wasm` both begin with
  `assert!(!pilot_public_network(..), "pilot Ledger authority and code are fixed")`.
- **Binding gate.** Both also call `pilot_require_unbound_configuration`, and the vault is bound:
  `stage_receipt` returns CCGK.
- **Simulation.** A live simulation of `propose_upgrade_wasm` with the right admin traps with
  `Error(WasmVm, InvalidAction)`.
- **Custody moves are frozen.** On the bound live vault, every custody reassign entrypoint is also refused.

StgV2's L5 (`sync_seized`) calls `stage_seize_reassign`, and its fallback calls `stage_idle`. Both exist
only in vault v2. Upgrading CCGK alone would therefore let L3 seize units while L5 could never sync them,
so CCGK and CD6N must stay untouched. The user chose option A.

## 1. Frozen bytes quoted here (pre-upgradeability)

Canonical build:
- `stellar contract build --locked`, stellar 26.1.0, rustc 1.96.0;
- init-admin constant GC4T…7FP;
- OpenZeppelin dependency at `/Users/joshua/Peridot/Peridot-Soroban/openzeppelin-stellar-contracts` (3f81125);
- artifacts in `target/clg/freeze/art/`.

| Contract | Bytes (< 131,072) | sha256 | Note |
|---|---:|---|---|
| Vault v2 | 124,762 | `72286249db318fcd459b666c994c83a167744719dec611f54f320eb2706b2394` | Reproduces the BUILD-REPORT hash exactly |
| Receipt StgV2 (`stg-mainnet-accepted,stg2-mainnet-accepted`) | 124,901 | `e6695dbdb29f27130729516d86135a169a44ae0f49ac82be93c58a858dbdc28e` | **Differs from the measured 47b4a147**, explained below |
| Group controller (`--features group`) | 64,267 | `debe1a36e658f9a6d491d7fc7f13d1c92e99b4e8fe27f1cc69242a8a4f66f55c` | Reproduces exactly |
| USDC group market | 89,808 | `5630a701bc7f29ea1334cfd1c12e349664da7376fd86df080d587ea9f685e452` (cash-upgrade code, **already on chain**; see §3b) | 4229b7ef was used in the first emulation run |

- **Why the receipt hash differs.** The measured 47b4a147 is the *default* (non-Mainnet) build, and it was
  measured with the Testnet network id (`harness/src/real.rs`). Neither the Mainnet-feature receipt nor any
  Mainnet gate had been exercised before §3.
- **These hashes will change.** The upgradeability stream is editing the vault, receipt and controller
  sources in this worktree. Re-run `tools/mainnet-sims.cjs quote` on the final bytes.
- **Size limit for the final bytes.** The live per-transaction write limit is 132,096 B, and an upload writes
  the code size plus about 100 B. A final WASM must therefore stay under the 131,072 project limit. The vault
  has 6,310 B of headroom.

## 2. Live Mainnet simulations (RPC `simulateTransaction`, ledger 65592356–65592357, protocol 29)

Tool: `tools/mainnet-sims.cjs`. Manifest: `tools/manifest-fresh-stack.json`. Raw output:
`sims/quote-fresh-stack-20261121T075710Z.json`.

How the figures are derived:
- `cap` = min resource fee × 1.2 + 10,000 stroops (the cash ceremony padding), plus a 10,000-stroop bid.
- The payer and source of every step is the admin Ledger **GC4T…7FP**.
- No step needs an authorization entry beyond the source account.

| Step | Simulated against | min resource fee (XLM) | cap (XLM) | CPU | write B | footprint ro/rw |
|---|---|---:|---:|---:|---:|---|
| U1 upload vault v2 | exact frozen bytes | **166.5830631** | 199.9016757 | 105,990,712 | 124,864 | 0 / 1 |
| U2 upload receipt StgV2 | exact frozen bytes | **161.8481089** | 194.2197306 | 106,107,577 | 125,004 | 0 / 1 |
| U3 upload group controller | exact frozen bytes | **91.5930759** | 109.9136910 | 55,134,666 | 64,368 | 0 / 1 |
| C1 create vault | proxy: live code e6638030 | 0.0209352 | 0.0271222 | 1,935,720 | 104 | 1 / 1 |
| C2 create receipt | proxy: live code 140dcaab | 0.0209333 | 0.0271199 | 1,871,457 | 104 | 1 / 1 |
| C3 create controller | proxy: live code 847894a6 | 0.0209222 | 0.0271066 | 1,500,690 | 104 | 1 / 1 |
| C4 create USDC market | **real** code 4229b7ef | 0.0209282 | 0.0271138 | 1,701,360 | 104 | 1 / 1 |

Notes:
- **Upload CPU is above the 100M project policy for financial transactions.** Uploads use the separate
  upload allowance, as the pilot did: its vault upload ran at 104.87M and succeeded. The network limit is
  400M instructions.
- **What uploads actually cost.** Upload fees are mostly refundable rent and write fees. The pilot paid
  143.33 XLM of a 164.83 XLM bid for the vault and 126.25 of 145.19 XLM for the receipt, about 87% of the
  min resource fee.
- **Why create can use a proxy.** Create cost does not depend on code size, apart from one code-entry read.
  All four creates came out within 0.0209–0.0210 XLM.

### Steps that cannot be simulated live yet

RPC simulation runs only against current ledger state. It cannot simulate `create` before the code is
uploaded, or `initialize` and later steps before the instance exists. Stellar RPC has no state override, so
these cannot be bundled into one simulation.

So **every step is simulated live immediately before it is signed**:
- the ceremony prepares each transaction from a fresh `simulateTransaction` at the current ledger;
- it does so only after the previous step's read-back confirms;
- it refuses if the fee exceeds the step cap.

For an ad-hoc check, `mainnet-sims.cjs invoke <contract> <method> '<json args>' --types …` simulates any
call on a live contract, and `create <hash>` works the moment a hash is uploaded.

## 3. Mainnet-gate emulation of the whole ceremony (frozen bytes, public network id)

**What this is.** An emulation, not RPC.
- **State.** Live Mainnet state refreshed at **ledger 65592376** (close 1795247926) with
  `state-driver.cjs refresh`. Missing keys were filled from RPC, and the final run had 0 unknown keys.
- **Network id.** The **public** id, so every `pilot_public_network`, `stg-mainnet-accepted` and
  `stg2-mainnet-accepted` gate is live.
- **Bytes.** The frozen bytes above, as fresh instances.
- **Everything else is real:** the Aquarius pool, CALI, CAFJ, the PYUSD/USDC/AQUA SACs, and the keeper
  account GBHW…ZON4.
- **Auth.** `mock_all_auths`, with test capital minted under host-mocked SAC admin auth.
- **Code.** `harness/src/fresh.rs`, test `fresh_mainnet`.
- **Evidence (outside git).** `target/clg/fresh/state/fresh-result.json`, sha256
  `1bfa15c69bbf53c3c11d9391bdfd8073b5c5ce1d3e9933773d30ed50d8b9d5e5`; state.json sha256 `ee0681a0…a3c`.

**Result: all 46 rows succeed.** The largest is F7 `stage_invest` at 56 entries, 58 keys max overall and
57.8M CPU, all within 100 / 100M.

| # | Step (one transaction each; signer) | ok | CPU | entries (m+d+w) | keys | auth |
|---|---|---|---:|---|---:|---:|
| V1 | vault.initialize(admin, pool CAPI…DQYX, 0, CALI) | yes | 6,913,118 | 10 (7+1+2) | 8 | 1 |
| V2 | vault.set_primary_reward_token(AQUA CAUI…OJPK) | yes | 2,116,515 | 6 | 4 | 1 |
| V3 | vault.set_max_deploy(10,000,000,000) | yes | 2,063,892 | 6 | 4 | 1 |
| R1 | receipt.initialize(PYUSD, 0, 0, admin) | yes | 5,568,900 | 61 (36+1+24) | 37 | 1 |
| G1 | ctrl.initialize(admin) | yes | 3,362,377 | 29 | 17 | 1 |
| G2 | ctrl.set_oracle(CAFJ) (first set is immediate) | yes | 4,333,903 | 21 | 15 | 1 |
| G3 | ctrl.set_oracle_asset_symbol(USDC, Some("USDC")) | yes | 3,180,720 | 18 | 14 | 1 |
| M1 | usdc.initialize(USDC, 0, 0, admin) | yes | 5,459,461 | 61 | 37 | 1 |
| M2 | usdc.set_min_cash_reserve_bps(admin, 1000) | yes | 4,209,245 | 36 | 34 | 1 |
| M2b | usdc.set_cash_borrowing_enabled(admin, true) | yes | 4,250,838 | 36 | 34 | 1 |
| M3 | usdc.set_borrow_rate(80000) | yes | 6,943,294 | 37 | 34 | 1 |
| M4 | usdc.enable_static_rates(admin) | yes | 4,418,540 | 37 | 35 | 1 |
| M5 | usdc.seed_minimum_supply(admin, 10000) | yes | 8,131,423 | 46 | 39 | 1 |
| M6 | usdc.set_peridottroller(group ctrl) | yes | 12,177,950 | 56 | 54 | 1 |
| G4 | ctrl.add_market(usdc market) | yes | 6,321,074 | 53 | 47 | 1 |
| G5 | ctrl.add_market(receipt) | yes | 6,422,782 | 53 | 47 | 1 |
| G6 | ctrl.set_pause_borrow(receipt, true) | yes | 3,677,385 | 19 | 16 | 1 |
| R2 | receipt.set_peridottroller(group ctrl) | yes | 13,929,121 | 60 | 58 | 1 |
| R3 | receipt.set_min_cash_reserve_bps(admin, 10000) | yes | 4,374,445 | 36 | 34 | 1 |
| R4 | receipt.set_supply_cap(10,000,000) | yes | 4,364,296 | 36 | 34 | 1 |
| R5 | receipt.seed_minimum_supply(admin, 10,000,000) (1 PYUSD from the owner) | yes | 8,339,083 | 46 | 39 | 1 |
| V4 | vault.stage_bind(admin, receipt) | yes | 5,981,603 | 42 | 39 | 1 |
| V5 | vault.stage_configure_guardian(admin, admin) | yes | 2,157,164 | 6 | 4 | 1 |
| V6 | vault.stage_set_invest_keeper(admin, keeper GBHW…ZON4) | yes | 2,154,910 | 6 | 4 | 1 |
| R6 | receipt.stage_activate(admin, vault, admin, backing cap) | yes | 8,332,868 | 41 | 39 | 1 |
| R7 | receipt.stage2_activate(admin, {ctrl, pool, CALI, USDC}) | yes | 4,812,046 | 36 | 34 | 1 |
| G7 | ctrl.group_set_sleeve(receipt) | yes | 2,500,795 | 15 | 13 | 1 |
| G8 | ctrl.group_set_liq_cache_only(true) | yes | 2,352,085 | 15 | 13 | 1 |
| G9 | ctrl.set_market_cf(receipt, 500000) (first set is immediate; later changes are timelocked) | yes | 3,311,600 | 20 | 14 | 1 |
| G10 | ctrl.set_pause_guardian(admin) | yes | 3,237,905 | 16 | 14 | 1 |
| F1–F8 | First command, 100 PYUSD: stage_open (owner) → convert → mint → 3 keeper checkpoints (keeper signs) → invest → finalise (permissionless) | yes | ≤ 57,826,233 | ≤ 56 | ≤ 39 | — |
| B1–B6 | checkpoint_nav → enter_market(receipt) → **enter_market(USDC market)** → cache_price(USDC via CAFJ) → borrow 10 USDC → repay | yes | ≤ 21,577,457 | ≤ 63 | ≤ 54 | — |

Findings from the emulation:
- **The Mainnet gates pass with the frozen Mainnet-feature bytes in this order.** Checked: the pinned
  pool/oracle/admin asserts in `initialize` and `stage_bind`, `stg-mainnet-accepted` on `stage_activate`,
  `stg2-mainnet-accepted` on `stage2_activate`, and the requirement that the receipt is bound to the group
  controller before `stage2_activate`. This was the first exercise of any of them.
- **A borrower must also `enter_market` the USDC market before `borrow`.** The controller's
  `hypothetical_liquidity_with_hint` panics "market not entered" otherwise. The first run reproduced this
  as a trap. The frontend and bot flow must send this call; it is not a contract defect.
- **CAFJ prices USDC correctly.** `Other(USDC)` cached 1.0017234 with scale 1e14. NAV rate was 999,808
  (1e6 = 1 USDC per unit). After a 10 USDC borrow against 100 PYUSD at CF 50%, liquidity was 40.037 USDC.

### 3b. Same 46 steps with the USDC group market on 5630a701 (the cash-upgrade code)

**5630a701 is compatible with the group controller, and the market should be created from it, not
4229b7ef.**
- **Result:** all 46 rows pass.
- **Identical footprint:** every row has the same entries, keys and writes as with 4229b7ef.
- **CPU:** +0.05 to +0.19M on the market rows (M1 5.51M, B4 borrow 21.76M, B6 repay 16.40M).
- **ABI:** the receipt-vault ABI the controller uses is unchanged. 5630a701 adds the flash-loan switch
  (off by default), exact flash fees, and `cancel_upgrade`.
- **Upload:** none needed; it is already on chain, live until ledger 67617548.
- **Evidence:** `target/clg/fresh/state/fresh-result-5630a701.json`, sha256 `a0b558fa…bbd7`.

### 3c. Vault v2 upgrade gate on the fresh stack (commit 6e83fd7 bytes)

Bytes and setup:
- **Vault v2 (upgradeable):** `6fd8599a9b369ab84ba9ee8fd95642cde50621c07d5cc37cd278e8f225a4870e`,
  125,559 B, built from HEAD with `git archive`.
- **v2′ marker** (`--features cl-pyusd-vault-b,upgrade-marker`):
  `d3ea4f91e3509c86bfe768d6d90266ddbcbc5fab83c0a3bc1b96fc0406606e3f`, 125,627 B.
- **Other contracts:** receipt and controller are still the pre-upgradeability frozen bytes; the USDC
  market is 5630a701.
- **Evidence:** `target/clg/fresh/state/fresh-result-upgrade.json`, sha256 `fc10d5ec…4650`.

**How the 24 h wait is emulated.** The proposal runs with the snapshot clock set 24 h + 60 s back, for that
step only. Execution then runs at the real capture time, so the later steps see live oracle and pool
state. `propose` reads no time-dependent external state.

| Row | Result |
|---|---|
| UC1–UC5: propose, then `cancel_upgrade`; `get_pending_upgrade` returns None; pause; `upgrade_wasm` at +24 h | Reverts as expected (nothing pending) |
| U1, U1b: propose; `get_pending_upgrade` returns (d3ea4f91…, ETA) | ok |
| U1c, U2: pause; `upgrade_wasm` before the ETA | Reverts (timelocked) |
| U3a, U3: after the ETA, unpause; `upgrade_wasm` while unpaused | Reverts ("pause vault before upgrade") |
| U5, U4: pause; `upgrade_wasm` with a wrong hash | Reverts |
| U6: `upgrade_wasm(admin, v2′)` | **ok**, 11 entries, 2.44M CPU |
| U7: `upgrade_marker()` | Returns **2**, so the new code is running |
| U8: `get_pending_upgrade()` | None |
| U9: unpause | ok |
| **State preserved** | Every field compares equal before and after the upgrade: vault supply, position liquidity, owner beneficial, ticks, binding, reward totals, receipt units, stage_state, stage2_config |
| P1–P8: real second command on the upgraded vault (open 100 PYUSD → convert → mint → 3 keeper checkpoints → invest → finalise) | **All ok.** P3 `stage_mint` 74 entries / 59.6M CPU (the first mint was 44); P7 `stage_invest` 56 / 59.4M |
| P9: `checkpoint_nav` | Reverts #2105 OracleStale. This is an **emulation artefact**: the captured CALI timestamp is frozen, and by P9 the emulated clock is more than 600 s past it. B1 shows the same call succeeding while the price is fresh |

## 4. Fee quote, step by step, in XLM (payer: admin Ledger GC4T…7FP unless stated)

| Steps | Basis | Expected | Cap reserved at signing |
|---|---|---:|---:|
| U1 upload vault | live sim, exact bytes | ≈ 145 (87% of 166.58) | 199.90 |
| U2 upload receipt | live sim, exact bytes | ≈ 141 (87% of 161.85) | 194.22 |
| U3 upload controller | live sim, exact bytes | ≈ 80 (87% of 91.59) | 109.91 |
| C1–C4 create ×4 | live sim (3 proxy, 1 real) | 4 × 0.018 = 0.07 | 4 × 0.027 = 0.11 |
| V1–G10 configuration, 30 transactions | Pilot actual `fee_charged` per analogous step (Horizon). Examples: initialize-vault 0.2687, initialize-receipt 0.4688, activate 0.1331, setters 0.0005–0.08. New steps without an analogue are budgeted at 0.03–0.47. Each is re-simulated live before signing | ≈ 2.5 | ≤ 5.0 (2× expected) |
| First command F1–F8 | Owner signs F1 (others permissionless; the keeper signs F4–F6 from its own balance) | ≈ 1 | ≤ 2 |
| **Total** | | **≈ 370** | **≈ 511** |

**Withdrawal needed first.**
- **Today's balance.** At 21 Nov 07:5x UTC the admin wallet holds **172.29 XLM**. With 4 subentries the
  reserve is 3 XLM, so about 169.3 XLM is spendable.
- **Worst case.** Without counting on refunds between steps, the wallet must cover every cap: 511 + 3
  reserve + about 20 for the bytes growing with the upgradeability change, so about 534 XLM.
- **Recommendation.** Withdraw **≥ 365 XLM** from the owner's XLM-market deposit (CBHMG4CN…4WMS).
  **400 XLM = 4,000,000,000 raw pTokens** at today's rate of exactly 1.0 gives margin.
  - Owner pTokens: 287,584,313,671 raw.
  - Market available liquidity: 287,634,313,671 raw.
  - Prepared with the existing `ceremony.cjs withdraw XLM 4000000000` in the main checkout.
- **Minimum if refunds are counted.** After each upload the unused refundable part returns, so a smaller
  withdrawal would work if steps are strictly sequential. It needs at least 200 XLM free at the first upload
  and about 370 spent in total, so withdraw at least 210 XLM.

**Not in this quote:**
- the PYUSD for the first command and the 1 PYUSD floor (`seed_minimum_supply`);
- the 0.001 USDC market seed;
- USDC supplier liquidity;
- liquidator inventory.

The owner holds 6.83 PYUSD and 17.71 USDC on the account today. Funding a real seed requires closing the
live pilot (StgV1 close on CCGK) or other PYUSD.

## 5. Kill switches available on the fresh stack (96e7b3c bytes)

| Switch | Who | Effect |
|---|---|---|
| `ctrl.set_pause_borrow / set_pause_deposit / set_pause_redeem / set_pause_liquidation(market, true)` | admin; guardian via `*_g` | Per-market pauses. Borrow pauses expire after 72 h (re-arm, as on CCGK) |
| `ctrl.emergency_disable_collateral(receipt, true)` | admin, immediate | CF → 0. **Every CL borrower with debt becomes liquidatable at once.** A last resort |
| `ctrl.set_market_cf` | admin | First set immediate; later changes timelocked (`admin_param_change_delay_secs`) |
| `usdc.set_cash_borrowing_enabled(false)` | admin | Durable borrow kill on the USDC group market |
| `usdc.set_borrow_cap / receipt.set_supply_cap` | admin | Caps |
| `vault.stage_pause_deposits` / `receipt.stage_pause_deposits` | admin (Ledger guardian) | **One-way** pause of new staged commands; no resume in these bytes |
| `receipt.stage_lower_backing_cap` | admin | Lower the staged backing cap |
| Owner exits | owner | `stage_prepare_close` (health-checked when the owner has debt); `stage_emergency_exit` only when not entered (G10 lets a debt-free owner exit the market first) |
| Upgrades | admin (pinned Ledger) | All three contracts: 24 h `propose_upgrade_wasm` / `upgrade_wasm`, plus `cancel_upgrade` and `get_pending_upgrade`. The vault executes only while admin-paused. The receipt executes only with the controller's deposit, redeem and borrow pauses on it |
| `vault.set_paused(admin, bool)` | admin | **Reversible** admin pause (works while bound from 6e83fd7). Blocks new deposits, investment and swaps. In-kind close, emergency exit and seizure sync do not read it |

## 6. Reproduce

```bash
# live quotes (read-only)
node research/cl-collateral-group-20261118/tools/mainnet-sims.cjs quote \
  --manifest research/cl-collateral-group-20261118/tools/manifest-fresh-stack.json --out /tmp/q.json
# emulation (fresh live state, public network id)
STELLAR_SDK_DIR=<main checkout>/bots/margin-liquidator/node_modules/@stellar/stellar-sdk \
  SPIKE_STATE_DIR=$PWD/target/clg/fresh/state node prototypes/cl-collateral-group/state-driver.cjs refresh
(cd prototypes/cl-collateral-group/harness && SPIKE_STATE_DIR=$PWD/../../../target/clg/fresh/state \
  SPIKE_WASM_DIR=$PWD/../../../target/clg/fresh/wasm cargo test --release fresh_mainnet -- --exact --nocapture)
```
