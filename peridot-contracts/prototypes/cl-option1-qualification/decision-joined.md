# Resource decision — joined atomic candidate, policy unadopted

**Continue bounded Option1 qualification; do not adopt it or enable borrowing.**
The complete measured roots fit its proposed workload bounds with useful unsigned
headroom. Core restoration, full financial application, supported signatures,
live workload bounds, oracle availability and economic funding remain separate
decisions. The unchanged100M policy requires a venue/accounting ABI redesign;
further wrapper tuning or a new financial-stage split has no demonstrated path.

This table uses the corrected joined V3 (`100,419 bytes, SHA256 ad9fbb7669ff126ff54e8d9b021d610b9313cb5f3dc595df6e9d7dabc3d7d482`) and its exact
seven-case joined run. It does not relabel old independently cold probes as
restored executions of every new route. Maxima across roots are not one combined
worst-case transaction. SDK counts, actual unique keys and network settings are
different quantities.

| Dimension | Internal, unchanged | Option1 proposed ceiling | Option1 workload boundary | Joined measured maximum | Remaining workload headroom | Captured network ceiling |
|---|---:|---:|---:|---:|---:|---:|
| instructions | 100,000,000 | 300,000,000 | 240,000,000 | 227,685,319 | 12,314,681 | 400,000,000 |
| memory_bytes | 41,943,040 | 33,554,432 | 26,843,545 | 22,793,790 | 4,049,755 | 41,943,040 |
| sdk_entries | 100 | 300 | 240 | 176 | 64 | No SDK-total field |
| unique_keys | 100 | 300 | 240 | 126 | 114 | 400 |
| write_entries | 50 | 100 | 80 | 52 | 28 | 200 |

| Option | Concrete implementation dependency | Evidence still missing | Decision |
|---|---|---|---|
| Keep100M/100SDK/50writes (and100 unique keys) | Replace or revise the venue's fee-inclusive valuation/liquidity ABI while preserving actual claims and fee ownership. Current Aquarius-exclusive open162.817M and close137.760M already exceed100M. Fee collection plus position deposit118.186M / withdrawal115.816M also exceeds100M. Reduce actual receipt/controller/custody touches; another wrapper does not reset a budget. | A controlled, reviewable cheaper venue implementation; supported supplier mint/redeem and complete open/close/recovery; entitlement correctness; migration/dependency control; full signed costs. No assumed savings or new split is credited. | Concrete venue/accounting redesign required if this policy remains mandatory. Stop incremental wrapper tuning. |
| Option1 proposal only | Keep present financial stages, all three claim-first boundaries, fresh valuation, reserves and atomic funded repayment/seizure. Use measured native restoration preparation, bounded new-risk admission and funded recovery independent of borrower/governance attribution. | Actual Core financial lifecycles, real account signatures/nonce/envelope costs, live archive discovery, tick/registry/history capacity, elapsed re-expiry, remaining mixed-asset/delivery/reentry and fee branches, funded/incentivized operators. The largest CPU case leaves about5.13% of240M before unmeasured costs; the20% ceiling reserve is not available workload. | Technically worth a bounded next qualification, economically conditional. No policy adoption until those gates and a funded operating model are established. |

The old checker first introduced100M/100/50 at2d92318 without a documented
quantitative rationale.100M matches host25's default;100/50 match SDK25.3.1's
entry/write defaults, whose CPU default was600M. A copied-default origin is an
inference, not established intent. The later named internal profile deliberately
keeps a conservative gate. Full provenance is in handoff12; no gate is rewritten.

Captured settings are ledger65270655, protocol29, hash
`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`:
all86 CPU and86 memory terms are used directly in host29.0.1. This is not a fresh
live-state observation. Native RestoreFootprint has separate applicable checks;
zero declared instructions is not zero measured Core CPU or memory. Disk/read,
write-byte, event/return, envelope, key/data/code-size, fee and missing-flow checks
all remain in force. Signatures and account application are not in these host
recording templates. SDK host-shadow counters do not replace acceptance limits.

## Supplier entry and exit remain required

Preserved atomic deposit/redeem evidence is a separate population; the joined V3
recovery run does not rerun these roots. Their unchanged internal failures remain.

| Root | CPU | Memory bytes | SDK entries | Unique keys | Writes | CPU room to240M |
|---|---:|---:|---:|---:|---:|---:|
| deposit_underlying | 166,914,792 | 14,418,955 | 70 | 49 | 22 | 73,085,208 |
| redeem | 139,966,854 | 10,640,393 | 65 | 47 | 19 | 100,033,146 |

Warm and cold costs must be assessed against actual collected income. The shared
eight-borrower recovery projection is613.5261428XLM, including60.6771448XLM
synthetic-authorizer rent; this is not eight independent cold probes. Operating
balance, supplier cash, dedicated loss reserve and debt-token liquidation capital
are separate. A positive resource margin alone does not establish profitability,
liquidator willingness, reserve coverage or a safe exposure cap.

A separate same-bytecode companion passes both profiles for own proceeds plus
external deficit in one converted Closing settlement, including500,000 previously
reserved fee shares and backing. Its46,980,924 instructions/9,091,183 memory bytes/97 SDK entries/70 unique host keys/28 writes leave only 3 SDK entries
under today's internal policy before missing real-auth costs. It does not change
these global maxima or qualify every other asset shape. See `mixed-results.md`.

See `operating-plan.md`, `lifecycle-proof-map.md`, `core-research.md` and the exact
assessment for evidence levels and remaining failures. Both global checkpoint
gates remain failed. No boundary is increased to turn a failure green.
