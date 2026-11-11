# Package1 freeze evidence

See [handoff28](../../../docs/deployment-handoffs/28-cl-stage1-package1-abi-freeze-20261111.md).
`manifest.json` pins the interface, golden vectors, proof map, fixture identities,
Cargo files, report and native logs. `wire-first.log` is the preserved initial
SDK25 compile error; `native-release.log` contains the final five passing tests.
No financial implementation, WASM, authorization execution, resource gate or Core
application was performed by package1. Earlier signed fixtures remain separate.

Run `python3 research/cl-stage1-abi-freeze-20261111/verify.py` from the workspace.
Economics has its own public-response manifest and offline verifier in the sibling
`cl-stage1-economics-20261111` directory. Package2 has not started.
