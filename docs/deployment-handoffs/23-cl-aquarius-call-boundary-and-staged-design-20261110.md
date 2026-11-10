# Aquarius single-call measurement and command-flow design — 2026-11-10

**Measurement: every observed Aquarius call is below100M CPU.** Largest open call:
`deposit_position` **64,233,503** inclusive instructions. Largest close call:
`withdraw_position` **62,561,356**. The162.8M and137.8M report21 figures are sums
of venue-exclusive work. They do not establish an indivisible call over100M.

The main thread independently checked the original captured tree, all84 recorded
venue nodes and the durable archive/profile/contract identities. No new harness,
contract execution, policy change, signature or live-state study was performed.
Option1 qualification is paused. The resource boundary is still a gate on each
**complete transaction**, not just its Aquarius subtree.

The design changes share issuance and investment ordering: actual fee collection
and claim-first mint against real cash backing, followed by separate LP investment.
A receipt command alone, invoking the old deposit/redeem function later, cannot
work within100M. The new design is **unqualified** until its full transaction costs,
fee allocation and narrowed risk/recovery ABI are established. Existing activation
and rescue entry/write failures remain explicit below. Nothing is built or adopted.

Reproduce the measurement offline from the already committed report21 archive:

```sh
python3 research/cl-call-boundary-20261110/measure.py
```

Run from `peridot-contracts`. It verifies the archive and member hashes, reconciles
inclusive/exclusive costs and rewrites only its local analysis JSON/Markdown.
Full inputs and outputs: [measurement.json](../../peridot-contracts/research/cl-call-boundary-20261110/measurement.json).
The measured cost is SDK29 call-boundary attribution with protocol29 captured
coefficients. It is not a per-call decomposition of the separate host simulation,
not a signed/Core financial application and not a worst-live-tick bound.

## open_with_actual_deposit

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.8.0.0 | get_is_killed_swap | 1,087,231 | 1,087,231 | YES |
| 0.8.0.1 | get_is_killed_deposit | 1,063,738 | 1,063,738 | YES |
| 0.8.5.3 | claim | 6,196,410 | 5,363,417 | YES |
| 0.8.5.5 | claim_all_position_fees | 53,952,286 | 53,275,588 | YES |
| 0.8.5.6 | estimate_withdraw_position | 4,770,986 | 4,770,986 | YES |
| 0.8.5.11 | get_is_killed_deposit | 1,067,269 | 1,067,269 | YES |
| 0.8.5.12 | estimate_swap | 8,785,882 | 8,785,882 | YES |
| 0.8.5.14 | swap | 10,778,299 | 10,220,469 | YES |
| 0.8.5.19 | estimate_deposit_position | 4,538,505 | 4,538,505 | YES |
| 0.8.5.20 | deposit_position | 64,233,503 | 62,549,781 | YES |
| 0.8.7.1 | claim | 5,684,682 | 5,050,928 | YES |
| 0.10.1 | claim | 5,672,237 | 5,043,434 | YES |

Exclusive venue total **162,817,228**, disjoint inclusive total **167,831,028**, complete SDK root **223,150,467**.

## later_open_without_deposit

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.8.0.0 | get_is_killed_swap | 1,194,688 | 1,194,688 | YES |
| 0.8.0.1 | get_is_killed_deposit | 1,062,468 | 1,062,468 | YES |
| 0.8.5.3 | claim | 6,552,699 | 5,769,158 | YES |
| 0.8.5.5 | claim_all_position_fees | 53,919,030 | 53,263,516 | YES |
| 0.8.5.6 | estimate_withdraw_position | 4,570,289 | 4,570,289 | YES |
| 0.8.5.11 | get_is_killed_deposit | 1,066,119 | 1,066,119 | YES |
| 0.8.5.12 | estimate_swap | 9,167,680 | 9,167,680 | YES |
| 0.8.5.14 | swap | 11,100,517 | 10,588,471 | YES |
| 0.8.5.19 | estimate_deposit_position | 4,114,540 | 4,114,540 | YES |
| 0.8.7.1 | claim | 5,596,172 | 4,983,794 | YES |
| 0.10.1 | claim | 5,611,106 | 5,004,729 | YES |

Exclusive venue total **100,785,452**, disjoint inclusive total **103,955,308**, complete SDK root **157,058,149**.

## close

| Path | Aquarius function | Inclusive CPU | Exclusive CPU | Under100M |
|---|---|---:|---:|---|
| 0.4.0.0 | get_is_killed_swap | 1,081,735 | 1,081,735 | YES |
| 0.9.0.0 | get_is_killed_swap | 1,069,738 | 1,069,738 | YES |
| 0.9.3.1 | claim | 5,946,047 | 5,225,045 | YES |
| 0.9.5.3 | claim | 5,720,808 | 5,076,474 | YES |
| 0.9.5.5 | claim_all_position_fees | 53,254,436 | 52,890,781 | YES |
| 0.9.5.8 | withdraw_position | 62,561,356 | 61,064,024 | YES |
| 0.9.5.12 | estimate_swap | 4,957,303 | 4,957,303 | YES |
| 0.9.5.14 | swap | 6,953,963 | 6,394,424 | YES |

Exclusive venue total **137,759,524**, disjoint inclusive total **141,545,386**, complete SDK root **183,245,406**.

Every captured node reconciles inclusive minus direct children to exclusive, and all exclusive nodes to its root. Input hashes match the durable report21 archive. The later open never calls deposit_position and is not a full deposit feasibility proof.

Largest observed invocation across this captured lifecycle: **deposit_position 64233503 CPU**. This is the largest observed indivisible external ABI call, not a universal tick/history bound. Separate fee collection and position mutation cannot simply be assumed safe across transactions: fresh fee ownership, valuation, all reward boundaries, debt/custody and recovery still require a design. No statement about source-line or instruction-level indivisibility is made.

## The different accounting boundary

Open: **mint against fully accounted cash first; invest later.** After entrant-only
conversion, execute an actual AQUA claim at old ownership, actual LP-fee collection,
fresh fee-inclusive incumbent NAV and verified net token contribution. Mint
backed shares into the command's custody with their beneficial owner recorded
in that same mint operation. No `deposit_position` runs in this transaction.
If the implementation instead introduces a separate beneficial-ownership change,
it owes another actual claim and its cost; the design does not authorize collapsing
that boundary by naming an escrow. Cash and its risks are real backing; no predicted LP output, promised exchange
rate, provisional transferable receipt or unbacked borrowing power is minted.

In a later transaction, invest only the authorized command amount of available
vault cash. This changes backing composition, **not share ownership**. Obtain
fresh position/composition and oracle values, a fresh deposit quote and exact
spend authorization, then check actual token/liquidity deltas. There is no
prepared fee-growth or price equality witness to invalidate after every trade.
The existing mint, transfer and custody-assignment claim-first boundaries all
remain; no earlier claim substitutes for either later claim.

This ordering complies with shared00's literal requirement to collect LP fees
before minting. A design that merely values uncollected LP fees at mint would
need a separate policy decision; it is not this proposal. Price changes between
transactions are borne by the current owners of the actual backing, explicitly
including the idle-cash interval. Immediate LP deployment is not promised.

**Execution-cost isolation is mandatory.** Entrant conversion happens before
mint, so that entrant bears its execution loss. A separate, explicitly authorized
entrant execution reserve covers only the bounded, one-shot investment tied to
this command. It is outside common NAV and is not supplier cash, loss reserve or
liquidator capital. Investment must reconcile before/after backing at the same
fresh valuation basis using exact balances, liquidity and fee liabilities. Current
pool-spot composition priced at an oracle is not by itself a proof of value
neutrality after pool-price reversion: preserve a protocol pool/oracle band and
review an LP valuation/rounding bound that protects incumbents and lenders;
donations are excluded. Unexplained differences revert. Any permitted rounding
loss is charged to that reserve, within its cap; insufficiency leaves cash-backed
shares and a recoverable command. Unexpected gains or loss outside the reviewed
rounding envelope also revert until their ownership rule is specified. Do not
silently burn shares, charge incumbents, or create an unbounded execution-credit
liability. Return unused reserve to its original beneficiary at terminal settlement. The
reserve is not transferable with vault shares. Before any unrelated transfer or
partial exit, cancel the investment permission or reduce its amount/reserve
ceiling atomically; a sold or liquidated share position cannot leave a live debit
authority against the former owner's reserve. In funded rescue, invalidate that
permission before the atomic repayment/seizure and separately account for any
reserve that was explicitly pledged as collateral.
A command cannot charge this reserve for unrelated investment/reinvestment.

After mint, the contributed cash is **common share backing**, not simultaneously
an entrant-owned refundable balance. The command grants a bounded investment
permission, not a second claim on that cash. Redemptions/recovery take priority;
if available cash changes, investment retries from fresh state or cancels into
actual shares. The command cannot lock the entire vault or block another user's
forced exit. Cancelling after mint returns shares/eligible in-kind claims after
debt checks, never the original contribution amount on top of those shares.

Close: use `withdraw_position` itself to collect accrued LP fees **before the
share burn**, instead of first calling `claim_all_position_fees`. Aquarius pays
all position fees on withdrawal; they cannot all be given to a partial exiter.
For each token, if exact pre-withdraw fees are F, idle backing is I, the
withdrawn principal is P, exiting shares q and old supply S, allocate:

`exit = P + floor((I + F) * q / S)`.

I excludes donations, other commands' allocated funds, execution reserves and
other liabilities. Retain the rest of I+F for incumbents, with explicit rounding. Determine P from
actual receipt minus exact F, reconciling the venue's output/events and token
deltas. A fee-inclusive full-position quote already includes F; never add it
again. The bounded existing tests support fee-inclusive estimates and all-fee
withdrawal, but do not establish every range/rounding/state case needed for this
new algorithm. Require one supported active range/position for this bounded design; old/multiple
ranges cannot silently be collapsed into one62.6M call. Range changes and remaining
legacy positions are an explicit recovery/accounting dependency, not an instruction
to prune them. If fee/principal separation is unavailable or inconsistent, revert;
there is no guessed split. Successful withdrawal physically collects the fees,
then the same transaction burns shares. If liquidity-to-burn rounds to zero,
collect fees explicitly before a nonzero burn; do not call both expensive paths.
Keep the actual normal AQUA claim immediately before the burn. Recovery uses the
existing entitlement/quarantine rules when claims fail and never forces a waiver.

## Command and authorization contract

Proposed new ABI names below do not exist or imply approved implementation:
`receipt.prepare_cl_command`, `vault.execute_cl_command`, and
`receipt.commit_cl_result`. A command is one bounded record containing version,
nonce, operation (deposit/withdraw/repay/release), canonical owner/beneficiary,
position and debt market, exact assets, amount ceilings, slippage/minimum outputs,
execution-reserve limit, immutable deadline and permitted continuations. Pin
contract/configuration versions, never mutable market prices. Record the source
of custody, allocated quantities, phase and terminal consumption.

Preparation authenticates the owner and checks present authorization/risk. A later
executor authenticates the exact stored command through the canonical receipt;
the vault uses its actual nested contract authorization for asset transfers.
Past transaction authorization trees are not reusable signatures. An external
payer must independently authorize its current maximum and transfers. Only the
owner can loosen minima or increase its funding obligation; stricter retries do
not reset expiry. Every effect and its phase/nonce transition commit atomically.
Cancellation and execution compete on the same nonce; duplicates cannot repay,
mint, withdraw or release twice. Config upgrades invalidate new execution;
they must preserve a compatible recovery path, not strand existing custody.

The receipt remains the authority for debt, reserve and collateral permissions.
A stored command is not a lasting health certificate. At each risk-increasing or leveraged value-moving
step, including conversion and investment, recheck current borrowing permission/
reserve after costs, fresh primary prices/issuer state, actual backing and current
debt/health. Protocol price-divergence/slippage floors protect lenders independently
of owner minima; a borrower cannot authorize a bad self-trade by choosing loose
limits. Activation checks these
again. No cached preparation NAV or signed quote authorizes later collateral
removal. The narrow ABI must read fresh minimum state, including whole-account collateral
validity wherever existing policy requires it; omission of checks is not a resource
saving. Workload bounds for admitting new risk cannot reject recovery of an
existing position or prune its unresolved entitlements. Total ledger-key/SDK/write costs remain unqualified.

## Committed states (shared00 section9)

D is the absolute user-authorized command deadline, bounded by a reviewed protocol
maximum; no numerical production deadline is chosen here. Retries do not extend
D. Expiry disables new exposure and investment; it does not erase debt, entitlements
or custody. Anyone may execute the exact authorized continuation; an absent owner
cannot block a permitted funded recovery. A third-party seizure needs fresh trusted
valuation and either current liquidation eligibility or explicitly consented
new-command timeout terms. Those timeout terms are a proposal for review, not an
existing right to seize healthy legacy positions. Shared00's historical
begin→execute→swap→activate shorthand does not override the actual V3 state machine.

| Committed state | Custody and beneficial ownership | Debt responsibility | Resume, deadline and cancellation | Revalidate on next step |
|---|---|---|---|---|
| O0 Prepared | Receipt reserves command/position; existing collateral stays locked in its original custody. No predicted shares. | No new loan yet. | Executor may fund until D. Owner or expiry cancels unused command and unlocks only if currently permitted. | Owner/nonce/asset binding, current cross-account collateral validity, borrow permission, reserve and prices. |
| O1 Funded cash | Vault holds precisely allocated command cash; borrower is beneficiary. Borrowed cash and debt record commit together. | Borrower owes actual cash draw; interest starts here.1x has no borrowing. | Anyone executes within D; after D only settlement. Cancellation must repay current debt before collateral leaves. Independent payer can atomically repay and seize permitted actual cash/collateral, returning surplus. | Current debt, balances, reserve/new-risk permission, oracle validity, pool quote and entrant min output. |
| O2 Converted mixture | Vault command holds exact two-token balances, separate from common backing. Entrant bears conversion loss. | Borrower, accruing. | Freshly mint until D, or cancel/recover in kind subject to debt. Never require reverse conversion for recovery. | Actual contribution, current common supply/backing/LP fees, primary prices, allocation limits and all claim-first requirements. |
| O3 Cash-backed shares | Vault holds command-locked shares for borrower. Cash is now common backing; execution reserve remains separate. | Borrower, accruing; shares and reserve are enforceable collateral. | Invest until D, or cancel into actual shares after repayment/solvency checks. Funded third party can seize valued shares atomically with repayment; no borrower waiver. No original-cash refund plus shares. | Fresh before/after valuation basis, current pool composition/quote, exact spend, reserve cap, available common cash and applicable new-risk guards. |
| O4 Invested, undelivered | Same locked shares/beneficiary; backing is actual LP plus remaining cash. No extra unbacked receipt. | Borrower, accruing. | Deliver until D; expired command settles through in-kind share recovery. Investment failure reverts to O3; no orphaned pool position. | Current share ownership, nonce, actual balances, transfer claim and destination custody binding. |
| O5 Delivered, pending activation | Canonical margin/receipt custody holds locked shares; borrower retains reward entitlement. Delivery transfer claim and custody-assignment claim are both executed at their actual boundaries. | Borrower, accruing. | Activate until D; failed/expired activation leaves a recoverable position. Independent funded share seizure and full repayment/release remain reachable. | Current debt/health, fresh collateral and primary valuation, issuer/pause flags, actual cash reserve after all open costs, exact receipt/custody proof. |
| A Active | Ordinary active margin custody and beneficiary ledger. | Borrower, accruing. | Normal close, funded liquidation, full repayment and debt-free release. | Existing live checks; no command certificate bypass. |
| C0 Withdraw prepared | Receipt locks exact owned shares; preparation does not burn or release them. | Borrower, accruing. | Execute until D; unexecuted cancellation leaves original position/custody. Active funded liquidation can invalidate the command atomically. | Fresh debt/health/asset binding, live ownership, current venue fee/principal data, claim and burn permissions. |
| C1 Withdrawn mixture | Vault holds exact exit claim under the position lien; shares are burned once. Borrower's historical rewards remain separately claimable/quarantined. | Borrower, accruing until actual payment. | Convert within authorized bounds or settle in kind. Cancellation cannot resurrect burned shares; independent payer settles against the actual mixture. | Current debt, exact token balances, trustworthy seizure value or fresh swap/oracle/min-output checks. |
| C2 Converted proceeds | Vault holds assigned debt-token proceeds and any residual second token. | Borrower, accruing. | Apply own proceeds and authorized external top-up; expiry does not disable repayment. Insufficient funding keeps custody, never releases collateral. | Current accrued debt, payer max/funds, actual own proceeds, fees/refund and fresh value for any seizure. |
| R0 Repay/release prepared | Receipt stores a maximum and destination; collateral/proceeds stay under existing lien. No price-based release is pre-authorized forever. | Borrower until payment commits. | Executor consumes stored funded authorization, or external payer authorizes anew. Reject insufficient max with rollback. | Current full debt via existing repay_full_for_margin helper; zero-debt result, valid recipient, exact release/seizure bounds. |
| R1 Terminal | Actual repayment, permitted seizure and residual release commit together with command consumption. Debt-free collateral may be shares or mixture, not promised original asset. | Zero debt for full settlement; partial repayment never receives full-release status. | Repeat rejects/no-op according to typed ABI; no second payout. Preserve unresolved/exited entitlements indefinitely. | No later transaction is needed to make the funded seizure conditional on repayment. |

No global lock, command scan or history enumeration belongs in liquidation. A
funder may cancel an investment command only through the authorized rescue that
atomically repays and seizes; it cannot redirect an otherwise healthy user's
assets. Lending/strategy pauses stop new risk but not repayment. Stale or invalid
primary prices still block price-based seizure. Non-seizing repayment and
permitted debt-free/in-kind exits should remain available; there is no new oracle
fallback or confiscation policy. Available rescue code does not prove funder
capital or willingness, especially when collateral is worth less than debt.

## Per-transaction resource plan — not measured new totals

**Prestate warning from Fable review:** both position mutations in report21 ran
after `claim_all_position_fees` in the same transaction. The proposed deferred
investment and combined withdrawal may encounter outstanding fees. Reusing64.23M
or62.56M is only a planning anchor; fee-pending costs and execution semantics have
not been established. Even the largest new single call might differ. The older
split181SDK/145unique activation and report21's158SDK/119unique activation are
separate builds/workloads, not conflicting observations of one root.

All numbers below are **existing captured observations or explicit arithmetic**.
There is no full transaction measurement for a new command ABI. Each new row must
fit100M CPU,41,943,040 memory,100 SDK entries,100 unique keys,50 writes and all
other checks including authorization/serialized bytes/rent. A subtree budget is
not a passing transaction; unknown overhead is not zero. Warm and restored states
need separate qualification and cannot assume a keeper pre-restored dependencies.

| Proposed transaction | Existing evidence / known CPU component | Room to100M for all omitted work | Current qualification |
|---|---|---:|---|
| Prepare receipt command (O0 or C0/R0) | Prior split begin32.329M full unsigned host; close prepare55.772M. Different existing ABIs, not command estimates. | Unknown for new command | UNMEASURED. Old begin100SDK; close prepare120SDK already blocks reuse. |
| Fund O1 | Prior split_fund50.134M full unsigned host,82SDK/67unique/16writes. |49.866M against that old whole call | New command/custody/reserve ABI UNMEASURED; cannot add independent counters to claim a fit. |
| Entrant conversion O2 | Prior split_convert29.442M full unsigned host,35SDK/27unique/9writes. |70.558M against that old whole call | New allocation/auth/donated-dust handling UNMEASURED. |
| Fee checkpoint + cash-backed mint O3 | Same report21 vault-deposit subtree165,098,817 minus disjoint deposit64,233,503, deposit quote4,538,505, deposit kill view1,067,269, swap quote8,785,882 and swap10,778,299 =75,695,359. Actual fee collection and mint claim remain. |24,304,641 against this arithmetic remainder | NOT a measured function. Two-token custody/contribution checks, reserve record, fresh valuation and auth changes must be measured. No assumed17M witness saving is added again. |
| Later investment O4 | Deposit64,233,503 + quote4,538,505 + two fresh position estimates at4,770,986 each + kill view1,067,269 =79,381,249. |20,618,751 for all non-listed work | Hypothesis. Existing one-estimate cost is reused as a planning component, not proof a second costs the same. NAV/reserve/auth/rounding work unknown; if an extra claim is required it must be charged. |
| Deliver O5, retaining two claims | Report21 share transfer9,682,957 + custody assignment9,534,260 =19,217,217 disjoint subtree CPU. Prior complete split_deliver24.128M unsigned host. |80,782,783 against subtrees | Command and receipt result commit UNMEASURED. No synthetic signature-cost credit. |
| Activate A | Report21 prepared63.353M host,158SDK/119unique/40writes. |36.647M CPU only | Existing route FAILS100SDK/100keys. New narrow fresh-risk/receipt ABI required; at least58 SDK and19 unique keys removed before new overhead. |
| Vault in-kind withdrawal C1 | Withdraw62,561,356 + actual burn claim5,720,808 + fee-view2,091,665 (older captured split maximum) + fresh position estimate4,770,986 =75,144,815. |24,855,185 for omitted accounting/auth/price work | Full new algorithm UNMEASURED. The53M standalone fee collection is replaced only if withdrawal's exact fee collection/allocation is proven. q→0 branch needs its own complete bound. |
| Optional close conversion C2 | Report21 estimate4,957,303 + swap6,953,963 =11,911,266 inclusive. |88,088,734 for all omitted work | Command custody, current valuation and exact output checks UNMEASURED. Conversion is not necessary for funded in-kind rescue. |
| Full repayment + authorized release R1 | Report21 separate repayment15.487886M/74SDK/63keys/12writes and release14.422759M/86SDK/65keys/22writes. |No valid combined-total inference | These existing roots are separately executable. Combined command terminal operation needs measurement of union footprint and accrued-debt/auth/refund logic; no receipt cert may replace fresh debt. |
| Atomic third-party repayment + seizure from every debt state | Report21 active liquidation67.310552M host/170SDK/122keys/50writes. Later candidate stalled settlement up to74.180636M/176SDK/125keys/52writes, separate population. |CPU may fit; entries/writes do not | Existing routes FAIL. Requires a narrow receipt/controller recovery ABI with fresh valuation and historical reward hooks retained. Cannot split repayment from seizure to hide this failure. |

Intervening trades within newly checked execution bounds permit progress without
a stale equality fence. Trades outside user minima, venue pauses or an invalid
price can still prevent investment/conversion; completion then means an authorized
cancellation or in-kind debt settlement, not forcing a bad trade. No promise is
made that LP deployment succeeds under every external pool state.

The exact cost of full new transactions, total lifecycle fees/rent and confirmation
latency are **unknown**. Do not sum the reused independently cold probes. The
receipt-command records add storage/restoration/authentication costs. Shared code
restoration may use separately measured native RestoreFootprint batches, but
command discovery, interrupted preparation and restored financial execution remain
part of the eventual complete journey. No new such study is run here.

## Why this is not a repeat of the eleven-transaction prototype

The earlier experiment moved conversion and fee collection earlier but kept LP
deposit, final mint, fresh witness/quotes and two claims together. Full mint was
131.9–136.1M, activation181SDK/145unique, prepare-close120SDK, swap-close187.8M.
Its state-equality witness rejected intervening trades; pre-delivery liquidation
required the owner. It also lacked complete lifecycle/transaction proof mappings:
those missing mappings were additional failures, not the cause of the CPU result.

This proposal moves **real cash-backed share issuance before investment**, so
fee collection and LP deposit no longer share a transaction. It does not rely on
an old fee witness or an earlier AQUA claim. On close it uses withdrawal's actual
fee collection with proportional allocation before burning. Typed receipt commands
replace broad synchronous receipt→facade→vault orchestration, but narrow ABIs must
still read current risk state. Independent recovery is a required primitive for
all command custody states from the outset, not a later owner-funded escape.

## Recommendation and build prerequisites

Proceed with review of this specific accounting/command design, not a claim of
100M qualification and not another build yet. The single-call measurement does
not force a venue replacement. It also does not establish that the complete
lifecycle fits. Three concrete dependencies remain before a credible build plan:

1. Accept the product semantics of immediate cash-backed ownership followed by
   delayed investment, with precisely bounded entrant execution-cost funding and
   in-kind cancellation. This is not merely an adapter implementation detail.
2. Establish exact pinned-Aquarius fee/principal allocation and same-valuation
   deposit conservation from available code/evidence; document remaining cases
   before proposing a focused test. No stale witness or double-counted fees.
3. Specify a minimal **fresh** receipt/controller authorization/risk/recovery ABI
   and its key inventory that can remove the observed activation/recovery
   SDK/key/write excess. Until then the design has explicit100M-policy failures,
   despite sub100M individual venue calls.

No implementation, additional benchmark or signing is requested automatically by
this document. Option1 remains paused pending the user's decision on this result.

## Independent Fable review and lead decision

User-approved Claude review ran with alias `fable`, maximum effort, resolved model
**claude-fable-5-2**, tools disabled. It independently reproduced all supplied
inclusive/exclusive totals and accepts the largest-*observed*-call conclusion.
The main thread and a local Codex reviewer independently verified original archive
bytes/all84 nodes; Fable only saw the bounded31-row table and design excerpts.
Its full response is preserved in [claude-fable-review.md](../../peridot-contracts/research/cl-call-boundary-20261110/claude-fable-review.md).

Fable's key additions are accepted: mutation costs were measured after fees were
swept; a fee-pending mutation cannot inherit that cost as a bound. A simple
before/after current-spot mark can miss later price-reversion loss. Lender price
floors must apply at every leveraged value-moving step. Multiple LP positions,
precise fee-getter rounding, current risk state and every recovery phase remain
unqualified. Its payload did not contain the older fee-getter measurements;
those2.058–2.092M observations are documented here with their separate source hash,
not represented as Fable-verified or universally exact.

**Difference in views:** Fable says cash-mint-before-invest shifts carry to the
entrant and favors reversing the order. The lead distinguishes already-accrued
entitlements (settled before mint) from future income after a fair purchase of
cash-backed fungible shares. Pro-rata future income is not automatically a theft
of old entitlements: incumbents also own the newly contributed cash. Nevertheless,
delaying investment changes strategy allocation, yield and exposure for everyone;
that product choice and delay incentives are not approved by this design. Therefore
the lead does not recommend implementing the cash-mint version yet.

Fable proposes depositing segregated liquidity first, then sweeping/minting, with
all interim fees/AQUA going to incumbents and no interim earnings for the entrant.
The lead does **not** adopt this convention: a pending depositor owns contributed
assets and would need an explicitly approved earnings policy; it cannot silently
replace historical entitlement accounting or be imposed in forced recovery.
Tracking exact pending-liquidity fees/rewards instead would add a second accounting
class and unmeasured work, and proportional minting by liquidity is only valid for
matching ranges plus correct idle/fee ownership. Its apparent lower component
budget is not a measured alternative or permission to waive claims.

**Final recommendation:** the measurement removes the claimed single-call100M
blocker, but neither proposed ordering yet establishes a safe complete100M/100/50
lifecycle. Keep Option1 paused and do not build a new split at this checkpoint.
Review the cash/LP ownership semantics, exact unswept-fee ABI behavior and the
narrow fresh-risk/recovery ABI first. If the user chooses a follow-up study, propose
only the bounded fee-pending deposit/withdraw/rounding comparisons using the
existing harness, with all output and costs measured; do not run it implicitly.
