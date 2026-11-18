# Receipt StgV2 (CL collateral class)

`staged_v2.rs` + `apply-stgv2.py` turn the frozen StgV1 staged receipt (option B) into the
collateral market of the fresh CL group controller. See `research/cl-collateral-group-20261118/STGV2.md`.

* **Base tree**: `target/cl-staged-market-integration/run-5/staged` in the main checkout
  (accepted receipt `4c98bdd3` + StgV1 overlay `prototypes/cl-staged-market-integration/overlay`).
  `apply-stgv2.py` pins the base `staged.rs` by SHA-256. Rebuilt here with
  `--features stg-mainnet-accepted` it is 112,328 B, the frozen size. Its hash depends on the
  absolute path of the out-of-workspace OpenZeppelin dependency (two copies of the same tree at
  two paths give two hashes; the same path reproduces), so 140dcaab is only reproducible at the
  run-5 path.
* **Overlay**: additive only (the script asserts zero removed lines). It adds `staged_v2.rs`,
  `d3_chain.rs` (from `../d3-chain`), dispatch prepends on ten controller-facing entrypoints,
  a `set_peridottroller` freeze, the exit health hook in `stage_prepare_close` /
  `stage_emergency_exit`, five accessors appended to `staged.rs`, and the
  `stg2-mainnet-accepted` feature (default builds refuse StgV2 activation on Mainnet).

```bash
python3 -I prototypes/cl-collateral-group/receipt-v2/apply-stgv2.py --base target/clg/ws/receipt-base --out target/clg/ws/receipt-v2
bash prototypes/cl-collateral-group/build/build-receipt.sh target/clg/ws/receipt-v2 target/clg/art/receipt-v2
```
