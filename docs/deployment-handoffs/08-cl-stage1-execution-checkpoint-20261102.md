# CL Stage 1 execution checkpoint — 2026-11-02

**Incomplete, validation only. No production approval.** The fixed-address setup defect is repaired. The compiled V3 pipeline passes begin-open, then fails the resource gate during its real pool swap/deposit composition. Ordinary fixture failures were corrected; limits and authorization checks remain enabled.

Branch: `feat/cl-share-stage1-checkpoint`, based on `leveraged-fix` at `47858c1a13c2588970395faa2212e7b68bd33060`. The prior work, configuration and original failing command/output were preserved first in WIP commit `2d92318d9af7d60e8dde7986edfa4b8a07f233d4`. Its evidence was moved intact into this repository-root deployment-handoff directory. Unrelated rollout scripts were excluded.

The lead used the Stellar Soroban skill and integrated four parallel work streams with separate file ownership. Shared policy was read from `docs/deployment-handoffs/00-shared-risk-model-and-rollout.md` in the reviewed checkpoint; it was not overwritten or changed. Handoffs 03 and 05 were not implemented.

## Reconciliation with PR #74

Reviewed [PR #74](https://github.com/PeridotFinance/Peridot-Soroban/pull/74), exact head `8375723a38c2f21b044f080e73c0f60944a5ee31`, against the preserved WIP. Both are unapproved candidates.

- Preserved its 27 pure reward-accounting model tests in `proof_model*.rs`. These do not prove the deployable vault’s runtime ledger.
- Adapted its facade authorization fixture to this candidate’s initialization/quote ABI. Added wrong-recipient authorization, retained donated-dust and failed-claim rollback cases, and ran it with a compiled facade. The controller and vault in this tier remain native synthetic fixtures.
- Integrated its native funded-settlement prototype behind `funded-liquidation-validation`; default WASM omits the funded/recovery/configuration entrypoints. Validation WASM rejects Mainnet. Added custody bridges and interrupted-open full-repayment recovery; positive settlement remains resource-blocked.
- Retained the candidate’s deployable vault/actual-pool harness and added receipt cash-reserve enforcement. No wholesale cherry-pick of older policy or handoff files was performed.

## Fixture and authorization boundaries

Cargo.lock resolves `soroban-sdk 25.3.1`, host `25.0.1`. The minimal regression reproduces the old G-address failure during **mock setup**, then accepts the generated C-address admin with exact `mock_auths` and rejects missing authorization. It does not prove real account signatures.

Local host: Darwin arm64, Rust 1.96.0, Stellar CLI 26.1.0, protocol 25. `Env::default` uses zero network ID, persistent/temporary minimum TTLs 4096/16 and maximum TTL 6,312,000. Controlled fixture starts at ledger100, timestamp1000. Artifacts use the explicitly synthetic admin `CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM`.

The full tier loads real compiled V3, receipt markets, share vault, facade, swap adapter and lending controller. Aquarius code hash is `12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6` (90,057bytes). Its state is initialized locally with SAC token fixtures, a native fixed-price oracle and a native plane fixture. This is actual venue **bytecode** integration, not a Mainnet state fork, actual issuer-asset test or production oracle proof. `pool-code-source.json` records the public code fetch; artifact hashes are in the checkpoint JSON.

The vault is unlevered. Existing V3 rejects zero borrowing, so its diagnostic Long fixture uses2x and an explicitly **HARNESS-ONLY** maintenance margin250000. Production maintenance margin remains unset.

No V3, facade or vault contract authorization is mocked. Their nested invocations and `authorize_as_current_contract` execute. No blanket authorization or resource-limit bypass was introduced. Narrow getter/init TTL reads removed unrelated footprint without removing the binding checks or debt-state TTL maintenance.

## Results

| Tier | Result | Meaning |
|---|---|---|
| Minimal SDK setup regression | PASS | G failure reproduced; generated synthetic admin works |
| Native vault |39 PASS | Includes27 preserved model tests plus runtime/module/synthetic-principal tests |
| Native facade fixtures |8 PASS,1 ignored | Ignored compiled fixture run separately |
| Compiled facade + native synthetic controller/vault |1 PASS | Both directions, dust, missing/wrong nested auth, claim failure |
| Full compiled stack + actual Aquarius bytecode |3 PASS,2 FAIL | Authorization/cancel,1x core behavior, nonzero rewards pass; custody transfer and V3 swap exceed resources |
| Receipt native regression |192 PASS,1 ignored | Includes10 reserve/context tests |
| Margin existing regression, excluding funded prototype |110 PASS,2 ignored | Does not clear new funded route |
| Native funded synthetic-prestate prototype |5 PASS,4 FAIL | Positive settlement and some rollback/auth attempts exceed footprint |
| Offline funding plan |2 PASS | Actual ABI, durable borrowing disabled |

After successful setup, compiled wrong-caller/controller-impersonation, unauthorized vault/receipt recovery and unauthorized pending recovery reject; begin-open cancellation succeeds. Missing nested authorization, wrong amount and wrong recipient reject in the **compiled facade/synthetic controller** tier. Funded missing-nested-auth encounters an auth error but exceeds101entries: it is **not** counted as a completed authorization proof.

Reached V3 stages: spot-to-margin, begin-open (99entries), then real swap/deposit composition. Activation and closing were not reached. The independent receipt test deposits shares into custody, then hits114entries on beneficial ownership transfer; its subsequent margin-custody ownership proof was not reached.

## Resources and fee estimates

`check_budget.sh` was run for the compiled report, funded report and reserve report. All report nonzero exit. The unchanged conservative local gate is100M CPU,40MiB memory,100total entries,50writes,200,000disk-read bytes and132,096write bytes. Entry totals include disk reads, memory reads and writes.

| Executed flow | CPU | Memory bytes | Entries | Modeled resource fee, stroops | Result |
|---|---:|---:|---:|---:|---|
|1x deposit |104,001,811|15,040,651|64|15,777,938|Behavior passes; CPU gate fails|
|1x redeem |139,904,484|14,150,763|59|369,498|Behavior passes; CPU gate fails|
|Emergency exit |123,678,340|10,757,854|62|273,945|Behavior passes; CPU gate fails|
|Receipt custody deposit |29,125,896|14,003,924|89|2,037,362|PASS|
|Receipt ownership transfer |31,259,126|12,389,605|114|1,379,000|Footprint FAIL|
|V3 begin |26,383,896|20,910,740|99|168,733,058|PASS, only1entry headroom|
|V3 swap-open |165,700,047|47,596,008|159|18,368,097|CPU/memory/footprint FAIL|
|Cancel before execution |10,342,633|3,161,519|87|222,287|PASS|
|Killed-claim emergency exit |125,669,900|12,196,163|65|616,821|Behavior passes; CPU gate fails|

Committed new text logs normalize trailing whitespace only; original local logs remain in `target`. Raw reports include write counts/bytes, disk bytes, events and rent ledger-bytes for every measured attempt. Failed-invocation numbers are attempted resource costs, not fees paid or proof of committed state. WASM sizes are additional metadata only.

Public network settings were captured read-only at ledger65,264,375, protocol29. Current network footprint/CPU limits differ from the conservative checker; the checker was not relaxed. Host25 quantities are repriced using that captured configuration, including modeled rent. These estimates exclude envelope/history/base fees and return-size overhead. The large begin estimate includes fresh long-TTL state in this artificial fixture. **No RPC transaction simulation or full live fee quote is claimed.** The host/protocol and initial-state differences prevent network-equivalence claims.

## Reward conservation

Actual pinned pool, one controlled in-range state: pending view22,732 = actual claim22,732 raw reward units. Holder paid22,731;1unit remains backing precisely checked holder fractional/index rounding. The equality check includes scaled fractions, pending, quarantine and rounding remainder. This single case does not establish equality across range, emission or rounding transitions.

Killing actual pool claims rejects normal transfer with named error801. Emergency `accept_reward_waiver=false` rejects with named error802; `true` releases principal. Runtime pending estimates remain unproven. The unresolved interval is recorded conservatively; original-owner waiver redistribution is **not implemented**.

Native custody conservation: received250 = Alice175 + Bob25 + quarantine50; custodians receive0. Native outage tests preserve borrower history without forced waiver. Native proportional principal/fee fixture pays each holder700/1300 and excludes donated50/60. These are not substitutes for the resource-blocked compiled custody/liquidation proofs.

## Reserve dependency and funding preparation

The previously missing receipt ABI now includes `set_min_cash_reserve_bps`, `get_min_cash_reserve_bps`, `get_borrowable_cash`, `assert_borrowing_reserve`, `set_cash_borrowing_enabled` and its getter. Optional append-only configuration selects local-cash borrowing, excludes donations/protocol claims, prevents synchronous strategy borrowing, and enforces reserve/target allocation. Existing unconfigured markets retain their behavior. Exits below the floor remain available; short strategy proceeds roll back.

The preparation script uses implemented entrypoints and explicit rate-model parameters. Borrowing remains durably disabled; the controller’s expiring pause is only an additional guard. It performs build/unsigned-plan preparation only. Complete-open reserve enforcement, deployed ABI/readbacks and deployment/funding lifecycle budgets remain release dependencies. There is no Upshift binding, deployment, activation or maintenance-margin setting.

CL receipt custody requires a deployed WASM custodian,1:1 empty nonborrowing collateral configuration and immutable controller binding. Recovery contexts bind owner/recipient/amount/operation/ledger. Authorized controllers can replace an orphan only in a later ledger; current-ledger contexts cannot be overwritten.

## Reproduce

From `peridot-contracts`, with the repository’s keeper Node dependencies installed:

```sh
node scripts/fetch_cl_pool_fixture.cjs
node scripts/capture_cl_network_config.cjs
bash scripts/run_cl_share_checkpoint.sh

CL_RESOURCE_REPORT="$PWD/target/cl-share-checkpoint/funded-resources-final.jsonl" \
  cargo test -p margin-controller --features funded-liquidation-validation funded_prototype -- --nocapture
bash scripts/check_budget.sh --local-report target/cl-share-checkpoint/funded-resources-final.jsonl

RESERVE_RESOURCE_REPORT="$PWD/target/cl-share-checkpoint/reserve-resources-final.jsonl" \
  cargo test -p receipt-vault reserve_test -- --nocapture
bash scripts/check_budget.sh --local-report target/cl-share-checkpoint/reserve-resources-final.jsonl
python3 scripts/tests/funding_plan_test.py
```

The first two commands only read public code/configuration. Re-fetching settings produces a new ledger pin and potentially different estimates. Remove/truncate only a local resource report before repeating a run; the principal runner does this automatically. All WASM compilation uses `stellar contract build`. Native tests use `cargo test`.

## Remaining proofs and stop reason

Resource failures are genuine composition blockers, not setup panics. A reviewed staged-execution/resource design is needed for V3 swap and receipt reward transfer; funded settlement also exceeds139–147entries. Gates were neither disabled nor replaced with placeholder flows.

Still required: compiled activation/close and interrupted-open/post-swap recovery; positive funded atomic settlement and repay-release; compiled borrower reward preservation; range/emission/rounding equality; runtime proven-entry shortfall/excess reconciliation; waiver redistribution to original interval owners; full SEP41 allowance hooks; TTL/archive, capacity/range and account-wide oracle validity proofs; complete reserve integration and deployment readbacks; actual asset/live-state and full transaction resource validation. Stage1 remains incomplete.

No network signing/broadcasting, secret/key access, Mainnet transaction, Upshift binding or production maintenance-margin change occurred. Nothing in this checkpoint authorizes production rollout.
