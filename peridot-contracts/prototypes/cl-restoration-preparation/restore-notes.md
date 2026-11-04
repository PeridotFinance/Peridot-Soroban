# Explicit restoration preparation: source audit and evidence requirements

This is a design and API audit, not an executed restore or release approval.
Shared handoff 00 was read first; its accounting, recovery and atomic funded
repayment/seizure requirements remain unchanged. The Stellar Soroban skill was
consulted. No transaction was signed, submitted or broadcast.

## Pinned implementation

The existing compiled harness pins SDK and host **29.0.1**, but its lockfile does
not yet contain `soroban-simulation`. Public crate `soroban-simulation=29.0.1`
was downloaded for inspection into `target/cl-restoration-research/`:

- Crate SHA256: `7fc430d8405a8215197cabcc8ceb6e798f5b80d70dabdd896f1de067dd2c083e`.
- Published VCS commit: `ff6ae01b3465ac44185d2824876491e62c9a4cc4`.
- Published `src/resources.rs` bytes independently match that commit.

The supported API is:

```rust
use soroban_simulation::{NetworkConfig, simulation::{
    simulate_restore_op, SimulationAdjustmentConfig, RestoreOpSimulationResult,
}};
// snapshot implements soroban_env_host::storage::SnapshotSource
let result: RestoreOpSimulationResult = simulate_restore_op(
    &snapshot, &network_config, &adjustment_config, &ledger_info, &keys,
)?;
let transaction_data = result.transaction_data;
```

The result contains **only** `SorobanTransactionData`. It contains no committed
ledger changes, CPU/memory measurements, signature verification or transaction
application result. `NetworkConfig::load_from_snapshot` loads fees, rent,
CPU/memory models and TTL parameters; `fill_config_fields_in_ledger_info` sets
the latter. It does not validate footprint, write-byte, disk-byte or entry-size
ceilings. Those require explicit validation against the same pinned capture.
Sources: [simulation API](https://github.com/stellar/rs-soroban-env/blob/ff6ae01b3465ac44185d2824876491e62c9a4cc4/soroban-simulation/src/simulation.rs#L268),
[network configuration](https://github.com/stellar/rs-soroban-env/blob/ff6ae01b3465ac44185d2824876491e62c9a4cc4/soroban-simulation/src/network_config.rs#L19).

## Restore resources and a fee discrepancy requiring explicit treatment

The helper requires existing archived bytes and TTL for persistent contract
code/data; temporary or missing entries fail. Already-live entries are omitted.
Its footprint is sorted, read-write only. Both disk-read and write bytes equal
the sum of complete serialized `LedgerEntry` sizes. Declared instructions are
zero because this is the native restore operation, not a contract invocation.
This does **not** measure zero Core CPU or memory. Rent uses minimum persistent
TTL and, for code, host `entry_size_for_rent`, including module-memory rent size.
Do not replace that with raw WASM length. Deduplicate requested keys explicitly;
sorting in this helper does not itself establish uniqueness.
Source: [resource construction](https://github.com/stellar/rs-soroban-env/blob/ff6ae01b3465ac44185d2824876491e62c9a4cc4/soroban-simulation/src/resources.rs#L172).

**Confirmed source discrepancy:** the pinned simulation helper passes an empty
restored-index vector into the generic resource-fee calculator. That calculator
counts classic keys plus those indices, hence zero disk-read entries for its
all-Soroban restore footprint. Core v29.0.0 instead explicitly counts every
read-write restore key as a disk read, for both validation and fees. Therefore
preserve the helper's exact output, and separately recompute fees with the
pinned host fee functions using `disk_read_entries = read_write.len()`.
At the preserved capture's 781 stroops/read-entry rate, the omitted component is
781 times that count before other fee adjustments. Do not label the helper fee
as exact Core parity or silently overwrite the discrepancy.
Sources: simulation resource calculation above,
[Core disk-read count](https://github.com/stellar/stellar-core/blob/v29.0.0/src/transactions/TransactionFrame.cpp#L67),
[Core resource fee](https://github.com/stellar/stellar-core/blob/v29.0.0/src/transactions/TransactionFrame.cpp#L1115).

Record nonrefundable instruction/read/write/history/bandwidth components,
refundable event and rent components, TTL overhead, inclusion fee separately,
and total fee. `SimulationAdjustmentConfig::no_adjustments()` still uses the
helper's maximum-envelope estimate: its template includes 20 dummy signatures,
muxed accounts and maximal preconditions. Those are size assumptions, not
measured signatures. Serialize the actual zero-signature envelope independently,
measure its bytes, and retain both estimates. Never turn dummy signature bytes
into a claimed authentication proof. Check fee-field representation/overflow.

## Limits and batching

For preserved protocol29 ledger **65270655**, capture SHA256
`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`:

| Quantity | Captured ceiling |
| --- | ---: |
| Instructions | 400,000,000 |
| Invocation memory | 41,943,040 bytes |
| Unique footprint keys | 400 |
| Disk-read entries | 200 |
| Write entries | 200 |
| Disk-read bytes | 200,000 |
| Write bytes | 132,096 |
| Transaction envelope bytes | 132,096 |
| Contract code entry bytes | 131,072 |
| Contract data entry bytes | 65,536 |
| Contract data key bytes | 250 |
| Return/events bytes | 16,384 |

Internal 100M/100-entry/50-write policy and all other existing checks remain
unchanged. SDK invocation entry totals do not apply as fabricated restore
measurements; report the operation's actual unique footprint and explicit
applicability of each gate. Missing native memory measurement remains missing.
A newer capture must be separately named, decoded and hashed; this table is
not a current-network assertion or authorization to adopt new policy.

For a restore-only batch every selected key is read and written; its byte sum
therefore must fit **132,096**, despite the larger disk-read allowance. Batch by
serialized entry bytes, key/write counts, envelope bytes and rent/fee bounds.
One entry cannot be split between transactions. Core checks full entry size
against current code/data limits, so old code permitted at upload may still fail
restoration under a changed limit. Inspect full code-entry XDR including its
extension and metadata, not just the `.wasm` file size.
Source: [Core entry-size validation](https://github.com/stellar/stellar-core/blob/v29.0.0/src/transactions/TransactionUtils.cpp#L2007).

## Exact state and unsigned evidence

Each batch should bind the input snapshot, protocol/config/cost-model hashes,
ledger sequence/time, canonical sorted key XDRs, archived entry XDRs and hashes,
old TTLs, each computed rent size, and a real serialized unsigned one-operation
`RestoreFootprintOp { ext: V0 }` envelope. Its read-only footprint is empty;
source/sequence/time bounds are synthetic and signatures empty. Keep exact
helper output, fee reconciliation and every rejected limit alongside it.

Successful Core restoration sets TTL to `sequence + minPersistentTTL - 1`.
The capture uses minimum persistent TTL **2,073,600** ledgers, maximum TTL
3,110,400. Hot-archive restoration and expired-but-not-evicted restoration have
different ledger metadata: Core rewrites hot entry last-modified sequence,
whereas its live-bucket path updates TTL. Preserve contract entry *data* exactly,
including exited beneficiaries and unresolved entitlements; do not claim full
entry metadata unchanged. Core can skip absent/already-live entries whereas
the simulation helper rejects a missing key. These distinctions need explicit
tests, not blanket equivalence.
Source: [Core restore application](https://github.com/stellar/stellar-core/blob/v29.0.0/src/transactions/RestoreFootprintOpFrame.cpp#L117).

The official `AutoRestoringSnapshotSource<T>::new(Rc<T>, &LedgerInfo)` is a
supported discovery wrapper: accessed expired persistent entries appear with
minimum restored TTL, their keys are recorded, and `simulate_restore_keys_op`
returns the corresponding restore simulation. It never mutates the underlying
snapshot or applies a transaction. It is suitable for discovering dependencies,
but a final invocation must not use an unconstrained wrapper that quietly
restores a key omitted from the paid batches. Use only the validated restored
key set; untouched expired state must remain expired. A snapshot projection is
**conditional post-restore evidence**, not Core-applied restoration.
Source: [official snapshot wrapper](https://github.com/stellar/rs-soroban-env/blob/ff6ae01b3465ac44185d2824876491e62c9a4cc4/soroban-simulation/src/snapshot_source.rs#L20).

Prove that the batch union covers every expired dependency actually required by
the final invocation, including code, instance, data and reward-history keys.
No holder enumeration belongs in liquidation: preparation can discover/batch
keys off-chain without changing the atomic funded repayment/seizure call.
After preparation re-run the original authorized invocation with all claim-first
boundaries, valuation and reserve checks. Missing batches, insufficient fees,
re-expiry, intervening ownership/history changes, already-live retries,
duplicates, missing archive bytes and an oversized single entry need honest
failure/refresh behavior. A third party can fund preparation without changing
ownership; source-account authorization still applies to any actual transaction.

Definitive validation additionally requires target Core transaction application,
source account/sequence/fee processing, actual TTL metadata, signed-auth costs,
and live-state testing. Host29 invocation evidence and native restore resource
simulation are useful separate layers; neither substitutes for those gates.

## CI observed for 627e2e2

At this review, baseline run **41613837844** has checker job125842187722 passing
and checkpoint job125842188506 failed with artifact upload successful.
Prototype run **41613837952** has
internal-policy job125842189056 failed, metadata tests7 passing and artifact
upload successful; captured job125842188319 has also failed its execution step
and is still uploading evidence. Internal summary
retains baseline/candidate fixture and checkpoint failures and failed/missing
live/data/cold comparisons (14 explicit summary errors). Artifact13221271132 is
52,956,783 bytes with ZIP SHA256
`5e5fb79fdd4e9b76bb2d1fe80bab59f42b71302765f4fbb3a5a03da82ff0704a`.
This is job/log verification, not artifact per-gate replay or CI approval.
