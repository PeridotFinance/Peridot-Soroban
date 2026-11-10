# Stage 1 private signed application — 2026-11-10

The signed table exists. **All nine measured warm financial roots pass actual Core29
application and the unchanged 100M CPU /41,943,040 memory /100 SDK total /100 unique
Soroban keys /50 writes checks.** Two complete cold preparation-to-execution probes
also pass. Keeper investment does **not** need a two-tranche split in these measured
states. This does not bound stressed histories, outages, tick paths or capacity.
Stage1 stays incomplete, PR75 draft, Option1 closed. No Stage1 build plan yet.

The user's new authorization permits private local-ledger application and signatures
from test accounts only. The fixture hard-codes a private passphrase and deterministic
public test keys; no real account keys, Mainnet, network service, funds, deployment or
production settings were used. Native Core signing remains disabled; the isolated
Node helper signs fixture envelopes and Core verifies them normally.

## Signed financial table

Each row includes actual compiled receipt/vault/Aquarius calls and nested authorization.
CPU/memory are the conservative maxima of Core29.0.0, enforcing host29.0.1 and SDK29.0.1;
the exact three values are preserved in JSON. Core native envelope verification is
outside Soroban CPU, whereas detached G-address authorization is host-metered. Native
validation/application timings are preserved, but are not production throughput bounds.

|Transaction|Core CPU|Conservative CPU max|Memory max|SDK total|Unique keys|Writes|Signed bytes|Charged stroops|
|---|---:|---:|---:|---:|---:|---:|---:|---:|
|stage_prepare|7358165|7397126|4784483|16|13|3|1460|960448|
|stage_fund|3651013|3754855|3007875|17|12|5|1848|190745|
|stage_convert|30930566|31245907|6606474|30|22|8|1884|196993|
|stage_mint|70687371|72062767|9095932|69|50|19|4640|1658395|
|stage_invest|93540354|95144812|10894011|61|43|18|3840|72237|
|stage_prepare_close|2667291|2724937|2898398|13|10|3|1300|845736|
|stage_withdraw_in_kind|76790425|78728736|9857357|78|52|26|4864|1345791|
|stage_withdraw|76283439|78061427|9839514|74|50|24|4696|1340040|
|stage_close_convert|23435334|23786729|6407954|32|23|9|1972|53269|
|stage_prepare-detached|7863089|7909748|4803785|19|15|4|1788|965045|
|stage_fund-detached|4217468|4331094|3053540|20|14|6|2176|195342|
|stage_prepare_close-detached|3161346|3213260|2913694|16|12|4|1628|850333|

All twelve rows PASS. The final three are additional authorization forms for the three
owner-authorized roots: keeper envelope plus real detached G-address-v1 signature.
Other financial roots need only the keeper envelope. This does not qualify arbitrary
multisignature, v2 authorization or custom-account code.

Investment: Core93,540,354; enforcing host95,144,812; SDK93,369,560. Conservative
headroom4,855,188 instructions. Cold investment below leaves4,794,234. Do not split
based on these measurements; do not assume this margin covers unmeasured stress.

**Correction to handoff25's display:** it called memory-read counts “SDK entries.”
The actual SDK/checker had always enforced memory reads + disk reads + writes.
The old unsigned maximum total was79, not52. This table uses the correct total
(investment61; in-kind withdrawal78). Unique Soroban keys are counted separately from
serialized RO/RW footprint. Native source-account fee/sequence bookkeeping and TTL
keys are not silently included in that count. Direct withdrawal declares26 RW keys;
Core actually writes25. Both figures are retained.

Warm rows are independently imported exact prestates, each sequence1, not one continuous
signed lifecycle. Owner C-to-G conversion explicitly adds classic trustlines, preserves
token amounts and maps typed ownership/command data. SDK native-authorizer pseudo-code
is removed; real financial code is unchanged. This does not pretend test-account setup
costs or trustline creation are included in financial roots.

Financial envelopes reserve100M CPU instructions, with warm1XLM maximum fee allowance.
Actual charges shown above are in stroops, with unused allowance refunded. Every warm
charge independently reconciles exactly to captured fee/rent coefficients and signed
envelope sizes; the JSON separates nonrefundable, refundable event, rent and inclusion
components. Reserved instructions are not consumed instructions. SDK partial fee estimates
are not used as complete transaction costs.

## Actual cold restoration and original financial execution

Both probes seed only the required existing persistent code/instance/data keys into a
controlled Core hot archive, absent from live state. Native signed RestoreFootprintOps
restore them in bounded batches. The same Core process retains its ledger while actual
restored data, installed TTLs and source state are exported for re-simulation/signing.
The financial operation then applies to that same ledger, without reconstruction or an
unmeasured keeper prerequisite. These are two independent cold probes, not a normal
lifecycle cost to sum together. No live archive discovery or natural eviction claim.

|Cold journey final operation|Core CPU|CPU max|Memory max|SDK total|Unique keys|RW|Result|
|---|---:|---:|---:|---:|---:|---:|---|
|cold-invest-v2|93648704|95205766|10935171|61|43|18|PASS|
|cold-withdraw-v1|76920464|78791924|9907085|78|52|26|PASS|

|Cold preparation transaction|Keys/writes|Read/write bytes|Signed bytes|Rent stroops|Total charged stroops|Result|
|---|---:|---:|---:|---:|---:|---|
|cold-invest-v2 batch 0|30|110760|2640|1279112012|1279250781|PASS|
|cold-invest-v2 batch 1|1|101136|240|1182754049|1182822484|PASS|
|cold-invest-v2 batch 2|1|94936|240|1097465716|1097530155|PASS|
|cold-withdraw-v1 batch 0|36|112324|3348|1281726304|1281879811|PASS|
|cold-withdraw-v1 batch 1|1|101136|240|1182754049|1182822484|PASS|
|cold-withdraw-v1 batch 2|1|94936|240|1097465716|1097530155|PASS|

Each native restore has one real keeper signature. Native CPU/memory are **not Soroban
metered**, not zero; Core enforces applicable byte/key/fee limits, and local elapsed
timings are recorded. Exact installed TTL = application ledger + captured minimum
persistent TTL -1. Every restored data value, source debit/sequence, fee pool, total
coins and duplicate-envelope rejection is asserted. All six native charges independently
reconcile exactly, including compiled-module memory contributions to code rent.

- cold-invest-v2: three preparation transactions plus financial execution; total 416.6882987 synthetic XLM. Financial charge 60.7279567 XLM.
- cold-withdraw-v1: three preparation transactions plus financial execution; total 417.0785571 synthetic XLM. Financial charge 60.8553121 XLM.

Shared code rent is not a per-borrower or per-operation normal cost. These totals cover
all preparation and final execution in each observed journey; no real XLM was spent.
They establish execution feasibility, not affordability, sustainable maintenance or
an approved operating balance/reserve. Cold final envelopes use measured rent plus a
bounded fee allowance. The original cold-invest-v1 run is preserved: restores passed,
then Core rejected financial execution because its inherited1XLM allowance did not
cover rent. No resource boundary was increased to resolve that ordinary fee failure.

## Proof level, remaining gates and review

Fourteen enforcing host/SDK tests and fourteen actual Core application cases pass:
nine warm roots, three detached forms, two cold journeys (the latter include six native
restores). `check_budget.sh` runs for every financial case. Exact gate/error replay14/14
passes; invocationPASS, lifecycleFAIL, transactionFAIL, checkpointFAIL in all14 reports.
Missing46 legacy required-flow labels stay visible. The existing generic transaction
checker expects its unsigned evidence schema; it has not been silently made to approve
these signed experimental rows. Separate Core/native evidence does not grant release.

Actual Core success, signatures, replay rejection, source charging and restoration
are now measured. Full financial post-state/return/event parity against enforcing host
output is not yet asserted by this new Core fixture. Prior compiled accounting tests
remain separate. Reward-outage mixture recovery, stressed ownership histories,
interrupted/re-expired preparation, intervening trades, live-state discovery and
capacity remain unqualified. In particular, the accepted prototype's post-mint cancel
only unlocks shares/refunds execution reserve; normal burn still blocks on reward
claim failure. Do not represent that as completed mixture recovery.

Independent read-only review agreed with warm attribution, corrected SDK counting,
private signature scope and same-ledger restoration design. It identified provenance
and fee-scope caveats now documented. There was no substantive resource conclusion
difference. This was the internal reviewer, not a new Fable result; prior Fable quota
failure remains unchanged. Warm source freeze is a post-run identity verification,
not a contemporaneous attestation. Core build-cache/bootstrap failures remain preserved.

## Reproduction, identities and CI

Commands and prerequisites: `peridot-contracts/prototypes/cl-stage1-signed-local/README.md`.
Exact tables, input packets, private-fixture signed envelopes, source snapshots, WASMs,
Core application logs, original failed runs, fee components, gate/error replays and
hash manifest are preserved separately at `evidence/cl-stage1-private-signed-20261110`.
Compiled Core binaries are hash-pinned, not stored in the archive; compile/link commands
reuse the pinned official29.0.0 source/objects from the prior local Core research setup.
This path name's historical “option1” does not reopen that closed policy.

Financial WASMs unchanged: vault101,034bytes SHA
`d5b8d5c6f1f2cd85cacb25fbcbe684cc09a3818ad5707808edf11835a72aa766`;
receipt94,836bytes SHA
`ee0a77f1ec99f3e478337b144bf2072659cd420f1899c2bdf706bb86db291bff`;
Aquarius SHA`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
Core official commit`a9d72b0cac3a89ebbcf926af449ac73089e0f8f7`; Core host29.0.0 and
preflight/SDK29.0.1 are explicitly different. Captured config65270655 and its coefficients
remain unchanged; local Core installs100M/100/50 constraints for these tests.

Accepted3188ae05 CI completed: baseline run42802555798 checkerPASS,
checkpointFAIL, artifact uploadPASS. Historical storage/restoration/closed-Option1
workflows also completedFAIL. Metadata is preserved separately. Those workflows do
not execute this signed prototype, and their result is not its approval. This checkpoint
adds no production deployment or build plan. Next priority is outage mixture recovery
and stressed histories, preserving the named pre-burn incumbent-fee invariant.
