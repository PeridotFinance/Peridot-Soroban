All original full-journey and financial reports remain unchanged. Prefix feasibility only covers validated unsigned restore batches and their exact pretrade chain.

| Run / profile | Batches / journeys | Prefix feasible | Financial invocation | Full journey execution | Lifecycle / transaction errors | Journey integrity / execution errors |
|---|---:|---|---|---|---:|---:|
| open-close / internal-policy | 30 / 5 | PASS | FAIL | FAIL | 2 / 27 | 36 / 20 |
| open-close / captured-network-diagnostic | 30 / 5 | PASS | PASS | PASS | 2 / 1 | 0 / 0 |
| combined / internal-policy | 22 / 9 | PASS | FAIL | FAIL | 2 / 2 | 0 / 4 |
| combined / captured-network-diagnostic | 22 / 9 | PASS | PASS | PASS | 2 / 1 | 0 / 0 |
| pending / internal-policy | 40 / 8 | PASS | FAIL | FAIL | 2 / 1 | 0 / 16 |
| pending / captured-network-diagnostic | 40 / 8 | PASS | PASS | PASS | 2 / 1 | 0 / 0 |
| liquidations / internal-policy | 48 / 8 | PASS | FAIL | FAIL | 2 / 9 | 0 / 32 |
| liquidations / captured-network-diagnostic | 48 / 8 | PASS | PASS | PASS | 2 / 1 | 0 / 0 |

| Run | Peak keys/writes | Peak serialized entry bytes | Peak unsigned envelope bytes |
|---|---:|---:|---:|
| open-close | 50 | 121,860 | 4,684 |
| combined | 50 | 107,848 | 4,288 |
| pending | 50 | 93,736 | 4,560 |
| liquidations | 50 | 102,280 | 4,360 |

Native RestoreFootprint declares zero instructions; SDK CPU and native Core memory are not invented for these batches. Peak 50 writes reaches the unchanged internal 50-write ceiling; batch bytes remain below 132,096. Core application, actual restoration TTL metadata, account signatures/fee charging, live-state execution, complete lifecycle mapping and release remain unproved.
