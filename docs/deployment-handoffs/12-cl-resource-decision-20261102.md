# Resource decision memo — analysis only, 2026-11-02

Stage 1 remains incomplete; PR #75 remains draft. No limit, policy, transaction stage, claim-first boundary or production guard changes are proposed for immediate implementation. The captured-network diagnostic succeeds for the preserved controlled-state workloads; the existing internal policy still fails. The decision is whether to fund a substantial venue/accounting optimization under the current caps, or separately consider a bounded resource policy after the missing cost evidence is obtained. Neither option currently supports release approval.

## Evidence and origin of the limits

The first CL local checker introduced 100,000,000 instructions, 100 entry total and 50 writes at `2d92318d9af7d60e8dde7986edfa4b8a07f233d4`, `peridot-contracts/scripts/check_local_budget.py:18–21`. It also checked 40MiB memory, 200,000 read bytes and 132,096 write bytes. Its comment required missing coverage to fail. The commit and checker do not document a quantitative risk, fee or headroom rationale for those three numbers. The prior `check_budget.sh` was an unsigned RPC reporting helper, not this numeric local gate. No numeric 100M/100/50 requirement was found in authoritative shared 00; it requires complete resource proofs and claim-first accounting.

`64093aa3c0f77ab510ebf110138195405bb0d430` added disk reads to the entry total while retaining the caps. `14ef4d813b476a0c06bd9a9f9792834b29b3f03a` named the existing numbers `internal-policy`, added `captured-network-diagnostic`, and applied `min(policy, captured network)` to every policy field. See `scripts/cl_resource_profiles.py:7–25,27–47` and `docs/deployment-handoffs/09-cl-resource-reconciliation-20261102.md:14–16` at that commit. Their documented continuing purpose is a conservative, separately enforced gate while reconciling runtime/network settings. That is a defensible description of current intent, not evidence that the original author derived a specific safety margin.

The numerical resemblance is to a **mixture of defaults**, not a single verified network snapshot:

| Source inspected | Instructions | SDK entry total | Writes | Meaning |
| --- | ---: | ---: | ---: | --- |
| host 25.0.1 `src/budget/limits.rs:43–46` | 100,000,000 | — | — | Host budget default; source explicitly directs embedders to customize to network configuration |
| SDK 25.3.1 `src/testutils/cost_estimate.rs:133–150` | 600,000,000 | 100 | 50 | Hardcoded invocation limits; source says these are not dynamically fetched |
| SDK 29.0.1 same file, lines 138–158 | 400,000,000 | 400 | 200 | Source describes a hardcoded snapshot dated 2026-07-10 |
| Captured ledger 65,264,920, protocol 29 | 400,000,000 | 400 unique transaction keys | 200 | Hash-pinned chain settings used by the current evidence |

Thus 100M is **not** SDK 25.3.1's invocation CPU ceiling. It matches the host default, while 100/50 match that SDK's entry/write defaults. Copying these defaults is a plausible inference, not documented provenance. SDK comments alone do not independently establish historical network state. The earlier capture at ledger 65,264,375 had the same four headline network ceilings; this analysis uses the newer complete capture and its full cost model, SHA256 `a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd`.

Primary SDK/host files were inspected locally and byte-compared with their cached crate archives. Crate SHA256 identities: SDK25.3.1 `4502f2e018f238a4c5d3212d7d20ea6abcdc6e58babd63b642b693739db30fd1`; SDK29.0.1 `6a3b97a76c88f413859bb670903b63af54d7ddb03303a5bd585fee8b9f2a3d8f`; host25.0.1 `43af5d53c57bc2f546e122adc0b1cca6f93942c718977379aa19ddd04f06fcec`. These can be reproduced by reading `src/testutils/cost_estimate.rs` or `src/budget/limits.rs` inside the respective crates, without fetching current documentation or treating it as historical evidence.

`budget().reset_unlimited()` removes one host execution ceiling; it does not disable SDK invocation-result checks. Current acceptance evidence uses explicit selected limits and protocol29 host execution with the captured coefficients. Profiling/shadow quantities are separately labelled and do not replace acceptance checks. Repricing host25 counters with protocol29 fees or setting a ledger-version field would not establish protocol29 execution.

## Measurements, including 1x deposit and redemption

The new Linux artifact is run [41449397462](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41449397462), source `ac6325589a0095084c2df641d9b743860b4795f8`, merge `904f2aa7a106dbcfe6d3af5c392dec434ab9a143`. Artifact 13146343183 ZIP SHA256 is `33dcb99170894384466ebe6ac9d0e3d2fc95dfeaec599fc2066eb682f6a56701`. Its durable payload SHA256 is `3f1496b938090f43ef248c719a938da8ec7a4705d762fd70380c6dd630f8fd5f`. The earlier Linux archive remains separate. The local ac63255 payload SHA256 is `d7edc70705e23ccd14fd841bf6471c2d8be2f29acb4f41d5517a3e268a53e612`. Linux and local WASM identities differ and are retained separately in [the machine analysis](evidence/cl-resource-decisions-20261102/evidence.json); neither is substituted for the other.

Both complete artifacts contain 1,490 SDK invocation rows and 2,562 unsigned transaction records. Their captured profiles have invocation/lifecycle/transaction/checkpoint gates all PASS and no errors. Their internal profiles have all four gates FAIL, 97 lifecycle errors and 284 transaction errors. The script verifies these individually, not aggregate exit 1. These are diagnostic/checkpoint results, never release approval. Full CI test completion and replay output are recorded in the companion durable CI index.

The following values are independent maxima across the two builds. SDK and unsigned transaction columns describe **separate sample populations**: SDK labels name proof cases; transaction labels name ABI entrypoints. For example funded-liquidation transaction maxima include every successful `liquidate_funded_v3` test and restoration variation, not only the SDK row labelled `funded_liquidation`. Maxima across columns need not co-occur.

| Flow | SDK CPU | SDK memory bytes | SDK entries | SDK writes | Unsigned CPU | Unique tx keys |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| deposit_underlying | 166,914,792 | 14,418,955 | 70 | 22 | 166,680,464 | 49 |
| redeem | 139,752,097 | 10,640,393 | 65 | 19 | 139,966,854 | 47 |
| receipt_custody_ownership_change | 34,389,841 | 6,595,879 | 124 | 25 | 37,442,914 | 100 |
| swap_open_position_v3 | 235,456,785 | 24,514,601 | 171 | 45 | 236,694,861 | 128 |
| activate_open_position_v3 | 81,227,898 | 13,326,755 | 182 | 40 | 85,934,083 | 144 |
| prepare_close_position_v3 | 53,765,018 | 11,093,755 | 120 | 28 | 55,499,700 | 93 |
| swap_close | 184,219,797 | 19,901,018 | 175 | 41 | 188,845,870 | 135 |
| finish_close | 58,856,717 | 6,824,744 | 97 | 30 | 58,696,465 | 67 |
| funded_liquidation | 100,065,893 | 14,591,435 | 213 | 56 | 112,983,372 | 158 |
| repay_release | 13,741,304 | 3,099,376 | 88 | 18 | 15,286,224 | 71 |
| history_scale_64_owner_exit | 138,682,384 | 14,125,779 | 75 | 24 | 132,860,630 | 53 |
| history_scale_64_owner_settle | 5,037,430 | 2,210,162 | 16 | 6 | 4,286,577 | 12 |
| history_scale_8_funded_fault_exit | 98,041,813 | 15,080,366 | 197 | 50 | 112,983,372 | 158 |

SDK entry totals are `memory_read_entries + disk_read_entries + write_entries` (host25 invocation-metering source lines 158–162 and 396–399); read/write accounting can count a key in more than one category. The transaction footprint is the **set union of serialized read-only and read-write LedgerKeys**. They are not interchangeable, and neither repeated getter call counts nor SDK totals can be called unique transaction keys. For example activation reaches 182 SDK entries while its successful unsigned entrypoint population reaches 144 unique keys. Keep a separate SDK diagnostic guard and actual unique-key gate.

Successful restored simulations number 726 in each artifact; 733 records observe restoration when expected rejections are included. Across successful restored transactions the maxima are 171,514,131 CPU, 18,758,943 memory bytes, 157 unique keys, 57 writes, 12 disk reads, 2,580 disk bytes and 14,764 unsigned envelope bytes. Across all successful simulations maximum total fee is 22,845,643 stroops, rent 22,686,852, non-refundable 162,160, refundable 15,147 and inclusion 100. These component maxima do not necessarily occur together and must not be summed. Restored-state inclusion is material even when CPU remains below the overall maximum; rent dominates the largest cost. These are controlled-state prices/TTL assumptions, not a funded production operating budget.

All-success maxima additionally include 19,236 write bytes, 15,092 unsigned envelope bytes and 532 authorization-template bytes. Real signature bytes and account-signature verification are absent. Recording authorization templates is not execution of real account signatures. Transaction-source account checks, nonces, trustlines and contract-wallet authorization can add keys, bytes and execution, depending on the eventual production authorization model; they cannot be assumed zero. No signing is authorized or performed in this checkpoint.

## Option A: retain 100M / 100 SDK entries / 50 writes

Keeping these limits requires changes to the measured venue path, not only wrapper cleanup. In the peak successful traces, the disjoint unchanged Aquarius state-changing subtrees alone cost approximately:

| Path | Current full SDK CPU, worst build | Unchanged venue subtree CPU, worst build | Full-stack reduction needed to reach 100M |
| --- | ---: | ---: | ---: |
| Direct deposit_underlying | 166,914,792 | 134,354,879 | 66,914,792 (40.1%) |
| Direct redeem | 139,752,097 | 126,765,769 | 39,752,097 (28.4%) |
| swap_open_position_v3 | 235,456,785 | 147,935,537 | 135,456,785 (57.5%) |
| swap_close | 184,219,797 | 130,983,173 | 84,219,797 (45.7%) |

These are measured floors **only if those venue subcalls are preserved**, not an impossibility theorem for a different venue implementation. Each exceeds 100M even if all other code were free. Staying at 1x does not solve deposit/redeem. The earlier trace's unrealistic deletion of every repeated getter saved only about 7.03M for swap-open, and some of those reads straddle mutations and cannot safely be deleted.

A concrete engineering route would require all of:

1. Design and benchmark a reviewed fee/position operation that avoids duplicate Aquarius fee-growth/tick/position passes. The current redeem peak spends 52,903,433 CPU in `claim_all_position_fees` and 62,219,427 in `withdraw_position`, in addition to claim and swap. A fused path is a research candidate; deleting fee collection outright would violate the requirement that swap fees enter NAV before share burn. It must establish actual received fees before pricing the shares, preserve fees for partial-exit co-owners, and prove identical output/slippage/rounding under range crossings. Deposit needs the same pre-mint fee ownership guarantee. If the pinned venue ABI cannot expose a safe combined operation, a reviewed new venue ABI/bytecode and renewed equivalence proofs are an explicit dependency. No saving is credited before measurement.
2. Preserve actual reward claim calls at deposit, share transfer and custody assignment. Do not merge those three ownership boundaries based on same-transaction timing or a pending estimate. Account/history updates can be optimized only with exact reward-conservation, outage, exited-beneficiary, waiver and custody proofs preserved. The proposal is to reduce internal work in each call, not remove calls.
3. Reduce the **number of touched accounting/configuration keys** in receipt/controller/custody paths, alongside CPU. Funded liquidation reaches 213 SDK entries and 56 writes (unsigned 158 unique keys/57 writes); activation 182 SDK entries; receipt ownership transfer 124. Caching repeated reads does not necessarily remove first-touch keys. Candidate changes include immutable-within-invocation configuration snapshots and a versioned compact representation for frequently co-read immutable settings. Re-read any mutable valuation or authorization dependency after its mutation. Packing mutable records needs migration, entry-size/rent/write-amplification/concurrency and restoration measurements; it is not a free reduction.
4. Retain bounded per-beneficiary historical settlement rather than scanning all holders during forced recovery. Even zeroing all historical settlement work cannot by itself erase the venue floor. Funded repayment and permitted collateral seizure remain atomic, and every valuation/authentication check remains inside measured execution.

Acceptance for this option remains all existing limits and proofs, with explicitly bounded stressed/restored workloads and real signed-transaction evidence. No additional transaction stages are assumed necessary, proposed for implementation, or used to move costs outside measurement. The available data does not support promising that wrapper-only optimization can satisfy these caps.

## Option B: separately proposed bounded internal policy — not adopted

A concrete candidate for discussion is 300M instructions, 300 SDK entry total, **separately** 300 unique transaction keys, 100 writes and 32MiB memory. This is a proposed internal envelope, not the captured network profile and not a new runnable profile. All other current caps/checks remain: 100 disk-read entries, 200,000 disk-read bytes, 132,096 write bytes, 16,384 event bytes, 250-byte keys, 65,536-byte data entries, 131,072-byte code entries and 132,096-byte transaction envelopes, including rent/fee accounting, complete lifecycle coverage and missing-evidence failures. Clamp any approved policy to refreshed network ceilings and exact target cost models; never merely switch the release gate to diagnostics.

Its rationale would be a smaller envelope than the verified network ceilings, plus a second reserve within that envelope for workload variation and currently unmeasured overhead. Require the supported worst-case workload, including signed authorization and restoration, to consume at most **80% of each candidate ceiling**. The 20% internal workload reserve is a proposed risk allowance, not an empirically established guarantee. It is separate from the candidate's reserve below network capacity:

| Dimension | Candidate ceiling | 80% workload maximum | Observed worst | Remaining to 80% boundary | Candidate below network |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU | 300,000,000 | 240,000,000 | 236,694,861 | 3,305,139 | 25% of 400M |
| Modeled memory bytes | 33,554,432 | 26,843,545 | 24,514,601 | 2,328,944 | 20% of 41,943,040 |
| SDK entry total | 300 | 240 | 213 | 27 | Supplemental SDK guard; not a network unique-key count |
| Unique transaction keys | 300 | 240 | 158 | 82 | 25% of 400 |
| Writes | 100 | 80 | 57 | 23 | 50% of 200 |

This deliberately tightens memory versus the existing 40MiB while proposing increases in CPU/entries/writes. It is not justified solely because unsigned examples fit. Only 3.305M CPU remains before the proposed workload boundary, so unmeasured real signature/authentication and stressed-state work may invalidate the candidate. The much larger gap between 236.695M and the 400M network ceiling is not available internal headroom under this proposal. If signed/stressed costs exceed the boundary, the decision returns to measured optimization, narrower explicitly enforceable workloads, or another separately reviewed proposal; it does not silently relax reserves.

The workload evidence currently covers distinct domains: 4 holders with a 1% reserve trade, 16 holders with a 5% reserve trade; 64 direct-share owners with 383 history checkpoints, 128 extra ownership changes, 8 outages and 8 exited owners; 8 margin positions belonging to one borrower with 16 missing-boundary faults; and the venue boundary matrix's emission changes, range exit/re-entry, rounding and partial exits. The 4-holder path can cost more than 16 holders because it selects a different position/fee branch. State-size monotonicity is not proved. The Cartesian product of maximum history, all tick/range stresses, many independent borrowers, restoration and real signatures **has not been executed**.

Before approving an alternative policy, specify and enforce maximum active positions/touched records, per-call settlement batches, admissible pool range/tick-crossing and trade-size domains, and supported authorization forms. Retained unresolved history cannot be pruned to meet these bounds; incremental settlement and funded archival retention must keep historical beneficiaries recoverable as total history grows. Stress both successful and reverting paths, archived code/data, ordinary deposit/redeem, close, funded liquidation and full repayment/release. Publish the complete fee/rent funding envelope and remeasure any new reserve or accounting patch rather than reusing the ac63255 measurements as its cost proof.

The decisive remaining gate is **complete signed-transaction execution and serialization in an authorized target-runtime test environment**, alongside the missing bounded-workload crossproducts and unavailable live-state validation. The current no-signing restriction remains intact. This memo does not approve option B or convert any current failure to PASS.

## Reproduction

From `peridot-contracts`, using the preserved new Linux archive directory and existing local archive:

```sh
python3 scripts/analyze_cl_decision.py \
  --linux-evidence ../docs/deployment-handoffs/evidence/cl-linux-ci-ac63255-20261102 \
  --local-evidence ../docs/deployment-handoffs/evidence/cl-feasibility-history-reserve-20261102 \
  --output target/cl-resource-decision-reproduced.json
```

The analysis verifies archive/member hashes, the pinned protocol29 capture, per-profile gates and error counts, runtime/configuration fields, required measured flows and reconciled exclusive CPU trees. It exits nonzero on missing or mismatched evidence. Its success means the **analysis input verification** passed, not either release gate. The full `check_budget.sh` replay and Linux test identities are in the companion CI index; use that replay for the actual existing gates. Historical source inspection is reproducible with `git show 2d92318:peridot-contracts/scripts/check_local_budget.py`, `git show 64093aa:peridot-contracts/scripts/check_local_budget.py` and `git show 14ef4d8:peridot-contracts/scripts/cl_resource_profiles.py`.
