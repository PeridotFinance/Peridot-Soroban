# Minimal operating-cost plan — 2026-11-03

Recommendation: reuse an existing suitable AWS host, existing primary oracle and existing authenticated RPC allowance before buying anything. **No new subscription or deployment is recommended now.** The repository does not establish which AWS resources exist or whether they have spare capacity. Consequently, $0 incremental hosting is a conditional reuse case, not a verified AWS bill. Stage 1 remains incomplete, borrowing disabled and PR75 draft; no signing, broadcasting, keys/secrets, Upshift, production configuration or infrastructure change was performed.

## Known infrastructure versus missing inventory

Read-only searches covered the repository's Markdown documentation, including the keeper/deployment README files and deployment handoffs, excluding target artifacts, node_modules, .aws, .env and AGENTS files. No EC2/Lightsail/ECS instance identifier, AWS region, instance size, current bill, quota or headroom inventory was found. The `rpc.lightsail.network` endpoint in handoff12 is an RPC address; its name is **not evidence of an owned Amazon Lightsail instance**. No AWS connector was available to the lead, and the lead confirmed `command -v aws` is absent. There is no authenticated inventory path available here without new setup. No CLI was installed and no AWS credential/configuration files or cloud account were inspected.

Known reusable software is the existing keeper, PostgreSQL intent journal, systemd/container supervision, host monitor and backup procedures. [Keeper operations](../../peridot-contracts/bots/margin-liquidator/README.md) require authenticated production RPC, preserve the canonical journal across restarts and distinguish simulations from submission. [Deployment documentation](../../peridot-contracts/bots/margin-liquidator/deploy/droplet/README.md) describes a DigitalOcean 1GiB pilot, historically quoted at $6/month, not an AWS server or a current AWS price. Its proposed host is not highly available. Neither that historical price nor prior zero-position process memory establishes present AWS capacity or CL stress capacity.

A non-secret inventory summary is needed before a concrete reuse allocation:

- AWS service, resource identifier/name, region, instance/container size, OS/architecture, uptime and billing owner; current committed/on-demand cost and existing usage allowances.
- Current co-tenants; sustained/peak CPU, free RAM, disk free space/IOPS, journal growth and egress. Container limits and safe headroom for keeper, PostgreSQL and monitoring; do not build Rust/WASM on the keeper host.
- Reachable RPC endpoints/protocol29 support, existing plan/quota/SLA, rate-limit headroom, egress/NAT charges and IPv4/IPv6 requirements. No token or credential values are needed.
- Existing database isolation/persistent volume, journal owner and single-worker/sequence coordination; backup destination, encryption/retention, restore rehearsal and recovery-point requirement.
- Existing external dead-man alerting, notification channel, log-retention limits and remaining CloudWatch allowances; who responds to alerts.

Missing inventory does not block local contract qualification. It blocks a claim that AWS reuse is free, adequately sized or ready for production.

## Itemized incremental monthly estimate

USD examples below were checked against official AWS pages on 2026-11-03. They are conditional list-price components, before tax, account discounts, region differences and overages. They do not authorize creation or assume free-tier credits.

| Item | Preferred reuse case | Conditional new-service comparison / missing cost |
|---|---|---|
| Keeper + PostgreSQL compute | **$0 additional instance charge** only if an existing host has measured spare capacity and no resize is needed; existing host's total monthly bill unknown | AWS Linux IPv4 bundle: 1GB/40GB is **$7/month**; 2GB/60GB is **$12/month**. These are size/price comparisons, not validated capacity or a purchase recommendation. [Official bundles](https://docs.aws.amazon.com/lightsail/latest/userguide/amazon-lightsail-bundles.html) |
| Existing primary oracle | **$0 for a newly added publisher**, because none is proposed | Existing oracle/provider operating charge is unknown and remains with the existing service. No secondary publisher, paid price feed or new oracle subscription is included. |
| RPC | **$0 incremental** only within a verified existing authenticated provider plan | Current provider/remaining allowance unverified; monthly charge and overage **TBD**. Public read-only RPC used for captures is not a substitute for the keeper's documented production provider requirement. No new provider is selected. |
| Keeper transaction fees | **0 actual spend under current no-signing/no-broadcast restrictions** | Future spend is variable raw stroops: sum of actual submitted transaction fees, plus needed restoration/maintenance fees. There is no validated monthly transaction cadence yet. Measured examples below are not an operating forecast. |
| Local supervision / monitor | Existing scripts and host services add no new service subscription; consume host resources | Existing alert delivery costs/coverage unknown. Host-local checks cannot detect complete host/network failure. |
| External dead-man metric/alarm | $0 incremental only if existing external monitoring covers the process | Illustrative US East rates: one custom metric **$0.30/month** + one standard-resolution alarm **$0.10/month** = **$0.40/month**, before API calls/notification delivery or credits. Region/account usage must be checked. [CloudWatch pricing](https://aws.amazon.com/cloudwatch/pricing/) |
| Logs | Reuse bounded local logs and existing retention if capacity permits | Example uncredited US East ingestion **1GB × $0.50 = $0.50/month**, plus retained storage, queries, API charges and notification delivery. Actual log rate and regional quote unknown; do not enable unbounded verbose execution logs. [CloudWatch pricing](https://aws.amazon.com/cloudwatch/pricing/) |
| Backups / boundary evidence archive | Existing approved object storage can have $0 incremental capacity charge if within allowance; not yet verified | Lightsail snapshots are **$0.05/GB-month**: 5GB retained snapshot data would be **$0.25/month**. A snapshot alone is not the canonical journal's incremental/off-host backup or a reward-evidence retention plan; object storage requests/retention/restore costs remain TBD. [Lightsail snapshot pricing](https://aws.amazon.com/lightsail/pricing/) |

Conditional arithmetic only: a new 2GB IPv4 host + one metric/alarm + 1GB log ingestion + 5GB retained snapshot data totals **$13.15/month for those four components** ($12 + $0.40 + $0.50 + $0.25). This excludes RPC, transaction fees, log storage/query/API/delivery, journal/object backups, egress overages and tax, so it is **not an all-in quote or budget cap**. The equivalent 1GB components total $8.15, with unqualified capacity. Reusing qualified existing services could avoid these new fixed charges entirely, but their existing bills and available capacity are unknown.

No managed database, Kubernetes cluster, load balancer, NAT gateway, standby host, premium monitoring package or paid RPC/oracle subscription is proposed. A single reused host has an outage risk; an external heartbeat can detect it but does not supply failover. Existing worker/journal isolation and failure semantics must be retained when colocating services.

## Raw transaction-fee evidence, not a keeper forecast

These are official-host unsigned controlled-state simulation estimates from the isolated storage candidate. They contain no real signatures and no live transaction application. Raw units are stroops. The maximum data-restored open example is **45,047,928 stroops**, of which **44,787,507** is modeled rent. It is an open-position lifecycle transaction, not a scheduled keeper checkpoint.

| Measured flow / state | Sample count | Observed total-fee range | Maximum-row components: inclusion + non-refundable + refundable + rent |
|---|---:|---:|---|
| `checkpoint_rewards`, warm fixture | 16 | 2,896–1,751,897 | 100 + 28,217 + 1,045 + 1,722,535 |
| `checkpoint_rewards`, data-restored fixture | 16 | 4,965–11,261,971 | 100 + 36,064 + 1,045 + 11,224,762 |
| `swap_open_position_v3`, warm fixture | 2 | 3,587,814–3,611,632 | 100 + 162,408 + 15,147 + 3,433,977 |
| `swap_open_position_v3`, data-restored fixture | 2 | 45,025,899–45,047,928 | 100 + 245,174 + 15,147 + 44,787,507 |
| `liquidate_funded_v3`, warm fixture | 8 | 5,243,329–7,163,781 | 100 + 112,408 + 5,049 + 7,046,224 |
| `liquidate_funded_v3`, data-restored fixture | 8 | 18,258,487–18,480,840 | 100 + 206,907 + 5,049 + 18,268,784 |

Sources are `target/cl-storage-candidate-{pending,roots,liquidation}-{live,data}/transactions.jsonl`. Each displayed maximum row reports successful simulated execution and selected invocation/resource limits passing; this does not turn missing lifecycle/transaction/release gates green. No deposit/funding/setup call is being substituted for a keeper fee.

The separate artificial code/instance-restoration experiment is substantially more expensive: `cl-storage-candidate-roots-retry` reports `finish_close_position_v3` at **2,924,308,051 stroops**, including **2,923,993,203** modeled rent. That deliberately cold-state diagnostic must not be mixed into a warm monthly average or ignored when planning restoration funding. Effective TTL/rent for handled-failure placeholders and full transaction application remain separate evidence gaps. No valid worst-case operating cap follows from these samples.

A future monthly budget must explicitly specify operation counts and state assumptions:

`monthly_stroops = Σ(actual successful and failed included transaction fees) + explicit restoration/TTL-maintenance fees`

For planning before submission, estimate each operation class from a freshly simulated envelope at the then-current network configuration, with its actual authorization shape, footprint, restored entries, rent and agreed fee margin. Keep recurring checkpoints separate from user opens/closes, event-driven funded recovery, one-time initialization and archive restoration. No monthly polling frequency, yearly restoration rate, liquidation frequency or XLM/USD conversion is assumed here. Liquidator repayment capital and account reserves are funding requirements, not hosting expense or transaction fees; their production sizes remain unqualified.

## Minimal next step

Obtain the non-secret AWS/RPC/monitoring inventory above, reuse the existing primary oracle, and allocate a bounded keyless process only after confirming host isolation and headroom. Review existing provider usage and logs before adding subscriptions. Measure the intended keeper decision/read cadence separately from actual submit cadence and identify who funds archival retention/restoration. The present no-signing/no-broadcast rule means this memo authorizes neither a recurring job nor any transaction. No infrastructure changes were made.
