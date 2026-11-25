# Sentora vaults as underlying for an isolated Peridot market group

Date: 2026-11-25. Status: research and prototype only. Nothing was signed, sent or deployed. No keys were used.

> **Phase 2 (25 Nov, owner decisions) supersedes the parameter proposals in §4 and the open questions in §10.**
> See §12 onwards: refined decisions, risk numbers, parameter table, PYUSD peg oracle, Sentora caps and kill path,
> reward path, measured footprint, keeper spec, licence handling and the updated test plan.

Branch `research/sentora-vault-markets` (worktree `.wt-sentora`) starts from `4c03928`, not `origin/main`.
`main` equals `origin/main` (`fe8d562`), and neither has the hardened ReceiptVault, `aquarius-lp-vault` or the
harnesses this work needs. `4c03928` was the committed HEAD of `feat/cl-share-stage1-checkpoint`.

## 0. Summary

- **These are not DeFindex vaults or share vaults.** All three run one 10.5 KB contract from IntoTheBlock's
  `StellarVaults` repository ("Sentora Smart Contracts"; wasm `ca6b85a1…`). It keeps a 1:1 principal record per
  depositor address.
  - The principal stays in the vault contract.
  - There is no share token, no NAV, no strategy, no Blend exposure, no view function and no on-chain yield.
  - Yield is a separate Merkle reward program. An admin publishes it per epoch on a shared escrow (wasm
    `f22e4a52…`), and it is paid in the deposit asset.
- **Option B (the vault share as the market underlying) is impossible as specified.** There is no transferable
  share and no share rate to price.
- **Recommended: Option A.** Each market's underlying is the plain asset. The ReceiptVault parks part of its idle
  cash in Sentora through a new small adapter, `contracts/sentora-strategy`, that speaks the existing boosted-vault
  ABI. It is prototyped and tested, including on a Mainnet fork against the real Sentora bytes and the deployed
  cash-group controller and receipt code.
- **Feasibility blockers** (owner and Sentora questions, §9). Neither is answered on-chain:
  - Is the program still live? In the last 7 days about 1.01 M USDC, 451 k XLM and 945 PYUSD were withdrawn.
  - Are contract depositors eligible for rewards? All 90 claims in the last 7 days came from G accounts.
- **Footprint.** Every measured operation fits the live network limits comfortably. Liquidation is 104–105
  footprint entries against the internal ≤100 rule. The same flow without Sentora already takes 98. These numbers
  are for a borrower in two markets with contract-type callers; a three-market borrower is unmeasured and will be
  higher.

## 1. Facts (Mainnet, ledger ≈65,663,700, 2026-11-25)

| | Vault 1 | Vault 2 | Vault 3 |
|---|---|---|---|
| Contract | `CA54LVHM…QEHJO` | `CAQRAXBU…QOYB` | `CAHEWHOP…MMJFF` |
| Deposit asset | XLM (SAC `CAS3J7…OWMA`) | PYUSD (`CCCRWH…HGU2`) | USDC (`CCW67T…MI75`) |
| WASM | `ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f` (all three; meta `source_repo: github:Into-The-Block-Corp/StellarVaults`, v1.0.0, SDK 23.0.2) | same | same |
| TotalPrincipal = vault SAC balance | 2,082,472.63 XLM | 286.31 PYUSD | 61,616.40 USDC |
| Deposit ids issued (`NextDepositId`) | 1,600 | 41 | 206 |
| Paused | no | no | no |
| Latest reward epoch (escrow) | ends 2026-11-09 18:00 UTC: 109,012.14 XLM, 689 leaves | ends 2026-11-19: 12.63 PYUSD, 29 leaves | ends 2026-11-19: 18,792.90 USDC, 91 leaves |
| 7-day flows (RPC events) | 35 withdrawals, 450,782 XLM, 0 deposits | 14 withdrawals, 945 PYUSD | 39 withdrawals plus 2 deposits, ≈1.01 M USDC (990 k from one address) |
| Fees | none on-chain (no fee code). Any off-chain cut is invisible in the reward leaves. | same | same |
| Exit | `withdraw(from, amount)` is immediate and 1:1 from the vault's own balance. No lockup or queue. Fails if paused. | same | same |

**Contract model.** Source is public, and the deployed interface matches it (`contracts/vault`, `contracts/escrow`,
commit `c29a8bf9`).

- `deposit(from, amount: u128, referral_id) -> deposit_id` does `from.require_auth()`, then
  `asset.transfer(from → vault)`. It adds to `Deposit(from) = {owner, amount}` (one aggregated record per address)
  and emits `DEPOSIT/create` with a fresh `deposit_id`.
- `withdraw(from, amount)` does `require_auth`, then reduces the record. It does `try_transfer(vault → from)`;
  a failure returns error 102.
- `Escrow` is stored but never used by the vault code.
- Persistent `Deposit(addr)` TTL: extended to ≤30 days on each touch (`LEDGER_WEEK, LEDGER_MONTH`).
- The instance is bumped the same way.
- There are no getters, so a contract cannot read its own principal from Sentora.

**Escrow** `CCZSNGVM…MHN3` (wasm `f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299`):

- `set_rewards_root(asset, vault, epoch, root, total, leaf_count, program_end_ts)` is admin-only. It requires
  `now ≥ program_end_ts`, strictly increasing epochs, and escrow balance ≥ committed + total.
- `claim(from, vault, epoch, {amount, deposit_id, leaf_index, proof})` requires `from.require_auth()`. The leaf is
  `sha256(0x00 ‖ xdr(vault, epoch, deposit_id, owner, amount))`, so a leaf pays only its owner. There is one claim
  per `(vault, epoch, deposit_id)`.
- `withdraw(actions)` lets the admin sweep any escrow balance above a kept amount. `sweep_expired_epoch` applies
  after 180 days.
- Escrow balances now: 19,762 XLM, 2.87 PYUSD, 544.0 USDC. About 97 % of the USDC epoch has already been claimed
  (18,249 USDC over 42 claims).

**Roles.**

- Vault and escrow admin is `GCOB2OVZ…Z5DI`, a classic account: 7 signers of weight 10, med/high threshold 40, so
  4-of-7. One signer is LOBSTR-vault-protected.
- That admin can, **instantly and with no timelock**:
  - `upgrade` the vault (and therefore all principal) and the escrow;
  - `set_status` (pause), which blocks deposits **and withdrawals**;
  - `update_admin`;
  - sweep the escrow.

**Sentora API and UI.** `vaults.sentora.com` is a JavaScript shell with API origin `services.vaults.sentora.com`,
which answered 403 to plain requests. `docs.sentora.com` does not resolve. APY, the fee split, proof distribution
and contract-address eligibility could not be sourced. They are owner questions.

**Read-only simulated footprints** (live RPC, real bytes): `deposit` 6 entries (2 RO / 4 RW), 0.99–1.02 M
instructions; `withdraw` 6 entries, 0.99 M instructions; escrow `set_rewards_root` 6 entries, 1.08 M instructions.

## 2. Risks

1. **Admin and upgrade trust (dominant).** A 4-of-7 multisig can replace the vault code and take all principal,
   or pause withdrawals, with no delay. Peridot inherits this for whatever it deploys into Sentora.
   - The adapter marks principal down to `min(principal, Sentora SAC balance)`. That catches only a drain below
     Peridot's own principal, not a partial or pro-rata loss, because `TotalPrincipal` cannot be read by a contract.
   - The real control is an off-chain monitor: compare the vault balance with `TotalPrincipal`, and watch for
     `upgrade`/`STATUS` events and wasm-hash changes. On any deviation, divest or stop deploying.
2. **Program liveness and yield.** The only yield is discretionary Merkle rewards, computed off-chain.
   - Recent flows look like a winding-down or migrating program.
   - The XLM vault has no epoch after 2026-11-09, while the other two have 2026-11-19 epochs.
   - Without rewards, Sentora adds pure trust risk for zero yield.
3. **Reward eligibility and attribution (unknown).** Leaves are per `deposit_id` and owner.
   - Every adapter `invest` creates a new id (one per ReceiptVault user deposit with auto-invest).
   - How the indexer attributes withdrawals from the aggregated record to ids (FIFO, LIFO or pro-rata) is unknown.
   - Whether it pays contract owners is unknown.
   - Proofs come from Sentora's backend, and the keeper must fetch and submit them.
4. **Share-price manipulation.** None in Sentora itself: principal is 1:1 and there are no shares.
   - On the Peridot side, donations to the adapter are ignored (books only, measured balance deltas).
   - Claimed rewards vest linearly (default 7 days), so a just-in-time ReceiptVault depositor cannot capture a
     step jump in NAV.
   - The ReceiptVault keeps its existing permanent-seed protection (`seed_minimum_supply`).
5. **Exit liquidity.** Normally instant and 1:1, because the vault holds the full principal.
   - While paused, exits are limited to the receipt's cash plus the adapter's free idle cash. This is tested: in
     `sentora_pause_deposits_liquidation_and_exits`, a 388.8 USDC seizure met 210 USDC of idle capacity, so the full
     exit failed until unpause and then succeeded.
   - Liquidations still work while paused (tested), because they move pTokens.
   - New receipt deposits keep working while paused. Auto-invest is best-effort: a failing Sentora `deposit` leaves
     the cash idle and emits `sentora_invest_skipped`. Before this fix, a paused Sentora reverted every Peridot
     deposit (Sentora error #100).
   - Size the idle buffer and caps for that.
6. **Oracle.** No share-rate oracle is needed, because the underlying is the plain asset.
   - Reflector `CAFJZQ…34DLN` (`Other("XLM")`, `Other("USDC")`) is used as in the cash group.
   - **PYUSD has no price there.** `lastprice` returns null for both `Stellar(PYUSD SAC)` and `Other("PYUSD")`,
     and the live cash controller's `get_price_usd(PYUSD)` returns null (checked 2026-11-25). A PYUSD market would
     need the cross-quoted route used in the LP work (native Reflector `CALI2BY…` PYUSD/USDC × USDC/USD via
     PriceRouter), or must stay collateral-free.
7. **TTL and archival.** Sentora's `Deposit(adapter)` record has a 30-day maximum TTL and is renewed only by
   deposit or withdraw activity.
   - Since protocol 23, an archived persistent entry is restored automatically inside an invocation, for a fee, so
     archival costs fees rather than funds.
   - A keeper should `ExtendFootprintTTL` that entry, the Sentora instance and code, and the adapter instance.
8. **Footprint.** See §5. The core liquidation path is already at 98 entries without Sentora.

## 3. Options

| | (A) Plain-asset market, idle cash in Sentora via the boosted-vault ABI | (B) Market underlying = Sentora share | (C) New SEP-41 wrapper token (1:1 claim on a pooled Sentora position) as a collateral-only underlying | (D) Do nothing in-protocol (users deposit in Sentora themselves) |
|---|---|---|---|---|
| Feasible | yes, prototyped | **no**: no share token; positions are address-keyed and non-transferable | yes, but the wrapper is a new token plus a custody vault | yes |
| Oracle | existing Reflector feeds | needs a share-rate oracle, but there is no rate | price = underlying × wrapper NAV; a new adapter or oracle path | n/a |
| Liquidation | seize pTokens (unchanged core path); the liquidator exits via ReceiptVault → adapter → Sentora | n/a | the liquidator receives wrapper pTokens, then must exit wrapper → Sentora (extra hop) | n/a |
| New code | one ≈580-line adapter (rustfmt); ReceiptVault and controller unchanged | n/a | wrapper token + custody + oracle adapter; more surface | none |
| Borrowing | borrowable cash = idle buffer only (cash-policy receipts never pull from the strategy to lend) | n/a | collateral-only | n/a |

**Recommendation: A.** It is the only option that reuses the deployed controller and receipt code unchanged.
It needs no new oracle, and it adds one small contract.

## 4. Recommended design (Option A)

**Contracts.**

- One **new** SimplePeridottroller instance. Use the deployed cash-group code (`f41e5260…`), but give it its own
  admin config, its own markets and its own oracle setting. It shares no state with the cash group (`CBMRS7…`) or
  the CL group.
- One ReceiptVault per market (deployed code `5630a701…`).
- One `sentora-strategy` instance per Sentora-backed market. Its constructor pins
  `(admin, receipt_vault, asset, sentora_vault, escrow, vest_secs)`. Sentora has no getters, so the deploy script
  must pin the Sentora vault and escrow addresses and the expected wasm hash `ca6b85a1…`.
- The adapter checks `receipt_vault.get_underlying_token() == asset`.

**Order** (the receipt requires a permanent seed **before** any strategy binding):

1. `initialize`
2. `enable_static_rates` or `set_interest_model`
3. `seed_minimum_supply`
4. `set_idle_cash_buffer_bps`
5. `add_market` / `set_market_cf` / `set_collateral_factor` / `set_peridottroller`
6. `set_boosted_vault(strategy)`; `bind_boosted_vault` is enforced by the controller
7. `set_cash_borrowing_enabled` only for a market that should lend

**Parameters (proposal; the owner decides).**

| Parameter | Proposal |
|---|---|
| Markets | start with **USDC** (collateral, Sentora-backed) and **XLM** (borrowable, plain cash or Sentora with a large buffer). Add PYUSD only after its Reflector path and its tiny vault (286 PYUSD) are reviewed. |
| CF | below the cash group's equivalent: USDC 0.50–0.60 and XLM 0.40–0.50 at launch. Sentora admin risk is a jump-to-default risk, which CF cannot price. |
| Idle buffer (`IdleCashBufferBps`) | 3,000–5,000 on Sentora-backed markets. It is the pause-time exit and liquidation liquidity, and the only borrowable cash. |
| `MinCashReserveBps` | the deploy floor is `max(idle target, borrowing reserve)`. The live cash group runs at 10,000, under which **nothing** is deployed to a strategy. The prototype and fork ran at the fresh-initialize default of 0, so the idle buffer alone set the split. Set it explicitly below 10,000 on Sentora-backed markets. |
| Supply caps | low and explicit per market, e.g. ≤5 % of the Sentora vault's TotalPrincipal, which is 61.6 k USDC today. |
| Borrow caps | explicit. Borrowing is on only for the market that is meant to lend. |
| Adapter | `vest_secs` = 7 days (≤30-day bound). `auto_invest` stays on until reward attribution is known; then possibly off with a daily `invest_idle` batch to limit `deposit_id` fragmentation. |
| Guards | the receipt's existing guards: 5-minute health cache, 90 % quote floor, inflated-quote clamp, permanent seed. Adapter: books-only NAV, SAC-balance mark-down, receipt-only deposit/withdraw, exact balance-delta checks, final exit pays everything. |

**Adapter (prototype) behaviour.**

- `deposit`: receipt-only; pulls cash; mints shares at NAV; invests in Sentora when ≥10,000 raw.
- `withdraw`: receipt-only; pays free idle first, then `sentora.withdraw`, and enforces the receipt's non-zero
  cash minimum.
- `get_asset_amounts_per_shares`: length 1; `[0]` for zero shares. It reads only the adapter instance plus one SAC
  balance.
- `claim_rewards(epoch, params)`: permissionless. It checks that the epoch asset equals the underlying, credits
  only the measured delta, and vests that delta.
- `invest_idle`, `divest`, `set_auto_invest`, `set_vest_secs` are admin-only. `bump_ttl` is permissionless.
- A bug found by the strict-auth test: an invoker auth entry from `authorize_as_current_contract` lives only
  until the contract's **next** sub-call (`soroban-env-host` `auth.rs` `pop_frame`). The balance read therefore
  has to happen before authorizing the nested SAC transfer. `mock_all_auths` hid this.

## 5. Liquidation path and resources

The liquidator calls `controller.liquidate(borrower, repay_market, collateral_market, amount, liquidator)`.
Repayment goes through `repay_on_behalf`, which uses `transfer_from`, so the liquidator approves first.
Collateral pTokens are seized by transfer. **No Sentora call and no adapter call is made in the liquidation.**
Source review: `seize` and `update_interest` read only the receipt's cached and accounting boosted values. The extra
entries come from the receipt's own boosted-state keys (binding, cache, buffer); they were not traced key by key.

The liquidator exits later with an ordinary `withdraw`, which pulls from Sentora when it needs more than the idle
buffer. Soroban allows one invocation per transaction, so "seize then exit" is **two transactions** (liquidate, then
a 57-entry withdraw), or one transaction through a helper contract, which is unmeasured. An atomic helper is not
recommended: it would revert the whole liquidation whenever the exit exceeds the idle buffer during a Sentora pause.

Measured with `harness/sentora-fork`: fork ledger 65,663,759, protocol 30, real Sentora and escrow bytes, deployed
controller `f41e5260` and receipt `5630a701` code, real SACs and Reflector. Values are the last top-level
invocation's `cost_estimate`. User calls run under strict auth.

| Operation | none (baseline) | USDC only (recommended) | both markets |
|---|---|---|---|
| deposit USDC 1,000 | 46 entries / 13.2 M | 56 / 19.7 M | 56 / 19.6 M |
| deposit XLM 100 k | — | — | 55 / 19.6 M |
| borrow XLM | 64 / 22.7 M | 65 / 23.0 M | 66 / 24.5 M |
| withdraw XLM 50 k pTokens (via Sentora when bound) | 47 / 17.6 M | 47 / 17.6 M | 56 / 28.2 M |
| `refresh_boosted_underlying` (keeper) | — | 11 / 2.8 M | 11 / 2.8 M |
| **liquidate** (repay 40 %, seize USDC pTokens) | **98 / 43.6 M** | **104 / 48.2 M**, 19 writes | **105 / 49.3 M**, 19 writes |
| liquidator withdraw of seized USDC (via Sentora) | 48 / 17.1 M | 57 / 27.7 M | 57 / 27.7 M |
| `claim_rewards` (real escrow, no signer) | — | 10 / 2.6 M | 10 / 2.6 M |

- Memory peaks at 6.3 MB.
- Disk reads are 0–1. Under protocol 23+, all contract state is in-memory.
- Writes peak at 20, and write bytes at 4.2 KB.
- All rows fit the live limits (400 M instructions, 200 disk reads, 200 writes, 400 entries) by more than 4×.
  Only the liquidation breaks the internal ≤100-entry rule, by 4–5 entries.
- A real G-account liquidator adds its account entry and a nonce entry. Estimate (unmeasured): about 106–107.
- A borrower with three or more entered markets adds entries per market (unmeasured).

Options:

1. Accept a group-specific internal ceiling for liquidation, estimated at about 120 entries for a two-to-three-market
   borrower; measure it before fixing the number. The network limit is 400.
2. Back only the collateral market with Sentora. This is already the recommended shape, at 104 entries.
3. Drop the adapter's SAC-balance mark-down from the quote: −1 entry, at the cost of the drain signal.
4. A ReceiptVault change so that seize does not take a live strategy quote. This is new core code and the most
   intrusive option.

## 6. Reused unchanged vs new

- **Unchanged.** Deployed code for SimplePeridottroller (`f41e5260…`), ReceiptVault (`5630a701…`), Reflector, the
  SACs and Sentora.
- **Unchanged ReceiptVault mechanisms.** Its boosted-vault handling (binding, asset-count, health cache, quote
  floor, redemption retry, final-exit) and the jump-rate model or static rates.
- **New.** `contracts/sentora-strategy` (adapter, about 580 lines after rustfmt, including events) and its deployment script, with
  pinned addresses and hashes.
- **New keeper jobs (§7).** A Sentora monitor and proof fetcher.
- **Not needed.** A share-rate oracle, controller changes and receipt changes, unless option 4 in §5 is chosen.

## 7. TTL and keeper duties

1. `refresh_boosted_underlying` on every Sentora-backed receipt **at least every 5 minutes** (11 entries, 2.8 M
   instructions). Account health fails closed when the cache is stale: borrow, redeem-check and liquidate panic
   with "boosted health cache stale". This is tested natively and on the fork.
2. `cache_price` as in the cash group.
3. **Reward claims.** For each published epoch, fetch the adapter's leaves and proofs from Sentora and call
   `claim_rewards` once per `deposit_id` (10 entries, 2.6 M instructions each). Unclaimed epochs can be swept by
   Sentora after 180 days.
4. **Sentora monitor.** Track the vault wasm hash, `Paused`, `TotalPrincipal` against the SAC balance, the escrow
   wasm hash, and admin signers/thresholds. Alert, then `divest` and set the Peridot supply cap to zero on any change.
5. **TTL.** Extend the adapter instance with `bump_ttl`, and the Sentora `Deposit(adapter)`, vault instance and
   code with `ExtendFootprintTTL`, well inside 30 days.
6. Optionally, a daily `invest_idle` batch if `auto_invest` is turned off.

## 8. Almanax scan (planned)

Almanax `scan_commit` addresses a commit by GitHub repo owner, name and SHA, so it can only scan a commit on the
remote. This branch is not pushed (per instructions), so the scan has not run. When it is pushed, scan
`4c03928..HEAD`, which covers `contracts/sentora-strategy` and `harness/sentora-fork`. Areas to direct it at:

- the nested-auth order in `invest`;
- the books-only NAV and the vest maths;
- final-exit and write-off behaviour with a marked-down principal (a partial exit while marked down pays from
  Sentora first-come);
- receipt-only gating;
- `claim_rewards` asset checks.

Re-scan after any change.

## 9. Test plan

**Done (local).**

- `cargo test -p sentora-strategy`: native tests, against the **real** Sentora vault and escrow
  WASM (sha256 asserted) with native ReceiptVault and controller. They cover:
  - strict-auth deposit, withdraw and no-signer claim;
  - final exit;
  - receipt-only gating;
  - one-leaf Merkle claim with linear vesting and double-claim/foreign-leaf rejection;
  - donation ignored;
  - Sentora pause limiting exits to the buffer;
  - drain mark-down to a 0.6 exchange rate;
  - full borrow → liquidate → liquidator exit;
  - stale health cache blocking borrow until refreshed;
  - constructor asset check;
  - paused Sentora: deposits continue (best-effort invest), liquidation works, and the exit is capped at idle until
    unpause.
- 10 native tests in total, all passing (phase 2: 15, see §21).
- `harness/sentora-fork`: 3 fork tests (none, USDC only, both), all passing, giving the §5 numbers (phase 2 adds
  `tests/group3.rs`, see §18 and §21).

**Remaining.**

1. Testnet, if Sentora provides a Testnet deployment. Otherwise deploy the public `StellarVaults` source on
   Testnet: full lifecycle with the keeper's 5-minute refresh, and real G-account liquidator auth.
2. A fork test of a real (not test-published) epoch claim. It needs an adapter-owned leaf from Sentora, so it
   depends on eligibility.
3. Fork variants of the paused-vault test (native already covers it on the real bytes), of a Sentora wasm upgrade,
   and of a three-market borrower.
4. A repeat-run soak of the fork under strict auth for the liquidator as well.
5. An Almanax scan and review before any Mainnet step.

## 10. Open questions for the owner (and Sentora)

1. Is the Sentora reward program ongoing? What APY is expected, which assets get epochs, and on what schedule?
   Was the recent ≈1 M USDC outflow a migration?
2. Are **contract** depositors (the adapter) eligible for reward leaves? Does Sentora's API serve their proofs?
   Is a referral id required?
3. How are partial withdrawals attributed to `deposit_id`s, and do many small deposits (one per receipt deposit)
   reduce or fragment rewards? Should `auto_invest` start off, with a daily batch?
4. Is the instant, untimelocked upgrade/pause power of a 4-of-7 multisig acceptable for protocol funds? Will
   Sentora add a timelock or commit to notice periods?
5. Which markets: USDC collateral plus XLM borrowable to start, and whether PYUSD is added (286 PYUSD TVL)?
6. CF, caps and the idle-buffer range (§4).
7. Accept a group-specific internal footprint ceiling of about 120 entries for liquidation (104–107 measured), or
   ask for option 3 or 4 in §5?
8. Who runs the extra keeper jobs (§7), and from which signer? A refresh every 5 minutes is new load relative to
   the cash group.

## 10a. Known prototype limitations

- If a reward claim lands while adapter supply is zero, the vested amount becomes ownerless value. The next deposit
  is then rejected with `BadConfig` (by design, against capture), so an admin sweep path is needed.
- The SAC-balance mark-down "recovers" when other users deposit into Sentora. It is a last-line signal, not a loss
  oracle.
- During a mark-down, a partial exit is paid from Sentora first-come; only the final exit writes off the loss.
- Phase 2: the kill path's short-pay branch (an upgraded vault that pays less than requested) is implemented: it books
  what arrived. It is not tested, because no short-paying vault code is available.
- (Resolved in phase 2, §20: Sentora's WASM is no longer vendored.) It is third-party code (IntoTheBlock). No licence file was found in its
  repository tree.

## 11. Files

- `contracts/sentora-strategy/src/lib.rs`: adapter.
- `contracts/sentora-strategy/src/test.rs`: native tests.
- (Removed in phase 2, §20; fetched at test time instead.) Real Mainnet Sentora vault and escrow WASM, 10.5 KB and 15.1 KB,
  sha256 asserted.
- Adapter WASM built with `stellar contract build --package sentora-strategy`: 13,919 bytes, sha256
  `710f7a42c71367d366ecfbb822342983def8b1381f601c46a41b054ddd7dc71c`.
- `harness/sentora-fork/tools/fetch-fork.cjs`: read-only fetcher. `tests/sentora.rs` holds the fork tests.
  Outputs in `.scratch/` are git-ignored: `fork.json` (sha256 `2fd134a2…`, 296 KB) and `*-results.json`.

---

# Phase 2 (25 Nov 2026)

Status unchanged: research and prototype. Nothing was signed, sent or deployed; every network access was a read-only
RPC/Horizon query. **No deployment until Sentora confirms that contract addresses earn rewards and can claim them**
(owner decision 7).

## 12. Decisions 25 Nov

First round, then the refinement that supersedes it where they differ:

| # | Decision | Status in this branch |
|---|---|---|
| 1 | Sentora program is live (Sentora's webpage). | Accepted. On-chain epochs still end 2026-11-09 (XLM) and 2026-11-19 (PYUSD, USDC). |
| 2 | Untimelocked 4-of-7 Sentora admin accepted. | Accepted, bounded by the absolute caps and kill path (§16). |
| 3 | Option A. Markets USDC, PYUSD, XLM. **Refined:** borrowables USDC and PYUSD only. XLM is collateral-only. CF USDC 90 %, PYUSD 80 %, XLM 55 % (or as justified). LI 2–3 %. | §13, §14. |
| 4 | Raise the internal liquidation footprint limit. **Refined:** about 120 entries approved; liquidation also ≤ 85 % of 200 disk reads (≤ 170). | §18. |
| 5 | Extra keeper on a separate DigitalOcean worker. | §19, `.do/sentora-keeper.yaml`, `bots/sentora-keeper/`. |
| 6 | PYUSD = $1 with a DEX deviation check. **Refined:** never above $1. Beyond the threshold, borrow and withdraw fail closed, but liquidation prices PYUSD at min($1, pool). | §15. |
| R3 | Absolute Sentora cap per market, a hash/admin/pause watcher and an on-chain kill path. | §16. |
| R4 | Rewards raise suppliers' exchange rate, vested against NAV jumps. | §17. |
| R5 | Remove Sentora's bytes from the repo and from this branch's history. | §20. |
| R8 | Prepare the branch for push (scan); do not push. | §22. |

## 13. Risk check of decision 3

**First version: CF 0.9 on USDC/PYUSD with XLM borrowable.** In this controller, the CF belongs to the collateral
market and is also the liquidation threshold. `simple-peridottroller` has **no borrow-side factor**: no
`BorrowFactor` key, and debt is summed at its price with no factor (checked in source). Receipt `set_borrow_cap`
bounds only aggregate debt, so a per-asset cap cannot lower one position's leverage. Option (i) therefore needs a
controller change. Results for a position borrowed to the limit (u = 1), with D/C = CF·u:

| | CF 0.9, LI 1.08 (default) | CF 0.9, LI 1.08, u = 0.95 |
|---|---|---|
| XLM rise that makes it liquidatable | any rise > 0 (1/(CF·u) − 1 = 0 %) | +5.3 % |
| XLM rise beyond which the full incentive cannot be paid (bad debt) | 1/(CF·LI) − 1 = **+2.9 %** | +8.3 % |
| XLM rise to insolvency (D > C) | +11.1 % | +17.0 % |

These compare with XLM moves (p99.9) of 2.2 % over 5 min, 3.7–4.1 % over 15 min and 8–10 % over 1 h. Oracle
resolution is 300 s with k = 2, and the health cache is 5 minutes, so the relevant horizon is 5–15 minutes. The
15-minute p99.9 already exceeds the 2.9 % gap, so this configuration was not safe.

Options:

- (i) A lower effective CF for XLM debt: needs a controller change.
- (ii) Stable-only borrowing, with XLM supply-only.
- (iii) Two controllers.

**The owner's refinement is (ii) with XLM as collateral, which this analysis supports.**

**Refined configuration, LI 1.03 (proposed).** CF·LI < 1 in every market, so each liquidation improves account health.

| Collateral → debt | CF | CF·LI | Relative drop that gives bad debt from max borrow (1 − CF·LI) | Drop to insolvency (1 − CF) | Compared with |
|---|---|---|---|---|---|
| USDC → PYUSD | 0.90 | 0.927 | 7.3 % | 10 % | Stable pairs: SDEX 90-day max 15-min average deviation 0.5 % |
| PYUSD → USDC | 0.80 | 0.824 | 17.6 % | 20 % | Same. The $1 valuation hides at most 1 % (band) + 0.1 % (fee) |
| XLM → USDC/PYUSD | 0.55 | 0.567 | XLM −43.4 % | −45 % | XLM 1-h p99.9 8–10 % (about 4× margin) |

**Why 3 % and not 2 %.** The LI is group-wide, and XLM collateral needs the higher of the two:

- A liquidator who sells seized XLM takes 5-minute price risk, and the 5-minute p99.9 is 2.2 %. A 2 % bonus would be
  below it; 3 % is above it.
- For the stable pairs alone, 2 % would be enough: bad-debt gaps of 8.2 %, 18.4 % and 43.9 % at LI 1.02.

**XLM CF.** The numbers support up to about 0.65: the bad-debt drop would be −33 %, still above 3× the 1-h p99.9.
I recommend launching at 0.55, as the owner proposed, because a 3 % incentive leaves liquidators little room during
an XLM crash.

## 14. Parameter table (proposal)

| Parameter | USDC | PYUSD | XLM |
|---|---|---|---|
| CF (controller `set_market_cf` = receipt `set_collateral_factor`) | 0.90 | 0.80 | 0.55 |
| Borrowable (`set_cash_borrowing_enabled`) | yes | yes | **no** |
| Supply cap | 100,000 USDC | 20,000 PYUSD | 500,000 XLM (≈ $61 k at $0.1218) |
| Borrow cap | 80,000 USDC | 15,000 PYUSD | — |
| Idle buffer (`IdleCashBufferBps`) | 3,000–5,000 (fork tests ran 2,000) | 3,000–5,000 | 3,000–5,000 |
| `MinCashReserveBps` | set explicitly, < 10,000 | same | same |
| **Absolute Sentora cap** (adapter `invest_cap`) | 20,000 USDC (Sentora USDC TVL 61.6 k) | 5,000 PYUSD (Sentora PYUSD TVL 286) | 100,000 XLM (Sentora XLM TVL 2.08 M) |
| Oracle (controller's single oracle = `peg-oracle`) | Reflector `Other("USDC")`, forwarded | min($1, median of 3), §15 | Reflector `Other("XLM")`, forwarded |
| Controller price band (`set_price_band`, 1e6) | [980,000, 1,020,000] (as the cash group) | **[990,000, 1,000,000]** | none |
| Group: LI / close factor / liquidation fee | 1.03 / 0.50 / 0. LI is timelocked 24 h after `initialize`; the group runs at the 1.08 default until it is applied | | |
| Group: `MaxUserMarkets` | **2 (deployed default; do not raise)**, §18 | | |
| Group: k / `MaxPriceAgeSecs` / fallback prices | 2 / 900 (default) / **disabled** (default; never enable for PYUSD) | | |
| Adapter vest / auto-invest | 7 days / on until reward attribution is known | | |
| Adapter guardian | the dedicated Sentora keeper key (§19) | | |

The caps keep Peridot at no more than about a third of the USDC vault. The PYUSD cap would make Peridot most of
Sentora's tiny PYUSD vault. Whether that is acceptable is a Sentora question, part of the reward-eligibility
questions, §23.

## 15. PYUSD peg oracle (`contracts/peg-oracle`)

**What the controller expects (checked in the deployed source, `f41e5260`).**

- One oracle address. `cache_price` calls `decimals()`, `lastprice(Asset)` and `resolution()` on it.
- The asset is `Other(symbol)` when `set_oracle_asset_symbol` is set, else `Stellar(token)`.
- Freshness is `timestamp + k·resolution ≥ now` and `now − timestamp ≤ MaxPriceAgeSecs` (900).
- Borrow, redeem and liquidation read `PriceCache` first and call the oracle only when it is cold.
- `get_price_usd` and the risk-adding paths also apply the **per-token price band**. Liquidation and repay use the
  **unbanded** price.

**Design.** A Reflector-shaped router:

- It forwards every asset other than PYUSD to Reflector `CAFJZQ`, and forwards `decimals` and `resolution` live.
- For PYUSD, requested either as `Stellar(PYUSD SAC)` or `Other("PYUSD")`, it returns `min($1, observed)` and
  **never more than $1**:
  - `observed` is the **median of three** USD observations:
    1. the 1,000-PYUSD executable bid of the concentrated pool `CAPIOQ…` (the one the LP vaults use);
    2. the same bid on the stable pool `CDMH535…`, about 25× deeper; both pools are listed by the router
       `get_pools`, never the HTTP API;
    3. the round-sampled Reflector DEX feed `CALI2BY…` (base USDC, the same wasm as `CAFJZQ`).
    All three are converted with Reflector USDC/USD.
  - If only two sources are available, they must agree within the band; the lower one is then used. Otherwise, or
    with fewer than two sources, the result is `None`. A single source never sets the price alone.
  - If `observed ≥ 1 − band`, the price is exactly `10^decimals`. Otherwise it is `observed`, a mark-down.
  - The timestamp is the older of the Reflector timestamps used, never `now`.
- The contract has no admin, no upgrade and no storage beyond its configuration.
- The constructor checks both pools' token order and both feeds' decimals and base.

**How the paths are split, without any controller change.** Set the PYUSD band to [0.99, 1.00]. A mark-down below
0.99 then has these effects:

- New borrows of PYUSD, or by any account holding PYUSD, revert with "price out of band".
- Redeems and collateral transfers by accounts that have debt revert the same way.
- `get_price_usd` returns `None`.
- Liquidation and repay use the marked-down price.
- Debt-free redeems read no price and stay open. A debt-free withdrawal cannot hurt solvency.

This is tested on the fork with the deployed controller and receipt bytes, in
`depeg_marks_down_blocks_borrow_and_liquidates_at_min_price`:

- Borrow and withdraw-with-debt revert.
- A debt-free PYUSD exit works.
- The liquidation seized PYUSD at an implied 0.9700 while the oracle said 0.9710.
- After restore, the price is $1 again.

**Threshold: 100 bps, from measured data.**

| Source | Window | Deviation from $1 (USD terms unless noted) |
|---|---|---|
| Both pools, 1,000-unit and 10,000-unit bid × USDC/USD, 50 samples every 30 s | 25 min, 2026-11-25 11:39–12:04 UTC | bid 0.99976–0.99990; ask 1.00181–1.00193; spread 20–21 bps (two 10 bps fees); concentrated-pool tick constant at −2 |
| Reflector DEX feed × USDC/USD | same | 1.00079–1.00086 (feed: PYUSD/USDC 0.999794–0.999795) |
| Reflector `prices()` history (13 rounds, about 1 h) | 2026-11-25 | USDC/USD 1.00100–1.00110; DEX feed PYUSD/USDC 0.99979–0.99980 |
| SDEX `trade_aggregations` (Horizon; the SDEX order book, **not** the Aquarius pools) | 90 d hourly / 30 d 15-min | average deviation p99 27 / 30 bps, maximum 50 / 50 bps; per-trade extremes p99.9 50–238 bps |

- The band is about 2× the worst 15-minute SDEX average and more than 50× the normal pool noise.
- It sits far below the 7.3 % and 17.6 % bad-debt gaps.
- The 10 bps fee in the bid uses up 10 bps of it.
- The DEX feed stayed at 0.99979 while hourly SDEX buckets traded at 0.9971 (10 trades). The feed therefore does
  **not** follow thin SDEX prints, but its methodology is not documented on-chain.

**Manipulation cost: real pool bytes and state on the fork, read-only fetched.** For each pool, the trade that pushes
its 1,000-unit quote past the threshold (`manipulation_cost_*`, `stable_pool_manipulation_*`):

| Pool, direction | Trade that crosses the threshold | Cost if swapped back in the same transaction | Durable cost at par (arbitrageurs restore) |
|---|---|---|---|
| Concentrated, down (bid < 0.99) | sell about 185 k PYUSD (drains its about 185 k USDC; the bid falls to 0.455) | $370 | $395 |
| Concentrated, up (ask > 1.01) | buy its whole 48.2 k PYUSD (55 k USDC in) | $6.8 k (step granularity) | $6.8 k |
| Stable, down | sell 3.0 M PYUSD (bid 0.982) | $5,989 | $10,952 |
| Stable, up | buy with 3.75 M USDC (ask 1.0113) | $7,490 | $8,837 |

What these trades achieve against the oracle:

- One source alone moves nothing (asserted on the fork for all four).
- An upward move moves nothing either: the price is never above $1.
- **To mark PYUSD down, an attacker must move two of the three sources.**
  - The cheapest pair is the concentrated pool ($370 in-transaction) plus either:
    - the stable pool: $6 k in-transaction plus $3 M of working capital; or
    - the DEX feed for a whole Reflector round, at a cost that is **unknown** because the feed's methodology is not
      documented.
  - If the DEX feed misses a round, the two pools must agree within the band, or the price is `None`. A crash of the
    concentrated pool alone then freezes PYUSD rather than pricing it (fork test
    `stale_feed_and_crashed_concentrated_pool_*`).

What an attacker can force by marking the price down to x:

- Liquidations of PYUSD-collateral borrowers whose margin is smaller than the mark-down.
- The liquidator's profit is `repaid × (1.03/x − 1)`.
- At the 20,000 PYUSD supply cap and CF 0.8, PYUSD-collateral debt is at most 16 k, so the profit is at most about
  $1.0 k at x = 0.97 and $2.3 k at x = 0.90.
- That is below the cost of the pool-only route (more than $6.4 k). It may not be below the cost of the DEX-feed
  route, which is unpriced.
- The PYUSD supply cap is the control. **Raise it only after the DEX feed's methodology is known.**

**Flash re-arm during a real depeg.** A transaction that restores one pool cannot move the median: the other pool and
the round-sampled feed stay depressed. This is tested natively. On the fork, the concentrated pool alone was restored
while the feed was depressed. The re-arm needs the stable pool or a feed round.

**Deviations from decision 2, for the owner:**

1. A deviation in the concentrated pool alone stops nothing. That is the point of the median.
2. An upward deviation stops nothing. The price is never above $1, so a premium cannot be expressed through a price
   band. The keeper alerts instead, so the guardian can use `pause_borrow_g`.
3. The borrow stop lags until `PriceCache` is refreshed. The keeper calls `cache_price(PYUSD)` within a minute of a
   status change (§19), and expiry caps the lag at 600 s.
4. Liquidation uses the median, not "the pool price".

## 16. Sentora exposure: absolute cap and kill path (`contracts/sentora-strategy`)

- **Absolute cap.** `invest_cap` is a constructor argument, with `set_invest_cap` for the admin. Every investment is
  clipped to `cap − principal`. On the fork, a 200 k XLM deposit invested exactly 100 k, where the 20 % buffer alone
  would have invested 160 k.
- **Pinned code.**
  - The constructor takes the reviewed vault and escrow wasm hashes, `ca6b85a1…` and `f22e4a52…`, and rejects a
    mismatch.
  - A contract **can** read another contract's code hash (`Address::executable`, soroban-sdk 25.3).
  - Every investment checks it. A changed vault code fires the kill path instead of depositing.
- **Kill path** (`kill`). It does three things:
  - sets `Killed`;
  - turns auto-invest off;
  - withdraws all recoverable principal, `min(principal, Sentora balance)`, back into the adapter's idle cash.
  NAV and shares are unchanged. If Sentora refuses the withdrawal (paused, or an incompatible upgrade), the kill
  still sticks and the call can be retried. A short payment is booked as received rather than reverting the kill.
  Funds can only come back to the adapter.
- **On a deposit**, a changed vault or escrow code hash sets the kill flag only, without calling the unreviewed code.
  The user's deposit succeeds and the cash stays idle (tested).
- **Who can trigger it.**
  - Permissionless: `exit_if_upgraded`, when the vault or escrow code hash differs from the pin.
  - Permissionless: `exit_if_drained`, when Sentora's balance is below Peridot's booked principal.
  - Guardian (the keeper key) or admin: `emergency_exit`, for what no contract can see: admin or signer changes,
    pause, or a `TotalPrincipal` mismatch.
  - The admin alone can `resume`, and `resume` is refused while the code differs from the pin. A new adapter is
    needed instead.
- **Tested.**
  - Natively: guardian kill, non-guardian refused, kill during a Sentora pause followed by a retry after unpause, a
    permissionless drain exit, and an upgrade of the real vault bytes followed by a permissionless exit with resume
    refused.
  - On the fork, against the real XLM and USDC vaults:
    - guardian `emergency_exit` under strict auth while paused: kill only, 9 entries;
    - guardian `emergency_exit` after unpause: full pull-out, 10 entries;
    - `exit_if_drained`: 10 entries;
    - `exit_if_upgraded` after a real `upgrade` call: 9 entries.

## 17. Rewards into supplier NAV

- `claim_rewards` is permissionless. The escrow pays only the adapter's own leaf, and the adapter credits only the
  measured balance delta.
- The claimed asset becomes idle cash in the adapter. It is part of the market's backing, withdrawable by the receipt
  and investable under the cap. It vests linearly over 7 days in `total_assets`.
- The receipt's boosted value, and so the pToken exchange rate, rises as the reward vests, once
  `refresh_boosted_underlying` runs.
- Native test `rewards_raise_supplier_rate_and_cannot_be_captured_just_in_time`:
  - 70 USDC reward against 1,000 USDC.
  - The rate is unchanged at claim.
  - A depositor who enters right after the claim and leaves 5 s later gains at most about 5 s of vesting (asserted
    ≤ 0.02 USDC).
  - The rate is about halfway up after 3.5 days.
  - The holder ends +69–70 USDC.
- **Deviation from decision R4:** rewards stay as **adapter** idle cash, not receipt cash. A late entrant who stays
  shares the remaining vesting pro rata to time held, like any accruing yield; only a step capture is prevented.
  Moving the reward into receipt cash directly would be a donation to the receipt, which prices instantly with no
  vesting.

## 18. Footprint: measured against the approved limits

The fork is ledger 65,664,491 on protocol 30, with:

- real Sentora vault and escrow bytes;
- both Aquarius pools;
- both Reflector contracts;
- the deployed controller `f41e5260` and receipt `5630a701` code;
- the peg oracle in the path.

The liquidator is a **real G account**: a synthetic account entry with an ephemeral, test-only ed25519 key, making
signed `Address`-credential calls, so its nonce entry is counted. "Cold" means every `PriceCache` has expired, so the
peg oracle and all three of its sources run inside the call.

| Operation | `MaxUserMarkets` 2 | `MaxUserMarkets` 3 (XLM + USDC + PYUSD) |
|---|---|---|
| Borrow, cold (oracle in the path) | 81 (XLM/USDC), 113 (USDC/PYUSD, 2nd debt) | 130 |
| **Liquidation, cold** | **113** (XLM collateral), **121** (USDC + PYUSD collateral and debt) | 138–140 |
| Liquidation, warm | 105 (same two-stable shape) | 121 |
| Withdraw by an account with debt in both stables, cold, exit through Sentora | **121** (29 writes, 82.6 M) | — |
| Disk reads / writes in liquidation | 2 / 19–22 | 2 / 19–23 |
| Liquidation instructions / memory | 51–80 M / 7–12 MB | 55–88 M / 7–13 MB |
| Liquidator exit through Sentora | 51–52 | 51–52 |
| Keeper: refresh / kill / drain / upgrade exit | 11 / 9–10 / 10 / 9 | same |

**Against the approved limits:**

- **≤ 120 entries.** It holds only with `MaxUserMarkets = 2`, the deployed default, and with warm prices (105).
  - The worst measured two-market operations are the cold two-stable liquidation and the cold withdraw-with-debt
    through Sentora, both **121, one entry over**.
  - Mitigations: the keeper refreshes `cache_price` for all three tokens every Reflector round, and liquidator bots
    send a `cache_price` transaction first when the cache is stale.
  - If the owner wants the cold paths inside the limit itself, it needs a limit of 125: 121 measured, plus a few
    entries of headroom; that is not the 15 % margin rule (owner decision).
  - Three markets per account (130–140) break the limit and are not recommended.
- **Disk reads ≤ 170.** Every row reads 0–2 classic (disk) entries, far below. Even counting all 121 entries as
  reads, the limit still holds.
- **Network.** Every row is below the live limits of 400 entries, 200 disk reads, 200 writes and 400 M instructions.
  The largest, 140 entries and 88 M instructions, is below 85 % of them.

## 19. Keeper spec: a separate DigitalOcean worker (`bots/sentora-keeper`, `.do/sentora-keeper.yaml`)

The worker is a skeleton only. Its entrypoint validates the configuration, prints the job plan and refuses live mode.
It has three passing configuration tests.

| Job | Cadence | Action |
|---|---|---|
| Health-cache refresh | every 240 s (< 300 s) | `refresh_boosted_underlying` on each receipt with boosted shares (11 entries, 2.8 M instructions). |
| Peg | every 60 s | Simulate `peg_status`. On a change of `at_peg` or `priced`, call `controller.cache_price(PYUSD)` at once, which applies the borrow stop or lifts it. Also refresh all three caches every round (§18). Alert on missing sources or a mark-down. Alert on an **upward** pool deviation > +100 bps for a guardian `pause_borrow_g` decision. |
| Sentora monitor | every 60 s | Check, for vault and escrow: wasm hashes (instances via `getLedgerEntries`), `Paused`, `TotalPrincipal` against the SAC balance, the admin `GCOB2…` signers and thresholds (7 × 10, 40/40), and `upgrade`, `STATUS` and `update_admin` events. On a hash change: permissionless `exit_if_upgraded`. On a drain: `exit_if_drained`. On anything else: `emergency_exit` as guardian. Repeat until `remaining_principal == 0`, alert, and recommend supply caps of 0. |
| Reward claims | hourly | Per new epoch: fetch the adapter's leaves and proofs (Sentora API, pending eligibility), simulate `claim_rewards`, and submit if the delta exceeds the fee. |
| TTL | daily | `ExtendFootprintTTL` for Sentora `Deposit(adapter)`, the vault and escrow instances and code, the peg oracle, the adapters, the controller and the receipts. Keep everything above 30 days. |

- **Signer.**
  - A new dedicated key whose only roles are fee source and guardian on the three adapters. It is not an admin.
  - It is never the aquarius-keeper `GBLVLG…`, a cash keeper, a depth reporter or the Peridot admin. `config.mjs`
    rejects the known keys.
  - `KEEPER_SECRET` lives only as a DigitalOcean encrypted runtime secret, and only when `DRY_RUN=false`.
- **Fee budget (estimate).**
  - The aquarius-keeper's measured refresh cost is about 0.00176 XLM per transaction (6 refreshes cost
    0.0105602 XLM).
  - Refresh: 3 receipts every 240 s is about 1,080 transactions per day, about 1.9 XLM per day.
  - Cache refresh: 3 tokens every round is about 860 transactions per day, about 1.5 XLM per day.
  - Claims, TTL and alerts: under 0.1 XLM per day.
  - Total: about 3.5 XLM per day, about 105 XLM per month. Inclusion fee 10,000 stroops; per-transaction cap
    0.2 XLM; daily hard stop 5 XLM in the draft. Re-set it from the first measured week.
  - Fund with 150 XLM and warn below 10 XLM spendable.
- **Deployment draft.**
  - One 512 MiB worker in `fra`, `DRY_RUN=true`.
  - A plain git source with no automatic deploys, from an immutable release branch (`sentora-keeper-v0.1.0`, not yet
    created).
  - Contract IDs are placeholders until the group is deployed.

## 20. Licence handling

- Sentora's wasm, and any test snapshot that would embed it, is no longer in the repository.
- `tools/fetch-fork.cjs --wasm-dir ../.scratch/wasm` writes every fetched ContractCode, read-only from Mainnet, to
  `harness/sentora-fork/.scratch/wasm/<sha256>.wasm`. This path is git-ignored, and the fetcher verifies each hash.
- Native tests load the vault and escrow from there and assert `ca6b85a1…` and `f22e4a52…`.
- Without the fetch, the `sentora-strategy` tests fail with a message that names the command. Workspace
  `cargo test` therefore needs the fetch first.
- `test_snapshots/` is git-ignored in `sentora-strategy`, `peg-oracle` and the harness.
- The two local commits were squashed into one commit on `4c03928` that contains no Sentora bytes. The old commits
  remain only in the local reflog, unreachable and never pushed.

## 21. Updated test plan

**Done (local).**

- `cargo test -p peg-oracle`: 10 tests. They cover:
  - par;
  - forwarding;
  - never above $1;
  - one manipulated source;
  - a median mark-down with its event;
  - the band edge;
  - the two-source agreement rule;
  - a stale feed with a crashed pool;
  - USDC gating;
  - the constructor checks.
- `cargo test -p sentora-strategy`: 18 tests on the deployed Sentora bytes. They add, to phase 1:
  - the invest cap;
  - guardian kill;
  - kill during a pause, then a retry;
  - permissionless drain exit;
  - an upgrade followed by permissionless exit;
  - pin rejection;
  - the reward rate rise with JIT fairness;
  - an upgraded vault that a deposit detects: kill flag set without calling it.
- `harness/sentora-fork`: `tests/sentora.rs` (3, phase 1) and `tests/group3.rs` (11). They cover:
  - the footprint matrix in §18;
  - manipulation cost;
  - a depeg mark-down with borrow and withdraw blocked and liquidation at min($1, observed);
  - a stale feed with a crashed pool;
  - the kill path and upgrade trigger on the real vaults.
- `bots/sentora-keeper`: 3 configuration tests.

**Remaining.**

1. Sentora must confirm that contract depositors earn and can claim (gates deployment).
2. A real epoch claim on the fork, once Sentora serves a leaf for an adapter.
3. Measure the stable pool's cost while the concentrated pool is also crashed, and learn the DEX feed's methodology,
   before raising the PYUSD cap.
4. Testnet rehearsal with the keeper's 240 s refresh and its alerting.
5. Almanax scan of the pushed branch (`4c03928..HEAD`). Areas to direct it at:
   - `peg-oracle` median and band edges;
   - adapter `kill` accounting;
   - `exit_if_*` triggers;
   - the cap clipping;
   - the vesting.
6. The production keeper: signer, fee guard, confirmation polling and journal, then a scan.

## 22. Pre-push scan (result)

The scan covers commit `4c03928..HEAD`. `git rev-list --count HEAD --not --remotes` is 1, so a push sends only
this commit. No branch, tag or stash contains the pre-squash commits.

| Check | Result |
|---|---|
| IP addresses | none |
| Key or identity paths (`.ssh`, `.pem`, `.stellar/identity`), absolute home or tmp paths (`/Users/`, `/private/tmp`, `/home/`) | none |
| Stellar secret keys (`S…` strkeys), tokens | none. `KEEPER_SECRET` appears only as an environment-variable name |
| `.wasm` files, binary patches or `\0asm` content in the diff | none. No Sentora hash (`ca6b85a1`, `f22e4a52`) in any tree blob |
| `LIQ_SEED = [42; 32]` in `tests/group3.rs` | deliberate. An ephemeral test-only ed25519 seed for the synthetic fork liquidator; it controls no real account |
| Public keys and contract IDs | Mainnet public addresses only: Sentora, Aquarius, Reflector, Peridot and the aquarius-keeper public key |

The base branch (`4c03928` and earlier) already contains Aquarius `.wasm` research files. They are not Sentora
code, and this branch does not change them.

## 23. Open items for the owner

1. Accept LI 1.03, XLM CF 0.55, the caps in §14, and `MaxUserMarkets = 2`.
2. The internal limit: keep 120 together with the pre-warm rule, or allow 125 for the cold two-stable liquidation
   (121).
3. The deviations from decision 2 (§15) and from decision R4 (§17).
4. Who holds the adapter admin key, and who is guardian (proposed: the new keeper key).
5. Sentora: reward eligibility and proof API for contract depositors. Is about 95 % of its PYUSD vault acceptable?

## 24. Files (phase 2)

- `contracts/peg-oracle/`: the oracle and its tests. WASM: 14,409 bytes, sha256 `d0c53463…858a39e`.
- `contracts/sentora-strategy/`: the adapter, with cap, pins, kill path and tests. WASM: 18,893 bytes, sha256
  `221eaa03…355954`.
  - The constructor is now `(admin, receipt, asset, sentora_vault, escrow, vest_secs, invest_cap, (vault_wasm,
    escrow_wasm))`.
- `harness/sentora-fork/tools/fetch-fork.cjs`: adds PYUSD, both pools, the DEX feed and `--wasm-dir`.
- `harness/sentora-fork/tools/peg-sample.cjs`: the read-only 25-minute sampler. Raw samples stay in `.scratch/`,
  which is git-ignored.
- `harness/sentora-fork/tests/group3.rs`: the phase-2 fork tests. Their results are written to
  `.scratch/group3-*.json`.
- `bots/sentora-keeper/`: the keeper skeleton. `.do/sentora-keeper.yaml`: the deployment draft.
