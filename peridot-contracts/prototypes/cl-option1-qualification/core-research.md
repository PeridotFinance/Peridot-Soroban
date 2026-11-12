# Controlled unsigned Core29 restoration application

Actual local Core application passes for the four bounded fixture modes. Stage 1, financial authorization, live-state validation and release approval remain incomplete. No policy, transaction stage, production contract or deployment configuration changed.

Official Core v29.0.0 is pinned at `a9d72b0cac3a89ebbcf926af449ac73089e0f8f7`; protocol29 host is `a721944b5a4ea830a4bf5c2dbb8dbe8c1e1354c5`. Final functional runtime SHA256 is `dcbf2fb09e28d777d819b4c2785d7f1758c7e6c8c54649f89bec66df78ca0a47`; fixture SHA256 is `247ba377b3319525f648bdfbbfc9c8159ce8cd80a5e557adca4306be8378a814`. Sources and runner are in `core/`; exact inputs, runtime/source identities, complete transaction metadata, failures and final logs are under `target/cl-option1-core/`. The initial successful code/state version remains separately preserved in `passed-v1/`.

## Results

Each mode covers expired live-bucket entries, controlled hot-archive entries and a wrong-preauthorization rejection. Four final runs pass 2,581 assertions. They apply twelve native operations: ten RestoreFootprint and two ExtendFootprintTTL. Four additional wrong-preauth cases return `txBAD_AUTH` before application, without source balance/sequence/signers or fee-pool changes. A `feeCharged` field in that validation result is an estimated admission fee, not a debit.

| Mode / operation | Explicit footprint keys | Declared read / write bytes | Unsigned envelope bytes | Actual charge, stroops | Assertions per mode |
|---|---:|---:|---:|---:|---:|
| Real receipt code Restore | 1 RW | 88,432 / 88,432 | 168 | 1,026,250,252 | 230 |
| Code + instance + 21 data Restore | 23 RW | 91,364 / 91,364 | 1,980 | 1,031,215,394 | 946 |
| Mixed genesis: only code expired | 23 RW | 91,364 / 91,364 | 1,980 | 1,026,300,772 | 1,034 |
| Fresh already-live code Restore | 1 RW | 88,432 / 88,432 | 168 | 60,092 | Included in 371 chain assertions |
| Subsequent code ExtendFootprintTTL | 1 RO | 88,432 / 0 | 172 | 513,117,036 | Included in 371 chain assertions |

The three-operation chain charges exactly 1,539,427,380 stroops (153.9427380 XLM). Its source sequences advance 6→7→8 and its three preAuthTx signers are consumed individually. Restore at ledger280 sets TTL2,073,879; the fresh already-live Restore at281 leaves TTL unchanged and incurs no rent; Extend at282 sets TTL3,110,681. All full LedgerEntryData bytes remain equal. Source debits, signer/subentry removal, sequence, total-coins conservation and fee-pool increments are asserted after every application. Maintenance replays specifically return `txBAD_SEQ`.

The mixed case seeds 22 receipt instance/data keys already live at genesis and one expired code key. It verifies the live entries and their TTL modification metadata remain unchanged while code restores. This is **not** an interrupted multi-transaction preparation journey. Actual financial Core execution and a complete required-key restoration journey remain outstanding.

Native instructions are declared zero, not measured physical CPU. SDK invocation memory/entry counters are inapplicable. Explicit XDR footprint keys exclude implicit source/TTL bookkeeping visible in the complete Core metadata. These results do not manufacture invocation rows or satisfy missing financial checker flows. Existing profile and release gates remain unchanged.

## Authorization and controlled-state boundaries

The source account is supplied directly at synthetic genesis: master weight zero, thresholds one, exact transaction-content-hash preAuthTx signers of weight one. The chain's three signers are seeded before its first operation. No intervening account mutation grants later authorization. Every envelope has zero signatures. Normal `TransactionFrameBase::checkValid`, validated transaction-set construction, transaction-set validation and `LedgerManager::applyLedger` run unchanged. This uses the normal protocol29 parallel Soroban application path, even with a single ledger application thread. No direct serial restore helper, ledger mock, authorization bypass or signed setup is used.

The runtime has a mandatory fail-closed guard at the start of `SecretKey::sign`; signature verification is unchanged. The first setup attempt correctly hit that guard because stock Application construction eagerly signs an otherwise unused PeerAuth certificate. The fixture now uses the official virtual application factory seam to omit overlay construction. Its `getOverlayManager` throws if accessed. LedgerManager, native operations, transaction authorization, resource validation, fees and invariants remain stock. Application/network startup is skipped; standalone mode, validator/FORCE_SCP false, no peers and ports zero remain set. Warning-level logging and disabled backtrace are asserted before genesis to prevent upstream INFO root-seed formatting. Internal unused synthetic identity construction does not export or sign with a key.

The original SDK snapshots encode `lastModifiedLedgerSeq=0`. Core rejected synthesized TTL entries with that metadata. Raw input evidence remains unchanged; the fixture explicitly imports entries and TTLs at synthetic genesis ledger279, preserving every LedgerEntryData byte. This is a disclosed metadata adjustment to controlled state, not authentic network ledger history. The mixed fixture also changes the initial TTL status of its 22 live entries intentionally.

Real receipt code is 88,329 WASM bytes, SHA256 `fc1ac7d397428438c58c50ce8930907c5965587aac7cc0f9988025235dcde362`. The instance executable matches that code; all included data belong to that instance. Earlier native-SDK mock-authorizer placeholders are excluded. Code magic/hash and instance association are checked. Transaction preauthorization does not prove real account signatures or Soroban address-credential authentication for financial calls.

Thirteen captured protocol29 configuration entries are imported unchanged from ledger65270655, SHA256 `4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`. Core asserts 400M instructions, 41,943,040 memory, 400 footprint keys, 200 writes, 200,000 disk-read bytes and 132,096 write bytes. Fixture ledger280 is synthetic; captured settings are not a live-state replay. Existing internal limits are not changed or replaced.

## Fees, build and failures

Core charges 102.6190160 XLM rent for the cold code Restore. Its rent size is 1,847,140 bytes under the captured protocol29 model; that is a rent quantity, not measured invocation memory. Extend rent is 51.3096706 XLM. The initial Extend expectation overcounted one disk-read entry by 781 stroops: protocol29 excludes live Soroban RO keys from that count while still validating declared read bytes. The failed assertion and original expectation are preserved; the independent estimator was corrected from stock `TransactionFrame::getNumDiskReadEntries`, then all chain assertions passed. No fee model or Core fee code changed.

All nine official protocol21–29 host release rlib hashes are identical before and after final linking. The native runtime uses Rust1.95.0, a diagnostic O2/O0 C++ build, bridge-only ThinLTO with 16 codegen units, and matching Rust ld64.lld. Native performance is unqualified. The bounded ThinLTO bridge compiled in 11m20s. Earlier no-LTO attempts failed at linking: Apple LLVM could not read Rust bitcode; matching ld64.lld then exposed internal host symbols that require Rust LTO. Exact flags, helper translation-unit additions, logs and source hashes are preserved. No production source changes beyond the isolated signing-refusal instrumentation occurred; all production translation units remain included.

The native `soroban-test-wasms` dependency only embeds upstream precompiled assets with `include_bytes!`; its optional manual WASM regeneration target was not run. Project contract artifacts remain the separately pinned StellarCLI builds.

Reproduce the four guarded modes using the staged inputs and instructions in `core/README.md`. First validate staged input hashes, then run only the named fixture. Do not run ordinary upstream test suites, which sign. Complete local entry/TTL application is now demonstrated for these native operations; financial Core authorization, signatures, live-state history, all required-key journey coverage and release approval remain separate unresolved gates.

Primary sources: [official Core29 release](https://github.com/stellar/stellar-core/releases/tag/v29.0.0), [native disk-entry fee classification](https://github.com/stellar/stellar-core/blob/a9d72b0cac3a89ebbcf926af449ac73089e0f8f7/src/transactions/TransactionFrame.cpp), [RestoreFootprint implementation](https://github.com/stellar/stellar-core/blob/a9d72b0cac3a89ebbcf926af449ac73089e0f8f7/src/transactions/RestoreFootprintOpFrame.cpp), [ExtendFootprintTTL implementation](https://github.com/stellar/stellar-core/blob/a9d72b0cac3a89ebbcf926af449ac73089e0f8f7/src/transactions/ExtendFootprintTTLOpFrame.cpp), [preAuthTx verification](https://github.com/stellar/stellar-core/blob/a9d72b0cac3a89ebbcf926af449ac73089e0f8f7/src/transactions/SignatureChecker.cpp). Exact local source bytes and hashes are indexed in the evidence.
