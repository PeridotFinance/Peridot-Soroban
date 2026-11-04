# Supporting resource decision notes — preserved comparisons

Final measurements and decisions are in handoff21. Stage1 remains incomplete and PR75 draft.
Nothing here adopts the storage/full-repayment prototypes, a resource policy,
transaction stages, production caps or a seed guarantee. Borrowing remains
disabled and production maintenance margin unset. No signing, broadcasting,
deployment, secret access, Upshift binding or shared00 change is authorized.

## What conditional restoration does and does not establish

The isolated preparation experiment separates native RestoreFootprint resource
simulation from the original financial invocation. Preparation changes no
beneficiary, reward entitlement, collateral ownership or debt. After preparation,
the funded financial root still repays debt and transfers permitted collateral
atomically with its original authorization and valuation checks. It does not
create an intermediate debt-bearing financial stage or allow forced reward waiver.

The simulation helper returns transaction resources and fee estimates, not Core
ledger application. The harness projects the expected restored TTL onto a
disposable snapshot and then executes the original compiled invocation. Its
success is conditional on those preparations actually being applied. Native
restore-operation CPU/memory are not measured as zero merely because declared
contract instructions are zero. Real source-account signatures, sequence/fee
processing, actual installed TTL metadata, live-state validity and full Core
transaction application remain unproved.

The captured minimum persistent TTL is2,073,600 ledgers. Every projected batch
must use that exact minimum; accidentally installing the longer maximum would
misstate downstream rent and retention. The pinned simulation helper also omits
the disk-read-entry fee component that Core counts for restored keys. Keep its
original estimate and the explicit Core-aligned count/fee calculation separately.
Neither should silently be relabelled as a fully applied transaction cost.

Final v7 has six compiled tests passing,30 prepared roots per profile and140 native batches per profile. Every prefix fits the unchanged applicable limits; internal financial execution still fails except full repayment/release. Captured host open227,912,081 CPU and close189,698,460 remain above100M. The final assessment and handoff21 provide exact per-root tables, source pins and gate failures; v2/v4/v5/v6 probes remain separate diagnostic evidence. Core application, signatures and live state stay unproved.

Preparation must remain independently fundable and resumable. An interrupted
prefix does not authorize the financial operation; it must be retried against
the updated snapshot after identifying the still-required archived keys. A
duplicate already-live batch may be empty; re-expiry or intervening pool/account
changes require fresh discovery, simulation, authorization and valuation.
Preserve existing-position recovery and unresolved/exited entitlements even
when the original borrower is absent. Explicit restoration funding and latency
are part of the operating design, not invisible prerequisites.

## Artifact identity correction: old evidence is not overwritten

The earlier local candidate-v2 receipt is88,231 bytes, SHA256 beginning
`292801`. It remains valid evidence only for those exact bytes. A reused Cargo
output meant its metadata/source intent did not establish execution of the
corrected receipt source. The source adds necessary renewal on the nonzero-cap
principal branch; native coverage and a later corrected build cannot
retroactively prove that branch in the older compiled artifact.

Fresh Linux compilation of receipt source
`a215cb99cdcb7b5ae54cedc743137d0e3d657fbab378bd59c11b6463362cf58e`
produces88,329 bytes with SHA256
`d36d3620732a2a6b2773cdc27a4e323b2db24d6620f06c086d5b1157a2637974`.
The controller is63,411 bytes, SHA256
`9130ee98cc37a38bd2f1fd459d6f03c3d1a0ba0da8b5b322c425fbff267c6870`.
Both Linux profiles use those same fresh candidate identities. The new clean Mac
receipt is also88,329 bytes but has its own `fc1ac7d...` identity; its full hash is `fc1ac7d397428438c58c50ce8930907c5965587aac7cc0f9988025235dcde362`.
Keep platform/code identities and each execution's source pins separate.

## CPU decision: restoration does not solve the 100M venue dependency

The offline [CPU attribution](cpu-attribution.md) independently reconciles six
preserved SDK call trees and three matched baseline/candidate call shapes.
These are the older local candidate's exact-byte traces, not a new measurement
of the corrected receipt artifact. Their value is the measured venue dependency
and the location of earlier savings. No Aquarius code change occurred.

| Older local trace | Baseline SDK CPU | Candidate SDK CPU | Aquarius disjoint inclusive | Aquarius exclusive | Outside Aquarius subtrees |
|---|---:|---:|---:|---:|---:|
| Aged open with actual pool deposit |237,214,100|223,144,751|167,831,028|162,817,228|55,313,723|
| Swap-close |188,506,613|183,241,286|141,545,386|137,759,524|41,695,900|

Exclusive CPU subtracts direct children, so every instruction is assigned once.
Aquarius inclusive totals add disjoint venue calls and include their token/plane
descendants. These are two views of the same work; do not add them together.
Outside-Aquarius work includes Peridot and other dependencies, not exclusively
removable wrapper overhead. Most aged-open savings were receipt-exclusive CPU:
31,314,820 to17,892,058, a13,422,762 reduction.

The full open retains all three claim-first ownership boundaries: deposit,
share transfer and custody assignment cost17,553,329 instructions inclusive in
total. Fee collection is53,952,286 and actual pool deposit64,233,503; that pair
alone costs118,185,789. Close's fee collection plus position withdrawal costs
115,815,792. Even Aquarius-exclusive costs exceed100M in both complete venue
paths. Eliminating Peridot overhead alone cannot make these unchanged paths fit.

Fresh principal valuation, pool estimates, guarded conversion, exact spend
authorization and reward boundaries remain in these measured trees. The open
uses an age-checked cached oracle NAV root; it is not evidence for the more
expensive expired-cache/direct-oracle branch. A later157M open does not actually
call `deposit_position`, so it cannot replace the full-deposit cost evidence.

Fresh unsigned host CPU is a separate measurement: in the older candidate,
data-restored open229,258,928 and close191,235,141. The SDK tree is not a
per-contract decomposition of those transaction totals, real signatures or
Core application. No direct fresh-host call-level trace was captured.

Retaining100M requires measured reduction of necessary venue fee/position work,
potentially an upstream implementation/ABI change that preserves fee-inclusive
valuation and all claims. No such new ABI or saving is assumed. Compact Peridot
state may help entry/restoration costs; it does not eliminate the venue CPU
floor. A helper wrapper gets no assumed saving. A different stage design would
need explicit custody, debt, deadlines, retries and borrower-independent
recovery, and still may not split funded repayment from permitted seizure.

## Completed CI for627e2e2, separately preserved

Both runs test merge `1728feeebe092280ffc650c11b8cda1b2e4df3d6` for head
`627e2e2c926b2f62dbb1b1cd0ef6f9949caf2d6d`. They do not execute the subsequent
explicit restore-batch or combined repayment experiment.

- Baseline run41613837844: checker42PASS; checkpointFAIL; artifact uploadPASS.
  Captured profile passes invocation/lifecycle/transaction/checkpoint with zero
  recorded errors. Internal profile fails all four, retaining106 lifecycle and
  303 transaction errors. Captured36 main plus5 archival tests pass; internal
  main4PASS/32FAIL and archival0PASS/5FAIL. Native rewards53PASS; default
  margin110PASS/2ignored. All ten baseline Linux WASM hashes match the previous
  baseline capture and are retained in the index.
- Storage run41613837952: both jobsFAIL and uploadPASS; each metadata suite7PASS.
  Captured baseline and candidate each pass4 fixture assertions; all three views
  have invocationPASS but lifecycle/transaction/checkpointFAIL. Live/data retain
  2 lifecycle and46 transaction errors per view; cold retains2/379 and28 missing
  resource rows. Internal baseline and candidate each fail4/4 fixtures with
  recorded Budget/ExceededLimit, and all gatesFAIL. The exact per-view error
  lists remain in the archive rather than being summarized as aggregate exit1.
  Captured live/data each match151 A/B records,59 with intentional TTL changes;
  cold comparison rejects missing complete host evidence. Reused view logs are
  not extra test executions.

Separate durable payloads under `docs/deployment-handoffs/evidence/`:

| Capture | Payload SHA256 | Members | Original artifact expiry |
|---|---|---:|---|
| `cl-linux-ci-627e2e2-20261103` | `cc53ab0679eaaa4f1cad756647c449412f6a483d90d99965dec9b7444d317bf5` |97|2026-11-17 10:10:21 UTC|
| `cl-storage-ci-627e2e2-20261103` | `69d7c40f473caee457bd4f34d8bf0d66a82c4e6358af6a1d9b8da975ebe33849` |546|2026-11-17 09:58:24 /10:13:43 UTC|

Every original ZIP size/digest and durable member was verified against original
ZIP or pinned Git bytes, including exact gates/errors, named tests and WASMs.
Full offline checker/XDR replay remains unperformed for these new CI captures.
This is preservation and source/result verification, not CI approval.

## Reserve decision: distinguish depeg exposure from additional LP loss

The product benchmark is holding the original deposited USDC. For initial gross
G and relative PYUSD/USDC price r after a costless initial50/50 conversion:

`G - LP value = (G - G(1+r)/2) + (G(1+r)/2 - LP value)`.

The first term is the underlying basket's loss versus original USDC; the second
is the additional LP loss versus holding that basket. Coverage clips the sum at
zero, not each term independently. A basket gain can offset LP loss. Path reserve
still includes claims already paid plus remaining marked loss; one cannot add
independent maxima of these two terms to obtain the path peak.

| G=1,000 USDC; USDC remains1USD | Basket loss | Additional LP loss | Original-USDC shortfall |
|---|---:|---:|---:|
| Full range, PYUSD declines50% |250|42.893219|292.893219|
| Full range, PYUSD declines75% |375|125|500|
| Hypothetical wide `[0.5,2]`, PYUSD declines50% |250|146.446609|396.446609|
| Hypothetical wide `[0.5,2]`, PYUSD declines75% |375|323.223305|698.223305|

These reuse the existing no-fee/no-reward model, not a new Aquarius economics
measurement. Common equal depegs can leave zero shortfall in USDC units while
destroying dollar purchasing power. Treating only basket-relative LP loss as
the product loss would underfund original-USDC coverage. Real entry/unwind,
slippage, fees, restoration, oracle outage, reserve-asset impairment and fill
capacity remain unknown addbacks or distinct risks. No cap is guaranteed.

Keep four separately earmarked funding purposes: supplier withdrawal cash;
dedicated loss coverage; funded-liquidator debt-repayment capital; operating,
transaction and restoration funds. A token cannot fund two buckets at once.
Liquidator principal is not a fee, loss seed is not withdrawal liquidity, and
modeled restoration rent is not a recurring monthly forecast. At2x, gross LP
exposure includes debt once; do not add the same collateral deficit on top of
aggregate LP loss. Previously earned rewards and quarantine cannot replenish
reserves. Day1 planning credits no speculative income, and future retention
requires an independently approved prospective policy.

The next decision should name the remaining blocker and its funding/evidence
requirements. Conditional restoration can address code/data availability only
if the explicit paid preparation actually succeeds; it neither cures100M CPU
nor changes the valuation, resource, signed-auth, release or economics gates.
