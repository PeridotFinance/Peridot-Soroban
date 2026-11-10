# Stage 1 private signed application evidence

Research only, based on accepted `3188ae05`. No production contract, policy or
financial-stage changes. PR75 stays draft; Stage1 is incomplete. Option1 is closed.
This fixture may sign **only deterministic public test accounts**, for the literal
private passphrase in `accounts.cjs`. It contains no RPC or wallet-configuration
access. Core remains standalone/offline, without SCP/overlay, and its existing
`CL_CORE_FORBID_SIGNING=1` guard remains mandatory. Core verifies external fixture
signatures; it cannot produce node/account signatures itself.

Nine independently captured financial prestates, three detached G-address-v1
variants, and two same-ledger cold preparation journeys passed actual Core29
application. See handoff26 and its durable evidence. The financial WASMs are the
unchanged accepted cash-first buildv4. Existing synthetic owner C authorization is
replaced with a real fixture G account and authorized classic trustlines. Test-only
SDK native-authorizer pseudo-code is excluded from Core import. Other real contract
calls and `authorize_as_current_contract` remain enforced.

## Evidence and limits

- CPU100M, memory41,943,040 bytes, SDK total100, unique Soroban keys100, writes50.
  SDK total = memory reads + disk reads + writes. The previous unsigned report
  mislabeled memory reads as SDK totals; its actual SDK/checker enforcement was
  already correct. Native fee/sequence bookkeeping and TTL keys are distinct from
  the declared Soroban footprint. Core actual writes may be less than declared RW.
- Core official v29.0.0 (`a9d72b0cac3a89ebbcf926af449ac73089e0f8f7`), host29.0.0;
  host preflight/SDK29.0.1 are separately reported. Use the maximum of measured
  Core/preflight/SDK CPU and memory, never claim the versions are identical.
- Source-envelope signature verification is native Core work outside Soroban CPU.
  Detached account signatures are metered inside the host. Native timings are
  diagnostic local timings, not throughput bounds. No multi-signature/custom-account
  upper bound is established.
- Financial envelopes reserve100M instructions. Warm maximum fee1XLM; cold maximum
  is computed from measured preflight fees plus10% and a small fixed allowance.
  Actual Core charge/refund is recorded. A reserve is not consumed CPU or paid rent.
- Warm rows are independent prestates, not one continuous signed lifecycle. Cold
  rows restore touched existing persistent code/instances/data from a controlled
  hot-archive genesis. Native RestoreFootprint has no Soroban CPU/memory reading.
  Its full signed byte/entry/read/write/rent fees are independently reconciled with
  Core. It is not labelled zero native CPU/memory.
- After each native restore, exact data, installed TTL, source debit/sequence,
  feePool/totalCoins and replay rejection are checked. Core remains alive while
  actual post-restore state is exported, re-simulated, signed and applied. No keeper
  pre-restoration, state reconstruction or unsigned application substitution.
- Invocation gates pass; existing lifecycle/transaction/checkpoint gates still fail.
  Generic transaction checker expects its existing unsigned schema and has not been
  silently changed to approve these signed experiments. Full accounting parity,
  outage mixture recovery, stressed histories, interrupted/re-expired preparation,
  intervening trades and live-state/capacity remain separate proofs.

## Reproduction

Run from `peridot-contracts`. Restore accepted part(b) evidence/buildv4 and the pinned
Core29 build described in handoffs22/25 first. Existing local Core objects are reused
only to relink this isolated test. `build-core.py` records compile/link arguments and
binary SHA; no production WASM is rebuilt or deployed. Archived input paths reference
this workspace; relocate those explicit fixture paths if reproducing elsewhere.

Warm input generation starts from the two complete accepted harness journeys:

```sh
python3 prototypes/cl-stage1-signed-local/prepare.py
# Existing harness: capture the two full Stage1 journeys with CL_PRIVATE_CAPTURE_DIR.
# Export those snapshots with signed_local::export_existing_snapshots_as_xdr.
node prototypes/cl-stage1-signed-local/accounts.cjs CAPTURE_DIR FRESH_INPUT_DIR
bash prototypes/cl-stage1-signed-local/run-host.sh FRESH_INPUT_DIR/stage_invest/input.json FRESH_RECORD_RUN
node prototypes/cl-stage1-signed-local/sign-local.cjs FRESH_INPUT_DIR/stage_invest
bash prototypes/cl-stage1-signed-local/run-host.sh FRESH_INPUT_DIR/stage_invest/enforce-input.json FRESH_ENFORCING_RUN
python3 prototypes/cl-stage1-signed-local/build-core.py FRESH_CORE_BUILD
```

For exact replay, the durable archive contains all12 final warm input packets,
envelopes, captured snapshots, enforcing host/SDK results, gate summaries and Core
logs. Run an input with the built private test binary:

```sh
CL_CORE_FORBID_SIGNING=1 \
CL_STAGE1_PRIVATE_CORE_INPUT="$PWD/target/cl-stage1-signed-local/inputs-v2/stage_invest/core-input.txt" \
target/cl-stage1-signed-local/core-v6/stellar-core-private test \
  '[cl-stage1-private-signed]' --version 29 --rng-seed 20261110 --ll warning
```

Cold runs require a fresh directory. Repeat with `stage_withdraw_in_kind` for the
separate exit probe. The callback performs offline host recording, public-fixture
signing, enforcing preflight/SDK execution, and `check_budget.sh` before final Core
application. It aborts on any failed invocation check.

```sh
node prototypes/cl-stage1-signed-local/cold.cjs \
  target/cl-stage1-signed-local/inputs-v2/stage_invest \
  target/cl-stage1-signed-local/cold-invest-REPLAY
CL_CORE_FORBID_SIGNING=1 \
CL_STAGE1_PRIVATE_CORE_INPUT="$PWD/target/cl-stage1-signed-local/cold-invest-REPLAY/core-input.txt" \
CL_STAGE1_PRIVATE_COLD_DIR="$PWD/target/cl-stage1-signed-local/cold-invest-REPLAY" \
target/cl-stage1-signed-local/core-v6/stellar-core-private test \
  '[cl-stage1-private-signed]' --version 29 --rng-seed 20261110 --ll warning
python3 prototypes/cl-stage1-signed-local/summarize.py
python3 prototypes/cl-stage1-signed-local/summarize-cold.py
python3 prototypes/cl-stage1-signed-local/replay-gates.py target/cl-stage1-signed-local/NEW_GATE_REPLAY
```

The summarizers intentionally pin the accepted run directories; they fail on missing
rows. Do not rename a new failure over an accepted result. Old module-cache/bootstrap
failures and the initial cold insufficient-rent-fee failure remain preserved.
The warm source freeze is an honest **post-run identity check**, not a retrospective
claim that run-time source attestation existed. Cold source/commands and all input,
output, signed-envelope and log identities are archived separately.

No Stage1 build plan is issued here. Signed resource feasibility is a milestone,
not recovery, accounting, deployment or release qualification.
