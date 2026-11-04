# Option1 qualification decision table (unadopted)

Measured maxima are independent, controlled-state unsigned prepared roots from9bb51aa. They are not signed Core application or live capacity.

| Dimension | Unchanged internal | Option1 ceiling | Option1 workload | Measured maximum | Remaining workload headroom | Captured network |
|---|---:|---:|---:|---:|---:|---:|
| CPU | 100,000,000 | 300,000,000 | 240,000,000 | 227,912,081 | 12,087,919 | 400000000 |
| Memory bytes | 41,943,040 | 33,554,432 | 26,843,545 | 22,242,145 | 4,601,400 | 41943040 |
| SDK entries | 100 | 300 | 240 | 172 | 68 | No SDK-total field |
| Unique transaction keys | 100 | 300 | 240 | 126 | 114 | 400 |
| Writes | 50 | 100 | 80 | 50 | 30 | 200 |

| Alternative | Concrete dependency | Missing costs/proofs | Recommendation |
|---|---|---|---|
| Retain100M/100/50 | Required Aquarius fee/position operations must change: current venue-exclusive open162.817M and close137.760M exceed100M before Peridot work. A cheaper fee-inclusive valuation/liquidity ABI must preserve actual claims and exact fee ownership. Receipt/controller accesses must also fit100 SDK entries and100 actual keys. | Changed venue correctness, fee/reward entitlement and complete exit/recovery costs; no saving credited. | Stop wrapper tuning. Keep this redesign narrowly scoped; no new financial-stage split. |
| Option1, unadopted | Preserve atomic repayment/seizure and claim-first boundaries; qualify explicit restoration and every pending-state recovery with bounded new-risk admission. Never reject existing recovery or prune entitlement history to enforce new-risk bounds. | Core application, real signatures/accounts and signed envelope costs, live archive discovery, elapsed preparation/re-expiry, sequential shared recovery, more expensive fresh-price branches and capacity. Largest CPU margin is only12.088M/5.04% of240M. | Technically plausible for measured roots; not qualified. Economic worth depends on amortized rent, funded recovery and actual collected income, not on consuming the20% reserve. |

Native RestoreFootprint has separate applicable footprint/byte/fee checks. A declared0 instruction field is not native CPU/memory measurement. All other unchanged checks (disk reads/bytes, write bytes, events/return, entry sizes, fees, missing flows) remain mandatory. No boundary is increased by this analysis.

## Preserved 1× supplier evidence, a different measured population

Deposit and redeem are required even with borrowing disabled. These are preserved atomic baseline measurements across the separately identified Mac/Linux artifacts, not reruns of the enlarged pending-recovery V3. Their existing resource failures remain.

| Root | CPU maximum | Memory maximum | SDK entries | Unique keys | Writes | CPU headroom to proposed240M |
|---|---:|---:|---:|---:|---:|---:|
| deposit_underlying | 166,914,792 | 14,418,955 | 70 | 49 | 22 | 73,085,208 |
| redeem | 139,966,854 | 10,640,393 | 65 | 47 | 19 | 100,033,146 |

Required venue/accounting changes under100M must therefore cover supplier mint and redemption as well as margin open/close. A wider range does not remove fee-inclusive NAV, claims, ownership accounting or exit costs. Signatures, fresh live state and a joined worst-case workload remain unmeasured.
