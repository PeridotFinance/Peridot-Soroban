# Split top-ups — operator amounts supplied; prerequisites pending

The user added a full USDC exit/redeposit check and a TTL-maintenance gate before completing top-ups. See [the current runbook](../stellar-cash-ttl-20261112/RUNBOOK.md) and frozen `topup-plan.json` in that directory.

| Market | Additional total | First half | Second half |
|---|---:|---:|---:|
| USDC | 5900 | 2950 | 2950 |
| XLM | 20220.7548616 | 10110.3774308 | 10110.3774308 |
| PYUSD | 1900 | 950 | 950 |
| EURC | 1714.7107166 | 857.3553583 | 857.3553583 |

XLM/EURC use the 2026-11-12 13:19:52 UTC quotes for $2900/$1900. First execute the full 90-USDC exit, verify only the permanent 1 remains and the exchange rate is unchanged, then redeposit 90 and record both applied checkpoints. First halves follow after wallet funding. Second halves remain blocked until actual fee-only deployer TTL extension transactions apply after all first halves and exact TTL readbacks pass. No top-up or TTL application is claimed at this checkpoint.

The completed original seed/deposit/withdraw checks and fresh contract identities remain in `fresh-deployment-manifest.json` and `LIVE-CEREMONY-COMPLETE-20261112.md`. No top-up is a new permanent seed. The DefiLlama adapter change follows completion of the requested sequence; only new addresses will be listed, and no adapter PR is opened here.
