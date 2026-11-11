# Medium 1 — storage lifetime and absent configuration

Current result is **v6**, documented at the end: three tests pass per profile,
including actual compiled initialization and the fresh funded lifecycle. The
following v4 inventory/evidence and v5 cap follow-up are preserved historical
checkpoints; their narrower counts are not the final totals.

The preserved v4 compiled replacement contracts pass 13 controlled protocol-29 cases
under both resource profiles. This closes the tested host-level distinction
between archived configuration and configuration that never existed. It is not
Core transaction application, live eviction discovery, initializer authorization,
or a complete lending/redeployment approval.

Production changes in this checkpoint are owned and integrated by the lead.
This review/prototype does not edit production contracts. Both raw failures are
preserved: the first fixture incorrectly assumed the registered instance had a
short TTL; a later SDK resource-row addition initially compared `Val` directly
and failed compilation. The final v4 run passes.

## Safeguard inventory

`P` denotes persistent contract data; `I` denotes the contract instance. An
expired persistent entry is retained in the archive. An instance contains its
instance values together, rather than independently expiring each flag.
The inventory is source review of safeguard classes. Only rows in the exact-test
table below were executed individually; the inventory does not claim an archival
test for every listed key.

| Contract / safeguard | Storage | Absent semantics and final checkpoint treatment |
|---|---|---|
| Receipt `Initialized`, `UnderlyingToken`, admin | P | Initialization/authority checks reject missing required identity. Admin absence does not grant caller authority. |
| Receipt `CashBorrowingEnabled` | I | Missing returns false. Borrowing remains disabled. |
| Receipt `MinCashReserveBps` | I | Historically missing selected legacy routing. Final v5 **default** code interprets absent reserve as `Some(10000)` (full cash reserve); the fresh initializer explicitly writes zero and borrowing false. Explicit zero is a configured zero reserve, not absence. `lp-engine` compatibility remains separate. |
| Receipt `PermanentMinimumSupply` | I | Missing is zero; cash deposits require an explicitly funded seed of at least 1,000. The same instance TTL covers this immutable floor and borrowing policy. |
| Receipt `SupplyCap`, `BorrowCap` | P | Former missing-to-zero was permissive. Final source requires the cap on deposit and ordinary/margin borrow. Present zero remains intentionally uncapped. Repayment/recovery must not require the cap. Final v5 exercises missing-cap fresh-risk rejection separately from actual cap archival; see the rescan follow-up below. |
| Receipt `RatesReady` | P | Explicit false rejects borrow. Legacy absence consults whether an interest model is configured; binding is not a borrowing-enabled override in configured cash mode. |
| Receipt `InterestModel` | P | Absent selects supported static-rate mode; it is not evidence of expiry resetting the model. Rate model binding is a separate deployment requirement when used. |
| Receipt `Peridottroller` | P | Absent selects local collateral calculation and omits controller pause/reward calls. This is a supported unbound configuration, not an acceptable replacement deployment binding. Readback must require exact fresh controller. |
| Receipt `MarginController` | P | Margin authorization expects a configured address. Absence blocks margin-specific entrypoints; it also means no external margin collateral-lock policy for ordinary receipt operations. Replacement cash setup must leave margin unbound. |
| Receipt `BoostedVault`, idle buffer/cache/asset count | P | Missing binding means no strategy allocation. Missing idle buffer defaults zero and is not a durable strategy-disable flag. Admin binding/rebinding remains an authorized pathway, outside the public borrowing-disable guarantee. Cash replacement readback requires no strategy. |
| Receipt `ClCustody`, `ClCustodyContext` | I | Missing custody flag is false; protected withdrawal context must exist when required. These are not used as cash prototype authority. |
| Receipt reserves/admin fees/fee rates | P | Missing totals/fees often default zero. They are accounting rather than discretionary access flags. Persistent archival must restore their actual values; this fixture does not enumerate all liability keys or prove financial parity after archival. |
| Receipt flash-active / withdraw bypass | P | Missing active/bypass is false. Flash disable uses configured cash borrowing policy in the final source; flash-active alone is a transient reentrancy guard, not durable disable configuration. |
| Controller `Initialized` | I | Initializer also checks persistent admin. Core TTL maintenance already includes the initialized instance. |
| Controller four pause flag maps | P | Final source returns paused when the **entire map** never existed. A present initialized map without a market entry intentionally means unpaused. |
| Controller pause expiry maps | P | A true pause with missing expiry remains paused. An elapsed configured expiry unpauses by policy, not by ledger TTL. Existing pauses are bounded to 72 hours and cannot substitute for a durable borrowing-disable flag. |
| Controller `PauseGuardian`, admin | P | Missing guardian cannot authorize a guardian caller. Admin authority remains separately required; absence does not make operations public. Keeper must include configured guardian and pause keys, not only core keys. |
| Controller supported markets/market underlying/CF | P | Unlisted markets fail supported-market checks. Configured market/risk metadata and exact underlying bindings must be preserved/read back; optional cached metadata is not a replacement for identity validation. |
| Controller `MarginLiquidationControllers` | P | Missing/false authorization does not permit a margin controller. No new margin binding is authorized. |
| Controller oracle/fallback/cache | P | Oracle and supported price paths remain separate risk prerequisites; fallback expiry is explicit policy. Supply-only setup is not debt/liquidation price qualification. |
| Rate admin + base/multiplier/jump/kink | P | `ensure_initialized` rejects any missing required parameter; no zero-rate fallback for absent configuration. Parameter getters renew their persistent TTLs. |

Fresh replacement instances avoid migrating compromised legacy markets. The default v5 reader deliberately makes an absent reserve key restrictive, rather
than retaining implicit legacy allocation. This semantic change must be considered
for any hypothetical upgrade of an old unconfigured instance. Before any hypothetical migration, inventory existing
mode, debt, bindings, explicit caps and required policy; configure only an approved
fresh deployment here. Retired Mainnet addresses remain unusable.

## Exact tests and observations

`prototypes/lending-archive-qualification/src/lib.rs` loads the final receipt,
controller and rate WASMs from `target/lending-medium-fixed-20261111/artifacts`.
The hash identities are copied into the final run's `artifact-hashes.json`.
Controlled genesis directly seeds named storage; it does not authenticate an
initializer or change the compiled administrator. No auth mocks are installed.
Every preflight enforces an empty authorization set, so these no-auth getters
cannot borrow external or own-contract authority.

| Label family | Assertions |
|---|---|
| `get_min_cash_reserve_bps-{warm,expired-restored}` | Exactly `Some(1250)` before and after restoration of the expired receipt instance. |
| `get_cash_borrowing_enabled-{warm,expired-restored}` | Exactly false before and after instance restoration. |
| `get_minimum_supply-{warm,expired-restored}` | Exactly 1,000 before and after instance restoration. |
| `never-configured-pause-map-fails-closed` | A separately constructed contract fixture with no pause map returns true. No key deletion simulates expiry. |
| `pause-policy-{warm,expired-restored}` | Configured true pause and future deadline survive restoration of controller persistent keys. |
| `rate-policy-{warm,expired-restored}` | Configured rate 10,000 survives restoration of all five required persistent model keys. |
| `never-configured-rate-rejects` | A different compiled uninitialized model rejects. Successful setup is distinct from expected contract rejection. |
| `explicit-receipt-keeper-renews-instance` | Still-live instance TTL 1,100 becomes exactly 1,000,100 at ledger 100; its bytes are not replaced. |

The five expired-reading cases alter **only TTL**, retaining exact original
entry bytes in the snapshot. Official host29 recording preflight observes actual
restored read-write indices. This proves preflight restoration; the snapshot is
not a genuine network hot-archive eviction. No claim of actual Core restoration
application or rejection of an unprepared ledger transaction is made.

Both profiles: 1 Rust test / 13 asserted cases pass; 12 successful preflights,
one expected rejection, five restoring preflights, seven measured warm SDK roots.
Maxima are separate observations, not one combined transaction:

| Measurement | Internal policy | Captured network diagnostic |
|---|---:|---:|
| Unsigned preflight CPU | 3,652,525 | 3,652,525 |
| Unsigned preflight memory bytes | 1,386,547 | 1,386,547 |
| Preflight disk reads / writes | 5 / 5 | 5 / 5 |
| Unique preflight footprint keys | 34 | 34 |
| Serialized envelope bytes | 2,860 | 2,860 |
| Total estimated fee, stroops | 972,876 | 972,876 |
| Warm SDK CPU | 2,754,317 | 2,754,317 |
| Warm SDK memory bytes | 1,399,915 | 1,399,915 |
| Warm SDK total entries / writes | 34 / 0 | 34 / 0 |

Independent component maxima: nonrefundable 13,039, refundable 49, rent 959,688,
inclusion 100 stroops. These maxima need not belong to the same row and must not
be added to manufacture a transaction fee. Each raw row reconciles its own total.

The final profile manifests retain captured ledger 65,264,920, protocol29 and
cost-model SHA `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`.
All unsigned rows contain complete fees, rent changes, exact footprint and XDR.
The local global checker consumes authentic warm SDK rows: invocation PASS,
lifecycle FAIL, transaction FAIL, checkpoint FAIL in **each** profile; 46 Stage 1
required labels, proof manifest and globally mapped unsigned evidence are absent.
The storage report does not reclassify those failures as release approval.

Reproduce with the command in the prototype README. Final local evidence is
`target/lending-archive-qualification/final-v4/`; the runner exits **1** because
the global checker fails, while each test process exits 0. Inspect each
`*-gates.json`, not just aggregate exit status.
The lead preserved this final evidence and the three separate WASMs in
`evidence/medium-host-and-wasm.tar.xz`, with the member hashes in the accompanying
`evidence/manifest.json`.

## Maintenance and remaining proof

The explicit receipt `bump_ttl` now maintains its instance as well as persistent
core keys. The unsigned fresh-ID keeper preparation script is separately owned
by the lead. A keeper must monitor **code and instance** TTL as well as all
configured persistent safeguard keys, batch extension/restoration within limits,
and perform post-restoration readback of exact policy and bindings. Receipt
keepalive alone does not renew every controller pause/guardian/allocation key or
the rate contract's code. Permissionless keepalive does not authorize policy
mutation, and a keeper outage must trigger restoration before use, never reset
configuration.

Remaining at v4 (the v5 cap-only additions below close part of this list): exact new-code Core29 hot-archive application/native restore fees;
public financial outflow behavior with archived caps, controller/model bindings,
guardian and accounting liabilities; interrupted restore/re-expiry; missing-cap
deposit/borrow rollback tests; final no-oracle supply-only initializer/deposit/
withdraw flow. The present fixture is bounded and does not claim these passed.
The rate denominator issue and permanent seed financial regressions are separately
owned proofs, not substituted by these storage readings.

## V5: compiled financial response to the three cap-expiry findings

The rescan of `10de9449e68285aa8dda2b736d14bfaca20a904f` left three MEDIUM
findings: deposit `d2d495e7-eee4-44bb-8668-710d103b2525`, ordinary borrow
`4cf58b3f-aa58-49bc-bd3a-a2978a74230b`, and margin borrow
`9639ca2b-6dd2-4379-ae21-efb94fdd4b30`. Their suggested absent-to-zero fallback
would disable the cap. It was **not adopted**. Both caps already participate in
`bump_core_ttl`, called by `ensure_initialized` at these entrypoints. Actual
archival and missing configuration are different conditions, now tested through
the compiled financial paths rather than inferred from getters.

Final receipt hash:
`4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e`.
The controller and rate model hashes remain separately recorded in
`final-v5/artifact-hashes.json`. A fourth, explicitly synthetic compiled caller
has hash `93b98c6a3abf182a6e7346ffb50e38dbd13b7394255afba4c5e5c95e206ea6fd`
(965 bytes). Only `stellar contract build` built this caller; it must never be
deployed as a production controller.

`financial_caps::compiled_financial_cap_expiry_and_real_absence` uses an actual
SAC and actual compiled receipt, narrow external SDK auth, and real receipt/token
contract authorization. Its controlled genesis explicitly enables borrowing for
these tests only. The margin caller invokes `init_margin_borrow_state` followed
by `borrow_for_margin`, with direct contract-invoker authorization and no mocked
receipt/controller authorization. This is a receipt ABI test, not full V3
health/collateral validation.

| Financial cases | Concrete assertions |
|---|---|
| Deposit, warm and archived `SupplyCap=7000` | Actual seed 1000 plus deposited 5000; another 1000 succeeds, 1001 rejects. |
| Ordinary borrow, warm and archived `BorrowCap=500` | Borrow 100 succeeds against actual supplied collateral; 501 rejects. |
| Nested margin borrow, warm and archived `BorrowCap=500` | Same 100/501 boundary through the compiled caller and actual receipt margin namespace initialization/authorization. |
| Repay with warm, expired supply cap, expired borrow cap | Full 100-unit funded repayment succeeds in each preflight; restored and warm complete poststates match. |
| Withdraw with warm, expired supply cap, expired borrow cap | All 5000 holder shares withdraw after repayment; restored and warm complete poststates match. |
| Supply cap never configured | Fresh funded-seed attempt rejects before transfer; a naturally seeded state cannot have never configured this required cap. |
| Malformed genesis missing supply cap | Separately reconstructed seeded state lacking the cap rejects deposit. This deliberately malformed absence fixture is **not** an expiry simulation. |
| Borrow cap never configured | Actual seed/deposit succeed; ordinary and nested margin borrowing reject; debt-free holder withdrawal still succeeds. |
| Absent reserve/borrowing policy | Final default receipt reads `Some(10000)` and false; no absent-key opt-out of the cash reserve. |

Each archive scenario expires exactly one cap entry by changing only its TTL.
The official host restores it, and successful cases assert its exact before/after
value. Every archived financial case also compares the **complete** poststate
data hash with an independent warm invocation from the same prestate, including
balances, debt, principal and supply. All expected rejections assert no ledger
changes and identical full pre/poststate. Optimized release WASM removes the
panic text: the fixture records the typed `WasmVm(InvalidAction)` and diagnostic
events, paired positive controls and cap-boundary inputs; it does not pretend
the host exposes a typed application-specific cap error.

The committed warm SDK journey funds a 1000-unit seed, deposits 5000, borrows
100, repays 100, then withdraws 5000. Debt returns to zero; exactly 1000 locked
shares/assets remain, and the holder recovers the deposit. Before the separate
cash withdrawal case the real admin setter clears the synthetic margin binding
(no committed margin debt exists). This avoids claiming that a synthetic caller
proves V3 collateral-lock enforcement. No unsafe cancellation or real margin
recovery route is introduced.

`target/lending-archive-qualification/final-v5/` preserves two passing Rust tests
per profile: 15 policy preflights plus 22 financial preflights, totaling 26
successes and 11 expected rejections; 15 preflights observe restoration. Eighteen
warm SDK roots are measured. Both profiles yield the same observed maxima:

| Measurement | Internal policy | Captured network diagnostic |
|---|---:|---:|
| Warm SDK CPU | 16,008,521 | 16,008,521 |
| Warm SDK memory bytes | 1,666,770 | 1,666,770 |
| Warm SDK total entries / writes | 53 / 13 | 53 / 13 |
| Unsigned preflight CPU | 17,524,364 | 17,524,364 |
| Unsigned preflight memory bytes | 3,045,578 | 3,045,578 |
| Unique preflight footprint keys | 43 | 43 |
| Preflight disk reads / writes | 5 / 14 | 5 / 14 |
| Serialized envelope bytes | 4,056 | 4,056 |
| Maximum total estimated fee, stroops | 1,923,149 | 1,923,149 |

Independent fee-component maxima are nonrefundable 32,931, refundable 2,002,
rent 1,894,291, inclusion 100 stroops. Do not sum maxima from different rows.
Every preflight row retains its complete fees, rent, footprint and unsigned XDR.
`check_budget.sh` runs against the actual measured SDK rows for each profile:
invocation PASS; lifecycle/transaction/checkpoint FAIL with 46 missing Stage 1
labels. The runner still exits 1. These cap tests are not relabelled as the missing
Stage 1 lifecycle proofs, and a stored raw preflight does not substitute for the
global transaction proof mapping.

Reproduction is in the prototype README, with the final receipt directory and
separately built synthetic caller. V4 remains intact. Intermediate build/runtime
failures also remain: unavailable SDK versions under offline resolution, ambiguous
fixture conversion, stripped panic-message assertion, omitted margin namespace
initialization, and the synthetic caller's absent lock-query ABI. No production
guard was weakened to fix these fixture failures.

This evidence rebuts the claim that archived cap data becomes an absent value
requiring a zero fallback in supported host29 preflight. It does not establish
Core application, automatic wallet restoration support, live eviction discovery,
or old-instance compatibility. An unsigned/client transaction lacking the needed
restoration preparation may still fail safely. Remaining financial archival work
includes actual Core application, controller/model/guardian and other liability
keys, interrupted restore/re-expiry and full V3 validation. The separate native
supply-only/no-oracle tests are not counted as compiled coverage here. The scanner
findings remain recorded until the lead explicitly dispositions them against
this evidence; test success does not silently change scanner state.

## V6: actual initialization and the fresh-only deployment boundary

The final-source scan of `e3eab76a74028a394b0cde4e48c2ec42662811cc` completed.
Its HIGH finding `90f942e8` concerns upgrading a nonempty old receipt
that lacks the new permanent seed. It is a real compatibility limitation. No
migration or legacy upgrade has been implemented. All compromised legacy
instances remain retired, and this checkpoint qualifies only fresh instances.
The LOW observation that a wholly absent pause map returns paused is the intended
fail-closed behavior; the earlier compiled missing-map assertion remains present.

`compiled_actual_fresh_initialize_seed_lifecycle_and_unsupported_upgrade` now
calls **actual compiled `initialize`**, not injected receipt genesis. The exact
public pinned Ledger admin is
`GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP`. SDK29 `mock_auths`
would register a mock contract at that address, which is invalid for a G-address.
The fixture therefore sets the public account as the host source and supplies an
exact `SourceAccount` authorization tree for each admin call, including the seed's
nested SAC transfer. It never registers a contract at G and never mocks receipt
or token contract authorization. Source-account signature validation belongs to
the transaction layer and is **not** established by this host fixture. No keys,
signatures, network calls or actual funds are used.

A synthetic classic account and zero-balance trustline permit real SAC minting
to that public-address fixture. These are explicitly controlled genesis entries,
not actual account/trustline creation on Stellar. The first v6 attempt failed
with the real SAC's missing-trustline error; `v6-run1` preserves this failure.
`v6-run2` fixes that account fixture and passes both profiles.

The actual initializer's resulting state is read back: reserve `Some(0)`, borrowing
false, both caps explicitly zero, rates-ready false, no controller/model/boosted
bindings, and no permanent minimum before seeding. Then a contiguous compiled
SDK journey executes funded seed 1000 → deposit 1234 → withdraw all 1234 holder
shares → next deposit 7. The holder recovers all 1234 assets on exit, the locked
seed remains 1000, and final supply/underlying are both 1007. Borrowing stays
disabled throughout this fresh-only journey. The initializer itself is measured
as a successful SDK root, rather than counted as an unsigned preflight row.

Three compiled rejection preflights separately prove: reinitialization rejects;
a controlled nonempty unseeded legacy-state counterexample rejects a new deposit;
and that state cannot use the fresh seeding entrypoint to retrofit a seed. All
three preserve complete prestate and export no ledger changes. The legacy fixture
explicitly omits the new floor while retaining 1007 existing shares/assets; it is
a counterexample, **not** a real upgrade transaction or archival simulation.
These assertions demonstrate the unsupported upgrade boundary instead of claiming
old markets work or weakening the seed guard.

Final v6 evidence: `target/lending-archive-qualification/v6-run2/`, preserving v4
and final-v5. Each profile passes three Rust tests, 44 unsigned preflights (30
successful, 14 expected rejections, 15 restoring), and 23 measured warm SDK roots.
The actual initialization uses CPU 4,619,414, memory 1,449,345 bytes, 58 SDK entries
and 23 writes. Across the whole v6 suite, CPU/memory maxima and unsigned resource
maxima remain as in v5; SDK entry/write maxima rise to 58/23. Both profiles still
report invocation PASS, lifecycle/transaction/checkpoint FAIL with the missing
global proofs unchanged, and the runner exits 1. No CI or release approval is
inferred from these bounded tests.
The durable final bundle is `evidence/medium-final-evidence.tar.xz`, indexed by
`evidence/medium-final-manifest.json`; it preserves v5, v6, intermediate failures,
separate native evidence and the exact compiled artifacts. The earlier v4 bundle
remains separate.
