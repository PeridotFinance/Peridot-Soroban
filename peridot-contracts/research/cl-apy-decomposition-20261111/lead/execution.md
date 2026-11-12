# Current-code private application checkpoint

All nine source-account-signed roots for **each** PYUSD/USDC and USDC/EURC pass actual Core29 application and unchanged invocation checks. [Exact table](signed-table.md), [machine-readable metrics and fee components](signed-table.json). No Mainnet signature, submission, deployment or configuration occurred. Stage1 remains incomplete and PR75 remains draft.

|Boundary|PYUSD maximum across nine signed roots|EURC maximum|Limit|
|---|---:|---:|---:|
|Conservative max of Core/host/SDK CPU|97,823,368|99,068,647|100,000,000|
|Memory|10,704,826|10,770,453|41,943,040|
|SDK entries, memory reads + disk reads + writes|81|81|100|
|Unique Soroban footprint keys|55|55|100|
|Declared writes|26|26|50|
|Signed envelope bytes|5,032|5,024|132,096|

Investment consumes the CPU maximum in each pool. It has61 SDK entries,43 unique keys and18 writes. Headroom is **2,176,632 /931,353 instructions**, before replacing the synthetic oracle, adding the final cap/access implementation, or qualifying larger/live-state workloads. This is not evidence that those costs fit. Do not extrapolate a passing small state or remove claim-first/valuation checks to preserve these passes.

Warm in-kind journey sums: **0.5642323 /0.5642217 XLM**; optional-conversion journeys **0.5691346 /0.5691608 XLM**. These sum separately applied roots from their corresponding captured lifecycle prestates. They are not one continuous signed Core ledger journey or live latency. Core source signatures are verified natively outside Soroban CPU; their envelope bytes, validation/application timings, account debit and fee charge are captured. No zero-cost signature claim. Detached-address signatures have not been rerun against these new pair artifacts. No new-code cold journey is claimed.

Every Core application verifies source sequence advancement, exact fee/source debit and replay rejection. The independent fee formula reproduces all18 charges, including resource components, rent and signed-envelope fees with100M reservation. Each SDK enforcement run has actual authorization entries. Contract calls and `authorize_as_current_contract` behavior execute; no blanket authorization bypass is used. Setup has explicitly synthetic administration and controlled classic account/SAC genesis; setup issuer ownership is **not** authenticated by this test. There is no mock authorization contract at an issuer G address in the final fixture.

Four compiled full journeys pass (two alternatives per pair) using current venue code and two actual intervening venue trades each. Reward conservation is exact at1e18 scale. PYUSD: received133 = pending132 + (fractions999999999870831589 + rounding129168411)/1e18; EURC: received132 = pending131 + (fractions999999999993033489 + rounding6966511)/1e18. Paid/quarantine are0 in these final snapshots. These synthetic reward amounts prove accounting for this history, not real emissions income or outage/quarantine correctness. Claim-first ownership boundaries remain in place.

## Bindings and evidence levels

Current pool code SHA256 `155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907`,110,247bytes. Public RPC captureledger65421210; exact config/gauge/reward-vault code and provenance in `../live-code/`. Current initializer adds `config_storage_addr`; new reward claims distribute from a child reward vault. Those actual WASMs and actual calls are included. Accepted old pool `12fca5…`, atomic/negative-split prototypes and old signed/cold evidence remain unchanged.

Pair vaults100,964bytes each, distinct hashes:

- PYUSD `8a3b3e6629cf0117e07864aab786ad9856724a1f6f17f1d18b7750279c448923`
- EURC `a20767db25cedf35e9416867f90e00efa27703a063c639b589462909926c32a3`

Only isolated range constants/fixture bindings changed financially. PYUSD[-2020,2000],20spacing,10bp; EURC[-3060,1020],60spacing,30bp; actual50%protocol fee. Canonical public asset IDs, codes and issuers are represented in a controlled private ledger. Initial liquidity is established by a real deposit using captured price ratios, with post-deposit price/tick assertions; no synthetic post-deposit tick rewrite. The oracle is the existing compiled fixture returning captured primary numeric prices, **not actual Reflector code or the proposed depeg guard**. Current pool state/migration/gauge history is not a complete live-state import.

Contract WASMs were produced only with `stellar contract build`. Core29.0.0 official application and SDK/preflight29.0.1 remain distinct. Captured cost coefficients and internal ceilings are enforced exactly as in the accepted fixture. Neither internal nor captured diagnostic profile was changed; Option1 remains closed.

## Failed attempts retained

1. Initial setup helper used the wrong SDK address conversion (compile failure).
2. Issuer G-address mock registration failed before financial execution; replaced with explicit synthetic SAC genesis admin assignment. Never counted as an auth rejection.
3. Private-derived SAC IDs reversed token sorting; imported canonical public SAC instances in genesis. A shared harness executable also caused the nominal EURC v3 log to execute PYUSD; it is **not EURC evidence**. Harness package names are now distinct.
4. The lead's first build omitted the test vault admin environment variable. Wrong-admin bytes are preserved separately.
5. A shared build target reused PYUSD vault bytes for EURC. Identical hashes exposed it; separate fresh build caches produced the distinct final hashes above. Earlier bytes remain preserved and are not final evidence.
6. First-liquidity seeding at equal amounts failed EURC range validation; real desired-ratio initialization replaced the inconsistent setup. The old pool-funded reward fixture also failed the actual child-vault payout; setup now funds that actual vault.
7. All successful final captures are v6; classic trustline export revision7 removes duplicate alias keys without changing balances. Existing failed logs, patches, manifests and explicit missing-report checker failures remain archived.

These are ordinary local fixture corrections, not production changes or authorizations to weaken validation. Twenty exact gate/error replays cover both captures and eighteen signed invocations: invocationPASS, lifecycle/transaction/checkpointFAIL. Missing required lifecycle and transaction proofs stay failed; the private source-signature table does not manufacture global gate completion.

## Remaining next work, scoped

Pair selection remains with the operator. At1,000USDC-equivalent aggregate cap, target500/leg; unrestricted on-chain deposits are now explicitly selected, so the frontend password is not an access gate. No cap setting has been written. After selection, bind the final primary/guard and cap artifact, then measure those exact bytes at the intended workload and rehearse close on the live-state fork before any Mainnet signature. New-code cold restoration/rent and detached authorization forms remain separate missing proofs. G1–G4 remain outstanding; latest unrestricted-deposit decision removed their earlier enforcement as allowlist-removal gates.

The deployment document records exact command forms/signature/read-back/abort stages, but is not a runnable release manifest while the pair is unset and prototype Mainnet validation guards remain. No invented setter or old compromised controller is used. The 2×/take-profit milestone is only recorded for after the pilot is live.

Reproduction from the accepted baseline artifacts: `prototypes/cl-stage1-live-pairs/README.md`; prepare into fresh directories, build via its isolated builder, capture, apply private fixtures with `apply-private.py`, reconcile with `summarize.py`, and replay exact gates with `replay-gates.py`. Existing attempt directories must not be overwritten. The evidence archive preserves final sources, public WASMs, changed build artifacts, transaction prestates and failed/final logs. Private signed envelopes are deterministic test-fixture evidence only, never Mainnet or user keys.

The separately completed previous-checkpoint baseline CI is [run42998883283](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/42998883283): checker-testsPASS, checkpoint-gatesFAIL at both-profile execution, evidence uploadPASS. Jobs and step identities are pinned in `previous-checkpoint-ci-jobs.json`. It does not execute this dual-pair private prototype. Historical storage/restoration/closed-Option1 workflows also failed; they are not new resource qualification. No aggregate CI failure is relabeled as success.

Durable execution payload:720members,1,604,004compressed bytes, SHA256`29a670c9e676908d63a82dcd7939e8bbd60e230812c96acc09d39991df9079de`. The offline verifier checks every member, not just the outer archive. Source freezing is retrospective and explicitly labeled; earlier erroneous builds and all corrective patches remain present.
