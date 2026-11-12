# Frozen Stage1 v1 semantics — package 1

This is the wire/invariant freeze implementing package1 of approved handoff27, based
on `3b962e371d74c32e7f6076296a58bbfcecaa7ff9`. The crate is **rlib only**, with Soroban
SDK25.3.1 contract types and generated clients. No financial contract implementation,
WASM entrypoints, deployment, signing, operational parameters or package2 work is included.
`src/lib.rs` is the authoritative field/type/signature definition; `abi-manifest.json`
and `wire-golden.json` pin the current source, signatures, key encodings and type specs.
An interface change after this checkpoint needs an explicit reviewed freeze revision;
never regenerate golden files merely to make a mismatch pass.

## Identity, amounts and canonical hashes

- Version1; receipt and vault bindings immutable once completed. Existing production
  contracts are not wired to this crate yet. Old compromised market identities are not
  configuration defaults. Synthetic contract addresses in native tests are not signers.
- All principal/share/LP values are raw unsigned units. Every value passed into an
  i128 token call must also be <=i128::MAX. No Rust unchecked cast is permitted.
  Token0 is deposited/settlement PYUSD, token1 USDC; actual issuer contracts, order,
  decimals, fixed range and pool are validated in BindingV1. Fixtures are not production
  parameter selections. No finite production cap is approved by this freeze.
- `OpenTermsV1.amount` excludes the execution reserve. `max_strategy_cost` is both
  maximum permitted charge and the token0 reserve funded in O2. Cost floor and maximum
  forward horizon come from RiskV1; default new risk is disabled. No keeper top-up or
  amendment. Holding cash after mint earns the approved common ownership weight.
- `CommandV1.requested_shares` is immutable:0 for opens, exact requested close quantity
  for closes. `shares` is the resulting/current command quantity. This prevents a
  close quantity from being omitted from the owner's immutable authorization hash.
- Recipient is deliberately **not** an independent field; it is always CommandV1.owner.
  No entrypoint accepts caller-controlled payout address, external route or replacement job.
- The terms hash is NOT a field in its own input. `terms_hash_v1` hashes canonical ScVal
  XDR of the tuple `(Symbol("PERIDOT_STAGE1_TERMS_V1"), network_id, receipt, vault,
  id, owner, binding_hash, requested_shares, CommandTermsV1)`. Network id and hash are
  BytesN<32>, id is u64, requested_shares u128. Domain/order/types are frozen. JSON,
  display strings and amounts converted to decimal text are never signed commitments.
- Binding digest is SHA256 of ScVal XDR `(Symbol("PERIDOT_STAGE1_BINDING_V1"),
  network_id, BindingV1)`. `binding_hash_v1` is shared by clients and contracts.
- `CloseModeV1::InKind=0` requires min_underlying=0; zero token minima may be valid at
  range edges. `ConvertToUnderlying=1` requires a positive final minimum. Min_amounts
  applies to normal C2 extraction; owner-authorized emergency recovery does not inherit
  stale conversion minima. An unsigned executor cannot switch a mode or waive minima.

## Exact authorization and phase rules

The receipt is command authority. The vault's Stage1Liability(id) independently binds
id/owner/hash/phase/category amounts and locked shares. Both update atomically. A matching
aggregate is not enough to refund, burn or unlock an individual command. No callback
receipt→vault→receipt and no fabricated owner/custody subledger in this1× path.

|Receipt function|Before → after|Authority / immutable args and nested calls|
|---|---|---|
|stage1_bind|Unbound → bound|Configured receipt admin; exact vault + binding hash, one time only|
|stage1_config / stage1_job / stage1_next|No change|Read only, no incidental TTL extension; job absence is Option::None, not a zero-funded command|
|prepare_open|None → Prepared|Owner; complete OpenTermsV1. Store computed id/hash; vault stage1_check_open requires bound receipt|
|fund_open|Prepared → Funded|Owner requires auth **for `[id, terms_hash, config_revision]`**, plus nested stage1_accept_funds(receipt,id,full job) and exact token0.transfer(owner,vault,amount+reserve). Custom require_auth_for_args is intentional, not just `[id]`|
|convert_open|Funded → Converted|Permissionless root; receipt contract authorizes exact full-job vault call; vault authorizes exact own token transfer(s) to Aquarius; amount/output deltas checked|
|mint_cash|Converted → MintedPendingInvestment|Permissionless; receipt→vault exact job. Successful upstream claim BEFORE mint, old-supply reward settlement and fee-inclusive NAV. Shares minted directly to owner and locked|
|invest_cash|MintedPendingInvestment → Invested|Permissionless; deposit/valuation/cost refund/unlock/terminal receipt+liability commit in SAME transaction. No new ownership boundary or gratuitous replacement claim|
|prepare_close|None → CloseLocked|Owner commits exact shares and CloseTermsV1; receipt→vault stage1_lock_close locks only available direct shares|
|withdraw_close|CloseLocked → Paid or ExitMixture|Permissionless; normal claim-first; allocation before burn; direct payout in same transaction or segregated claim according to stored mode|
|convert_close|ExitMixture → Paid|Permissionless; quote/oracle and original minimum revalidated; fixed owner payout and terminal state atomically|
|cancel|Prepared/Funded/Converted → Cancelled|Owner when now<=deadline; anyone when now>deadline; return actual held mixture/reserve, no reward claim, swap or oracle|
|cancel|MintedPendingInvestment/CloseLocked → Cancelled|Same authority; unlock shares and refund unused reserve only, no burn or principal cash refund|
|cancel|ExitMixture → Cancelled|Same authority; actual segregated mixture payout, no new shares or conversion|
|recover_in_kind|MintedPendingInvestment/CloseLocked → Paid|Owner, exact id + boolean + consent; bound receipt and owner authority enforced at vault. Recover proportional current backing, not old contributed cash|
|Any mutator on terminal job|Invested/Paid/Cancelled unchanged|AlreadyTerminal, no extra output/refund; missing job UnknownCommand, not terminal success|

Deadline equality: forward action allowed at now<=deadline; permissionless expiry only
at now>deadline. Owner cancellation/recovery is available independently. Stale risk
revision blocks fund/convert/mint/invest and priced optional close conversion; it does
not block in-kind prepare-close, in-kind withdrawal, release, mixture payout or owner
emergency recovery. No fresh oracle/swap call in those non-conversion recovery paths.

All vault stage calls require the exact configured receipt's actual `require_auth`;
owner equality is not authorization. Deep calls using vault-held assets require exact
`authorize_as_current_contract` entries. `stage1_recover_kind` requires owner auth even
when waiver=false; fund likewise. Owner consent authorizes no admin/global recovery
transfer capability. Permissionless executor cannot supply waiver true on anyone's behalf.
The source-envelope and detached-G forms remain separate G4 tests, not native ABI proofs.

### Binding and revision setup

Two-step local initialization avoids a circular callback: receipt stage1_bind stores
vault/hash after admin validation of its initialized immutable identity; then vault
stage1_bind_receipt(admin, BindingV1) validates the receipt getter against itself and the
computed digest. Until both agree, fresh-risk calls fail closed. No funding is accepted
in a half-bound state. Set-once records have no erase/disable/rebind API. Their presence
freezes relevant receipt/vault code-upgrade execution, including a previously queued
upgrade. This freeze cannot be bypassed by turning off RiskV1.new_risk_enabled.

`stage1_set_risk(admin, expected_revision, risk)` is admin-authorized CAS: expected equals
current revision; new revision must equal checked(expected+1). Getter assembles
VaultConfigV1 from binding and risk, with no lending-state sweep. It does not set a cap
or maintenance margin. NewRiskEnabled=true requires valid finite existing vault cap,
positive maximum horizon and reserve floor. No production values are initialized here.

Future staged-mode edits to `set_max_deploy`, `set_slippage_bps`, `set_paused`,
`set_oracle`, `set_oracle_max_age_mult`, `set_oracle_symbol`,
`set_max_pool_divergence_bps`, `set_nav_root_max_stale`, `set_nav_root_max_age` increment
the SAME revision atomically with their change. Changing bound tokens, decimals, pool,
range, receipt or reward identity after binding is prohibited in v1. `set_range_policy`,
`rebalance`, pooled reward route/harvest/sweep and legacy atomic entrypoints cannot mutate
staged backing; their staged-mode disable/compatibility tests belong to packages2–4.
An admin transfer changes authority, not command recipients or owner commitments.

## Recovery and holder consent

`recover_in_kind(id, false, None)` tries the normal claim first; when that fails it
requires an authentic proven boundary or returns existing RewardWaiverRequired802.
For `true`, consent must be present and bind id/hash/holder/max_shares/current interval,
history revision, boundary sequence and **exact execution ledger**. Capture/check these
against the pre-ownership-change state. If the claim succeeds there is no waiver; if it
fails, the same validated consent scopes only the unprovable interval. `false+Some` or
`true+None` rejects InvalidTerms; no silently ignored consent fields. Changes in ledger,
interval/revision/boundary/id/owner/hash/share bound reject ConsentMismatch1712. A missed
ledger requires fresh authorization, never automatic keeper extension.

The direct unlocked-share entrypoint is `emergency_redeem_v1(holder,shares,bool,
Option<HolderConsentV1>)`, with the same interval/revision/boundary/ledger restriction;
Soroban auth binds this distinct method and vault. It cannot be replayed as command
recovery. The old unscoped `emergency_redeem` must be unavailable in staged mode to avoid
bypassing this consent. It remains available in preserved legacy fixtures. Neither
entrypoint can burn encumbered or another command's locked shares. Staged recovery may
unlock only its own LiabilityV1 shares inside the same atomic extraction/burn operation.
No future forced liquidation route may acquire a waiver parameter.

Owner unavailable + both upstream reward calls failing: expiry release/refund may still
complete without a burn; the owner keeps shares and historical claims. This is explicitly
not completed mixture redemption. Missing/disabled/capped token trustlines retain claims
and a recoverable failure, not a forced swap or redistribution. AQUA is booked, not pushed
out during principal ownership changes; reward-token trustline absence cannot freeze exit.

## Storage, events and compatibility

|Contract key|Durability|Frozen value / role|
|---|---|---|
|Receipt Stage1ConfigV1|Instance|ReceiptConfigV1; one-time binding and upgrade-freeze presence|
|Receipt Stage1Next|Instance|u64 monotonic next id; checked overflow|
|Receipt Stage1Busy|Instance|bool per-transaction reentry guard|
|Receipt Stage1Job(u64)|Persistent|CommandV1, terminal records retained; never use temporary TTL|
|Vault Stage1Receipt|Instance|VaultBindingV1, no erase/rotation|
|Vault Stage1RiskV1|Instance|RiskV1 authoritative revision/default new risk disabled|
|Vault Stage1Reserved(Address)|Instance|ReserveTotalsV1 per actual pool token|
|Vault Stage1Liability(u64)|Persistent|LiabilityV1; create at fund or close lock, retain consumed terminal marker|
|Vault Stage1Locked(Address)|Persistent|u128 aggregate command locks; all balance-decreasing paths honor it|
|Vault Stage1Allocation(u64)|Persistent|AllocationV1 before-burn audit record; bounded size|

Every module's raw variant names are audited against115 existing receipt/vault/facade
key variants. Instances are single physical ledger keys despite multiple logical fields.
Persistent records stay individually bounded. No beneficiary vector or history migration.
Terminal job records do not disappear with unresolved reward rights. Temporary allowance
expiry never restores spending permission; existing custody/history namespaces unchanged.

Receipt emits topics `(Symbol("stage1"), u32(1), u64(id))`, data TransitionV1, after a
successful transition; no event on rolled-back success paths. Vault emits allocation
`(Symbol("stage1_feealloc"), u32(1), id)` with AllocationV1 BEFORE burn event
`(Symbol("stage1_burn"), u32(1), id)` with `(shares, old_supply, new_supply)`.
These names/topic types/order and payload contracttype specs are frozen; G4 will test
actual implementation ordering/bytes. No emit code exists in this package.

Facade has **no new Stage1 ABI** and zero expected keys in nine financial roots. No
Stage1 facade deployment. Keep legacy facade behavior for comparison; staged vault's
atomic quote/deposit/redeem routes fail closed, so facade estimate0 and swap rejects
before input spending. A facade must never claim asynchronous success or consume locks.

Cash identity per token at every committed boundary:
`raw = Excluded(donations) + Book(common_idle) + Reserved.Unminted + Reserved.Execution + Reserved.ExitClaims`.
Category aggregates equal the live per-id liabilities. LP and AQUA are separate assets/
entitlements. After mint contribution is common backing; cancel cannot refund it again.
Execution cost never exceeds reserve; reserve is not keeper XLM, supplier withdrawal
cash, loss reserve or liquidation working capital. Backing shortfall preserves recorded
claims and rejects new risk; no unapproved loss waterfall is encoded here.

## Frozen invariant IDs and downstream gate closure

- S1-ID: network/pair/counterparty/owner/id/hash/version fixed; no half-bound funding.
- S1-AUTH: actual contract+owner trees, exact amounts and recipients, no blanket bypass.
- S1-PHASE: only permitted transition, atomic dual receipt/liability update, no replay.
- S1-CASH: category identity/donation exclusion; no principal/reserve/claim double count.
- S1-MINT: successful claim, old-supply reward settlement, fresh actual fee-inclusive NAV.
- S1-INVEST: current actual AND oracle-convergence value preserved; bounded entrant cost;
  fixed cash-first participation, no earlier claim replacing a later boundary.
- S1-FEE: INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN, old supply, all fees,
  zero-principal/one-unit case, no surplus paid to newcomer.
- S1-LOCK: direct/allowance transfer/burn/redeem/custody cannot spend locked shares.
- S1-REWARD: exact scaled conservation, exited beneficiaries, quarantined missing evidence,
  proportional shortfall, attributable excess, historically eligible waiver redistribution.
- S1-CONSENT: owner-only scoped waiver; no booked/other/later entitlement erased.
- S1-RECOVER: expiry/config/pause/new-risk checks cannot block authorized release or
  in-kind recovery; issuer/venue restrictions retain claims, no false completion.
- S1-HISTORY: bounded affected-owner append; no history-length denial of existing recovery.
- S1-RESTORE: authentic code/instance/data restoration, installed TTL/source fee+sequence;
  actual interruption/re-expiry and complete journey costs, no edited-TTL substitute.
- S1-PARITY: exact financial state/return/events vs Core; explicit native fee/sequence
  differences, invalid auth/rollback semantics, no broad normalization.
- S1-RESOURCE: unchanged100M/41943040/100SDK/100keys/50writes and all byte/event/rent guards.

`proof-map.json` binds nine financial rows and the four planned gate families to these
IDs. It explicitly marks them unimplemented/unrun. Native schema/hash tests are package1
checks, not passing resource, accounting, recovery or release rows. No new financial
flow exists to measure with check_budget.sh in this package; every implemented flow
must use it in later packages. Existing profiles and failing combined gates are unchanged.

## Reproduction and artifact identity

```
python3 interfaces/cl-stage1-interface/freeze.py
CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 cargo test --locked -p cl-stage1-interface
```

No --initialize or STAGE1_WRITE_GOLDEN in verification. Those one-time initial creation
paths refuse to overwrite existing files. Cargo.lock adds only this library package;
SDK remains25.3.1, while accepted native resource harness29.0.1 and Core29.0.0 remain
separate runtime identities. Rust native tests do not create a deployable WASM; later
contract WASMs must be built only with stellar contract build. Package2/3 consume this
rlib explicitly and preserve the frozen wire shapes unless reviewed before adoption.
