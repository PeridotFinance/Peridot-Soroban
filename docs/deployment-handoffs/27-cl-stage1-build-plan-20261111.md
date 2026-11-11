# Stage 1 build plan: 1× cash-first PYUSD/USDC — 2026-11-11

**Proposed implementation plan, awaiting operator approval. No build authorized by this
document.** Accepted checkpoint `7f66fb056d0289fcfcd7526399608284521c79e3` closes Stage1's
resource feasibility question. Preserve it and `3188ae05` as comparison baselines.
Aquarius stays; Option1 is permanently closed. Stage1 remains incomplete and PR75 draft.

The proposed implementation has **nine transaction row types**, not nine mandatory
transactions for each user. A direct in-kind journey is five open transactions plus
two close transactions; a converted close is five plus three. Restoration operations
are additional native maintenance transactions, explicitly measured and paid. There
is no hidden financial finalization transaction.

Scope is the transferable1× share vault, its receipt command interface and keeper.
Debt is zero in every state. No margin activation, borrowed funding, new2× execution
ABI, liquidation implementation, active range optimization or secondary oracle enters
this build. Existing ordinary transfer and custody authorization/reward hooks remain
intact; custody reassignment itself belongs to the later2× milestone. No legacy
compromised Peridot Mainnet address is reused. Existing limits, borrowing-disabled
preparation, unset production maintenance margin, validation Mainnet prohibition,
no-Upshift and no handoff00-edit restrictions remain. Signatures are limited to the
already authorized private fixture. No deployment or production parameters approved.

## 1. Accepted evidence and implementation boundary

|Row|Public receipt operation proposed|Measured CPU ceiling used here|SDK total / unique keys / RW|
|---|---|---:|---|
|O1 prepare|`prepare_open(owner, terms)`|7,397,126|16 /13 /3|
|O2 fund|`fund_open(id)`|3,754,855|17 /12 /5|
|O3 convert|`convert_open(id)`|31,245,907|30 /22 /8|
|O4 fee checkpoint + cash-backed mint|`mint_cash(id)`|72,062,767|69 /50 /19|
|O5 invest + finalize|`invest_cash(id)`|95,144,812|61 /43 /18|
|C1 prepare close|`prepare_close(owner, shares, terms)`|2,724,937|13 /10 /3|
|C2a withdraw + in-kind payout|`withdraw_close(id)` with InKind mode|78,728,736|78 /52 /26|
|C2b withdraw + reserve mixture|`withdraw_close(id)` with Convert mode|78,061,427|74 /50 /24|
|C3 optional conversion + payout|`convert_close(id)`|23,786,729|32 /23 /9|

These are accepted observations, not upper bounds for arbitrary state. SDK total means
memory reads + disk reads + writes; unique keys mean serialized Soroban footprint keys,
excluding implicit native fee/sequence and TTL bookkeeping. Each implementation change
must preserve100M CPU,41,943,040 memory bytes,100 SDK total,100 unique keys,50 writes and
all existing byte/event/rent checks. This is regression qualification, not reopening
policy research. The cold investment observation95,205,766 leaves4,794,234 instructions.
Do not introduce a two-tranche split or a second facade hop without new measured need
and an explicitly reviewed design.

Sources: accepted [signed report26](26-cl-stage1-private-signed-application-20261110.md),
[staged report25](25-cl-stage1-staged-transaction-footprints-20261110.md),
[fee-pending report24](24-cl-stage1-fee-pending-comparison-20261110.md), prototype
`prototypes/cl-stage1-cash-first/{receipt,vault,command_types,stage_storage}.rs`, and
`prototypes/cl-stage1-signed-local/`. Report26 supersedes report25's incorrectly labelled
SDK display column; SDK/checker enforcement had already used the correct total.

The measured call path is **receipt → vault → Aquarius/tokens/oracle**, with direct
beneficial ownership. The compiled facade is a fixture dependency, not an invoked hop
in these nine rows. A plan that inserted it would have different costs and authorization.

## 2. Contract and ABI work

ABI names and error IDs below are a concrete proposal to freeze at work package1;
the typed consent and per-command liability record incorporate Fable review. Existing research `stage_*` names are not a production
compatibility promise. Use versioned types and explicitly namespaced storage keys.
A Rust enum's type name does not namespace the ScVal encoding of its variants.

### Receipt market: command authority, no share custody

Retain existing cash-only market behavior. Add a narrowly bound Stage1 command module;
do not route Stage1 cash through receipt supply, boosted allocation or borrow helpers.
The receipt stores the sole authoritative command. It passes an exact typed copy to
the bound vault under real contract authorization; the vault must not callback into
the receipt during the same call. Both contracts' updates commit atomically.

Proposed types:

- `OpenTermsV1`: contributed token0 amount, conversion input, minimum token1 output,
  minimum shares, maximum strategy execution cost in token0, forward deadline,
  accepted config revision and canonical terms hash. Funding authorizes both id and
  that hash/revision, including the exact nested transfer amount. Funding pays
  contribution plus the separate cost reserve.
- `CloseTermsV1`: explicit `InKind` or `ConvertToUnderlying` mode, token0/token1
  minima where applicable, final conversion minimum, forward deadline, config revision.
  Replace the prototype's `min_swap == 0` mode sentinel; do not overload zero with consent.
- `CommandV1`: immutable owner/fixed payout recipient (same address in Stage1), id,
  kind, schema version, terms, phase, actual token amounts, remaining cost reserve,
  minted/locked shares and terminal result. Bounded record; no beneficiary/history array.
- `WaiverConsentV1`: command id/terms hash, holder, maximum shares, current reward
  interval id, history revision, boundary sequence and exact execution ledger. Required
  only when `accept_reward_waiver=true`; check all fields against the execution boundary.
  A missed ledger, changed interval/revision/boundary or different job requires new
  owner authorization, not a keeper extension. This binds unknown-amount consent to
  an explicit boundary; it cannot promise a numeric maximum on an unknowable reward.
- `PhaseV1`: Prepared, Funded, Converted, MintedPendingInvestment, Invested,
  CloseLocked, ExitMixture, Paid, Cancelled. Results distinguish share release from
  actual mixture payout; a cancellation event must not imply assets were withdrawn.

|Public ABI|Authorization and checks|Atomic effect|
|---|---|---|
|`stage1_config()`, `stage1_job(id)`|Read only; no broad TTL maintenance|Versioned configuration/status for client and keeper|
|`prepare_open(owner, terms) -> id`|Owner auth; positive checked amounts; supported pair; bounded deadline; current configuration/fresh-risk checks|Create immutable terms and Prepared command|
|`fund_open(id)`|Owner auth plus exact nested token transfer; phase/deadline/revision|Vault receives contribution + cost reserve; command Funded|
|`convert_open(id)`|Permissionless executor; stored terms; fresh quote/oracle/band, protocol pauses/caps as applicable|Convert segregated command assets; record actual balances; Converted|
|`mint_cash(id) -> shares`|Permissionless executor; claim-first; fresh fee-inclusive backing and oracle; current cap; owner minimum|Release contributed cash into common backing; mint owner shares and lock them; MintedPendingInvestment|
|`invest_cash(id) -> liquidity`|Permissionless; fresh valuation/quote/band; current risk bounds; available common cash; reserve sufficient|Aquarius deposit; preserve actual and convergence backing; charge bounded strategy cost; refund surplus; unlock shares; Invested|
|`prepare_close(owner, shares, terms) -> id`|Owner auth; positive available unencumbered shares; valid deadline|Lock exact shares; CloseLocked|
|`withdraw_close(id) -> amounts`|Permissionless under recorded close; claim-first on normal burn; old-supply fee allocation|Withdraw once, allocate fee/principal, burn; direct payout→Paid or reserved mixture→ExitMixture|
|`convert_close(id) -> amount`|Permissionless; stored minimum; current oracle/quote/band; no recipient override|Convert segregated exit mixture and pay fixed owner; Paid|
|`cancel(id)`|Owner before expiry; anyone after expiry; no waiver argument|Phase-specific fixed-owner refund, share unlock or mixture payout; no new risk|
|`recover_in_kind(id, accept_reward_waiver, consent)`|Owner auth; only post-mint locked/close-locked shares; explicit waiver semantics below|Proportional mixture withdrawal through authorized reward-recovery path, allocation then burn, reserve refund and terminal result|

Do not offer term amendment/top-up in the first build. If bounds cannot be met, cancel
and create a new command; never let a keeper relax them. Fresh-risk config revision,
cap or deposit pause checks must not strand recovery, share release or already allocated
payouts. A user may voluntarily wait; retries cannot consume or reset owner consent.
A genuine venue withdrawal prohibition or asset issuer restriction may prevent payout;
retain an enforceable claim and expose the reason rather than report completion.

Binding/configuration: set each direction exactly once under its configured admin,
validate the counterpart/schema before accepting funds and reject mismatched/rebind
attempts. Receipt/vault Stage1 deployments must freeze code upgrades and binding
rotation while staged mode is enabled; existing admin upgrade entrypoints must enforce
that freeze, rather than trust an address through arbitrary replacement WASM. No new
upgrade mechanism is planned. A future upgrade/migration needs separate approval and
liability/history continuity proofs. Give new-risk parameters one authoritative vault
config revision; each relevant Params/oracle/wiring change increments it. Commands
commit to that revision for forward risk, while recovery uses original asset/owner
identity and must not fail solely because revision changed.

### Vault: accounting and exact authorization

Add internal `stage1_check_open`, `stage1_accept_funds`, `stage1_convert_open`,
`stage1_cash_mint`, `stage1_invest_cash`, `stage1_lock_close`, `stage1_withdraw_kind`,
`stage1_convert_exit`, `stage1_cancel_kind`, `stage1_recover_kind`. Each receives
`(receipt, id, CommandV1)`; recovery additionally takes the owner's boolean and typed
waiver consent. Require the exact bound receipt's authorization and schema/phase/kind consistency;
funding and voluntary waiver recovery additionally require the owner. Only the receipt can supply command state.
A user cannot manufacture a job by calling the vault directly. The receipt remains command/terms authority; add a minimal fixed-size persistent
`Stage1Liability(id)` in the vault as a second accounting check: owner, terms hash,
phase, unminted token amounts, execution reserve, exit amounts and locked shares.
Create it at funding or close-lock; compare and update it atomically on every later
transition. The bound receipt cannot refund or unlock against another job merely
because aggregate balances suffice. Terminal records remain consumed. This is a
bounded per-id ledger, not a holder enumeration or duplicate history. It is a proposed
change requiring measurement, not part of the accepted resource table.

The vault authorizes only its exact downstream token/venue invocation trees. Equality
with a configured address is insufficient authorization. Keep non-reentry guards in
receipt and vault; reject custom-account callbacks that attempt command reuse or
concurrent withdrawal. No blanket mocks of receipt/vault/facade authorization.

Accounting requirements:

1. Segregate unminted cash, entrant cost reserves and post-burn exit claims from common
   NAV. Maintain three named aggregate subfields per token in the same instance record,
   plus the per-id liability records; their sum is Reserved. Existing donation exclusion and balance-delta accounting remain; one liability
   cannot fund another command or an incumbent withdrawal.
2. At mint, claim upstream rewards and settle the old owner balances/supply, collect
   pool-token fees, and use fresh **actual withdrawable composition**, valued at the
   primary oracle. Include fees once. Release only contributed cash into common backing.
   Mint directly to the owner; lock the new shares until investment or release.
3. Once minted, command amounts are investment intent, not a continuing segregated
   principal claim. Other legitimate exits may consume common cash. Investment then
   retries or cancels safely; it cannot reserve a second claim on that cash. The entrant
   participates in common gains/losses after mint under the approved cash-first semantics.
   This includes temporary idle-capital participation; do not promise zero carry/free-riding.
   Admission must reject zero/insufficient execution funding and already unsatisfiable
   terms using a current cost floor, and enforce a protocol-bounded deadline. Values are
   not approved production parameters. Test repeated mint/cancel/exit cycles explicitly;
   do not silently remove pre-investment fee/reward weight or impose a new forfeiture.
4. Investment revalidates current oracle/pool prices and available cash, checks both
   actual backing and conservative value at the oracle's price, charges only the
   entrant's bounded strategy reserve, then refunds/unlocks/marks complete in that same
   transaction. Remove prototype hard-coded range math only in favor of reviewed exact
   arithmetic for the selected fixed range and actual token decimals. No active recenter.
5. Preserve **INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN**. With old supply S,
   burn q, idle I and all pending pool-token fees F, allocate floor((I+F)q/S), reconcile
   actual principal versus the venue's all-fee payout, fund the exit liability and verify
   retained backing before any share burn. One-unit/zero-principal and zero-LP-burn paths
   need explicit tests. Fee collection cannot be deferred past the ownership boundary.
6. Normal mint, burn and transfer keep same-transaction claim-first boundaries, including
   successful zero claims. Allowance transfer/burn and existing custody hooks retain
   their checks. Lock/unlock without ownership change and investment do not invent an
   extra claim; no previous claim substitutes for a later actual ownership boundary.
7. Route the owner's existing emergency redemption and staged recovery through the same
   safe proportional extraction kernel. Ordinary claim outages still revert normal
   mint/transfer/burn. Authorized emergency recovery records historical weights/readings,
   settles booked rewards and distinguishes proven, waived and unresolved amounts.
8. An unproven voluntary emergency burn with `accept_reward_waiver=false` reverts with
   `RewardWaiverRequired`; true requires that owner's authorization and applies only to
   the unprovable interval entitlement. Never waive already booked rewards, another
   holder's entitlement or subsequent accrual. Consent binds interval/revision/boundary
   and exact execution ledger; delayed/replayed consent fails. Missing boundary evidence stays missing;
   a later cumulative reading cannot establish it. Forced2× liquidation is outside this
   build and must never acquire a waiver parameter.

Lock checks apply to every balance-decreasing API: direct/allowance transfer, direct/
allowance burn, redeem, emergency redeem and custody movement. Preserve unlocked-share
transferability. Existing custodial/encumbered shares must not be accepted as direct
Stage1 owner shares; no receipt or margin custody is created in this1× flow.

Bootstrap and compatibility are part of the implementation, not a tenth unmeasured
financial stage: zero supply/zero liquidity must support first cash mint and first
investment without the prototype's old atomic seed deposit. Final exit must cleanly
handle the venue deleting a zero-liquidity position. Treat legacy atomic deposit/
redeem/zap entrypoints as unavailable on the new staged product; retain historical
implementations/evidence separately. Do not change a deployed layout in place.

### Facade: explicit separation, no unmeasured extra hop

`cl-share-pool-facade` currently exposes synchronous V3 `get_tokens`, `estimate_swap`
and `swap`, returning output in the same invocation. It cannot truthfully represent
these asynchronous commands. **Stage1 requires no new facade execution ABI and uses
no facade storage on its nine financial roots.** Clients and keepers address the receipt.

Do not deploy a facade for Stage1, and never deploy an uninitialized facade that an
attacker could initialize. Preserve the existing facade only as a disabled legacy
fixture/comparison; any future deployment requires separately reviewed initialization. Add compatibility protection
at the new staged vault boundary: legacy atomic deposit/redeem calls must reject with
an explicit `AsyncStageRequired` result, and the legacy atomic quote must be unavailable
for that staged configuration. Facade estimate must consequently return0, and direct
swap must reject with a typed unavailable-quote/async-route error before it can spend input; failure must roll back any attempted token
movement. Preserve token discovery and existing valid legacy fixture behavior separately.
Do not add a new staged-vault capability getter to every hot path merely for convenience.

Test that the facade cannot bypass stage locks, deposit pause or reward recovery rules;
donated dust neither funds a command nor prevents one. Existing facade Mainnet guard
and exact nested authorization remain. Later asynchronous2× delivery/activation/recovery
ABI and key inventory are a separate **document-only** milestone, not work hidden here.

## 3. Intermediate states and recovery obligations

All rows have **zero debt**; there is no interest, lending guarantee or borrower-funded
liquidation prerequisite in Stage1. Every payout is fixed to the recorded owner.

|State after transaction|Custody and entitlement|Next-step fresh validation|Deadline/cancellation and permitted executor|
|---|---|---|---|
|Prepared O1|Owner still owns wallet cash; receipt has command only|At funding: exact terms, phase, binding/revision and pause/deadline|Owner may cancel; anyone may expire; no funds to recover|
|Funded O2|Vault holds segregated token0 contribution + strategy reserve for owner; no shares|O3 fresh oracle, issuer restrictions, pool quote/band, minimum and deadline|Owner cancel or permissionless expiry returns held token0; no swap required|
|Converted O3|Vault holds actual segregated token mixture + reserve; no shares|O4 current claim, fees, oracle/backing, cap, minimum and deadline|Cancel/expiry returns actual mixture + unused reserve, not promised original-token equivalent|
|MintedPendingInvestment O4|Owner holds locked shares; contribution now common backing; reserve still personal|O5 fresh oracle/quote/band, current cap/pause, cash availability and both NAV checks|Expiry cancel unlocks shares/refunds reserve without burn; owner may request in-kind recovery under explicit reward waiver rule|
|Invested O5|Owner has transferable shares and historical reward rights; reserve settled|A later normal share movement claims again; close creates its own terms|Original open is terminal; no duplicate refund. Owner prepares close or uses authorized holder emergency route|
|CloseLocked C1|Owner still owns locked shares and earns on them until burn|Normal C2 claim, old-supply fee allocation and actual venue amounts; mixture extraction needs no conversion oracle|Owner cancel/expiry releases lock; owner emergency recovery if claim unavailable; executor cannot invent waiver|
|ExitMixture C2b|Shares already burned; vault owes segregated actual token amounts; historical rewards remain payable to former holder|C3 fresh oracle/quote/band/minimum and deadline|Cancel/expiry pays original mixture without requiring swap or oracle; never recreates shares or pays twice|
|Paid C2a/C3, Cancelled|No unsettled command principal; independent reward entitlements remain|None for terminal command; outstanding reward settlement follows its own evidence|Repeat execution rejects; terminal record/reward references retained|

Forward expiry is not forfeiture. Before expiry, only the owner cancels. After expiry,
anyone can execute fixed-owner release/refund; no caller may redirect it. If the owner
is absent and both reward calls fail, release shares without burning or waiving them.
Do not relabel that as successful token-mixture redemption. An authorized owner chooses
whether to accept an unresolved-reward waiver for a voluntary burn, as handoff00 requires.

The shared handoff's old V3 `begin→execute→swap→activate` wording remains an already
flagged coordination item, not this Stage1 sequence. This document neither edits00 nor
uses its outdated ordering to add a Stage1 dependency on margin execution.

## 4. ABI key inventory and access rules

This is a key-family inventory for the proposed ABI, not a fabricated post-build
footprint count. Preserve accepted exact XDR keysets in report26's archive. The build
must publish a root-by-root diff including restored writes, code and instances. A new
logical instance field may add serialization cost even when it adds no physical key.

|Contract/family|Physical storage and logical contents|Touched by / access|Retention and restoration|
|Receipt existing initialization|Persistent `Initialized`, `UnderlyingToken`; receipt code + instance|Every measured command root reads these; no lending/controller sweep|Restore only actual dependency keys; preserve identity validation|
|Receipt Stage1 configuration|Instance `Stage1ConfigV1`, `Stage1Next`, `Stage1Busy`; one physical instance|Binding/schema read; counter writes only prepare; guard write on mutating roots|Versioned additive fields, immutable binding for commands; instance/code TTL|
|Receipt command|Persistent `Stage1Job(id)` bounded record|Prepare creates; each transition R/W exactly its job; query read only|No command vector; retain live liabilities/results; no pruning in first build|
|Vault base configuration/state|Instance Config/Params/State/Admin/Initialized; persistent PositionAmounts and token oracle symbols/wiring as current definitions|Fresh-risk roots read relevant config/feeds; mint/invest/withdraw mutate supply/liquidity/backing; no irrelevant reward routes/admin proposals|Preserve existing serialized types; no broad keepalive sweep|
|Vault staged accounting|Instance `Stage1Receipt`, `Stage1Reserved(token0/1)` with Unminted/Execution/Exit subfields; existing Book/Excluded/Busy|Bound caller check all stage roots; reserve totals R/W fund/convert/mint/invest/payout; donation accounting on asset-changing roots|Aggregate segregated liabilities exactly once; restore instance/code, not every job|
|Vault per-command liability|Persistent `Stage1Liability(id)` bounded owner/terms-hash/phase/token-category/locked-share record|Create O2 or C1; compare+update every later transition; reject cross-job refund/unlock|One additional job key proposed; terminal consumed records retained; restore with receipt job|
|Vault owner balance/locks|Persistent `Shares(owner)`, `Stage1Locked(owner)`|Mint/lock/unlock/burn R/W; every transfer/allowance/custody movement checks available shares|Owner rights persist through expiry/archive; cannot reset lock because job is archived|
|Vault fee allocation audit|Persistent `Stage1Allocation(id)` fixed-size record; allocation/burn events|C2/recovery writes once, prior to burn; getters read only|Old supply/principal/fee/retained amounts; no growing history vector|
|Reward accumulator and holder|Instance Index/Count/Interval/Open/Totals; persistent Holder(owner), Boundary(id), Bucket(interval), Evidence/Proposal when needed|Claim/settle at actual ownership change; recovery appends affected boundary; booked personal claim independent of pool outage|AQUA excluded from NAV; preserve exited holders and unresolved claims|
|Historical attribution|Persistent HistoryEnabled/Revision/Tracked/Count(owner)/Weight(owner,index)/Segments/Segment(id)/Interval(id)/Waiver(interval,n)/WaiverIndex(interval,owner)/Settled(segment,owner)/Proposal(segment)/Totals|Bounded affected-owner checkpoint on movement; indexed bounded continuation for reconciliation/waiver settlement|Do not scan all beneficiaries in an exit; no pruning unresolved intervals; governance evidence does not become automatically verified data|
|Custody compatibility|Existing Registered(c), Allocation(c,owner), Total(c), Beneficial(owner), Unassigned, Transfer(c)|Direct-holder validation may read relevant keys; no new Stage1 custody transfer|Preserve existing hooks and historical owners; actual2× path deferred|
|Facade|Existing Config/Busy/NextController/ReadyAt inside its instance; code|**Zero keys in nine Stage1 roots**; separate compatibility tests only|No Stage1 asset or reward liability entrusted to facade|
|Aquarius and plane|Pool/plane code+instances; position/range/ticks, position index, fee/reward accumulators and relevant killed flags|Per-call exact footprint; investment/withdraw mutate position/token state; claim mutates reward records|Exact code/instance/data manifest; distinct shared-code and position restoration rent|
|Tokens / issuer / oracle|SAC code/instance where applicable; balance or classic trustline/account keys for source/vault/pool/recipient; oracle instance/feed/symbol records|Fund/swap/deposit/withdraw/payout; reward token only required reward paths; fresh oracle on price-dependent steps|Do not guess token order/decimals; local fixture issuer state is not live validation|
|Authorization and native bookkeeping|Source account, detached address account/nonce as applicable; TTL entries|Native envelope fee/sequence outside Soroban footprint; detached auth keys inside if accessed|Real private signatures; nonce expiry/replay; no blanket mock; account metadata not casually omitted from parity|

Existing tracked reward-history source uses bounded per-record checkpoints; this
wording does not claim it is deployed or production approved. Reuse it
only after gateG1/G2 establish semantics on this staged implementation. Do not declare
all history keys mandatory for every root: publish actual per-key RO/RW/restore access
and why it is touched. No helper contract provides a second budget in one transaction.

## 5. Keeper: execution, finalization and restoration

Implement a restartable command worker using existing off-chain infrastructure where
possible. It indexes command events and reconciles each candidate with the authoritative
on-chain job before action. Recover a lost index by off-chain bounded pages over
`0..Stage1Next` and job status, restoring the explicit job keys when needed; do not
assume RPC retains every historical event. This adds no per-owner on-chain index.
Classify token trustline/issuer refusal as owner-action-required and stop hot retries;
retain the command/claims, then re-simulate after that state changes. Do not create
per-token payout stages or change the all-or-nothing mixture rule in this build. Record `(network, receipt, id, phase, transaction hash,
source sequence, attempted ledger)` and separate known failure from unknown submission.
Never blindly resubmit an unknown result. No secret handling or production deployment
is part of this plan approval; private fixture accounts suffice for tests.

- **Investment:** schedule O3/O4/O5 under immutable owner bounds, fresh simulation and
  current pause/config/deadline. If price or common cash no longer permits investment,
  retry within deadline or expose expiry cancellation; do not loosen terms, charge
  incumbents or suppress required valuation/claim calls.
- **Finalization:** O5 atomically deposits, settles bounded reserve cost/refund, unlocks
  shares and records terminal state. C2a/C3 atomically pay and finalize; C2b records
  exact remaining mixture liability. No off-chain success flag substitutes for state.
  Repeated calls return a typed terminal-state error, not duplicate asset movement.
- **Restoration:** discover exact archived keys from current simulation/state, group
  bounded native RestoreFootprint batches (at most50 RW keys,100 keys,200,000 disk-read
  bytes,132,096 write bytes; all other existing limits apply). Include code/instances,
  not just position data. Determine actual rent/fee allowance, preserve source sequence,
  apply the batch, verify installed TTL and re-simulate. After all needed batches,
  re-simulate the original financial operation against current pool/command state.
- **Interruption/re-expiry:** persist completed batches and transaction outcomes; discover
  missing or re-expired keys again. Another keeper may continue. No borrower/owner
  signature is needed for generic restoration or a previously authorized permissionless
  command transition. Owner-required funding or voluntary waiver remains owner-required.
- **Capital accounting:** strategy execution reserve is token0, not the XLM transaction
  budget. No automatic debit of supplier principal, loss reserves or quarantined rewards
  to pay restoration. Keeper XLM operating balance, supplier withdrawal cash, seeded loss
  reserve and later funded-liquidator capital remain separate. Accepted cold totals are
  known expensive probes, not a production budget approval. Shared-code rent is paid once
  per restoration and must not be charged again per user in accounting.

Use operation-specific TTL maintenance, not whole-market sweeps. Native restoration
CPU/memory remain separate from Soroban meters. Native application, signatures, installed
TTL/fee tests and complete journey costs must stay visible in evidence.

## 6. Four remaining gates and exact closing tests

Tests named below are **planned**, not already passing. Build a dedicated Stage1 proof
map binding every required assertion and its resource rows. Keep the existing combined
checker and its46 missing legacy labels unchanged and visibly failing; do not make2× a
Stage1 prerequisite or silently turn aggregate exit1 into success. Stage1's separate gate
requires all nine roots, four gates, authorization/accounting invariants and measured
failure/recovery branches. No absent row may count as pass.

### G1 — Outage mixture recovery

Closing family: `compiled_stage1_outage_mixture_recovery` plus private Core replay of
its successful/failed payout branches. Assert recovery/extraction has no oracle or
swap dependency; cancellation without a burn has no Aquarius/oracle dependency. Test
missing, frozen and limit-capped trustlines for each payout token, including empty
reward-token trustlines: accrue personal reward claims, do not push AQUA during exits. Reuse the pinned actual Aquarius bytecode and
existing controlled-state claim/pending-view failure setup; distinguish injected faults
from natural venue behavior. No mock of protocol-owned authorization.

Exercise Funded/Converted refund, MintedPendingInvestment owner recovery, CloseLocked
owner recovery and ExitMixture payout with claim failure, pending failure, both failures,
stale oracle and disabled swaps. Prove fixed recipient, unencumbered ownership, retained
booked rewards, bounded history append, state/liability conservation and no normal-path
bypass. `waiver=false` with missing evidence must reject without financial changes;
owner `true` affects only the unprovable interval, with matching typed boundary consent.
Replay the consent in a later ledger/interval/revision and require rejection. Expiry executor cannot waive for an
absent owner; share release must remain possible. If actual venue withdrawal or token
issuer authorization prevents payout, keep recoverable claims and report failure honestly.

Required reward sequence: original co-owner later exits, newcomer joins, rewards arrive;
verified redistribution reaches only historical eligible owners. Test proportional
shortfall, attributable excess, repeated reconciliation and unavailable boundary evidence
remaining quarantined. Missing earlier reading is never inferred from a later cumulative
reading. All scaled conservation terms must reconcile, with paid, pending, original,
redistributed, quarantined and rounding amounts distinct.

Reuse the named pre-burn fee invariant, including one liquidity unit paying all fees
with zero principal. Force second-token payout failure after withdrawal: pool changes,
burn, command transition, allocation, reward boundary and liability updates must roll
back atomically. Private Core may still consume the valid envelope fee/sequence; compare
that separately. Gate closes only when both fund return and post-mint **actual mixture**
recovery are proven; unlocking shares alone does not close it.

### G2 — Stressed histories

Closing family: `compiled_stage1_stressed_histories`, parameterized from the existing
64-holder/128-change/8-outage fixture; compare256 holders/1024 changes/32 unresolved
intervals as a proposed test scale, not a production admission cap. The specific question
is whether critical exit/ownership steps touch bounded records as history grows.

Include mint/invest sandwich attempts at the oracle-band edge, repeated mint/cancel
carry cycles, dust after final exit before a new first mint, and per-category Reserved
equal to the sum of job liabilities. These are accounting/adversarial gates, not a
new economics study. Verify incumbent backing per share under the declared valuation
and bounded cost rules; disclose timing/value uncertainty beyond the model.

Include exited beneficiaries, repeated outage/reconciliation, old-owner waiver then
exit/new arrival, emission changes, relevant tick/range crossings, first mint/final exit,
concurrent funded/minted/close commands and interleaved incumbent withdrawals. Test
investment cash depletion safely cancelling rather than claiming cash twice. Verify
allowance hooks and direct transfers cannot spend locked balances. Archive relevant
historical beneficiary records and measure restoration/settlement cost without pruning.

Require exact scaled reward and principal conservation, old-owner/newcomer isolation,
per-call bounded history access and each complete financial/recovery root within unchanged
limits. Settlement can be incremental; recovery cannot enumerate all holders or reject
solely because unresolved history accumulated. A failed workload is a failed gate, not
permission to weaken limits or refuse existing exits. Preserve actual key inventories,
serialization sizes and retained records; do not hide growth in one packed value.

### G3 — Interrupted and re-expired preparation

Closing family: `private_core_stage1_interrupted_restore_resume_reexpire`. Extend the
accepted actual Core restoration fixture, not projected TTL edits. Apply only a prefix
of restore batches, persist state, stop/resume or switch keepers, and continue without
re-importing a reconstructed financial ledger. Include replay/unknown-result recovery.
If the fixture only pauses in one live process, label that narrower proof and retain a
failed restart requirement until persisted-Core restart is exercised.

Include the cold G1 waiver recovery with the longest G2 history, not only investment
and healthy withdrawal. Change Params/config revision between each stage pair.

Run an actual intervening Aquarius trade before re-simulation/execution, plus deadline,
fresh-risk pause and configuration changes. Ensure later execution observes new prices
or takes permitted cancellation/recovery; no stale command executes new risk. Advance
real ledger time through TTL expiry and supported Core eviction, then restore again;
assert authentic archived contents, installed TTLs, source balance/sequence, rent and
fees. Do not rewrite TTLs to manufacture a pass. Restore without owner participation;
never use absence as permission to waive rewards. Count every preparation/retry/financial
transaction and distinguish shared-code costs from per-command data.

### G4 — Core accounting parity

Closing family: `private_core_stage1_financial_parity`, continuous seven-transaction
in-kind and eight-transaction converted lifecycles plus G1/G3 recovery branches. Use
identical per-step prestates for enforcing host/SDK comparison and the persisted private
Core chain, including real source-envelope and detached G-address authorization forms.

Compare decoded financial state: token/SAC balances and classic trustlines, share supply
and owners, LP liquidity/composition, receipt phase/terms, locks, reserves, donations,
exit allocations, reward indices/debts/pending/fractions, historical weights, quarantine,
returns and ordered contract events. Compare TTLs exactly or document narrowly intended
maintenance differences. Native fee debits, source sequence, last-modified ledger metadata
and Core system events have explicit separate comparisons; never omit entire accounts,
all events or all reward records to make hashes match. Require exact fee/rent reconciliation.

Negative cases: mismatched binding, rebind and upgrade attempts while staged mode is
enabled, direct facade/legacy bypass, invalid envelope signature, wrong amount/recipient, missing nested auth,
expired or reused detached nonce, duplicate command execution and custom-account reentry.
Invalid envelope validation must not consume fee/sequence; a valid included transaction
with host failure may consume them while financial state rolls back. Mock authorization
cannot establish these Core signature, nonce, fee or application properties. No public-
network signatures or real keys are needed or authorized.

## 7. Order of work, ownership and estimate

Estimate assumes one experienced Soroban engineer-day is roughly one focused working
day, existing pinned tools remain available, and accepted code/evidence is reused.
It includes implementation and local tests, not waiting for operator decisions, external
audit, deployment, production capacity/economics approval or a new architecture redesign.
Subagents can prepare bounded work; the lead integrates and reviews every change.

|Order / work package|Scope and dependency|Engineer-days|Subagent suitability|
|---|---|---:|---|
|0 approval|Operator approves this document; no build before approval|Not counted|Lead only|
|1 freeze ABI and invariants|Versioned command/types/errors, key namespaces, auth graph, exact fixture pins and Stage1 proof map|3–4|Read-only ABI/key reviewer; lead decides interface|
|2 receipt module|Command transitions, binding, expiry/cancel, lock/auth integration, compatibility tests; depends on1|4–6|One agent owns receipt files/tests after ABI freeze|
|3 vault and G1|Cash/liability accounting, range/decimal math, first/final holder, lock hooks, fee-before-burn and outage mixture recovery; depends on1|6–9|One agent owns vault; separate reviewer supplies adversarial test cases|
|4 facade compatibility|Unavailable atomic route, no extra hot-path hop, dust/auth/lock bypass regression; depends on1, integrates2/3|1–2|Bounded independent agent; separate files|
|5 keeper|Discovery/reconciliation, investment/finalization worker, fee budgeting, restore batching and durable retries; starts after1, integrates2/3|4–6|Keeper agent, separate directory; no production credentials|
|6 G2 history stress|Targeted scaling/concurrency/allowance/archived-history tests on combined implementation; depends on3|3–5|Test agent; lead reviews accounting/conservation|
|7 G3 Core restoration|Interrupted persisted state, keeper change, actual re-expiry/eviction and trade; depends on5 and pinned2/3|3–5|Core-fixture agent; do not share mutable Core data directories|
|8 G4 Core parity|Continuous journeys, exact state/events/TTL differential and signed negative cases; depends on2/3, shares frozen fixtures with7|4–6|Parity agent after fixture interface freeze; coordinate Core files|
|9 integration and CI|Combined proof map, every gate/error check, ABI/storage review, durable artifacts, no false release green|3–5|Lead integrates; independent review agent and CI agent on owned files|
|**Total engineering**|No double-counting G1–G4 outside their packages|**31–48**|Parallelism reduces elapsed time, not total effort|

Allow a separate **6–10 engineer-day contingency** for ordinary test/fixture and integration
failures: planning envelope37–58 engineer-days. With a lead plus two effective parallel
contributors, expect roughly **4–6 working weeks**, constrained by ABI freeze, combined
vault recovery, then Core parity—not31 days divided mechanically by agent count. A
fundamental recovery/accounting redesign or unsupported Core eviction path triggers a
new estimate and an explicit failed gate, not silent stage or policy changes.

Milestones after approval:

1. ABI/key inventory frozen; receipt and vault owners can work in parallel. No signature
   or financial-stage change inferred from a type rename.
2. Combined normal flow plus G1 passes before calling mixture recovery implemented;
   report per-root resources and named conservation assertions.
3. G2 and keeper integration, then G3/G4 in isolated Core workspaces. Frozen bytecode,
   toolchain, profiles and sources bind every run. Re-measure only changed/affected
   roots plus the complete required lifecycle gate; preserve accepted baseline evidence.
4. Dedicated CI executes actual Stage1 tests/proof map, uploads artifacts even on failure,
   replays each profile's exact gates/errors and records Linux WASM hashes separately.
   Existing baseline CI does not cover this prototype. Deployment readiness and any
   future borrowing activation remain separate approvals.

No additional study, range optimization, infrastructure subscription, helper contract
or financial-stage split is authorized or started by this plan.

Re-audit surface: receipt command authorization/state/upgrade-freeze additions; vault
liability accounting, extraction/recovery, lock enforcement, new schema and price/range
arithmetic; facade rejection compatibility. Preserve unrelated cash-lending behavior
and its regression tests. No SimplePeridottroller, V3 debt or production reserve-parameter
change is needed for this1× build plan. Mutation/adversarial checks must target omitted
phase/auth/liability checks, waived-owner identity and fee allocation before burn.
External audit effort and production rollout are outside the engineer-day estimate.

Accepted7f66fb05 CI has now completed: baseline42835303812 checkerPASS,
checkpointFAIL. Historical storage/restoration/closed-Option1 jobs alsoFAIL. Those
workflows do not run the accepted signed prototype; this planning checkpoint makes no
new CI qualification claim and runs no financial tests.

## 8. Fable review and lead decisions

Claude Fable resolved to `claude-fable-5-2`, requested effort **high**, one completed
review with tools disabled. Payload was only the prepared ABI/key-inventory excerpt;
no repository, secret, private signed artifact or external source was supplied. Raw
response, exact payload and metadata are preserved in `peridot-contracts/research/cl-stage1-build-plan-20261111/`.
It did not independently verify code or accepted measurements. The sandbox attempt
reported no login; the approved bounded-payload retry completed. This is a new completed
review, separate from earlier quota failures.

|Review issue|Lead disposition in this revised plan|
|---|---|
|B1 new recovery roots and delta costs not measured|Agree; G1/G3/G4 explicitly measure them; add static change inventory and stops below. Do not reopen the accepted feasibility decision or fabricate numeric CPU bounds for unbuilt code.|
|B2 aggregate-only vault trust, mutable receipt code|Agree; add per-id Stage1Liability, category totals, set-once mutual binding and staged-mode code-upgrade freeze. Measure the extra key after build.|
|B3 legacy/emergency routes and cash identity|Agree; common extraction kernel, complete reader/writer compatibility audit, explicit accounting identity and shortfall handling below.|
|B4 cash-first temporary idle participation|Retain operator-approved cash-first ownership. Explicitly disclose carry, reject knowingly unfundable admission, bound deadlines and test cycling/sandwiches. Do not adopt investment-first weights or deposit-first mint. No claim of zero carry is made.|
|B5 delayed waiver scope|Agree; retain handoff00 boolean but require typed owner consent to job/holder/shares/interval/history revision/boundary/execution ledger. Re-sign if it changes.|
|Facade can be initialized by attacker if left uninitialized|Agree; no Stage1 facade deployment. Enforce legacy rejection at staged vault; test zero facade keys on financial roots.|
|Missing caller matrix, typed results/errors/events, TTL ownership|Agree; proposed ABI tables below make them reviewable before code.|
|Pack legacy receipt keys to save access|Not adopted. Keep Initialized/UnderlyingToken validation and accepted layout; no speculative storage migration or new tuning study.|
|Unknown asset-loss priority|Do not invent a loss waterfall or guarantee. Preserve recorded claims and block new risk on backing shortfall; explicitly retain this economic/issuer limit. No automatic confiscation of quarantine or earlier payout priority.|

Fable's original verdict was “not approve as written,” based on the initial excerpt.
The lead incorporated the changes above; **Fable has not re-approved this revised plan**.
The operator's plan approval, not the reviewer's verdict, is the build authorization.

## 9. Concrete ABI adjuncts from review

### Caller/state matrix and results

`P` means permissionless executor acting only on stored terms; `O` requires the recorded
owner; `E` means anyone only strictly after the stored deadline. At exactly the deadline,
forward execution is permitted and permissionless expiry is not: forward `now <= deadline`,
expiry `now > deadline`. Owner cancellation remains available. Fresh-risk revision checks
apply to convert/mint/invest, not in-kind close admission, release or fixed payouts.

|Entrypoint|Required state|Caller|Result / next state|
|---|---|---|---|
|prepare_open|No command|O|`CommandCreatedV1{id, terms_hash}` / Prepared|
|fund_open|Prepared|O, exact nested auth covers id/hash/revision|`FundedV1{actual0,reserve0}` / Funded|
|convert_open|Funded|P|`ConvertedV1{amount0,amount1}` / Converted|
|mint_cash|Converted|P|`MintedV1{shares,locked}` / MintedPendingInvestment|
|invest_cash|MintedPendingInvestment|P|`InvestedV1{liquidity,cost0,refund0,unlocked}` / Invested|
|prepare_close|Available direct shares; no job|O; InKind ignores new-risk pause/cap/revision|`CommandCreatedV1` / CloseLocked|
|withdraw_close|CloseLocked|P|`ExitV1{amount0,amount1,paid}` / Paid or ExitMixture|
|convert_close|ExitMixture|P|`PaidV1{amount0,amount1:0}` / Paid|
|cancel|Prepared/Funded/Converted|O or E|`ReleasedV1{cash0,cash1,shares:0}` / Cancelled|
|cancel|MintedPendingInvestment/CloseLocked|O or E|`ReleasedV1{reserve0,shares}` (no burn) / Cancelled|
|cancel|ExitMixture|O or E|`PaidV1{actual0,actual1}` / Cancelled|
|recover_in_kind|MintedPendingInvestment/CloseLocked|O; typed consent required for true waiver|`ExitV1{actual0,actual1,paid:true}` / Paid|
|holder emergency redeem|Unlocked, direct, unencumbered shares|O; same typed waiver consent|Same extraction/reward rules, no command replay|
|any mutator of terminal job|Invested/Paid/Cancelled|Any|`Stage1AlreadyTerminal`; no asset change|

Plan enum error IDs reserve the following proposed block; work package1 checks collision
with all existing contract errors before freezing. Existing reward801/802/803 retain
meaning, with structured failure reasons in events/status where safe.

|Proposed ID / error|Keeper handling|
|---|---|
|1700 Stage1UnknownCommand;1701 Stage1BadPhase;1702 Stage1AlreadyTerminal|Reconcile on-chain record; never assume unknown submission failed|
|1703 Stage1Expired;1704 Stage1RevisionChanged|Stop forward work; offer permitted release/recovery|
|1705 Stage1BindingMismatch;1706 Stage1TermsMismatch;1707 AsyncStageRequired|Configuration/authorization defect; no retry or fallback route|
|1708 Stage1InsufficientExecutionReserve;1709 Stage1CashUnavailable|Wait within bounds or cancel; never charge incumbents/top-up silently|
|1710 Stage1MinimumNotMet;1711 Stage1FreshPriceUnavailable|Re-simulate within owner terms; expiry release remains available|
|1712 Stage1ConsentMismatch;1713 Stage1SharesLocked|Require correct owner action; no recovery-auth bypass|
|1714 Stage1BackingShortfall;1715 Stage1UpgradeFrozen|Preserve claims, stop new risk; operator-visible issue|
|Existing RewardWaiverRequired802 / ClaimUnavailable801|Only authorized emergency choice may change path; never auto-waive|
|Token/venue host rejection|Keep typed upstream cause; owner-action-required for trustline/issuer restriction; no manufactured success|

`Stage1TransitionV1` event carries schema, id, owner, old/new phase, terms hash, config
revision, actual token amounts, locked/burned shares and charged/refunded strategy cost.
Keep it bounded. Emit allocation event before burn event; emit terminal status only in
successful transaction. Store equivalent authoritative results in the job for discovery
without event retention. No false success event from a handled failed subcall.

### Cash identity, compatibility and growth

For each pool token, at a quiescent successful boundary:

`raw balance = excluded donations + Book(common idle) + Unminted + ExecutionReserve + ExitClaims`.

`Reserved = Unminted + ExecutionReserve + ExitClaims`; each category equals the sum of
live `Stage1Liability(id)` amounts. Tests may enumerate all fixture jobs to verify this
identity; financial contracts must not enumerate them. LP assets and reward-token
liabilities are separate terms, never counted as the same idle cash. Temporary internal
flows must reconcile before transaction completion.

Audit every caller of balance/NAV/extraction and every asset-moving entrypoint in
`cl-share-vault/src/contract.rs`, accounting, custody, allowances and reward modules:
legacy deposit/redeem/quote disabled for staged mode; normal mint/invest/close updated;
transfer/transfer_from/burn/burn_from check lock; emergency redemption shares the safe
kernel; harvest/recenter/pooled reward swap and sweep/rescue cannot consume command
liabilities or reward quarantine. Unsupported active recenter/pooled conversion stays
disabled. Expose no generic admin recovery that spends another owner's reserve.

On an issuer clawback or unexplained backing shortfall, do not silently reduce claims,
spend quarantined rewards or promise full coverage: reject new risk and preserve the
accounting/evidence for a separately approved fair loss-recognition decision. Existing
liabilities remain claims, not guaranteed immediately redeemable tokens. Missing/frozen
trustlines likewise require owner/issuer action; they are not solved by new transaction
stages. This limit is distinct from an upstream reward outage, which G1 must handle.

### Static proposed footprint changes and stop conditions

|Root family|Proposed physical-key change from accepted shape|Unmeasured cost / admission rule|
|---|---|---|
|Prepare open|No new per-id vault record until funding; version/config fields remain instance/job contents|Serialized job/instance/event bytes and validation CPU must be measured|
|Fund, convert, mint, invest|One persistent Stage1Liability(id) RO/RW record|At most one added unique/RW key; up to3 additional SDK entries if read/disk-read/write all count; restoration cost separate|
|Prepare close, withdraw, convert exit|One persistent Stage1Liability(id) record, existing allocation remains separate|Same structural delta; no duplicated allocations or extra facade instance|
|Cancel/share release/payout|Job+liability+owner lock/reserve/token dependencies, no claim/burn when just releasing|No accepted complete cancellation bound; measure every phase; assert no oracle/Aquarius keys|
|Emergency mixture recovery|Normal extraction dependencies plus bounded reward boundary/history/waiver records and liability guard|Actual additional key set differs by outage/waiver branch; G1/G2 must enumerate and measure it, not assume it fits the78-entry normal withdrawal|
|Facade compatibility|Zero extra keys on nine financial roots|Assert by exact footprint diff|

This is a structural key-delta bound, **not a numeric CPU/memory bound** for unbuilt code.
A new RW key can consume multiple SDK entry units. Instance totals stay constant-sized,
while job/waiver records are bounded and histories indexed. Stop if a completed root
exceeds any unchanged limit or if a critical path scans a history/registry. Report the
exact cause before suggesting a stage/architecture change; do not increase a limit or
claim the accepted feasibility table already covers these changes.

### TTL, evidence authority and operating responsibility

No new production TTL values are set. Retain existing500,000-ledger threshold /
1,000,000 extension and the existing share-specific1,000,000 /5,000,000 only as inherited
code/test values pending validated configuration; cap extensions to network maximum.
Captured minimum-persistent-TTL restoration is separately enforced by Core. Decisions
below assign who maintains and pays, not an always-on-keeper assumption.

|Keys|Maintenance owner and trigger|Payer / recovery rule|
|---|---|---|
|Shared code/instances/config|Keeper near-expiry inventory; financial root only touches required keys|Dedicated XLM operating balance; actual restoration when missed|
|Live receipt job + vault liability + owner lock|Submitting actor/keeper renews only touched records near threshold; indexed job sweep off-chain|Keeper budget or explicit transaction payer; missing archived job is not zero liability|
|Shares and personal pending rewards|Existing holder access and indexed maintenance; holder may self-restore|Dedicated operating policy/holder fee; zero shares never erases exited rewards|
|Boundary/interval/weight/waiver/settlement records|Evidence index tracks references; bounded settlement renews accessed records only|Archival funding separately budgeted; no unresolved pruning or full-history sweep inside exit|
|Terminal commands/allocation audit|Retain first build, archive normally; query restoration explicit|Do not renew every historical command on every transaction|

Governance attribution keeps existing admin authorization, evidence-hash commitment and
86,400-second delay; release at most the specific quarantined/remaining interval amount,
with idempotent settlement and auditable original versus redistribution buckets. Published
boundary evidence includes ledger/time, exact venue/WASM identity, raw readings, ownership
revision and conservation reconciliation. Governance assertion is explicitly marked as
such. If authentic boundary evidence never exists, amounts remain quarantined indefinitely;
forced future exits cannot depend on attribution or waive the beneficiary's entitlement.

Namespace test: serialize every key variant with representative payloads, grouped by
contract and durability, assert distinct encodings and snapshot intended stable keys.
Same variant names across different contracts are not collisions; same-contract modules
must be checked. Existing History-prefixed variants are not silently renamed or packed.
