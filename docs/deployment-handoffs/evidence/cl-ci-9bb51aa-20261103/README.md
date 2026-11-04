# Completed 9bb51aa Linux CI evidence

Stage 1 remains incomplete, PR75 draft, and release approval false. This archive is separate from every earlier Linux capture. It replaces pending-CI wording for head `9bb51aa108d9cac72fd2dbc46051e89ac7a839b0`, not for later prototype source. GitHub artifact names use pull-request merge SHA `3fbeb951db492628a7605f172843b2e945c9a4ac`; metadata independently identifies the reviewed PR head.

| Workflow | Completed result | What the artifact proves |
|---|---|---|
| [Baseline41626834491](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41626834491) | Checker tests PASS; checkpoint job FAIL | Captured profile: all four gates PASS, no lifecycle/transaction errors. Internal: all four FAIL,106 lifecycle and303 transaction errors. Baseline CI does not execute the newly added Option1 recovery methods. |
| [Storage41626834469](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41626834469) | Both jobs FAIL | Captured baseline/candidate four outer tests each PASS; internal baseline/candidate four each FAIL. All prototype complete-checkpoint gates remain failed. Live/data/cold report views share test logs; they are not additional Cargo executions. |
| [Restoration41626834559](https://github.com/PeridotFinance/Peridot-Soroban/actions/runs/41626834559) | Job FAIL | Six outer compiled tests PASS. Captured post-preparation invocation gates PASS; lifecycle/transaction/checkpoint remain FAIL. Internal invocation gates FAIL. Every native batch passes its batch checks, but internal open/close full-journey integrity also fails where finite host rejection leaves incomplete financial evidence. Core/signature/live-state gates remain false. |

All38 recorded gate reports were independently replayed with pinned source, matching each gate, error list and invocation resource/error row; aggregate exit1 was not the criterion. The replay's exit1 preserves the actual failed gates. Four original warm invocation reports were verified in a separate supplemental replay; together the exact report-name set equals all38 archive reports, with no missing or extra report. The replay index and lossless replay archive preserve outputs, driver source and earlier unsuccessful replay attempts.

Named compiled-test outcomes and Cargo totals are indexed. Parallel native logs have incomplete serial-parser name coverage, explicitly labelled; exact Cargo summaries and raw logs remain available. Baseline captured compiled groups are36 harness,3 archive,2 history-archive and1 integrity PASS; native funded9 PASS. Internal corresponding groups are4/32,0/3,0/2,1/0 and5/4 passed/failed. Separate default-build summary is110 PASS/2 ignored; reward summary53 PASS. These are separate execution groups, not a new joined-candidate qualification.

The archive contains90 WASM files representing20 distinct hashes. Every file retains its build/path identity and exact size; no Linux hash is substituted for a Mac artifact. Restoration final V3 is92246 bytes/SHA693d40bed827d0ba256c601a8d55947e9de36b3cc0e5a807d60a36ca4cedce33; receipt88329/SHA d36d3620732a2a6b2773cdc27a4e323b2db24d6620f06c086d5b1157a2637974; controller63411/SHA9130ee98cc37a38bd2f1fd459d6f03c3d1a0ba0da8b5b322c425fbff267c6870. Other identities are in index.json.

Original artifacts expire November17; their829 files plus pinned replay sources are preserved here now. No wallet data, signatures, keys, compiler caches or deployment are included.

Payload: `96d9fb8e5d51a55f3a63698a3cccb264006749854b920fb3f3370eb65f94fc50`, 10,553,240 bytes. Replay archive: `700b0606b5543943b99994c442e1e07913be9418a6c2afb22180f7b5328c9d86`, 66,976 bytes.

From peridot-contracts, verify and reproduce in a fresh target directory:

```sh
python3 prototypes/cl-option1-qualification/ci-evidence.py --verify ../docs/deployment-handoffs/evidence/cl-ci-9bb51aa-20261103
python3 prototypes/cl-option1-qualification/replay-ci.py ../docs/deployment-handoffs/evidence/cl-ci-9bb51aa-20261103 --output target/cl-ci-9bb51aa-replay-new
```

The first command verifies hashes, exact indexed results and identities; the second reruns every checker and intentionally exits1 because required historical gates failed. Installed Node decoder dependencies must match the archived package lock. No contract execution, network access, signing or broadcasting occurs during replay. Full archived reports retain both network captures and their original hashes.
