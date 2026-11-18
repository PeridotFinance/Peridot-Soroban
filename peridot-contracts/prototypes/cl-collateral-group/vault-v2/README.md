# Vault v2 (CL collateral group)

Self-contained workspace: `cl-share-vault` plus the two mocks its tests need.

* **Base** = the frozen Vault-B source
  `target/cl-pyusd-vault-b/release-live-3/workspace/contracts/cl-share-vault/src` in the main
  checkout. Per-file hashes: `BASE-SHA256SUMS`. Building the base with
  `--features cl-pyusd-vault-b`, `--locked` and the public init-admin constant reproduces the
  deployed vault byte for byte: `e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9`,
  123,430 B (re-verified 18 Nov 2026 in this worktree).
* **v2** = the commits after the base import (`git log -- prototypes/cl-collateral-group/vault-v2`):
  settle-only `stage_seize_reassign`, `LastClaim`, and the D6 seizure boundary.

Build: `bash prototypes/cl-collateral-group/build/build-vault.sh <workspace> <out>`.
Tests: `cargo test --locked -p cl-share-vault --features cl-pyusd-vault-b`.
