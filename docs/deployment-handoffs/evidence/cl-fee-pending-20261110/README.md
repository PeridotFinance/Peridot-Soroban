# Bounded Aquarius fee-pending comparison evidence

Part(a) only. Stage1 incomplete; Option1 permanently closed; no signatures or
production changes. Existing protocol29 harness, actual pinned Aquarius bytecode,
controlled SAC state. Every new complete 1x financial stage remains unmeasured.

Final run: `runs/cl-fee-pending-final-v7`, six tests/60 branches/210 top-level calls.
Internal100M/100SDK/100keys/50writes invocation PASS; combined lifecycle,
transaction and checkpoint gates FAIL. Exact errors are preserved in gates.json.
V6 is a separate clean predecessor. Earlier failed fixture/compiler/runner
attempts remain individually identified. Default-feature isolation lists zero
study tests; it is not an execution proof. Baseline CI does not run this opt-in study.

Archive SHA256: `8cd1e09de4324398946050a3591fbb7c068408973a3b3217fca1ecc97d246769`

130 indexed members, including raw resources, calls, unsigned envelopes,
footprints, fee/rent inputs, comparisons, logs, source pins, executed source copies,
eight fixture WASMs and pinned Aquarius. No compiler caches or unrelated target
contents are included. Every member and the archive were hash-verified after packing.

Reproduction: follow `peridot-contracts/research/cl-fee-pending-20261110/README.md`.
The final summarizer validates exact per-call limits, full comparison coverage and
all four gate outcomes. This archive is separate from all prior Linux/CI archives.
No new CI result or real-signature/Core financial qualification is inferred.
