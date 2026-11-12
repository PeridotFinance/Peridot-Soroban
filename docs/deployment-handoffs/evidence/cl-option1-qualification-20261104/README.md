# Option1 local qualification evidence — 2026-11-04

Stage1 incomplete. PR75 draft. Both checkpoint gates fail; Option1 is unadopted.
This archive is separate from the completed9bb51aa Linux archive and prior local
restoration, storage, atomic and negative-split experiments.

`qualification-evidence.tar.xz`: **6,594,292 bytes**, SHA256
`3796c2aafe20fb45e39c13ea4aea6a854e0b49d27051fc0842f990b198de4036`.
The explicit1117-member inventory contains38 exact gate reports,21 test logs and
112 WASM files with13 distinct identities. It retains failed/interrupted probes,
raw unsigned transaction/state artifacts, original and corrected proof maps,
compiled generated sources, Core inputs/source patches/build commands and failures.
No compiler cache, native binary, wallet artifact or signed envelope is included.
The only bootstrap-download member is the reviewed public URL/hash index, not
its payloads. The initial packaging refusal is preserved.

Verify every member and expose exact gates/errors/outcomes/WASM identities:

```sh
python3 verify.py . > verified.json
```

This verifies archive integrity and all stored gate arrays, not a fresh full
checker replay. The separately completed Linux archive includes its38 exact
checker replays. `verification-results.json` is the local verification output;
`assessment.json` and `assessment.md` bind both final seven-case runs to their
profiles/configuration/bytecode and reviewed gate summaries.

| Final joined profile | Compiled tests | Invocation | Lifecycle / transaction / checkpoint | Lifecycle / transaction errors | Missing flows |
|---|---|---|---|---:|---:|
| Captured network diagnostic | 7PASS | PASS | FAIL / FAIL / FAIL | 2 /46 | 46 |
| Internal policy | 2PASS,5FAIL | FAIL | FAIL / FAIL / FAIL | 34 /58 | 46 |

The separate same-bytecode mixed-funding case passes1/1 in each profile;
invocation passes, full gates fail (2 lifecycle/46 transaction errors,46 missing
flows each). The shared eight-borrower test passes1/1: captured8/8 sequential
financial roots, internal0/8. Its restoration and fee path remains projected.
Every incomplete/rejected root remains unqualified, even when a partial counter
is low. See the [checkpoint](../../22-cl-option1-qualification-20261104.md)
for complete resources, conservation, scopes and reproduction.

`core-results.json` separately records2581 assertions and12 actual native
Core29 operations (10Restore/2Extend), plus four authorization rejections.
Controlled preAuthTx genesis, normal transaction validation and ledger application
prove the tested archive/TTL/source-account/fee changes with zero signatures.
It is not full financial Core application, real-account signature or live-state
evidence. Runtime hash/source/build details are retained; the executable is not.

Source account/signature costs, complete interrupted and elapsed-re-expiry
financial journeys, live history/tick capacity, economic funding and remaining
recovery branches remain gates. No signing, broadcasting, deployment, production
limit/cap/maintenance-margin change, borrowing activation or handoff00 edit.
