# CL collateral group v2: upgradeability behind the admin Ledger (21 Nov 2026)

Branch `feat/cl-collateral-group`. Nothing was signed, sent, uploaded, deployed or upgraded on any network.
This covers the NEW fresh-stack contracts only. The live pilot vault CD6N…WL63 and its code path are unchanged.
CD6N still runs e6638030, whose upgrade path asserts `!pilot_public_network`; see MAINNET-SIMS.md §0.

## 1. Decision and cost

- **Estimate sent to the lead:** about 0.5 day, well under the 1-day threshold. The change is therefore in v2, not deferred to v3.
- **Actual work:**
  - About 110 lines of contract code across three contracts.
  - Native, compiled-bytes and fresh-stack emulation tests.
  - About +0.8 KB per large contract. All three stay at least 5.4 KB under the 131,072 B limit.

## 2. Contract changes (code commit 6e83fd7)

The three contracts share one upgrade shape, the same as the cash markets in `.wt-cash-upgrade`:
- `propose_upgrade_wasm(hash)` records the target and sets ETA = now + 86,400 s.
- `upgrade_wasm(hash)` requires an exact hash match and now ≥ ETA, clears the pending state, then swaps the code.
- `cancel_upgrade` removes the hash and ETA; it reverts if nothing is pending.
- `get_pending_upgrade() -> Option<(BytesN<32>, u64)>` reads the pending state.
- Each step emits an event: `upgrade_proposed(hash, eta)`, `upgrade_cancelled(hash)` and `upgraded(hash)`, each with a single-symbol topic.
- Re-proposing overwrites the target and restarts the full 24 h.

| Contract | Auth | Extra execute precondition |
|---|---|---|
| Vault v2 (`cl-share-vault`) | `admin_addr == stored admin` and its signature. On Mainnet, `admin_addr` must also equal the pinned Ledger `GC4T5MP5…7FP`. | `params.paused == true` (the reversible admin pause) |
| Receipt StgV2 | Stored admin `require_auth` (unchanged base code) | The group controller has deposit, redeem and borrow paused for this market (unchanged base code) |
| Group controller (patch G11) | `require_admin` (unchanged base code) | none |

### Vault changes

- **Mainnet gate removed while bound.** `propose_upgrade_wasm` and `upgrade_wasm` lost `assert!(!pilot_public_network)` and `pilot_require_unbound_configuration`. Both now call `pilot_require_pinned_admin` and then `require_admin`.
- **Admin stays fixed on Mainnet.**
  - `initialize` already pins the Ledger through the compiled init-admin constant and `pilot_require_pinned_admin`.
  - `set_admin` and `accept_admin` keep their Mainnet freeze. So "behind the admin Ledger" holds for the vault's lifetime.
  - Changing the vault admin is itself an upgrade decision.
- **`set_paused` usable while bound.** `pilot_require_unbound_configuration` was replaced by `pilot_require_pinned_admin`.
  - All other legacy configuration setters stay frozen once the vault is bound.
- **Why the reversible pause and not the guardian pause.** `stage_pause_deposits` is the guardian pause: it is one-way, and no code path resumes it. Gating on it would close deposits permanently at every upgrade. The admin pause is reversed with `set_paused(admin, false)`, on either the old or the new code.
- **What `params.paused` blocks:**
  - new money: `deposit`, `stage_admit_deposit`, `stage_cash_mint` and `stage_invest_cash`;
  - `stage_swap`, which covers `stage_convert_open` and the swap leg of `stage_convert_exit`;
  - the deposit side of `quote`.
- **What it does not block:**
  - in-kind close (`stage_lock_close`, `stage_withdraw_kind`);
  - the emergency pair (`stage_emergency_extract`, `stage_emergency_pay`);
  - seizure sync (`stage_seize_reassign`);
  - cancels (`stage_cancel_kind`) and `stage_finalise_cash`.

  During the pause window, owners can therefore still leave in kind, and liquidation steps L3–L5 still settle.
  A close that wants its USDC converted to PYUSD has to wait for the unpause or take the in-kind route.
- **Test-only marker.** Feature `upgrade-marker` adds `upgrade_marker() -> 2` in its own `#[contractimpl]` block. The release build never enables it.
- **Build script.** `build-vault.sh` gained `FEATURES_EXTRA`, appended to `cl-pyusd-vault-b`.

### Receipt changes

These are additive anchors in `apply-stgv2.py`, which still asserts zero removed lines:
- `cancel_upgrade`, `get_pending_upgrade` and the three events;
- the test-only `stg2-upgrade-marker` feature and its marker block.

### Controller changes

Patch G11 in `apply-patches.py`, not feature-gated:
- `cancel_upgrade`, `get_pending_upgrade` and the three events;
- the test-only `upgrade-marker` feature.

The pre-G11 controller rebuild still reproduces debe1a36.

### Admin transferability

- **Vault:** not transferable on Mainnet (frozen).
- **Receipt and controller:** the admin stays transferable through `set_admin`/`accept_admin` (two-step, base code). Both are initialized to the Ledger by their compiled init-admin constants. A transfer needs the Ledger's signature.

## 3. Invariants an upgrade must preserve (review checklist, not enforceable in code)

Soroban `contracttype` encodes enum variants by name, struct fields by name, and the enum or struct name not at all.
1. **Keys.**
   - Never rename or remove a `DataKey`, `StageVaultKey`, `StgKey` or `Stg2*` variant, nor any reward or custody key. Add new keys only.
   - Renaming a variant silently orphans its stored value.
2. **Stored structs.**
   - Never add, remove or rename fields of stored structs: `Config`, `Params`, `State`, `PositionAmounts`, `StageJob`, `StageAllocation`, the journals, reward `Holder`/`Totals`/`Boundary`, the receipt staged state and `Stg2Config`.
   - The existing code's pattern ("keep new variants at the end and outside Config") is the rule: new state goes under new keys.
3. **Stage commands in flight.**
   - A command carries its phase and its `StageJob` through the receipt and the vault, so the new code must accept every phase the old code can leave behind.
   - The admin pause stops new opens from advancing past funding. Opens already funded can still be cancelled.
4. **Custody conservation.** Total supply equals the receipt's custody balance, which equals Σ beneficial, before and after. The upgrade itself runs no accounting.
5. **Receipt sequencing.**
   - Receipt `upgrade_wasm` calls `update_interest` before the swap.
   - The pause precondition reads the bound controller, and the StgV2 binding is fixed. A controller upgrade must therefore keep `is_deposit_paused`, `is_redeem_paused` and `is_borrow_paused`.
6. **Reapply the overlays to new code.** A v3 built from new source must reapply this upgrade path and the Mainnet pinned-admin check. Otherwise it is the last upgrade.

## 4. Tests

| Suite | Where | Result |
|---|---|---|
| Vault native, Mainnet network id, bound vault, real entrypoints | `vault-v2/contracts/cl-share-vault/src/upgrade_tests.rs` | 4/4 |
| Vault compiled bytes v2 (6fd8599a) to v2′ (d3ea4f91), `#[ignore]`d by default | same file, `--include-ignored` | 2/2 |
| Whole vault crate | `cargo test --locked -p cl-share-vault --features cl-pyusd-vault-b -- --include-ignored` | 68/68 |
| Controller and receipt compiled bytes, Mainnet id, against their marker builds | `prototypes/cl-collateral-group/upgrade-tests` (`cargo test`) | 2/2 |
| Fresh-stack emulation, live Mainnet state, public id (Mainnet-sim agent) | MAINNET-SIMS.md §3c and FINAL | upgrade rows U1–U9 pass, P1–P8 pass |

### Vault native tests

- **Propose, view, cancel.** Cancel clears the pending state. A second cancel reverts. A cancelled hash cannot execute. Re-proposing restarts the 24 h.
- **Execute gates.** ETA − 1 s reverts. A wrong hash reverts. An unpaused vault refuses the upgrade. A refused attempt consumes nothing.
- **Unauthorized callers.**
  - A non-admin address is refused for propose, `set_paused`, cancel and execute.
  - The pinned address without its signature (only an attacker's auth mocked) is refused for `set_paused`, cancel and execute.
  - A vault whose stored admin is not the Ledger is refused on Mainnet even when that admin signs.
- **Pause scope.** With the vault paused:
  - seize-reassign, close lock and cancel, open cancel, in-kind withdraw and the emergency extract/pay pair all succeed, with exact payouts;
  - `stage_cash_mint` is refused;
  - unpausing works.
- **Compiled-bytes tests:**
  - v2 has no marker.
  - Propose, pause, and ETA − 1 s reverts. At ETA the upgrade succeeds and the marker returns 2.
  - Admin, supply, custody balance, beneficial units, binding, config, params and reward account are all equal after the upgrade; params differ only in `paused`.
  - Unpause works. v2′ can be proposed and cancelled again.
  - A non-admin is refused; cancel blocks execution; `set_admin` stays frozen.

### Controller and receipt compiled tests

- **Controller:**
  - Propose and cancel by a non-admin are refused. Propose sets the ETA; cancel clears it; a second cancel or an execute then reverts.
  - Execute is refused at ETA − 1 s, on a wrong hash and from a non-admin.
  - The upgrade to v2′ (5d96a522) succeeds. The marker returns 2, and admin and close factor survive.
- **Receipt (Mainnet features):**
  - Same propose, cancel and non-admin coverage.
  - Execute is refused before ETA, unpaused, and with only deposit and redeem paused.
  - With all three paused, a non-admin is refused and the admin's upgrade to v2′ (67380219) succeeds. The marker returns 2.
  - Admin, underlying, supply, exchange rate, cash-reserve setting and controller listing survive.
  - The pauses lift, and propose/cancel works again.
- **Deposits after an upgrade.** The fresh receipt takes deposits only through the staged vault route, so "deposits work after an upgrade" on real bytes is the emulation's P1–P8: open, convert, mint, three checkpoints, invest and finalise on the upgraded vault.

## 5. Artefacts

`target/clg/upg/`, listed in `SHA256SIZES.txt`. All built with `stellar contract build --locked` and the Mainnet admin constant.

| Artefact | Bytes | sha256 |
|---|---:|---|
| `vault-v2/cl_share_vault.wasm` (release) | 125,559 | `6fd8599a9b369ab84ba9ee8fd95642cde50621c07d5cc37cd278e8f225a4870e` |
| `receipt-v2-mainnet/receipt_vault.wasm` (release: `stg-mainnet-accepted,stg2-mainnet-accepted`) | 125,589 | `bf8864e11a6f40740fc15b2832b07eaa58523310c8d3c4fc931162ebe6f41da6` |
| `ctrl-group/simple_peridottroller.wasm` (release: `group`, G0–G11) | 64,931 | `719914b17f2ceeffe4e77fe0e0a5369066da4ccb8201691e09fea5333710234d` |
| `receipt-v2-default` (refuses Mainnet activation) | 125,600 | `68b7d6f7f116374d7fa8ca6f661fbd689775576b4e13a6b057d86d7835506c97` |
| `vault-v2-marker` (test only) | 125,627 | `d3ea4f91e3509c86bfe768d6d90266ddbcbc5fab83c0a3bc1b96fc0406606e3f` |
| `receipt-v2-marker` (test only) | 125,666 | `6738021942991d27fde78b42dec1f091abb56df17a2cdb1a9839da150f7a9b6f` |
| `ctrl-group-marker` (test only) | 64,999 | `5d96a52271ad6c1a76823f15f8978fc4f627b716c356ce854d10a13fccb5e499` |

For comparison, the pre-change bytes were:
- vault 72286249, 124,762 B;
- controller debe1a36, 64,267 B;
- receipt default 47b4a147, 124,916 B;
- receipt with the Mainnet features dbc42f12, 124,895 B.

### Reproduction

- **Vault:** `bash prototypes/cl-collateral-group/build/build-vault.sh prototypes/cl-collateral-group/vault-v2 <ABSOLUTE out dir>`. Set `FEATURES_EXTRA=upgrade-marker` for v2′.
  - Pass an absolute out dir: both build scripts `cd` into the workspace first, so a relative out dir lands inside the workspace.
- **Controller:** `python3 -I prototypes/cl-collateral-group/controller/apply-patches.py`, then `stellar contract build --locked --features group` with `SIMPLE_PERIDOTTROLLER_INIT_ADMIN` set to the Ledger.
- **Receipt:** `apply-stgv2.py --base target/clg/ws/receipt-base --out target/clg/ws/receipt-v2`, then `build-receipt.sh target/clg/ws/receipt-v2 <abs out> stg-mainnet-accepted,stg2-mainnet-accepted`.
  - The tree's materialized sources were verified identical to a fresh application of HEAD's overlay, which includes test-agent fix F1, 137de32.
  - The receipt hash depends on the tree path, because the OpenZeppelin dependency is path-relative (BUILD-REPORT §4.8, MAINNET-SIMS reproducibility note). bf8864e1 is reproducible only at `target/clg/ws/receipt-v2` in this worktree. Rebuild at the canonical path before upload if that is the release rule.

## 6. Open points

- If the pending controller fix F2 (G10 path) lands, the controller hash changes again and must be requoted. The receipt already includes F1.
- **No upgrade guardian or veto.** The Ledger alone proposes, cancels and executes. The 24 h window lets observers react through the `upgrade_proposed` event and the `get_pending_upgrade` view; it does not stop the Ledger.
- **Upgrade order.** The receipt's upgrade precondition depends on the controller's pause views. Upgrade the controller first only if its new code keeps those views.
