# Shared eight-borrower recovery measurement — 2026-11-03

The captured-network profile completed **eight sequential, independently funded expired PendingOpen recoveries from one joint state**. The internal-policy profile rejected its first recovery and applied none. Stage 1 and both complete checkpoint gates remain failed. This is an isolated, unadopted recovery prototype; no production policy, contract, stage, borrowing configuration or maintenance margin changed.

The fixture has 64 initial holders, 128 additional real share transfers, **391** initial historical checkpoints, eight swapped PendingOpen debts and eight distinct synthetic funders. The active-position fixture's 407 checkpoints do not apply here. It uses real compiled V3/receipt/vault and the pinned Aquarius bytecode with controlled state, a compiled synthetic oracle, generated external administrators/funders and narrowly scoped external authorization. Controller/vault contract authorization remains real. One continuous actual pool User-record fault causes both reward claim and pending-view calls to fail through all eight recovery boundaries; it is not eight independent outage intervals.

The exact corrected six-overlay V3 is `ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482`, 100,419 WASM bytes. Host/SDK is 29.0.1; protocol 29 cost models and ceilings are pinned at ledger 65270655, network-config SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`. Build `target/cl-option1-shared-build-v2`; evidence `target/cl-option1-shared-v2`. One compiled test passed in 582.55 seconds under local CPU/memory contention; this is not network latency.

## What was executed

Warm discovery applies each official host result, including explicit deletions and TTL changes, to a disposable sequential copy of the same original state. It also executes the SDK root on that exact prestate to collect observed persistent/code dependencies, including native authorization setup. The restoration union is archived once on a fresh copy of the original state. Eight native RestoreFootprint batches project restored TTLs, then eight financial roots run sequentially with those financial changes and TTLs carried forward. There is no merge of independently mutated snapshots and no sum of independent cold-probe fees.

Each accepted root compares actual SDK and official-host return values, whole contract data/code and events. Only explicitly listed fresh temporary mock nonce pairs for the same declared external funder may differ. Complete deletion-aware host projections must match the host's poststate hash. Previously recovered borrowers remain debt-free; the remaining borrowers retain pending states and positive debt. Each current borrower's booked rewards and historical records are preserved without a new waiver; the new funder gets no older entitlement. Each root also passes exact scaled reward conservation and reward-token backing checks on separate assertion clones.

The union contains **214 unique original keys**, versus 592 summed per-root key memberships: 54 keys are used by all eight roots and 160 by one root each. Official-host union and combined host/SDK-observed union are equal here; zero SDK-only keys remain outside preparation. All eight financial roots have zero SDK disk reads and empty official-host automatic-restoration indices. SDK observed storage-map keys are not an exact access-mode footprint, and their count must not replace the SDK invocation resource-entry total.

## Resources and gates

| Measurement | Captured network | Internal policy |
|---|---:|---:|
| Completed recoveries | 8/8 | 0/8; first rejected |
| Maximum SDK instructions | 67,797,501 | 66,476,858 at first rejection |
| Maximum SDK memory | 13,560,832 bytes | 13,250,965 bytes |
| SDK resource entries | 152 | 152, exceeding 100 |
| Writes | 50 | 50 |
| Maximum host instructions | 72,114,656 | 70,778,637 |
| Maximum host memory | 13,536,835 bytes | 13,222,066 bytes |
| Unique host footprint | 103 | 103, exceeding 100 |
| Invocation gate | PASS | FAIL |
| Lifecycle / transaction / checkpoint | FAIL / FAIL / FAIL | FAIL / FAIL / FAIL |

Each financial envelope is 9,792 unsigned bytes; maximum write bytes are 11,400 and return/events resource bytes 1,808. Every native batch was independently decoded and checked against its profile, full fee/rent arithmetic and captured module-memory cost coefficients. Native RestoreFootprint declares zero Soroban instructions and exposes no native memory meter; that is not a measurement of zero Core CPU or memory.

The captured checkpoint retains 46 missing required flows, two lifecycle errors and 46 transaction errors. Internal retains 46 missing flows, three lifecycle errors and 47 transaction errors. Both full checkpoint commands exit 1. The successful bounded Rust test and captured invocation gate are not Stage 1 or CI release approval. The original v1 compile error (non-exhaustive diagnostic executable classifier) and its failed gates remain preserved.

## Shared costs and synthetic authorization costs

These additive per-key amounts include entry rent and the fixed TTL-entry write fee. Batched TTL write-byte rounding and ordinary transaction overhead stay separate, so no component is counted twice. Every key, code identity, byte size, modeled rent size and category is retained in [shared-results.json](shared-results.json).

| Restoration category | Keys | Serialized entry bytes | Modeled rent bytes | Assigned rent + TTL-entry writes (stroops) |
|---|---:|---:|---:|---:|
| Shared code | 7 | 439,488 | 9,678,636 | 5,377,028,753 |
| Shared instances | 10 | 7,948 | 7,948 | 13,259,170 |
| Shared data | 37 | 4,944 | 4,944 | 8,286,261 |
| Position-specific data | 144 | 33,920 | 33,920 | 56,713,376 |
| Synthetic funder code | 8 | 1,120 | 1,089,656 | 605,374,448 |
| Synthetic funder instances | 8 | 832 | 832 | 1,396,672 |

“Position-specific” means observed in only one recovery in this fixture; it does not establish a permanent schema partition. Shared code comprises Aquarius, vault, controller, V3, interest model, compiled fixture/oracle and the common receipt code. Native mock funder WASM and instance identities are explicitly linked in the raw executable records. The exact restoration-rent marginal from removing **both** sets of synthetic funder entries is 606,771,448 stroops (60.6771448 XLM), including the resulting batch-rounding difference. The earlier raw report's 1,396,836 marginal is **instance-only**. Gross measured fees retain all synthetic costs. Subtracting these entries does not establish production account-signature or alternate-payer cost.

| Fee component | Preparation, 8 batches | Financial roots, 8 calls |
|---|---:|---:|
| Non-refundable | 800,458 | 886,876 |
| Refundable | 0 | 35,320 |
| Rent, including TTL overhead | 6,062,063,069 | 71,474,105 |
| Inclusion | 800 | 800 |
| Total stroops | **6,062,864,327** | **72,397,101** |

The preparation rent consists of the per-key amounts above plus 4,389 stroops of batched TTL write-byte fees. The complete 16-transaction unsigned estimate is **6,135,261,428 stroops = 613.5261428 XLM**. An explicitly analysis-only 25% operating-fee contingency gives **766.9076785 XLM**. Neither number includes source-account reserve, real signatures, retries, actual elapsed-time re-expiry, live-state changes, or liquidator debt-asset principal; it is not an all-in deployment seed or capacity bound.

Each synthetic funder authorizes a 27,500,000-raw maximum, pays 25,000,321 raw after interest and receives the 2,499,679-raw refund. Across eight calls: authorized maxima 220,000,000; net debt repayment 200,002,568; refunds 19,997,432. With the synthetic SAC's seven decimals these are 22, 20.0002568 and 1.9997432 test debt-asset units. Each account was independently prefunded with 300,000,000 raw as fixture setup; that oversized setup funding is not a measured required capital floor. No live Circle-USDC identity, supplier capacity or profitable liquidation assumption follows.

## Reward result and limits

Final received 100,671 = paid 0 + pending 100,632 + physical quarantine 0 + fraction/rounding backing. At scale 1e18, fractions are 38,999,999,999,973,007,512 and rounding is 26,992,488; the equation holds exactly for 74 observed holder records including exited/custody/new funder records. Booked borrower entitlement remains unchanged through every accepted recovery. This scenario does not deliver later rewards: physical quarantine zero does not establish zero unobserved accrued entitlement. Missing-boundary attribution and later reconciliation remain distinct proofs.

All eight host roots are unsigned recording executions followed by SDK authorization/state checks. Synthetic sequence-1 templates are not a sequence of applied Core transactions. Real Core restoration metadata, real account signatures, live venue/oracle state, actual re-expiry and economic incentives remain unqualified. The warm SDK dependency helper retains a legacy post-preparation scope string; its `discovery.jsonl` / `shared_sdk_discovery` records are explicitly warm offchain discovery and are excluded from accepted financial costs. No source or policy changes are inferred from this evidence.

## Reproduction

From the repository root, after producing the corrected joined six-overlay build:

```sh
python3 prototypes/cl-option1-qualification/prepare-shared.py \
  --build target/cl-option1-joined-build-v2 \
  --output target/cl-option1-shared-reproduce-build
bash prototypes/cl-option1-qualification/run-shared.sh \
  target/cl-option1-shared-reproduce-build \
  target/cl-option1-shared-reproduce-run
```

Use a fresh output path; the runner intentionally exits nonzero for incomplete checkpoint gates. It runs the one compiled case, preserves both resource profiles, validates native batches and shared-chain evidence with [shared-verify.py](shared-verify.py), and runs `check_budget.sh --gate checkpoint` independently for both profiles. The original measured run's exact sources, manifest, unsigned envelopes, snapshots, proof map, failed gates and helper hashes remain under the evidence directory above. No additional execution was performed while preparing this report.

## Bounded read-only Closing review

The corrected final Closing root rejects a live zero-top-up close at `timestamp <= expiry` before applying proceeds or waiving the contingent fee. Funded recovery uses the position's own proceeds first, sizes permitted seizure only from the external contribution, checks debt is zero, then delivers collateral/residual assets. No additional concrete defect was found in this review.

That review does not upgrade coverage: the six recovery shapes are Long cases; the nonzero-fee test adds a ninth active position, and does not close every original position. The 30-year timestamp jump with only 20 ledger ticks isolates residual-debt behavior rather than realistic expiry/restoration costs. The successor test covers before deadline and deadline+1, not equality. Short cases, the full pause matrix, real custom-account/reentry behavior, failed collateral delivery after repayment and target-Core application of the financial snapshot remain separate unexecuted proofs. No extra execution was performed for this review.
