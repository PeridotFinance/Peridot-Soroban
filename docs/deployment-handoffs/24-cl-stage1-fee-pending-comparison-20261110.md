# Stage 1 part (a): fee-pending Aquarius comparison — 2026-11-10

**Result: the bounded venue comparison passes 100M CPU / 41,943,040 memory bytes /
100 SDK entries / 100 unique keys / 50 writes.** All six tests pass, covering 60
branches (30 matched pairs) and 210 measured top-level calls. Deposit reaches
**70,527,015** unsigned-host instructions; withdrawal reaches **64,893,641**.
These replace the earlier 64.2M/62.6M planning anchors for this tested matrix.
They are not universal limits over arbitrary live pool state or complete vault
transactions. No Stage 1 build plan is approved by these component results.

Option1 is **permanently closed**. Aquarius stays. The accepted design is
claim-first, fee-inclusive cash-backed minting, later keeper investment, bounded
entrant execution-cost funding and in-kind cancellation. Fable's deposit-first
convention is not adopted. The old implementations, negative split and captured
profiles remain preserved. The unfinished Option1 shared-file edits are archived
as `closed-option1-unadopted.patch.txt`; the active profile/checker files match
checkpoint 1a11bb8 again. No production contracts, caps, borrowing setting,
maintenance margin or handoff00 changed.

## Exact measured workload

The existing protocol29 harness loads the pinned Aquarius WASM
`12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6`.
No new harness, contract WASM build, deployment, signature or network call is used.
Host/SDK29.0.1 coefficients match all 86 CPU and memory coefficients in capture
ledger65270655, SHA256
`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`.
This historical capture records 400M instructions, 400 footprint keys and 200
writes; the required 100M/100/100/50 boundary is enforced independently. SDK entry
totals and transaction footprint keys are separately reported, never conflated.

Fixture background liquidity is the existing [-2000,2000] position with
100,000,000,000 raw units desired per token. Each compared external LP owner has
exactly one position, seeded with 1,000,000,003 raw units desired per token.
Amounts are seven-decimal controlled SAC units, not a production cap or capacity
claim. The fixture includes existing synthetic margin initialization solely to
reuse its contracts; measured calls do not borrow, activate margin or modify
supplier share ownership. The synthetic owner is narrowly authorized with exact
nested SAC transfers. No venue/vault/controller contract authorization is mocked.

| Range | Paths | Actual measured tick | Pending fees per token before mutation |
|---|---|---:|---|
| Narrow [-40,40] | No trades | 0 | 0 / 0 |
| Narrow [-40,40] | Rounding trades | -1 | 971 / 971 |
| Narrow [-40,40] | Range exit | -76 | 1,003,976 / 971 |
| Narrow [-40,40] | Exit and reentry | -2 | 1,003,976 / 975,237 |
| Wide [-2000,2000] | Rounding trades | -1 | 29 / 29 |
| Full [-887260,887260] | Rounding trades | -1 | 2 / 2 |

Rounding trades are actual swaps of 3,000,017 token0, 2,999,983 token1, 10,003
token0 and 10,007 token1. Range exit adds 5,000,000,000 token0; return adds
4,900,000,000 token1. These are controlled comparisons, not an economic safety
endorsement of those trades. Each path compares deposits of 3 and 100,000,003 raw
units per token, withdrawals of 1 liquidity unit, floor(L/7), and all L. Every
operation is compared with and without an immediately preceding actual fee sweep.
Each pair's complete initial ledger snapshot hashes match before the sweep.

Setup runs under an explicitly captured diagnostic profile. Every measured fee
view, quote, sweep, deposit and withdrawal enforces the 100M profile in both the
SDK invocation and separate unsigned official-host simulation. Setup acceptance
is not a Stage 1 lifecycle claim. Archival/restoration is not included here.

## Costs of actual calls

Each cell is a maximum for its column across the bounded cases, not a fabricated
single transaction. CPU includes the venue's nested calls. Host simulation records
an unsigned envelope and authorization tree; external signature validation remains
unmeasured. SDK invocation uses scoped synthetic authorization. Full staged-root
checks and accounting overhead belong to part (b).

| Aquarius entrypoint | Calls | SDK CPU | Unsigned host CPU | Peak memory bytes, SDK/host | SDK entries | Unique keys | Writes | Result |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| `deposit_position` | 24 | 69,830,045 | 70,527,015 | 5,875,748 / 5,475,268 | 33 | 34 | 11 | PASS |
| `withdraw_position` | 36 | 64,055,309 | 64,893,641 | 5,680,578 / 5,269,099 | 35 | 36 | 15 | PASS |
| `claim_all_position_fees` | 30 | 54,744,041 | 55,678,698 | 4,469,181 / 4,317,604 | 28 | 29 | 7 | PASS |
| `get_all_position_fees` | 60 | 2,063,511 | 2,450,433 | 1,455,647 / 1,463,604 | 6 | 6 | 0 | PASS |
| `estimate_deposit_position` | 24 | 9,216,945 | 9,356,279 | 1,662,722 / 1,652,129 | 2 | 2 | 0 | PASS |
| `estimate_withdraw_position` | 36 | 5,212,433 | 5,555,262 | 1,582,636 / 1,588,634 | 5 | 5 | 0 | PASS |

Range-specific unsigned-host maxima (different controlled workloads, not a yield
or allocation recommendation):

| Range | Deposit CPU | Withdrawal CPU | Deposit quote CPU |
|---|---:|---:|---:|
| Full | 70,527,015 | 64,893,641 | 9,356,279 |
| Simple wide | 58,809,117 | 59,206,870 | 3,166,086 |
| Narrow diagnostic | 64,145,569 | 62,741,386 | 4,626,729 |

Deposit CPU headroom is 29,472,985 and withdrawal headroom is 35,106,359 against
100M **for these venue roots only**. This is not a budget reservation for a new
stage; complete-stage accounting, claim-first rewards, fresh prices, commands,
authorization and restoration must be measured together. No independent maxima
or fee probes are added to manufacture a staged-transaction result.

| Entry point | Max unsigned envelope bytes | Max write bytes | Max auth bytes | Max events / return bytes | Fee estimate range, XLM |
|---|---:|---:|---:|---:|---:|
| `deposit_position` | 3,684 | 8,288 | 576 | 1,380 / 84 | 0.0137560–0.0142018 |
| `withdraw_position` | 3,540 | 8,316 | 260 | 1,388 / 52 | 0.0128399–0.0144139 |
| `claim_all_position_fees` | 2,684 | 3,628 | 176 | 844 / 52 | 0.0117666–0.0127008 |
| `get_all_position_fees` | 736 | 0 | 4 | 0 / 52 | 0.0003150–0.0003162 |
| `estimate_deposit_position` | 364 | 0 | 4 | 0 / 84 | 0.0002393–0.0004502 |
| `estimate_withdraw_position` | 668 | 0 | 4 | 0 / 52 | 0.0003170–0.0003946 |

At the largest deposit fee, components are 47,072 non-refundable + 3,575 refundable
+ 91,271 rent + 100 inclusion = 142,018 stroops. Largest withdrawal: 49,252 +
3,516 + 91,271 + 100 = 144,139 stroops. The synthetic external author's nonce
accounts for temporary rent; this is not a signed G-account cost estimate. Fees
are simulated, never paid. Complete per-row components, envelopes, footprint XDR,
return/events and rent inputs remain in the evidence. No normal lifecycle fee is
inferred by summing independent counterfactual branches.

## Accounting and rounding findings

- **Deposit does not sweep old LP fees.** In every pending branch the exact old
  fee balance remains pending after added liquidity. With a pre-sweep, that same
  balance is paid first. Principal used and added liquidity match both branches;
  wallet receipts plus remaining fees conserve the original fee amounts exactly.
  This supports the approved cash-first ordering without a second LP-fee sweep
  solely because investment happens later; it does not replace any required
  AQUA claim at an actual share ownership change.
- **Withdrawal pays every accrued fee for the position**, even on a partial or
  one-liquidity-unit withdrawal. A fee-inclusive quote matches the actual payout.
  Subtracting the contemporaneous exact fee reading gives the same principal as
  the swept counterfactual. Full withdrawal removes the position with the exact
  Aquarius missing-position error2118 and leaves zero pending fees.
- The one-unit reentry withdrawal returns zero principal but all 1,003,976 and
  975,237 fee units. Forwarding that entire payout to a tiny share exiter would
  steal incumbent fees. For each token, retain the non-exiter share of fees and
  allocate only `principal + floor((idle + fees) * burned_shares / supply)`.
  Actual swept/unswept principal equality and exact fee conservation are tested;
  the future vault allocator and its liability/donation exclusions are not built.
- Deposit authorization must cover the bounded **gross requested transfer**.
  Aquarius can transfer 100,000,003 then refund one raw unit even where the net
  quote is 100,000,002. The original fixture's narrower transfer authorization
  correctly failed. Corrected tests bound gross funding, compare actual net
  principal and added liquidity, and preserve exact fee accounting.
- Pending versus swept unsigned-host CPU rises by at most 1,136 for either
  deposit size, 851,340 for dust withdrawal, 453,944 for partial withdrawal, and
  394,656 for full withdrawal in this matrix. Fee magnitude alone is not a proof
  of a universal execution bound.

## Gates and limits of the conclusion

Final clean run: six tests PASS, 60 accounting branches PASS, 210 unique measured
calls PASS under the selected resource boundary. `check_budget.sh` invocation
PASS; lifecycle FAIL, transaction FAIL, checkpoint FAIL. The unchanged combined
checker retains 46 missing flows, two lifecycle errors and one transaction error
(`transaction proof-to-call mapping missing`). Part (a) supplies venue-call
evidence, not false lifecycle labels. The future 1x-only gate must be defined
separately from the preserved combined/margin gate; 2x is not a Stage 1 build
prerequisite under the new user scope.

The new measurements are **empirical maxima over explicitly listed states**.
They do not establish enforceable production bounds on arbitrary pool tick and
bitmap state, position counts, accrued history, archived code/data, account auth
forms or live liquidity. The old 64.2M and 62.6M observations were insufficient;
this matrix itself observes larger costs. Part (b) must carry the complete-state
scope and cannot use these maxima as unconditional constants or omit cost-bearing
reads. Underlying depeg, capacity and economics are not measured by this study.

Earlier fixture failures and logs are preserved: v1 exact transfer mismatch;
v2 queried the deleted position; v3 passed the original 150-call matrix; v4 had
a Rust assertion type error. V5 passed all 210 calls, but its shell runner was
edited while waiting for Cargo and failed on resumption; pins were reverified
and its checker run separately. V6 is a clean 210-call run. The final V7 run additionally makes this study
opt-in through the `fee-pending-comparison` harness feature, preserving baseline
CI's existing test population and avoiding SDK29-only dependencies in SDK25
contract tests. A separate default-feature test listing confirms zero study tests
selected; that is isolation evidence, not a passing execution proof. Both successful
runs and their exact source hashes are retained.

## Reproduction and next milestone

From `peridot-contracts`, with the same pinned public artifacts:

```sh
bash research/cl-fee-pending-20261110/run.sh internal-policy compiled_fee_pending target/cl-fee-pending-replay
python3 research/cl-fee-pending-20261110/summarize.py target/cl-fee-pending-replay
```

The summarizer requires all six tests, exact 60-branch coverage, 210 calls,
matching prestates and exact individual gate outcomes, and independently checks
both CPU/memory meters, SDK entries, unique footprint keys and writes. No aggregate
exit1 inference or missing-flow suppression. WASMs and executed source hashes
are preserved with raw reports and failed attempts in the durable archive.

**Part (b) is next, not completed:** prepare/fund/convert/fee-checkpoint+cash-mint/
keeper-investment and in-kind-withdrawal/optional-conversion as complete measured
1x transactions. Fable at high effort will review those actual footprints and
the later document-only 2x ABI key inventory; no Fable opinion is claimed for
this new part-(a) test. A Stage 1 build plan follows only if every part-(b) row
passes. No signing, broadcasting, deployment, borrowing activation or handoff00 edit.

Durable evidence: [archive index and scope](evidence/cl-fee-pending-20261110/README.md).
Full per-call rows: [results.json](../../peridot-contracts/research/cl-fee-pending-20261110/results.json).
