# Stage 1 staged transaction footprints — part (b), 2026-11-10

Stage1 remains incomplete and PR75 draft. **All nine measured transaction rows fit
100M CPU /41,943,040 memory bytes /100SDK entries /100unique transaction keys /50writes.**
This is compiled, controlled-state, unsigned evidence for an isolated1× cash-first
prototype, not adoption or a production workload bound. Option1 is permanently closed.
Aquarius remains the venue. The existing internal and captured profiles are unchanged.

The table reports per-dimension maxima across the observed concrete transactions.
SDK entry totals and serialized transaction footprint keys are intentionally separate.
CPU and memory include receipt command handling, vault accounting, authorization,
tokens, rewards and Aquarius in the same invocation. No helper receives a fresh budget.

|Transaction|CPU SDK|CPU unsigned host|Memory max bytes|SDK entries|Unique keys|Writes|Result|
|---|---:|---:|---:|---:|---:|---:|---|
|stage1_prepare|7132108|7489746|4691726|15|16|4|PASS|
|stage1_fund|3682956|3909593|3096927|14|15|6|PASS|
|stage1_convert|30634903|31234608|6617679|22|22|8|PASS|
|stage1_fee_checkpoint_cash_mint|71099168|72044765|9205278|51|51|19|PASS|
|stage1_keeper_investment|94552119|95217621|11352459|43|43|18|PASS|
|stage1_prepare_close|2642912|2803619|2858289|12|13|4|PASS|
|stage1_withdraw_in_kind|78322323|78912704|10132255|52|53|26|PASS|
|stage1_withdraw_for_conversion|77400092|78038740|10132255|50|50|24|PASS|
|stage1_optional_close_conversion|23305643|23856324|6432658|23|23|9|PASS|

The last two rows are the alternative close route: allocate the mixture, then optionally
convert and pay it. Direct in-kind withdrawal already includes both token payouts;
there is no unmeasured payout prerequisite. Maximum investment CPU is95,217,621,
leaving4,782,379 before missing signature costs. MaxSDK entries52, uniquekeys53,
writes26, memory11,352,459. The adverse mint-price test has51keys versus50 on the
ordinary journey. These maxima are not promised upper bounds for other histories,
tick paths, first/final positions, concurrency or restored state.

## Execution and metering

Final buildv4 uses compiled share vault and receipt market, actual Aquarius bytecode,
compiled oracle/test fixtures, and the existing `harness/cl-runtime29` engine; no new
runtime implementation. SDK29.0.1/official host29.0.1 execute WASMs built against SDK25.3.1.
Captured cost coefficients, fee and archival settings are pinned at ledger65270655,
configuration SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`.
Synthetic ledger state/networkID remains explicitly separate from live state. The old
capture records400M/400keys/200writes; it is preserved honestly. Acceptance here uses
the required100M/100/100/50 profile. Captured diagnostic rerun has identical results.
Selected finite budgets and SDK invocation limits are enforced. Diagnostic shadow
instrumentation is separately metered, not a substitute unlimited execution budget.

Actual contract authorization runs through receipt→vault→Aquarius/token. SDK mocks only
scoped external synthetic actors. Independent unsigned host execution records external
authorization, preserves real contract authorization and serialized footprints; it does
not verify real G-account signatures or apply financial transactions through Core.

Initial callback ABI failed because Soroban forbids vault reentry into the active receipt.
The corrected receipt passes its stored typed command under actual contract authorization;
external callers cannot supply or edit that command through the receipt execution ABI.
The failure and earlier compiler/test failures remain archived. Production Rust unchanged.

## Named close rule and mint correction

**INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN** is enforced and tested.
With old supplyS, exitsharesq, commonidleI and exactpendingLPfeesF:
`A=floor((I+F)*q/S)`, retained=`I+F-A`. Aquarius withdrawal pays principalP plusALLfees;
exit claim=`P+A`. Fund that claim and verify retained backing before any vault-share
burn. A one-liquidity-unit exit produces zero principal and collects9raw token0 fees:
old supply219,878,426, burnedshares3, newcomer shares unchanged, retained fee entitlement
preserved. The test proves `feealloc` precedes `stageburn`. A getter/event-buffer test
mistake initially failed and is preserved; it was not an authorization rejection.

Internal review found that the first prototype priced shares using the LP's composition
at the oracle-implied pool price. That can undervalue the actual withdrawable basket
when pool spot differs, causing overmint. Final buildv4 uses a fresh actual-composition,
fee-inclusive snapshot valued at the oracle, counting fees exactly once. The regression
shows actualNAV19,992,879 versus convergencevalue19,990,415: mint199,858,894, rejecting
199,883,529. Conservative convergence valuation remains only as an additional investment
cost-protection check. Investment also preserves actual backing; its cap check uses
actual backing. Earlier resource tables and WASMs remain historical, not final evidence.

Both full journeys conserve reward accounting at scale1e18 exactly:
received132, paid0, pending131, quarantine0, holderfractions999,999,999,844,440,694,
roundingscaled155,559,306. Fractions+rounding=1e18. Physical reward balance equals
received−paid. This specific test has no reward outage and does not prove quarantine
reconciliation or exited-history bounds. LPfee ownership is separate from AQUArewards.

## Per-transaction bytes and fees

Fees below are unsigned host estimates in stroops, including measured rent. Byte columns
are per-dimension observed maxima; fees use each row's representative continuous journey.
No fees were paid. Source-account/signature application overhead remains unmeasured.

|Transaction|Envelope bytes|Write bytes|Event bytes|Total fee stroops|
|---|---:|---:|---:|---:|
|stage1_prepare|1572|3624|12|1018821|
|stage1_fund|1988|4160|240|254002|
|stage1_convert|1812|7148|1012|176205|
|stage1_fee_checkpoint_cash_mint|4624|11508|1048|1616447|
|stage1_keeper_investment|3792|15180|1400|69924|
|stage1_prepare_close|1400|1208|12|902676|
|stage1_withdraw_in_kind|4856|16976|3000|1706639|
|stage1_withdraw_for_conversion|4592|16528|2528|1326551|
|stage1_optional_close_conversion|1928|7420|1264|29558|

|Complete journey|Transactions|Inclusion|Non-refundable|Refundable events|Rent|Total XLM|Synthetic elapsed|
|---|---:|---:|---:|---:|---:|---:|---:|
|Open→in-kind close|7|700|267741|16419|5459854|0.5744714|30s|
|Open→converted close|8|800|289783|18352|5085249|0.5394184|35s|

Totals come from two separate complete continuous journeys, never summed independently
cold probes. In-kind costs more in this controlled state because it creates a paired-token
recipient balance with rent; conversion pays an existing underlying balance. Synthetic
5-second transaction spacing is not measured network/keeper latency. This is warm operation
with actual touched-entry TTL/rent accounting, not a full cold-restoration journey.
Largest write payload16,976bytes, envelope4,856bytes, events3,000bytes. All existing
byte/entry/fee/rent checks remain. Decoded per-key purposes, RO/RW status and code identities
are in each final run's `footprints.json`; raw envelopes and ledger changes are retained.

## State and recovery scope

Receipt persistentcommand stores owner, kind, phase, deadline, amounts, minima, reserve
and shares. Assets remain at the vault. Segregated command cash, execution reserve and
allocated exit claims are deducted from common backing; donations remain excluded.
Prepare holds no assets; fund holds owner cash; convert holds actual owner mixture;
mint releases cash into common backing and mints locked direct-owner shares; investment
charges bounded costs to that command reserve, refunds unused reserve and unlocks shares.
Owner authorizes preparation/funding and close preparation. Deterministic later execution
is permissionless; phase transitions are atomic. Debt is always zero in this1× path.

Before-mint cancellation returns the actual mixture plus reserve. After-mint cancellation
only returns unused reserve and unlocks existing shares; **it is not immediate underlying
mixture delivery**. Close preparation locks existing shares without changing beneficial
ownership. Withdrawal burns after allocation; optional-conversion claims remain reserved
for the original owner. Beforedeadline cancellation requiresowner; afterexpiry anyone may
execute it with fixedownerrecipient. These routes are implemented but cancellation,
expiry, payoutfailure and reward-outage recovery are not qualified by this measurement set.
Normal staged burn fails closed when upstream rewardclaim fails. No forced-waiver shortcut.

All existing claim-first ownership boundaries remain; the1× direct-holder path does not
exercise margin custody reassignment. Required transfer/custody hooks are not replaced by
an earlier claim. The2× activation/delivery/funded-rescue milestone remains separate and
unbuilt. No new debt-bearing intermediate state is introduced here.

The earlier11-transaction split failed because mint still included deposit and exceeded
100M, marginactivation exceededentrylimits, and stale equalityfences blocked progress.
This1× prototype separates feecheckpoint/cashmint from laterinvestment and tolerates two
actual interveningpooltrades. It does not rehabilitate that old margin design or prove its
borrower-independent recovery. Future2× work is narrowABI/key-inventory documentation only.

## Tests, gates and review

Final runs `cl-stage1-internal-v6` and `cl-stage1-captured-v6`:13tests each PASS.
Both complete journeys, nine isolatedroots, one-unitnamedinvariant and off-oraclemint
regression are mapped in the prototypeREADME. `check_budget.sh` runs on every measured
flow. Invocation PASS; lifecycle FAIL; transaction FAIL; checkpoint FAIL. Each retains
46missingcombinedflows,2lifecycleerrors,1missingtransaction-proof-maperror. These include
historical2× labels; they are preserved evidence, not newly imposed1× requirements.
A focused1× resource pass is not approval of the combined historical checker or release.

Fable high review attempted with tools disabled and the approved bounded payload. CLI
returned429 `usage_limit_reached` with zero model input/output; no Fable verdict exists.
The preserved payload describes the pre-correction version. The internal reviewer found
the mint issue and independently verified final correction and resource rows. This is
explicitly internal review, not Fable substitution. Fable must review final tables/design
when available; no agreement or disagreement is fabricated.

Before Stage1 implementation/adoption: finish executable in-kind cancellation/outage
recovery, failedpayoutrollback, deadline/retry/duplicateauth tests, lock/legacyentrypoint
compatibility, firstmint/finalexit/concurrentliability bounds, range/rounding proofs,
restoration and signedauthorization costs. Numeric part(b) rows pass; these remaining
proofs stay open. No production build plan or production integration executed here.

Baseline CI for accepted7d432d3b: run42795706209 checker-tests SUCCESS,
checkpoint-gates FAILURE, artifact upload SUCCESS (job metadata verified). Other historical
prototype workflows also failed. Baseline CI does not execute this isolated prototype;
local13-test passes are not a CI pass. New checkpoint CI status is reported separately.

## Reproduction and evidence

See [isolated prototype README](../../peridot-contracts/prototypes/cl-stage1-cash-first/README.md)
for exact build/run commands and proof mapping. Existing harness, no new runtime. Final
sharevault WASM101034bytes SHA256 `d5b8d5c6f1f2cd85cacb25fbcbe684cc09a3818ad5707808edf11835a72aa766`;
receipt94836bytes SHA256 `ee0a77f1ec99f3e478337b144bf2072659cd420f1899c2bdf706bb86db291bff`;
Aquarius `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.

Durable440-member archive: [index](evidence/cl-stage1-staged-20261110/index.json),
SHA256 `7842eda4b75d0ebcb9800091e9a1f165d64976d113583fde1cca4d01133ea925`.
It preserves first compile/reentry/event failures, pre-correction measurements and sources,
final WASMs, unsigned transactions, key inventory, profiles, fees, rent, tests and reviewstatus.
No compiler caches or unrelatedtargetwallet artifacts included. Full member hashes and
both profiles' exact gates/errors replayed; aggregateexit alone is not the verification.

No signing, broadcasting, deployment, secrets, borrowing activation, cap/MM changes,
Upshift binding or handoff00 edits. RetiredMainnet addresses are not reused. All earlier
checkpoints, negative split and atomic baseline remain preserved.
