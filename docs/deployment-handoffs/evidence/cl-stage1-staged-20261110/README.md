# Stage 1 staged transaction evidence, 2026-11-10

440 explicitly indexed members; only public source, fixture WASMs and unsigned local
execution artifacts. Stage1 incomplete; PR75 draft. No release approval.

Final buildv4; final internal/captured runs v6. Each has13 passing tests and9 measured
transaction rows within100M/100SDK/100keys/50writes. Complete in-kind and converted
journeys pass. Invocation gatePASS; lifecycle/transaction/checkpointFAIL remain exact.

Historical runs preserve compiler, reentry and event-test failures and the first mint
valuation implementation. Final mint uses actual backing, verified by off-oracle regression.
Fable request returned429 usage limit before execution; internal review is separate.

Verify every member and both profiles' exact gate summaries/errors from peridot-contracts:

```sh
python3 prototypes/cl-stage1-cash-first/verify_archive.py ../docs/deployment-handoffs/evidence/cl-stage1-staged-20261110
```

Archive SHA256:7842eda4b75d0ebcb9800091e9a1f165d64976d113583fde1cca4d01133ea925.
`verification.log` records successful exact offline replay, not execution of archived WASM.
Per-profile JSON/Markdown tables and raw gate summaries are provided separately for review.
Prior CI JSON pertains to accepted7d432d3b only; baseline CI does not run this prototype.
