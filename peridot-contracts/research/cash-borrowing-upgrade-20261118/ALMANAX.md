# Almanax scan plan: 2026-11-18 go-live

The Almanax project is Peridot-Soroban-Margin, repo PeridotFinance/Peridot-Soroban.
Almanax scans one commit diff at a time, so each commit below is scanned by its full SHA.

## Scan-config paths to add (repo-relative)
- `peridot-contracts/prototypes/cl-pyusd-vault-b-pilot-tools/` (keeper v5, Ledger owner tools, keeper v4 dependencies)
- `peridot-contracts/bots/cash-liquidator/`
- `peridot-contracts/research/cash-borrowing-activation-20261117/tools/`
- `peridot-contracts/research/cash-borrowing-ops-20261118/` (droplet bootstraps, `.sh`)

The contract `src/` paths are already in the config, because their scans started.
The commits include `.cjs`, `.mjs`, `.sh` and `.json` files. If the filter works by
extension, those types must be allowed too.

## Branch feat/cash-borrow-upgrade-scan (base 4c03928)

The upgrade commits were applied from `upgrade.patch`, sha256 `5d5bb001879cb33a7b985ea9cfe0c1acb443325b4441a31622035511305e6181`.
The resulting tree is byte-identical to `.wt-cash-upgrade`. The untracked
`harness/cash-borrow-footprint/` is not part of the patch and was not scanned.

| SHA | Contents | Result |
|---|---|---|
| de7ea249f905d3bf1b3a9b62c948faf63f2017b3 | keeper-plan-v4.cjs and keeper-streak-probe.cjs plus tests (846 lines) | `invalid: no changed files match the project's scan configuration` |
| 6778a185b1a743b4a86832001ff76d7d3ab441f6 | controller src (+335/-9) | scan 91bd3075-6665-455b-8086-9dba3f3d73b2 COMPLETE, 2 findings |
| 4989b0c52c0b146330054c96eecaa4e7d964918a | receipt src (+52/-6) | scan 2e4bd522-4d37-42e8-a801-5acfafda5738 COMPLETE, 1 finding |
| f99a84acde0a0f6bfc994b221429c1cea024f4e6 | controller tests (+698) | scan 376fec56-3cfe-4411-a9e7-a1dd6b597188 COMPLETE, 0 findings |
| 3d17af39edeb89df6ca20c8f65690abc309b61ea | receipt tests (+506) | scan c6b1cf7d-e443-416f-aa0c-53a267ee2299 COMPLETE, 0 findings |

de7ea24 isolates two files from 355f739, byte-identical to that commit.
keeper-inspect-v5, keeper-worker-v5 and keeper-loop-v5 require these modules at runtime.

## Branch feat/keeper-v5-cash-liquidator-scan (scan again after the paths are added)

| SHA | Contents | Result |
|---|---|---|
| 355f739599b7d7bbe5309672872282a4bef351f9 | v2–v4 history (11.5k lines) | `invalid: commit diff is too large to scan` |
| 526f2bfe23c6b5d6cd37162b921755c35d7f3e2e | keeper v5 and Ledger owner call/claim tools | `invalid: no changed files match the project's scan configuration` |
| 9b989d5fb82e5f66f5b7423f992c6862028be025 | cash liquidator | same as above |
| 80c7c452090a5ab6762de5d693f5b279fc5a2266 | ceremony tooling | same as above |
| b7a2906181c8ea16c9f5a7cfa8b7d62b255cc0fd | v5 regression test | same as above |
| cabb369295b59cb6f72427cc36bd7bf297fa88e6 | watcher, refresher and guardian step | same as above. It spans two paths, so both must be added. |
| 6eb2fbff5aea52574bc1454b9e6b55f6c3df70a2 | droplet bootstraps | same as above |

Of 355f739, only the two files isolated in de7ea24 are live dependencies. No bootstrap or
deploy file invokes any other v2–v4 script. The rest of that commit stays unscanned.

## Open findings (none dismissed)
- HIGH 9a8aeb81-a443-42b7-8f13-14ef94b68276, receipt-vault storage.rs:61.
  The new `FlashLoansEnabled` variant is inserted before `PermanentMinimumSupply`, which the finding says shifts the `DataKey` discriminant.
  Reviewer note, unverified: Soroban `#[contracttype]` unit variants serialize by variant
  name (a Symbol in an ScVec), not by ordinal. This needs a test against a pre-upgrade
  `PermanentMinimumSupply` entry before it is triaged.
- MEDIUM 429ff659-d00f-463e-a32a-56b1833ccf21, controller contract.rs:256.
  The finding says instance-stored risk params (`MaxPriceAgeSecs`, `PriceBand`, `MaxUserMarkets`, `FallbackPricesEnabled`) are never TTL-bumped.
- LOW 5b1d98fa-15d9-4570-8db2-88789fef6397, controller contract.rs:746.
  `cancel_upgrade` does not bump the pending-upgrade TTL.

## Not covered
These files have uncommitted edits in the main checkout and are not on either branch:
- `scripts/cash_ttl_keeper.cjs`
- `scripts/cash_operator_checks.cjs`
- `scripts/cash_topup_commands.sh`

It still needs to be confirmed whether they are in scope.
