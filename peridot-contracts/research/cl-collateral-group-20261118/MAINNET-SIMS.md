# CL collateral group, fresh stack (option A): Mainnet simulations and fee quote (21 Nov 2026)

Stream 2. Read-only throughout: nothing was signed, sent, uploaded, deployed or upgraded. The network was
used only for `getLatestLedger`, `getAccount`, `getLedgerEntries` and `simulateTransaction` on
`https://soroban-rpc.mainnet.stellar.gateway.fm`, plus Horizon reads of pilot transactions.

## FINAL: release candidates of commit 6e83fd7 (this section supersedes the figures in §1, §2 and §4)

Artifacts are in `target/clg/upg/` (SHA256SIZES.txt there). All are under 131,072 B, and every upload
write stays under the live 132,096 B per-transaction write limit.

| Contract | Bytes | sha256 |
|---|---:|---|
| Vault v2 (upgradeable) | 125,559 | `6fd8599a9b369ab84ba9ee8fd95642cde50621c07d5cc37cd278e8f225a4870e` |
| Receipt StgV2 (Mainnet features) | 125,589 | `bf8864e11a6f40740fc15b2832b07eaa58523310c8d3c4fc931162ebe6f41da6` |
| Group controller (G0–G11) | 64,931 | `719914b17f2ceeffe4e77fe0e0a5369066da4ccb8201691e09fea5333710234d` |
| USDC group market | 89,808 | `5630a701…e452`, already on chain |

**Live RPC quote.** Ledgers 65592556–65592557 (`sims/quote-final-20261121T081351Z.json`, manifest
`tools/manifest-final.json`). The payer and source of every step is the admin GC4T…7FP.

| Step | min resource (XLM) | cap (XLM) | CPU | write B |
|---|---:|---:|---:|---:|
| U1 upload vault v2 | 167.6576637 | 201.1911964 | 106,660,682 | 125,660 |
| U2 upload receipt StgV2 | 162.7074984 | 195.2509980 | 106,685,953 | 125,692 |
| U3 upload group controller | 92.3864607 | 110.8657528 | 55,692,865 | 65,032 |
| C1–C4 create ×4 | 4 × 0.0209 | 4 × 0.0271 | ≤ 1.94M | 104 |
| Configuration, 30 transactions (§4 basis) | ≈ 2.5 expected | ≤ 5 | — | — |
| First command | ≈ 1 | ≤ 2 | — | — |
| **Total** | **≈ 372 expected** (uploads at about 87% of min resource) | **≈ 514.5** | | |

**Withdrawal.** Recommend **400 XLM = 4,000,000,000 raw pTokens** from XLM market CBHMG4CN….
- The worst case is 514.5 + 3 reserve + buffer against 172.29 XLM held.
- The strict minimum, relying on the refunds between sequential uploads, is about 230 XLM.

**Emulation on these exact bytes.** Public network id, live state at ledger 65592376,
`target/clg/fresh/state/fresh-result-final.json`, sha256 `de1e2c75…a669`.
- All 30 ceremony rows pass, then the first command (8), borrow and repay (8), and every vault
  upgrade-gate row: cancel, timelock, unpaused refused, wrong hash refused, then `upgrade_wasm` to the
  d3ea4f91 marker succeeds.
- **State is preserved** across the upgrade.
- The post-upgrade second command passes P1–P8; P9 is the frozen-oracle artefact described in §3c.
- **Maxima:** 74 entries (second `stage_mint`), 59.6M CPU.

**Reproducibility note on the receipt.** bf8864e1 was built at `target/clg/ws/receipt-v2`. That tree's
Cargo.toml points the OpenZeppelin dependency at `../openzeppelin-stellar-contracts` (the ws copy), not at
the canonical absolute path `/Users/joshua/Peridot/Peridot-Soroban/openzeppelin-stellar-contracts`.
The receipt bytes depend on that path, as shown by e6695dbd versus dbc42f12 on the same pre-upgradeability
source. The rule from the cash release is "canonical path only". Either rebuild at the canonical OZ path
before upload, or record the exact tree path as part of the reproduction recipe.

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

## 5. Kill switches available on the fresh stack (current, pre-upgradeability code)

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
| Upgrades | admin | Group controller and receipt: 24 h `propose_upgrade_wasm` / `upgrade_wasm`; no cancel entrypoint (re-propose the deployed hash). **Vault v2 as frozen here: none on Mainnet** (same assert as CD6N). The upgradeability stream is changing this |

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
