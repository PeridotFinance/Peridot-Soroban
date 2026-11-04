# Joined candidate measured roots

No policy adoption, signed authorization, live-state or Core financial-application proof. Maxima use complete unsigned host executions and SDK attempts separately. Missing/rejected roots are explicit; they are not zero-cost passes.

## captured-network-diagnostic

| Root | SDK accepted/attempted | Host completed/attempted | CPU maximum | Memory maximum | SDK entries | Unique keys | Writes | CPU headroom to240M |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| swap-open | 2/2 | 2/2 | 227685319 | 22793790 | 172 | 126 | 48 | 12314681 |
| activation | 1/1 | 2/2 | 63256252 | 11848088 | 157 | 119 | 40 | 176743748 |
| prepare-close | 1/1 | 1/1 | 45068223 | 10695350 | 111 | 81 | 31 | 194931777 |
| swap-close | 1/1 | 1/1 | 189438037 | 19188362 | 164 | 123 | 42 | 50561963 |
| finish-close | 1/1 | 1/1 | 38655115 | 7220826 | 96 | 66 | 30 | 201344885 |
| active funded liquidation | 1/1 | 1/1 | 67249744 | 12784837 | 170 | 121 | 50 | 172750256 |
| full repayment | 3/3 | 5/11 | 15504090 | 4356551 | 74 | 63 | 12 | 224495910 |
| debt-free release | 3/3 | 3/4 | 14411567 | 4100539 | 86 | 65 | 22 | 225588433 |
| expired PendingOpen | 1/1 | 2/8 | 70469573 | 13240348 | 152 | 103 | 50 | 169530427 |
| unhealthy PendingOpen | 1/1 | 1/6 | 69208840 | 13077840 | 152 | 103 | 50 | 170791160 |
| stalled funded settlement (six Long asset states) | 6/6 | 6/9 | 74180636 | 14802394 | 176 | 125 | 52 | 165819364 |

Exact gates: `{"checkpoint": false, "invocation": true, "lifecycle": false, "transaction": false}`. Per-flow errors remain in the bound gate report.

## internal-policy

| Root | SDK accepted/attempted | Host completed/attempted | CPU maximum | Memory maximum | SDK entries | Unique keys | Writes | CPU headroom to240M |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| swap-open | 0/1 | 0/1 | 100000038 | 14002583 | 135 | MISSING | 29 | UNQUALIFIED |
| activation | 0/0 | 0/0 | MISSING | MISSING | MISSING | MISSING | MISSING | UNQUALIFIED |
| prepare-close | 0/1 | 0/0 | 41761960 | 10442355 | 111 | MISSING | 31 | UNQUALIFIED |
| swap-close | 0/0 | 0/0 | MISSING | MISSING | MISSING | MISSING | MISSING | UNQUALIFIED |
| finish-close | 0/0 | 0/0 | MISSING | MISSING | MISSING | MISSING | MISSING | UNQUALIFIED |
| active funded liquidation | 0/1 | 1/1 | 67249744 | 12784837 | 170 | 121 | 50 | UNQUALIFIED |
| full repayment | 3/3 | 5/11 | 15504090 | 4356551 | 74 | 63 | 12 | 224495910 |
| debt-free release | 3/3 | 3/4 | 14411567 | 4100539 | 86 | 65 | 22 | 225588433 |
| expired PendingOpen | 0/1 | 2/7 | 70469573 | 13240348 | 152 | 103 | 50 | UNQUALIFIED |
| unhealthy PendingOpen | 0/1 | 1/5 | 69208840 | 13077840 | 152 | 103 | 50 | UNQUALIFIED |
| stalled funded settlement (six Long asset states) | 0/0 | 0/0 | MISSING | MISSING | MISSING | MISSING | MISSING | UNQUALIFIED |

Exact gates: `{"checkpoint": false, "invocation": false, "lifecycle": false, "transaction": false}`. Per-flow errors remain in the bound gate report.

CPU breakdown, fee ranges, source hashes and remaining headroom in every proposed dimension are in the sibling JSON. SDK entries are read+write+disk counts, not unique LedgerKey cardinality. Host maxima include every completed recording of the named ABI in its exact test group, including recording-only counterparts of SDK authorization negatives; host success does not prove their authorization. Stalled settlement groups all six asset states conservatively. Diagnostic shadow counters are excluded. Fees are per independent recorded invocation, not a lifecycle sum.
