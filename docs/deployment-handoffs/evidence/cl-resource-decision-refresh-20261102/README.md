# Resource-decision refresh — analysis only

This evidence updates existing handoff12. Stage 1 incomplete; PR #75 draft; no
resource policy, production stage, borrowing or operational restriction changed.
The split prototype remains a negative result, preserved in its own archives.

Read-only Mainnet settings: ledger65270655, protocol 29, capture SHA256
`4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a`.
All 13 raw setting XDRs decode to the saved values. Ceilings and 86 CPU/86 memory
cost terms match the execution capture at 65264920. Only the live-state-size
window changed; effective rent remains1000 stroops/KiB and other fee inputs match.
The verification checked all archive/member hashes, required flows, exact saved
profile gates/errors, reconciled call trees and fee estimates of preserved
successful transactions under the refreshed prices. No contract execution reran.

`analysis.json.xz` holds separate Mac/Linux9cf atomic populations and both negative
split archives. SDK entry totals and unique transaction keys remain distinct;
independent maxima are not falsely combined into one transaction. The proposed
300M/300 SDK/300 unique/100 writes/32 MiB envelope remains analysis only. Its 80% CPU
boundary leaves3305137 instructions before missing signed/stressed costs.

`network-comparison.json`, `source-hashes.json`, compressed verification output
and independent architecture advice make the comparison reproducible. A deliberate
changed-cost-model input was rejected without producing a result or active profile.
The initial restricted-network fetch failure is retained; the subsequent approved
request read only network/configuration methods, never accounts or secrets.

From peridot-contracts, reproduce this exact snapshot analysis:

```sh
python3 scripts/refresh_cl_resource_decision.py \
  --capture ../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json \
  --output target/resource-decision-analysis-fresh.json
```

To refresh again, use `scripts/capture_cl_network_config.cjs` with a fresh output
path. Changed runtime, cost coefficients or effective prices require reconciliation;
the analyzer refuses silently treating incompatible measurements as current.
Its success means input analysis verified, not release or checkpoint approval.

CI is reported separately in [the completed bb154ec archive](../cl-linux-ci-bb154ec-20261102/README.md): checker tests pass 32, captured profile passes all four gates and internal profile fails all four. Exact errors and completed tests are preserved; this is baseline CI and does not execute the isolated prototype. The prior Linux archives remain intact. Artifact verification is distinct from full offline replay, which was not repeated for this new capture.
