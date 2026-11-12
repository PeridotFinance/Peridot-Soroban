# Stage 1 replan and fee-pending comparison

2026-11-10. Checkpoint 1a11bb8 is preserved. **Option1 is permanently closed**;
its artifacts remain historical evidence, not an alternative qualification plan.
The unadopted shared-checker/profile edits are preserved in
`closed-option1-unadopted.patch.txt`; the active shared files are restored to
their unchanged historical two-profile implementation. The unfinished isolated
Option1 prototype remains preserved and unused.
Aquarius remains the venue. No production settings or handoff00 edits.

Accepted entrant semantics: actual fee collection and claim-first cash-backed
minting, later keeper-driven investment, bounded entrant execution-cost funding,
and in-kind cancellation. The deposit-first/no-interim-earnings convention is
not adopted. Historical entitlement and quarantine requirements remain intact.

Work order:

1. **Part (a), this checkpoint:** on the existing protocol29 harness, compare
   actual pinned Aquarius deposit/withdraw bytecode with pending versus swept
   fees from identical complete ledger prestates. Measure quotes and collection
   too. Test dust/normal deposits and dust/partial/full withdrawals; narrow
   original range, simple wide range and full range; actual trades and narrow
   range exit/reentry. Report these bounded observations before part (b).
2. **Part (b):** measure complete 1x staged transactions: prepare, fund, convert,
   fee checkpoint plus cash-backed mint, keeper investment, in-kind withdrawal
   with fee allocation, optional conversion. Require every row to meet 100M CPU,
   41,943,040 memory bytes, 100 SDK entries, 100 actual keys and 50 writes plus
   existing byte/fee/rent/auth checks. Review footprints with Fable at high effort.
   A venue-only measurement is not a passing staged transaction.
3. **Part (c):** only if every complete row passes, deliver the Stage 1 build
   plan. Production build/adoption is not inferred from this measurement task.
4. **Separate later 2x milestone:** document the narrow fresh-risk/recovery ABI
   and exact key inventory for delivery, activation and funded rescue. Fable
   reviews that inventory; no margin implementation or build now.

The historical profile names are preserved for reproducibility. The captured
protocol29 settings at ledger65270655 record 400M CPU/400 footprint keys/200 writes;
100M/100/50 is the required boundary for this work regardless. SDK entry totals
are not a network footprint-key setting. This study does not refresh or override
the historical capture or use its larger ceilings to approve Stage 1.

Run from `peridot-contracts`, using a fresh output directory:

```sh
bash research/cl-fee-pending-20261110/run.sh internal-policy compiled_fee_pending target/cl-fee-pending-internal
```

The runner opts into `fee-pending-comparison` on the existing harness (baseline
CI retains its original test population) and uses and existing compiled fixture WASMs from
`target/cl-restoration-combined-build/artifacts` (override `CL_ARTIFACTS` if the
same pinned artifacts were extracted elsewhere). Aquarius SHA256 is enforced:
`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
All source and WASM hashes are recorded before execution and checked afterward.
No contract WASM is rebuilt. Native Rust test compilation is not deployment.

Fixture setup uses the explicitly recorded captured profile; each measured
operation installs/enforces the selected profile. The existing fixture includes
synthetic margin configuration only to reuse its initialized contracts; this
test never opens debt or changes production configuration. The tested owner is
an explicitly synthetic external account with exact nested SAC authorization.
No vault, receipt or controller contract authorization is mocked; this part does
not yet exercise the future vault command ABI. Unsigned host transaction evidence
and invocation enforcement are separate from real signature/Core application.

`check_budget.sh` runs on every report, with full missing lifecycle/transaction
gates retained. Inspect tests, comparisons, resource rows and individual gates;
an aggregate exit status alone is not proof. No finite test matrix establishes a
universal bound over arbitrary ticks, positions, ownership history or archival.
