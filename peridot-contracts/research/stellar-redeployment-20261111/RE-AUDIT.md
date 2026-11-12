# Lending changes requiring review

Changes to previously reviewed source are explicit; this is not an assertion that the new WASMs are unchanged audited binaries.

- Accepted e3e6f73: flash loans use the existing durable cash-borrowing switch and reserve floor. Removing the guard failed the scoped mutation check.
- Fresh default initialization writes cash policy zero / borrowing false. Missing default reserve configuration now means 100% cash reserve, never legacy routing; the lp-engine artifact is excluded from this deployment.
- Missing supply/borrow caps reject fresh risk. Present zero still explicitly means uncapped. Recovery cap fallbacks are unchanged. Existing persistent cap TTL maintenance is retained.
- Missing entire controller pause map means paused. An initialized map without a market entry remains unpaused; explicit pauses still expire after 72 hours.
- Receipt permissionless keepalive renews the instance containing policy and permanent supply.
- Exact U256 proportional mint and redemption replace the truncated exchange-rate quote in financial conversions.
- Explicit Ledger-funded immutable permanent seed, minimum 1,000 raw shares, held by the receipt itself; transfer, withdrawal and seizure preserve it. Ordinary deposits require seed and never silently fund it.

The final scan snapshot is e3eab76a74028a394b0cde4e48c2ec42662811cc on review/lending-only-20261111. It includes 33 lending-package/manifest files. Scan scope excludes unrelated contracts, history, research and wallet artifacts; dependencies are not fully audited by this scan. Original and intermediate findings remain recorded. Medium+ findings require a fix/test or direct semantic counterexample test, not reasoning-only dismissal.

Ledger-only reserve/fee withdrawal and strategy binding are accepted capabilities under the operator's clarified rule. They must not be used in supply-only mode. Public underlying outflows remain subject to the durable borrowing checks; no external strategy is bound. No old contract is invoked, upgraded or reused.
