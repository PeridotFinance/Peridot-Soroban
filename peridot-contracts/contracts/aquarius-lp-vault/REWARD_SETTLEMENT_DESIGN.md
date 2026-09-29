# Hybrid reward settlement proposal

Updated: 2026-09-17. Status: lean all-stream/recycled-yield coordinator, persistent
ownership ledger, real-token backing and Aquarius bridge integrated in native tests. Production hooks are
NOT implemented, audited, or deployed. This is not an upgrade candidate.

## Intended behavior

Retain threshold-gated automatic compounding. On withdrawal, also settle the
withdrawing holder's eligible, not-yet-compounded reward portion into the market's
settlement asset: XLM, PYUSD, or USDC respectively. This does not introduce arbitrary
output-token selection or merge the two stablecoin settlement strategies.

Only that holder's eligible reward portion should be swapped for the withdrawal;
do not call a whole-vault harvest and describe it as user-only settlement.
Already-compounded rewards are included in share backing and must not be paid again.
NAV/cache maintenance and concentrated-position rebalancing remain independent.

If a reward route is blocked or the amount is dust, the intended fallback is to
retain an attributable, later-claimable reward balance rather than confiscate it.
Principal exit should not require a successful reward swap. This does not bypass
existing principal redemption, oracle, liquidity, or minimum-output checks.
The implementation must prove that fallback preserves accounting atomically;
silently skipping a failed ownership checkpoint is not a safe fallback.

## Integration points confirmed in the current source

- ReceiptVault owns the user pToken balances. The bound Aquarius strategy sees
  ReceiptVault as its holder, not the individual withdrawing user.
- ReceiptVault `withdraw` may use idle cash without calling the strategy. Reward
  settlement must therefore begin at the receipt layer even for an idle-only exit.
- ReceiptVault `deposit` prices shares before receiving the new deposit. A reward
  ownership checkpoint must respect this boundary and exclude incoming capital
  from the historical accrual period.
- Both `transfer` and `transfer_from` pass through `transfer_internal`. There are
  also direct pToken moves in `seize`, including the fee recipient. Every balance
  mutation needs an explicit ownership rule; adding hooks only to deposit and
  withdrawal is insufficient.
- Existing `accrue_user_rewards` calls concern controller incentives, not AQUA.
  Keep those ledgers and behavior separate.
- Strategy `harvest` currently claims and converts pooled rewards, then deploys
  available settlement cash. There is no per-user reward liability ledger.
- Unconverted AQUA is excluded from current strategy NAV. A late depositor must
  not acquire historical AQUA merely by buying shares before the next harvest.
- Final-holder exit burns all pTokens and unwinds the strategy. Outstanding reward
  claims must remain valid after supply reaches zero and after later deposits.

## Accounting requirements before contract implementation

1. Define separate ownership for invested backing, unconverted rewards, reserved
   exit claims, and converted proceeds. Each token unit can back at most one claim.
   Reserved amounts must be excluded from deployment, ordinary NAV, and future
   harvest consumption unless the corresponding liability is settled atomically.
2. Specify a bounded checkpoint/index mechanism, including how rewards still
   unclaimed in Aquarius are measured at balance changes. Indexing only the tokens
   already held by the strategy does not prevent late-depositor capture.
3. Specify transfer semantics explicitly. Recommended: crystallized historical
   reward claims remain with the earning account; transferred pTokens participate
   in future accrual. Reconcile this with pooled compounding before adopting it.
4. Prove how an account's unconverted entitlement becomes compounded backing
   without being redistributed to later holders or counted twice. An ordinary
   reward-per-share index alone does not establish this. Specify normalization or
   another bounded mechanism with worked multi-user examples before coding.
5. Define partial-withdrawal allocation, repeated-withdrawal rounding, self-transfers,
   zero-supply epochs, donation handling, and deterministic remainder ownership.
   Use checked arithmetic and demonstrate conservation across all transitions.
6. Retain old token claims when the primary reward token, route, or gauge changes.
   Bound active reward-token work per call; governance must not erase liabilities.
7. Authenticate receipt/strategy settlement calls and user claims. Apply existing
   route floors and slippage protections, a user-specified settlement minimum where
   appropriate, and actual input/output balance-delta verification.
8. Make the feature explicitly opt-in for these LP receipt markets. Existing
   lending/DeFindex markets, account-health snapshots, and MarginController must
   not acquire new reward-contract dependencies or storage-footprint requirements.

## Executable allocation prototype — September 15

`contracts/receipt-vault/src/reward_accounting_model.rs` is included only under
`cfg(test)`. It changes no production storage, entrypoint, share balance or NAV.
It models one reward stream, deliberately without claiming to implement the real
pool, swap adapter, custody, authentication, or compounding-share pricing.

The candidate allocation mechanism closes an epoch whenever the available raw
rewards are fully converted into separately backed compounded-reward claim units.
Reserved withdrawal rewards are excluded from that conversion. A new deposit or
transfer first recognizes all simulated unclaimed rewards at the old balances.
Historical claims stay with the earning account. Partial withdrawals reserve only
their proportional whole raw-token amount; fractions remain with the same account,
including after zero balance. A failed reward swap leaves the reserve intact.

The model keeps a scaled raw-reward-per-weight index `I` and a cumulative
closed-epoch claim-unit-per-weight index `J`. Each closed epoch records its final
`I`, measured unit conversion rate `k`, and final `J`. An account stores its epoch,
index checkpoint, balance weight, crystallized raw claims and converted claims.
For an account that last checkpointed in closed epoch `e`, catch-up credits:

```text
first-epoch units = (saved raw claim + weight * (I_end[e] - I_saved)) * k[e]
later-epoch units = weight * (J_current - J_end[e])
current raw claim = weight * I_current
```

The implementation applies explicit fixed-point scaling and floors conversions.
Catch-up needs one historical epoch lookup, not an iteration over every missed
harvest. The index updates do not enumerate holders; test-only rollback cloning
and the invariant checker do visit model accounts. Global raw inventory, withdrawal reserves and
compounded claim-unit inventory are separate. Unallocated rounding dust remains
backing, never reassigned to the next depositor or silently swept to governance.
This prototype rejects intermediate arithmetic overflow rather than wrapping.

Sixteen tests cover exact four-holder ownership across two epochs, late deposits,
transfers, no double payment after compounding, blocked partial and final exits,
zero supply/re-entry, fractions, repeated withdrawals, rollback, overflow, missing
history, and independent old/new reward-stream claims. A dormant holder catches
up after 1,000 epochs without needing the middle epoch records. A deterministic
state-machine test checks backing conservation after 8,000 mixed operations.
These are executable examples and regressions, not a formal proof or full fuzzing.

## Real-token backing library — September 15

`receipt-vault/src/reward_backing.rs` now implements actual underlying custody,
receipt-share backing, reinvestment and redemption. It is compiled only for tests
or the explicit `hybrid-rewards` development feature. It adds NO callable contract
entrypoints and no production code calls it. `reward_backing_test.rs` supplies a
test-only contract interface and uses the real ReceiptVault methods, real SAC
transfers, and a mock boosted strategy holding real SAC funds. It does not yet use
an actual Aquarius reward claim/swap or connect to the allocation model.

The backing is pTokens owned by ReceiptVault itself, included exactly once in the
ordinary pToken supply. Separate reward units are claims on those escrowed pTokens,
not extra claims added on top of ordinary NAV. Newly received settlement assets
are measured by actual balance delta and priced at the pre-receipt NAV. Managed
cash and tracked deposits increase by that measured receipt, real escrow pTokens
are minted, and the existing idle-buffer strategy deployment helper reinvests the
managed excess. Untracked donations are not swept into this deployment.

If conversion delivers `R` settlement units, of which `Y` belongs to existing
reward backing, let `M` be newly minted pTokens, `P` old escrow pTokens and `U`
old reward units. New-claim pTokens are `N = floor(M * (R-Y) / R)`. The remaining
`M-N` pTokens belong to old units. New units are then
`floor(N * U / (P + M-N))`, or `N` for initially empty backing. Thus old backing
gets its yield before new claims enter. Example: 100 old units/100 pTokens, 100
recycled settlement plus 100 new settlement at par produces 200 old-backed
pTokens and 100 new-backed pTokens, issuing only 50 new units. The old holder
receives 200; the new holder receives 100. Integer dust favors existing backing.

Only already-funded units can be allocated. Redemption moves the attributable
escrow pTokens internally to the owner and calls the existing Rust withdrawal
path, not an external callback into ReceiptVault. Existing owner authorization,
principal safety checks and strategy unwind still apply; an additional actual
underlying-delta minimum rolls back the entire call on failure. Owner authorization
is performed once by the reused withdrawal, not twice in the same Soroban frame.

Reward share pricing requires a successful positive live strategy quote rather
than ReceiptVault's fail-soft accounting fallback. Funding rejects a changed NAV
between snapshot and receipt; a real swap that changes invested strategy value
therefore needs explicit reconciliation. These stricter checks are confined to the
development reward path. Existing principal withdrawal behavior is unchanged.

The pending-reward gate prevents funding/redemption while old backing has rewards
to settle. Its count and the `recycled` amount are currently trusted internal
coordinator inputs, NOT permissionless production controls or verified pool data.
Test-only allocation/pending setters require admin authorization. Production must
derive both from all streams, and must defer a blocked reward leg without trapping
principal; merely exposing these test hooks is not an acceptable integration.

Seventeen integration tests cover real funding, reinvestment and strategy yield,
increased-NAV pricing, recycled rewards, rounding, last-claim payout, principal
exit/re-entry, authorization failure and exact successful auth trees, over-allocation,
double redemption, donations, supply caps, short transfers, pending gates, stale
quote failure, changed NAV during funding, and rollback after minimum-output failure.
Targeted suites pass: ReceiptVault 170 passed/1 intentional WASM ignore; Aquarius
68 passed/2 intentional WASM ignores. Native feature compilation and formatting
also pass. No candidate WASM, exact-WASM migration, security scan or release yet.

## Aquarius claim/conversion integration — September 16

The native-only `aquarius-lp-vault/src/reward_bridge.rs` now provides
receipt-authorized `hybrid_claim` and `hybrid_swap` operations. Their entrypoints
exist only under tests or the explicit `hybrid-rewards` feature. A compile-time
guard REJECTS that feature on WASM: it must not be deployed before complete
ownership hooks, activation and legacy-harvest exclusion are implemented.

`hybrid_claim` discovers the configured primary token and pool gauge tokens,
bounded to four distinct non-pair tokens. It strictly claims both streams, checks
reported amounts against actual token deltas, and transfers ONLY newly claimed
amounts to the bound receipt. Pre-existing strategy inventory is not swept or
reassigned; pool fees are not included. Primary and gauge rewards using the same
token are measured separately and combined exactly once. A failed gauge claim
rolls back an earlier successful primary claim. Pair-token emissions and missing
primary configuration are currently unsupported and fail closed before attribution.

`hybrid_swap` pulls only the requested raw amount from the bound receipt, reuses
the configured route/floor/slippage protections, verifies exact input consumption
and actual output, then returns only the newly produced settlement asset. The
input-delta check is essential: the legacy whole-balance harvest helper's replay
protection does not hold when leftover reward inventory exists. Price guards,
killed routes, replayed transfers and underpayment roll back the entire strategy
subcall, including the initial receipt-to-strategy transfer. The native receipt
test harness catches that failed subcall and retains the raw reward balance.

The real-token integration composes ReceiptVault, AquariusLpVault, SAC transfers,
a mock concentrated pool and mock reward route. It now exercises pool claim →
conversion → real escrow-share backing → separate reinvestment → redemption.
The initial bridge-only methods use ADMIN TEST STAND-INS for allocation. The
persistent-ledger methods below now derive ownership instead; neither interface
is a production entrypoint. Bridge rotation tests establish preservation of token
inventory, not yet a production multi-token coordinator.

The original combined conversion/backing/reinvestment call used 104 footprint
entries and was rejected by the SDK's 100-entry transaction limit. New
`reward_backing::finish_idle` credits managed cash and mints real backing in the
conversion transaction, leaving that cash immediately claim-backed while idle.
Reinvestment is a separate existing receipt rebalance transaction. In the tested
fixture, conversion/backing uses 77 entries and 9,528,921 instructions; reinvestment
uses 77 entries and 6,409,359 instructions. Resource limits stay enabled. These are
native mock-pool measurements WITHOUT the ownership/controller hooks, not proof
of deployed-WASM fit. Do not combine the calls again without new budget evidence.

Twelve new tests cover actual claims, primary/gauge deduplication, preserved legacy
inventory, exact partial conversion, contract auth with no blanket mocks, replay
attempts, underpaying routes, route/price/user-minimum failures, partial-claim
rollback, wrong primary configuration, token rotation and backed reinvestment.
The fixture uses an idempotent parity range quote: the mock's default reserve-ratio
requote can round twice and mismatch exact spend authorization. This is another
reason actual pool-WASM and deployed-state tests remain mandatory.

Targeted regressions: ReceiptVault170 passed/1 intentional WASM ignore;
Aquarius80 passed/2 intentional WASM ignores. Native development-feature compilation,
new-file formatting and diff checks pass. No new candidate WASM, security scan,
commit/push, deployment, keeper change or Mainnet transaction was performed.

## Persistent per-holder ownership integration — September 16

`receipt-vault/src/reward_ledger.rs` implements the epoch/index allocation mechanism
in Soroban persistent storage. Each token has a stream record, separate holder
records and closed-epoch records, with distinct key names and TTL extension on
access. Up to four token streams can be registered; there is no remove/reset
operation. A required missing stream or epoch fails closed. Zero-weight account
records are retained because they may still own fractions or reserved exit rewards.
The receipt's escrow address is not an ordinary reward holder.

Claim recognition uses a consumed pre-call snapshot and an exact token balance
delta, so old donations cannot become newly earned rewards. Epoch closure verifies
both exact consumption of the stream's unreserved raw balance and the increase in
real backing units/unallocated units. Allocations are calculated from the holder's
index history and transferred into `reward_backing`'s owner record; arbitrary
caller-supplied allocation amounts are no longer needed by this path.

The new native receipt coordinator reads actual pToken balances, claims before
balance changes, and integrates deposit, direct transfer/self-transfer, partial
and final withdrawal, compounding, lazy allocation/redemption and later settlement
of reserved raw rewards. Transfers retain past rewards with the earner. Partial
withdrawals reserve only the proportional whole raw-token entitlement; fractions
stay with the same owner. Failed conversion leaves that reserve intact, including
after all principal/pTokens are withdrawn. A reserved payout requires owner auth,
checks actual settlement received by that owner, then debits the reservation.

This coordinator is intentionally SINGLE-STREAM and rejects new pool emissions
while compounded escrow backing exists: the all-stream recycled-yield coordinator
is still missing. That fail-closed guard can block a combined checkpoint/exit, so
it is NOT the promised production liveness solution. Existing legacy test methods
are also still callable and must not coexist with an activated production mode.
No real ReceiptVault entrypoint has been wired to the new ledger yet.

Thirteen new tests cover late-depositor exclusion, transfer/self-transfer ownership,
partial and final exit reservations, blocked-swap retry, exact payout authorization,
failed unauthorized allocation/redemption, donations, separate-token history,
20-epoch lazy catch-up without intermediate records, missing required history,
account/stream/history TTL renewal, and the recycled-yield guard. The many-epoch and
separate-stream tests use a measured-token adapter to isolate ledger behavior; they
do not pretend to implement recycled Aquarius emissions.

IMPORTANT NATIVE RESOURCE BLOCKER: the combined strategy-funded deposit with recognition
and ownership writes fails under the SDK's default 100-entry test limit at 106 entries.
This is not a current Mainnet-limit measurement; limits are network configuration.
Strategy-funded withdrawal combinations also failed default-limit tests. Functional
deposit/exit tests therefore use a 100%-idle receipt fixture with normal transaction
limits, while transfer tests still use the strategy fixture. One separately named
diagnostic test DISABLES limits only to measure/assert the known >100-entry deposit
blocker; it is not evidence of an executable network transaction. Do not cite the
passing test count as resolving this limit. Footprint reduction/resumable design
and actual deployed-pool WASM measurements are required before activation.

Targeted suites: ReceiptVault170 passed/1 intentional WASM ignore; Aquarius93
passed/2 intentional WASM ignores (including the explicit budget diagnostic).
Native development-feature compilation, new-file formatting and diff checks pass;
clippy completes with legacy ReceiptVault warnings and the existing
`option_env_unwrap` allowance. Both Aquarius AND ReceiptVault now reject explicit
`hybrid-rewards` WASM builds. No new scan, commit, push or Mainnet/keeper mutation.

## Separate LP receipt core — September 16

At the user's request, `contracts/lp-receipt-vault/src/contract.rs` is a separate
supply-only engine for the XLM/PYUSD/USDC settlement markets. It does not modify or
replace the generic lending/DeFindex ReceiptVault. The same lean implementation
can serve each of the three separately configured receipt instances; the two
stablecoin settlements still have different strategies on their shared pool.

The distinction is runtime work, not file length: this engine omits borrowing,
repayment, interest accrual/JRM, flash loans, collateral checks, liquidation,
margin locks and controller incentive calls. It renews only LP accounting keys,
not the generic receipt's borrowing/interest state. LP receipts in this design
are NOT collateral or borrowing markets; future lending would require a separate
design/review, not a toggle. Legacy controller incentives/liabilities must be
reconciled before migration, not silently discarded.

Retained primitives include owner/admin authorization, one-time initialization,
supply caps, deposit pause with unpaused withdrawal, exact actual cash accounting,
donation exclusion, checked/U256 share math, one permanently attached single-asset
strategy, full final-holder strategy redemption, and escrow transfer protection.
Mint/burn pricing uses exact NAV ratios without a truncated intermediate exchange
rate. Pool/range/oracle/slippage/reward-floor protection remains in Aquarius.
The lean engine intentionally requires a successful positive strategy quote; it
does NOT yet port the generic receipt's stale-quote exit recovery. Principal-exit
liveness during outages remains an explicit release requirement.

The native package reuses the existing key type, token metadata precision and
events, and includes the SAME development `reward_backing.rs` and `reward_ledger.rs`
sources via path modules. It does not fork their ownership math. Those shared
modules resolve to the lean core in this package. A release should extract the
shared schema/accounting cleanly instead of carrying native-only dependency flags.
The original package's runtime implementation is not called by the lean engine.

There is deliberately no public contract ABI or upgrade/migration entrypoint.
An LP activation marker prevents generic legacy storage from silently entering
this mode. The package rejects WASM compilation; the Aquarius
`lp-receipt-prototype` test feature also rejects WASM. The production build script's
explicit package list remains unchanged and excludes this prototype. Native
harness-only bypass methods must never become the production interface.

Verification commands:

```sh
cargo test -p lp-receipt-vault --locked
cargo test -p aquarius-lp-vault --locked --features lp-receipt-prototype reward_bridge_test
```

Nine lean-core tests cover authorization/admin controls, caps, pause/exit,
donations, exact share rounding, omitted lending state, escrow restrictions and
legacy activation rejection. All28 lean reward integration tests pass, including
both settlement indices, compounded backing reinvestment/redemption, exact owner
auth without blanket mocks, failed pool exit rollback and ownership reservations.

In the native real-SAC/Aquarius + mock-pool fixture, the strategy-funded ownership
deposit drops from106 entries to75 (10,947,080 instructions); the non-final and
final holder exits each use66 (8,581,135 / 8,453,396 instructions). Conversion/backing
uses53 entries and separate reinvestment52. All these lean paths run with SDK
transaction limits enabled; only cumulative native-host accounting is reset.
This resolves the measured fixture overrun, NOT a deployed-pool/WASM budget proof.
The original generic receipt's separately labeled over-limit diagnostic is kept.

This split does NOT finish the all-stream/recycled-backing coordinator, failed-claim
exit liveness, token rotation, fee attribution, events, restoration or migration.
Existing strategies are permanently bound to their receipt ADDRESS: a new address
cannot simply be substituted. Prefer evaluating a compatible in-place receipt
upgrade, but first verify all debt/collateral/margin/incentive obligations, actual
balances, legacy storage and external consumers. No live-state migration is
implemented or approved here. Controller pause/discovery compatibility and upgrade
timelocks/admin policy must be deliberately retained or replaced before release.

## Lean all-stream and recycled-yield coordinator — September 17

`lp-receipt-vault/src/reward_coordinator.rs` adds native-only orchestration around
the lean principal engine, shared ownership ledger and actual Aquarius bridge.
The old single-stream harness remains as a historical regression; its deliberate
recycled-yield rejection is superseded ONLY by the new `co_*` test harness paths.
No public ABI, keeper routing, migration or deployable artifact was added.

At each checkpoint the coordinator snapshots all retained token balances, claims
the pool, verifies each full cash delta, and splits new rewards using pre-mutation
pToken supply `S` and escrow shares `E`: ordinary inventory is
`floor(received * (S-E) / S)` and the remainder belongs to old backing units.
That rounding favors old backing. Ordinary inventory, withdrawal reservations and
recycled inventory are distinct. Checks reconcile actual token custody, ordinary
weights, exact escrow shares, the derived pending-stream count, and aggregate
unallocated ledger units against real backing. Donations are not indexed.

Deposit, transfer/self-transfer and withdrawal checkpoint every retained stream.
Withdrawal reserves only the exiting owner's proportional ordinary raw claims,
then executes principal redemption without calling any reward route. Historical
raw claims, converted units and reservations remain with their earning owners.
This covers native supply-only paths, not a production `transfer_from` ABI.

Before issuing or redeeming backing units, all recycled streams must settle. A
new opaque recycle-only funding snapshot permits pending inventory to enrich OLD
units but rejects any new-unit issuance; it does not itself clear pending flags.
Successful earlier streams can remain settled when a later route fails. The
operation returns `Deferred` without issuing/redeeming units, and a retry cannot
reconsume the settled stream. Owner payout/minimum/auth failures roll back the
entire invocation, including earlier successful claims and swaps.

Permissionless per-token `recycle` preparation provides a bounded staged path.
Every later compound/payout still freshly checkpoints emissions. Preparation does
NOT grant a right to bypass newly accrued rewards or prove continuous-emission
execution fits. Conversion creates managed idle backing; later reinvestment is
still separate. A reward swap never implies that funds were deployed into the LP.

The registry retains at most four tokens, including retired ones. New-token
registration is admin-only; unknown claimed assets revert atomically until
registered. Old histories cannot be reset or evicted. An instance initialization
marker now prevents a missing persistent registry from being silently recreated
when registering a different token. Missing recycled/history records fail closed;
TTL renewal is tested, but full restoration/migration policy is still outstanding.
Pair-token rewards and a fifth lifetime token remain unsupported, not silently
discarded. Safe pre-rotation checkpoint/configuration still needs production policy.

New regressions cover old-unit yield before later claims, both settlement indices,
two-token failed-route partial progress/retry, blocked conversion with successful
principal exit, all-escrow supply, full principal exit/re-entry with retained
reservations, four retained streams, retired/unregistered assets, donation exclusion,
missing state/TTL, exact owner authorization, payout-minimum rollback and fresh
emissions after preparation. A shared backing test rejects using a recycle-only
snapshot to mint new units or bypass a different pending stream.

Verification:13 new functional coordinator regressions plus1 quarantined budget
diagnostic; lean bridge total42 passed. Ordinary targeted suites: ReceiptVault171
passed/1 intentional WASM ignore, Aquarius93 passed/2 intentional WASM ignores,
LP core9 passed. Strict LP-library clippy (`--no-deps -- -D warnings`) passes.
Run the lean suite with
`cargo test -p aquarius-lp-vault --locked --features lp-receipt-prototype reward_bridge_test`.

Resource scope is native real-SAC/Aquarius with mock pools, NOT deployed pool WASM:

- Two-stream strategy-funded principal exit:81 entries /12,903,622 instructions.
- Two-stream staged reward payout:82 entries /15,603,991 instructions.
- Four-retained-token, single-stream recycle:74–75 entries /~12.1M instructions.
- Combining a recycled second-token swap and strategy-funded reward payout hit
  102 entries and FAILED with normal SDK fixture limits in the initial regression.
- The explicitly named four-route combined payout diagnostic measures152 entries /
  53,043,437 instructions with SDK resource limits DISABLED. It asserts the >100
  fixture overrun and is NOT network-executable success evidence. Other new tests
  retain normal transaction limits (only cumulative host accounting is reset).

The SDK's100-entry fixture ceiling is not a current network-limit measurement.
Exact deployed-WASM resource validation remains mandatory. Staged native tests do
not solve continuously accruing multi-route worst cases by themselves.

At this intermediate stage, claim failure and stale/failed strategy quotes blocked
principal exit. The outage follow-up below supersedes that limitation only where
reward debt remains independently observable and strategy redemption remains safe.
Do not catch and ignore unknown ownership failures. Fees/pair emissions, keeper legacy
harvest exclusion, user-facing outcomes/events, dust policy, restoration and live
migration/controller obligations still block production. Generic lending/DeFindex
production behavior and all existing deployment gates remain unchanged.

## Observable reward outages and proportional principal exit — September 17

The native coordinator records per-token `PoolOwed` separately from receipt cash.
If the atomic claim fails, the bound strategy reads primary `get_user_reward` and
complete gauge `gauges_get_reward_info` observations (`to_claim`). Only increases
above already-recorded debt accrue to the pre-mutation holder/escrow weights.
The claim failure must have left receipt cash unchanged. A later successful claim
must deliver at least the recorded amount; collection itself does not accrue that
amount twice. Missing, unregistered or decreasing observations fail closed.

Custody checks cover cash plus receivables, but receivables NEVER enter receipt
NAV. Conversion/payout for a token waits until its recorded debt is fully funded.
Reserved claims survive partial/full principal exits and later deposits; future
emissions use remaining weights. Escrow's uncollected yield retains the all-stream
gate before new backing units or reward-unit payouts. If both claim and quote
fail, principal exit still blocks rather than guessing entitlement. Rotation must
preserve collection of old debt; a trusted but wrong primary-token configuration
is not made safe by this observation mechanism.

`withdraw_proportional` needs no receipt NAV quote. It redeems the holder's floored
fraction of actual strategy shares and managed idle cash, measures actual proceeds,
checks exact share burn/cash deltas and finally enforces the user's nonzero payout
minimum. The strategy receives a floor of ONE raw settlement unit, not the user's
minimum: its existing idle-cash behavior must not let that input draw another
holder's funds. Existing strategy cached-exit, oracle-divergence and liquidity
guards remain. A final holder burns all strategy shares; donations remain excluded.
An unreachable minimum atomically rolls back principal and reward checkpoints.

Six new regressions cover two-token debt/recovery, late deposit exclusion, repeated
failed claims, future escrow yield, decreasing debt/short collection rollback,
both stale-oracle settlement legs, malicious minimums against another holder's
idle cash and exact owner authorization. The unobservable-claim negative regression
remains. Lean bridge48 passed/2 explicit-WASM ignores; full ordinary workspace595
unit and3 doc tests passed. Earlier native resource numbers above are historical:
the updated four-route diagnostic is161 entries/~53.87M instructions with limits
DISABLED, not transaction-fit evidence. Continuously accruing worst cases remain
unresolved. Strict LP-library clippy passed.

### Exact deployed-pool local rehearsal

Read-only downloads from XLM pool
`CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F` and stable pool
`CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX` have the same SHA256:
`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
The test pins this hash and initializes that WASM locally with real SACs and real
primary emissions. It pauses genuine pool claims, exits with exact owner auth,
then unpauses, collects and pays the retained reward. Both settlement indices pass,
one with a stale controlled oracle:87 entries/87,325,135 instructions and87 entries/
95,443,345 instructions respectively, with normal SDK transaction limits enabled.

Reproduce after downloading that public artifact to a local file:

```sh
AQUARIUS_CONCENTRATED_WASM=/absolute/path/pool.wasm \
  cargo test -p aquarius-lp-vault --locked --features lp-receipt-prototype \
  exact_deployed_pool_ -- --ignored --nocapture
```

Scope: SDK25 host, fresh local pool state, NATIVE receipt/strategy, controlled
oracle, no-op plane/zero-boost-supply feed and mock reward route. This is NOT a full
compiled-stack/Mainnet simulation or deployed gauge-WASM validation. No Mainnet
transaction was signed. All prototype WASM gates stay in place. Full compiled
resource/auth tests, migration, token rotation, fees/pair emissions, restoration
and production hooks remain release blockers even if a security scan is clean.

Security scan scope must include this whole LP diff: the new `lp-receipt-vault`
package, reused ReceiptVault reward ledger/backing/model, Aquarius bridge,
coordinator/exact-pool tests, mocks and feature gates. A previous MarginController
scan or an Aquarius-only project filter does not cover this change.

## Primary-token lifecycle and durable exit intent — September 17

The native coordinator pins the primary token in receipt instance storage during
first primary-stream registration. Later claims reject out-of-band changes to the
strategy's configured token. The pin cannot be recreated after ledger activation;
this is native schema evolution, NOT an automatic migration for deployed receipts.

Primary rotation is deliberately two bounded, receipt-admin-authorized stages:

1. `prepare_rotation(minimum_cash)` withdraws all strategy shares into managed
   settlement cash and verifies actual cash/share deltas. Its floor covers total
   managed settlement cash, including existing idle cash—not original principal
   or a price guarantee. It changes neither receipt shares nor reward weights and
   keeps the old denomination. Unknown rewards remain unmodified until checkpoint.
2. `rotate_primary(next, minimum_cash)` requires a registered token and actual zero
   strategy shares NOW; it does not reuse a preparation certificate. It freshly
   collects all old rewards (including those checkpointed by the pool withdrawal),
   rejects any remaining per-token receivable, and verifies actual pool raw and
   weighted liquidity are zero and all observable old claimables are zero. It
   validates the new route's two-token shape and nonzero floor, updates the bridge
   and receipt pin atomically, and emits the old/new token and managed cash.

Old registry entries, raw reserves, fractional ownership, compounded backing and
routes are retained. Re-selecting an old token does not reset its history. The
four-lifetime-token cap remains; fifth-token migration is not implemented. Gauge
source retirement/replacement and pair-token emissions remain separate work.

Preparation is custody management, not a global freeze: subsequent safe operations
still use the old denomination and fresh reward checkpoints. If a later deposit
or rebalance reinvests cash, rotation completion fails until custody is prepared
again. Governance may use the existing deposit pause during an operational change;
production orchestration/policy has not been wired. Old funded raw reserves can
still be paid without waiting for a new primary claim.

The pinned deployed pool exposes no primary-token address getter. Our rotation
does NOT change its external emission configuration. After changing the strategy's
expectation, independently quoted IOUs are disabled until a strictly validated,
positive actual PRIMARY transfer proves the new denomination. Zero claims, gauge
payments of that same token, and reverted partial primary/gauge claims cannot
enable fallback. Successful zero-cash checkpoints are permitted; an actual mismatch
fails closed. Bootstrap primary configuration remains an operator trust assumption.
Changing primary again behind the receipt's back cannot bypass the receipt pin.
If Aquarius changes/removes an old source before its debt is collected, this flow
cannot manufacture recovery or re-denominate the liability; coordinated pool-side
transition/upgrade compatibility remains required.

`exit_request.rs` records a signed per-owner nonce, share amount and nonzero minimum
without contacting reward APIs. Shares are NOT moved, burned, frozen or locked;
the user keeps earning while waiting and can cancel or use another safe action.
`execute` requires fresh owner authorization plus the existing proportional-exit
checkpoint, current sufficient shares and stored final minimum. A failed quote,
claim, principal exit, minimum or authorization leaves the request intact. Completion
or cancellation retains a tombstone/nonce, preventing replay; read/use renews TTL.
No public FIFO, user enumeration, keeper spending authority, admin override or
arbitrary historical reward estimates are introduced. This is a durable retry
workflow, NOT guaranteed/immediate principal liveness when all reward data is lost.
Archival restoration must preserve these records as well as reward/account history.

### Lifecycle evidence and limits

Nine native regressions cover retained old backing/reserves, future owner fairness,
new-denomination proof (including gauge-only and rolled-back claims), outstanding
debt, bad routes/floors, failed unwind/minimum rollback, direct configuration bypass,
exact receipt-admin/owner auth, staged reinvestment races/fresh tail rewards,
outage request/retry, current-share changes, cancellation/replay and TTL renewal.
The historical retired-token test now uses the coordinated rotation, not direct
configuration replacement. Default resource limits remain enabled for these tests.

The explicitly enabled exact-pool suite adds staged rotation against the same
hash-pinned WASM on BOTH settlement indices. Preparation uses79 entries and
77,110,311 /85,113,518 instructions; completion67 entries and22,899,125 /22,897,175.
Initial combined work measured108.7M, then105.1M on the second leg even after
removing a duplicate claim, exceeding our conservative100M assertion. Splitting
custody preparation from fresh collection/configuration solves this measured shape
without raising the assertion or using stale ownership. The100M ceiling is a test
policy, not a new Mainnet resource-limit read. The original paused-claim exit tests
also pass (87 entries,87.71M /95.83M). These are native receipt/strategy plus actual
pool WASM with controlled oracle/plane/reward-route dependencies, NOT fully compiled
candidate contracts, real gauge WASM or a live new-token emission migration.

Final checks for this step: full workspace595 unit/3 doc tests; lean bridge57
passed/3 explicit-WASM ignores; all3 explicitly enabled exact-pool tests passed;
strict LP-library clippy (`--no-deps -- -D warnings`) and diff checks passed.

### Still required before production hooks

The migration/production-interface draft and native local accounting rehearsals
are now in [the LP migration specification](../lp-receipt-vault/MIGRATION.md).
This is NOT an implemented migration. Normal controller delisting requires zero
pToken supply, old incentives and historical pooled yield need an explicit
retirement/disposition protocol, and a complete mutation fence is still missing.
Local zero-debt inspection cannot prove absence of per-owner or external claims.
Keep all activation/WASM gates; no test-only shadow marker insertion is a release
entrypoint. Raw balances/allowances/metadata must be preserved, never rescaled.

- Wire the persistent ownership ledger to the separate LP receipt's production
  interface only after real-WASM footprint and recycled-yield blockers are resolved.
  Leave generic lending/DeFindex methods untouched. Native tests now
  derive actual backing allocations, but production methods remain untouched.
- Validate the new native all-stream/recycled coordinator against actual deployed
  pool WASM and continuously accruing worst-case budgets. Complete token-rotation
  policy, dust handling and outage liveness before introducing a production ABI.
- Integrate the measured claim deltas before EVERY holder balance change,
  including claim-failure liveness, fees, pair-token incentives and zero-supply
  residue. The bridge alone does not establish when a holder earned the tokens.
- Define whether/how accrued converted units are settled in a partial withdrawal,
  and map the existing receipt idle-cash and final-exit paths to the new ledger.
- Settle/checkpoint existing Peridottroller incentive obligations at migration.
  The lean prototype has no controller incentives or collateral/seizure path. If
  those integrations are retained in a final design, every internal reward share
  mint/move must checkpoint them and resource measurements must include them.
- Plan bounded token registries, persistent epoch/account TTL and restoration,
  legacy migration and opt-in activation. Missing history must never reset claims.
- Choose a non-reentrant receipt/strategy call graph. Do not introduce a
  ReceiptVault -> strategy -> ReceiptVault callback loop in deposit/withdrawal.
  Existing permissionless strategy harvest/sweep paths must not bypass the ledger.
- Replace ALL legacy test stand-ins with the complete ownership coordinator and
  explicit deferred-conversion events/outcomes; never expose bypass entrypoints.
  Preserve reserved settlement cash against ALL deployment paths, including the
  existing admin idle-cash rebalance; a successful swap must not imply LP deployment.
- Test storage TTL/restoration, all actual Aquarius auth trees, same-pool swap NAV
  reconciliation and combined-call budgets. The pending gate must never require
  disabling price protections just to unblock a principal exit.
- Only after these backing/orchestration tests pass, add production entrypoints,
  storage and keeper routing; run auth, real-token, exact-WASM and footprint tests.

## Verification and release gates

Before production implementation is considered complete:

- Demonstrate two-holder timelines spanning accrued-but-unclaimed rewards, a new
  deposit, transfers, keeper compounding, partial exit, and final exit. Reconcile
  assets and liabilities at every boundary, including rounding and dust.
- Test idle-only and strategy-funded withdrawals; blocked/missing routes; partial
  multi-token conversion; failed checkpoints; full exit with deferred claims;
  re-entry after zero supply; token rotation; and unauthorized settlement calls.
- Test that a keeper harvest cannot consume reserved claims, and that withdrawing
  after a harvest cannot receive the same reward twice.
- Exercise all pToken mutation paths and preserve legacy multi-market health and
  borrowing behavior. Do not modify unrelated Margin work for this feature.
- Run exact deployed-WASM migration tests and realistic transaction simulations
  for auth trees, storage footprints, execution budgets, and atomic rollback.
  If the combined path does not fit, design an explicit resumable claim flow;
  do not silently reduce reward settlement guarantees.
- Review and scan the final contract/keeper range, then obtain scoped upgrade
  approval, respect timelocks, and verify deployed hashes and invariants.

No production reward mode, contract, or governance setting changed for this
prototype. The separate keeper freshness release v0.4.1 went live on September 15
and remains unchanged by this work.
