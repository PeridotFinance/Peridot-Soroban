# Option1 qualification boundaries and evidence levels

Option1 remains a proposal. The current internal and captured profiles are
unchanged; no production admission rule, maintenance margin, cap or borrowing
setting is installed. The joined candidate combines the isolated atomic TTL,
operation-specific storage, full-repayment and three pending-state recovery
overlays. Its exact seven selected cases pass captured limits on V3 hash
`ad9fbb76…d482`; global lifecycle/transaction/checkpoint gates remain failed.
Earlier partial variants retain their own identities and evidence.

## Enforceable workload, not a claimed safe exposure cap

The proposed workload boundaries are 240M CPU, 26,843,545 modeled memory bytes,
240 SDK entries, separately 240 unique transaction keys, and 80 writes. All
other current checks remain. The outer proposed ceilings are 300M / 32 MiB /
300 / 300 / 100; passing only the outer ceiling is insufficient. No runnable
Option1 acceptance profile is created here.

| Dimension | Candidate admission rule | Recovery obligation / outstanding proof |
|---|---|---|
| Implementation and route | Bind reviewed venue, token, vault, receipt, controller and adapter identities and versions; direct two-token route only. | The exact compiled combination must execute. Reject unsupported **new** routes before custody/debt changes; existing positions retain settlement paths through their recorded dependencies. |
| Registries | Existing maximum 32 pair configurations, 64 indexed position IDs per borrower and at most eight compaction steps are actual contract bounds. | Current eight-borrower tests do not establish all 32 configurations or the largest registry combination. Either measure those existing bounds or propose an explicit tighter new-risk check. A UI limit is insufficient. |
| Venue traversal | Amount/slippage limits and a wide/full range do not bound initialized tick crossings or fee-growth work after public trades. | Measure the complete supported initialized-tick domain or require a venue ABI that bounds work without stale prices. No new-risk cap can justify blocking existing recovery; in-kind funded recovery must avoid obligatory venue conversion. |
| Reward history | Settlement uses explicit interval/beneficiary indexes and existing bounded waiver batches; never pack all history into one growing object. | 64 owners, 128 ownership changes, eight unresolved intervals and eight borrowers are tested samples, not global retention limits. New risk may be paused when operating funding/evidence is inadequate. Forced exits must still preserve every unresolved/exited entitlement and use bounded per-position work. |
| Archive work | Discover exact code/instance/data keys, measure bounded RestoreFootprint batches, persist successful preparation, and re-simulate the financial root. | No assumed pre-restored keeper state. Re-expiry and changed mutable dependencies require discovery and payment again. Preparation does not freeze debt, price or pool trades. |
| Authorization | Candidate first targets ordinary single-signature G source credentials, then a separate G transaction source plus G actor authorization. | Neither class has measured signature costs yet. Contract wallets/multisig are unsupported for new risk until measured. Recovery cannot require the absent borrower's account implementation to cooperate. |
| Funding and valuation | Enforce existing reserve/freshness/issuer-pause/execution-price checks; admit no borrowing in this checkpoint. | Repayment/seizure stays atomic. Fresh trusted seizure valuation remains necessary. A stale primary oracle or explicit liquidation pause can block seizure; no fallback, bypass or secondary oracle is introduced. |

No approved finite CL exposure cap exists. The $1,000/$5,000/$10,000 planning
sizes and 1×/2× scenarios do not establish a resource or loss-reserve cap.

## Exact remaining authorization experiment

After separate explicit authorization for **local synthetic signing only**, use
an isolated protocol29 Core ledger with disposable test accounts and no peers.
Do not use production wallets or network submission. For each supported class:

1. Construct the exact combined-candidate open, activation, close, independent
   pending recovery, active funded liquidation and full-repayment/release
   envelopes from the worst qualified prestate. Include code/instance/data
   preparation where needed and the same elapsed-interest/history/trade stress.
2. Class A: a real Ed25519 G source authorizes the operation; use source-account
   credentials where the actual ABI permits. Class B: a different G source pays
   fees while the actor supplies its own real Soroban address authorization,
   nonce and expiration. Keep nested controller/facade/vault authorization real.
3. Measure complete signed serialized bytes, signature verification, account and
   nonce keys, fees/refunds/rent, events/return and Core-applied balance/sequence
   changes. Keep SDK entry totals and unique serialized keys separate.
4. Reject changed payer/recipient/amount/root/subcall, absent/expired/duplicate
   authorization, insufficient maximum/funds and reentry. Check complete financial
   rollback, while separately recording included-transaction fees and sequence
   effects. A validation rejection and an included operation failure differ.
5. Compare exact financial state, beneficiary claims, quarantine, events and
   authorization trees with the unsigned controlled-state counterpart. Use the
   unchanged proposed workload boundaries; do not consume the 20% allowance or
   increase a limit to make the experiment pass.

No signatures are created in this checkpoint. A synthetic genesis-installed
`preAuthTx` signer can authorize an exact unsigned local Core transaction using
normal signature-checker rules. That can prove native RestoreFootprint
application, source sequence and fee charging. It cannot measure Ed25519 or
contract-account signing costs, or prove that production accounts have installed
the signer. A zero-threshold account alone is not a valid substitute in Core29.

## Separate evidence levels

| Level | What it can establish | What it cannot establish |
|---|---|---|
| Native fixture | Arithmetic/state-machine behavior with synthetic dependencies. | Compiled authorization or venue execution. |
| Compiled controlled state | Actual V3/receipt/vault/facade/Aquarius bytecode, narrow external test auth, nested contract authorization and selected resource checks. | Real account signatures, live prices/liquidity, Core fees/sequence. |
| Official host/simulation | Captured protocol29 cost model, unsigned footprint/envelope/fees, explicit restoration projections. | Actual archive consumption, installed TTL or charged source balance. |
| Local Core application | Normal ledger application and transaction validation with explicit local test authorization. | Production account authorization, live-state discovery, network contention or throughput. |
| Signed local combined lifecycle | Real supported account authorization and complete transaction application. | Live-state or operational capacity without those separate measurements. |
| Live-state discovery / capacity | Exact current deployed identities/archive availability and bounded operational load. | Approval to activate borrowing or adopt a policy. |

All missing levels remain failed qualification gates. An unsigned resource fit
is a reason to continue a bounded experiment, not a deployment approval.

## Operating funding is not reserve backing

The minimum source balance for a chosen incident is its actual account minimum
balance and liabilities **plus** the maximum fee exposure of the measured
preparation/recovery sequence, concurrent outstanding transactions, retries and
an explicitly selected fee contingency. Current unsigned modeled totals do not
establish that number: supported account authorization, live discovery and
complete joint Core application are still needed. No dedicated operating wallet
balance is read or presumed funded in this work.

Keep four ledgers/funding purposes distinct: supplier withdrawal cash; dedicated
loss-cover assets; XLM operating/rent funds; and the liquidator's debt-token
working capital. A full-repayment test with no remaining collateral consumes the
funder's contribution permanently. That part is loss-cover spending, not reusable
working capital. An available permissionless method does not create a willing
or funded liquidator. Zero-bonus healthy-timeout settlement and any treatment of
unreserved contingent close fees remain explicit local design choices requiring
review before adoption; previously reserved fees and beneficiary rewards remain
owed.

Compare recurring eligible **collected** strategy income with actual operating
expense before deciding affordability. Future income, unclaimed rewards and
quarantine provide no day-one backing. The 50% income-retention scenario neither
funds an existing shortfall nor changes the distinct original-USDC/hold-basket
benchmarks in the reserve study. No dollar conversion, approved seed/cap,
principal guarantee or paid infrastructure purchase follows from this research.
