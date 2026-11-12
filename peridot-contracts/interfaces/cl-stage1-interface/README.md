# Stage1 interface v1 — package1 freeze

Typed SDK25.3.1 `rlib`, generated receipt/vault clients, canonical identity/terms hashes,
versioned command/liability/waiver/result/event-data types, namespaced keys and stable
error IDs. No contract implementation or deployable WASM. Production contracts do not
depend on this crate yet; package2 has not started.

- [Semantics and invariant freeze](SEMANTICS.md): exact authority, phases, custody,
  deadline equality, hash domains, set-once binding/upgrade freeze, scoped waiver,
  storage durability and event ordering.
- [Source](src/lib.rs):29 typed client signatures and30 contract-type/error specifications.
- [ABI manifest](abi-manifest.json): signatures and115 existing key variants audited.
- [Wire vectors](wire-golden.json): native SDK-generated spec/XDR vectors, not hand-entered
  encodings. Golden creation refuses overwrite; ordinary test runs compare only.
- [Proof map](proof-map.json): nine financial rows and G1–G4, all marked unimplemented/
  unrun for the new build. Baseline passes are not passed off as new-code tests.

Verify from `peridot-contracts`:

```sh
python3 interfaces/cl-stage1-interface/freeze.py
python3 interfaces/cl-stage1-interface/verify.py
CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 cargo test --locked -p cl-stage1-interface
```

Five native tests cover type/spec/XDR stability, new-vs-existing key collisions,
serialization roundtrip/bounds, and mutation of every committed open/close/binding field.
These tests do **not** exercise authorization, principal/rewards, Core application or
resource limits. No financial root exists here to meter. All later implemented flows
must run check_budget.sh and preserve all four failing/unrun lifecycle gates until
actual evidence exists. The shared policy, existing profiles and approved measured
baseline remain untouched. No signing, deployment, keys or network actions are used.
