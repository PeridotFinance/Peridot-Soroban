# Atomic qualification: authorization and admitted workloads

This is a local qualification plan, not an active admission policy. No builds,
signatures, network calls, keys, production edits or new transaction stages were
used to prepare it. Stage 1 remains incomplete, borrowing disabled and PR75
draft. Handoff12's proposed 300M/300 SDK entries/300 unique keys/100 writes/32MiB
profile is analysis only. Its 80% workload boundaries are 240M/240/240/80/
26,843,545 bytes. Earlier evidence left only 3,305,137 CPU; the focused
qualification now exceeds that boundary in the unchanged combined-history
baseline. Use handoff16's measured results, not the earlier headroom, for the
next decision. All existing checks, claim-first boundaries and recovery rules remain.

## Recommendation and scope

Qualify the existing atomic path first, using one fixed two-asset venue binding,
the pinned compiled contracts and target runtime, and an explicit authorization
class. The qualification must cover deposit/redeem, receipt and margin custody,
open/activation, close, full repayment and funded recovery. A successful open
alone cannot define an admissible product. Limits on **new risk** may reject a
new deposit/debt increase before mutation; they must not be added to repayment,
forced liquidation or historical settlement to make those calls disappear from
the measured population. Narrowing a supported workload is useful only if its
conditions are enforced, or its resource independence is established from code
and worst-state execution. Client conventions are not contract enforcement.

## Existing bounds and missing enforcement

| Dimension | Current access pattern / enforcement | Qualification requirement |
| --- | --- | --- |
| Position IDs per borrower | `MAX_USER_POSITIONS=64`; `push_user_position` rejects a full vector. Removal iterates that vector but does not load every sibling position. Compaction examines at most8 IDs, while reconstructing the bounded vector. | Measure length64, removal at beginning/middle/end, stale IDs and archived sibling IDs. A claim based on only8 positions does not qualify the existing64 bound. Compaction is an optional maintenance route, never a prerequisite for closing an existing position. |
| Pair policy registry | `MAX_PERPS_PAIR_CONFIGS=32`; configuration lives in shared contract state. | Exercise the full permitted registry's serialized size even when only one pair is active. A two-entry fixture does not bound the production instance cost. |
| Swap route | `validate_direct_pool` requires exactly2 tokens and an allowed adapter binding. | Pin pool ID/address, asset identities/decimals and executable identities in evidence. Keep the tested route direct. Binding admission is not an assertion that mutable venue state remains cheap. |
| Amount and pool traversal | `max_deploy` limits deployed value when nonzero; slippage/oracle guards constrain price/output. They do not directly cap initialized ticks traversed or intermediate quote work. | Determine explicit supported trade-size/liquidity/tick-path bounds for fresh entry and measure boundary/adverse branches. An amount/reserve percentage alone is not a tick-count bound. Missing enforceable traversal/complexity bounds remain a qualification blocker. Do not cap forced exits using a new-entry trade limit. |
| Reward history | Per-owner checkpoints persist; settlement accepts a historical index. Waiver preparation has batch1..4 and bounded binary search over a u32 checkpoint count (at most32 search iterations). Critical ownership updates do not enumerate all holders. | Measure deep checkpoint searches, missing/archived checkpoints and multiple unresolved intervals. An explicit-index settlement validates neighboring checkpoints; include those reads. Do not cap total retained history or erase unresolved records. Counter exhaustion and sustainable rent funding still need an operating bound/upgrade plan. |
| Global owners / borrowers | No new global64-owner or8-borrower admission cap was found in these paths. Existing tests are samples, not such caps. | Demonstrate that unrelated beneficiaries/positions stay outside the touched footprint, while shared serialized records remain bounded. If cost grows with global state, identify the exact record and bound its growth before admitting new risk; do not silently rely on a frontend customer limit. |
| Authorization | Public methods use `Address` authorization; the contract generally does not enforce a source-account-only, single-signature product class. | Distinguish a measured client/auth class from enforced permissionless support. A gateway's envelope filter can bound that gateway's service, not other callers. No unsupported wallet may be trapped by a new recovery allowlist. |

Source anchors: `contracts/margin-controller/src/{constants,helpers,perps}.rs`,
`contracts/cl-share-vault/src/{storage,reward_history}.rs`. No setters or new
enforcement entrypoints are assumed. Production maintenance margin is not
configured by this plan.

## Later authorization experiment — no signing now

The current runtime helper in `harness/cl-runtime29/src/transaction_evidence.rs`
uses a synthetic zero source and recording external authorization, then serializes
an unsigned envelope. Actual nested contract authorization still executes, but
that helper cannot establish the final account-signature cost. The following
experiment requires a later explicit authorization for local synthetic signing;
no real account, existing key store, production secret or broadcasting is needed.

Use the same pinned snapshot, exact cost model, compiled bytecode and successful
root/arguments for paired comparisons. Every signed case executes independently
from the same prestate; it is not a second execution on already-mutated state.
Preserve authentic errors and complete before/after state hashes. Measure both
the Soroban host and the transaction application layer; do not add overlapping
inclusive counters. If a matching full transaction runtime is unavailable,
report that as a missing gate rather than treating host success as full execution.

1. **G account as transaction source and external actor.** Use one synthetic
   Ed25519 account with one sufficient signer, no extra signers, and source-account
   credentials for the exact authorization tree. Execute real transaction-envelope
   signature checking, source sequence/fees/threshold checks and Soroban invocation
   in a protocol29 transaction runtime. Local host29 `auth.rs` creates source
   credentials with no address nonce or signature value: the host trusts the
   transaction layer's source authentication. A host-only source-credential run
   therefore measures none of the missing envelope verification by itself.
2. **G address authorization with a different G transaction source.** Keep the
   actor and source as distinct single-signer accounts; sign the exact Soroban
   authorization payload with its network ID, invocation tree, nonce and expiry,
   plus the source envelope. Enforce the account's actual threshold and signature
   in host execution. Include account reads, temporary nonce creation/TTL/rent,
   signature vectors, footprint, return/events, envelope and fee components.
   This is the sponsored/relayed-actor class; it is not equivalent to case1.
3. **Authentication failures and replay.** Mutate caller, nested amount/recipient,
   network payload, expiry and signature; repeat a consumed address nonce and
   an already-applied source sequence. Verify the intended authentication stage,
   unchanged debt/collateral/rewards and transaction-layer fee/sequence behavior
   appropriate to the rejection stage. Do not assert that every invalid included
   transaction leaves its source fee/sequence unchanged. Existing synthetic
   admin authorization remains explicitly separate from these account tests.

Record separately: signing/envelope bytes; host signature CPU/memory; transaction
layer verification work; additional account, trustline and nonce keys; read/write
bytes; disk restoration; nonce/contract TTL rent; inclusion, refundable and
non-refundable fee components. Whether account keys belong in Soroban footprint
or other transaction-layer accounting depends on the path: report the actual
serialized sets rather than assuming every account read adds one Soroban key.
Report end-to-end wall time separately from network latency. Never infer fees
by adding maxima from different transactions.

Initial qualification targets cases1 and2 separately. Each root may require
multiple distinct external authorization entries even for one actor; record and
bound the actual forest rather than assuming exactly one auth entry. The
liquidator/payer is the actor for funded liquidation and must not need the
borrower's signature. Controller/facade/vault authorization remains real in all
cases; no mock authorization contracts or blanket bypasses are allowed.

Multisig and contract wallets are separate, initially unqualified classes. For
multisig, specify thresholds, signer types/counts, submitted signatures and
number of external authorizers, then qualify the maximum supported combination.
The local host's maximum20 account signatures is a protocol implementation
bound, not evidence that20 fit the proposed workload. For a contract wallet,
pin its `__check_auth` executable and configuration, signer/storage/recursion
bounds and restoration costs; arbitrary wallet code cannot inherit the G-account
budget. Fee-bump, delegated/AddressV2 authorization, muxed-account handling and
multiple external actors likewise require explicit scoped evidence before being
advertised as supported. None is silently blocked from existing recovery by this
document; third-party funded recovery must remain usable independently of an
owner wallet's availability.

## Concrete combined-state qualification matrix

Use the existing maximum-state fixtures as starting points, not new production
caps. At minimum cross the peak root with these dimensions, including reverting
branches and actual restoration. Begin with the highest observed branches;
number of holders alone is not a monotonic cost predictor.

* **Ownership and borrower diversity:** compare one borrower with64 position IDs
  against at least8 independent borrowers with8 positions each, while retaining
  the64 direct beneficiaries/history fixture. Exercise separate receipt balances,
  custody assignments, fee debts, exited beneficiaries and one liquidator distinct
  from every borrower. Demonstrate unrelated-owner independence through measured
  footprints, not just a low aggregate count. This64-position comparison is a
  test shape, not a global position cap.
* **History/outages:** retain the383-checkpoint/128-extra-transfer/8-outage/
  8-exited-owner baseline, combine it with repeated receipt/margin custody changes
  and both upstream calls failing, and include the worst per-owner search depth
  attainable in the fixture. Add unresolved intervals and waivers without
  processing all of them in a critical exit. Settle original/exited owners later;
  missing authentic boundaries remain quarantined. Forced exits never waive.
* **Venue stress:** exercise both existing1% and5% reserve-trade paths, odd rounding,
  out-of-range/re-entry and emission changes; combine sparse and dense initialized
  ticks, quote traversal and low in-range liquidity. Include claim success,
  killed claims and pending-view failure. No unmeasured live tick-density bound
  is inferred from a synthetic reserve percentage.
* **Archival and age:** restore touched vault/receipt/controller/pool code and
  persistent data; combine borrower position/index, custody/reward checkpoints,
  interval/segment/waiver data and relevant pool ticks in the same transaction
  when the target runtime supports that restoration path. Vary expired/near-expiry
  TTL and accrued debt age. Account/trustline state is classic ledger state, not
  falsely labelled restorable Soroban data. Measure real nonce TTL in address-auth
  cases. Preserve both automatic-restoration and any required separate restore
  transaction evidence, fee payer/funding and state-before-application semantics.
* **Configuration and failures:** full allowed pair registry, longest position
  vector, reserve shortfall after open costs, pause transitions, expired pending
  state, failed collateral transfer, insufficient liquidator funds and duplicate
  execution. Complete rollback and independent recovery must hold. Admission
  gates must not be copied into recovery merely to shrink its footprint.

Missing numeric tick/traversal bounds, supported auth-tree maxima, combined-state
rent funding and long-lived history growth are explicit blockers. Any future
on-chain new-risk check needs its own measured overhead and cannot be treated as
already present. Auth classes or economic positions can change after entry, so
the recovery design must remain independent of continued compliance by an owner.

## Decision output

Produce one row per full root and auth/workload class: completed stage, exact
WASM/runtime/settings identities, full resource/footprint/fee values, rollback
results, and distance to each proposed80% boundary. Preserve unchanged internal
and captured-profile failures separately. The local unsigned work can identify
measured optimizations now, but release qualification remains incomplete until
the later authorized signature experiment and unavailable live-state evidence
exist. If any complete necessary recovery exceeds the admitted envelope, reject
new-risk admission; never waive that recovery proof or prune old entitlements.
