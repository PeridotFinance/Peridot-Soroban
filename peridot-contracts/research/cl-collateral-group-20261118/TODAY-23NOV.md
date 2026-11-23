# CL collateral group: uploads and configuration now, soak today, go-live after the scan (23 Nov 2026)

User decision 23 Nov: uploads, creates and configuration do not wait for the Almanax scan. The scan gates only the
go-live switch G45 and everything after it. Deposits and borrowing stay off until go-live. Migration of the 800 PYUSD
comes after go-live. Recorded in `tools/clg-mainnet.json` (`artifacts.frozen=true`, `userDecisions.orderAndDepositsOff`).

Fresh live-Mainnet simulations at ledger 65628197 (`clg-ceremony.cjs simulate-all`): G1–G6, G10, G59, G60 all pass.

| Step | CPU | Entries | Min fee XLM | Max fee XLM |
|---|---:|---:|---:|---:|
| G1 withdraw 500 XLM | 19.71M | 56 | 0.0043 | 0.0072 |
| G2 withdraw 600 USDC | 19.78M | 56 | 0.0044 | 0.0072 |
| G3 withdraw 60 PYUSD | 18.91M | 52 | 0.0058 | 0.0090 |
| G4 upload vault 6fd8599a | 106.66M (allowance 107.5M) | 2 | 167.66 | 201.19 |
| G5 upload receipt 424454b9 | 106.68M (allowance 107.5M) | 2 | 162.71 | 195.25 |
| G6 upload controller 642421f1 | 55.71M | 2 | 92.43 | 110.92 |
| G10 create USDC market (5630a701) | 1.72M | 3 | 0.0209 | 0.0271 |

G7–G9 and G11–G44 cannot be simulated before their targets exist; `step` re-simulates every one on live Mainnet
right before it is signed and refuses on any failure.

Deposits off before go-live (new emergency builder `pause-deposit` added 23 Nov, tests 11/11):
- vault v2 `set_paused(admin, true)` right after G26: durable and reversible; blocks new opens and investment only
  (in-kind close, emergency exit and seizure sync do not read it).
- USDC group market: controller `set_pause_deposit(usdcMarket, true)` right after the owner deposit G42. It expires
  after 72 h on its own; renew it if go-live slips past Thu 26 Nov.
- Borrowing: `set_cash_borrowing_enabled` stays false until G45.

Bot release for the Droplet: commit 8b71f32 (67/67 tests). Deterministic tarball:
`git archive --format=tar --mtime='2026-11-22 00:00:00 +0000' --prefix=cl-liquidator/ 8b71f32:peridot-contracts/bots/cl-liquidator | gzip -n -9`
sha256 `2aa4469eefbf1503c1ddb9e21d93dbc5b029f4ba61fff308826e9c26267f0670` (identical rebuilds seconds apart). Without `--mtime`, archiving a tree (commit:path) stamps the current time into every header, so the hash changes every second; the earlier eee9db4d figure was such a same-second coincidence and is void.
`git archive --format=tar.gz` is NOT reproducible here; never use it for SRC_SHA256.

Cash-market effect of G2: cash USDC supply 3,541 → 2,941 USDC; borrowable at the 70 % reserve 1,062 → 882 USDC
(the cash borrow cap is 900).

The numbered operator commands are in the chat message of 23 Nov and follow LAUNCH.md §5 and §6.
