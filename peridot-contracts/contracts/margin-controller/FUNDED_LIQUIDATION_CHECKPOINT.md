# Funded liquidation: principal-settlement checkpoint

Base: `47858c1a13c2588970395faa2212e7b68bd33060` (`leveraged-fix`).

This is a **native-only prototype**, not an LP liquidation release. The new
entrypoint exists only in native tests or under the non-default
`funded-liquidation-prototype` feature. Enabling that feature for a WASM target
is a compile error. The ordinary production build does not export this route.

## Implemented boundary

`liquidate_funded_v3(liquidator, position_id, max_repay, min_seize_ptokens)`
returns `(actual_repaid, seized_ptokens)`.

For an unhealthy, active V3 position, the liquidator authorizes the fixed maximum
payment. The real debt receipt vault pulls that amount directly, repays the
current full debt, and refunds any excess. The controller transfers receipt
pTokens worth debt plus the configured bonus, rounded down and capped at the
position's collateral. It credits all remaining pTokens to the original owner's
free margin. Repayment and seizure occur in one transaction.

The route requires fresh nonzero oracle prices and respects liquidation pauses.
A failed fresh read cannot use the positive cached value. It does not quote a
pool, invoke a facade, redeem a strategy, consume controller cash, or request a
flash loan. The exact minimum seizure and maximum payment protect the funded
liquidator. If a gap leaves insufficient collateral, the liquidator can accept a
lower minimum and fully repay anyway; this is **not** an assurance that anyone
will fund such a transaction.

The first checkpoint accepts only `Open` positions. It intentionally rejects
pending opens, staged closes, and already-started staged liquidations rather
than guessing where their collateral is held.

## Validation

Run from `peridot-contracts`:

```sh
CARGO_BUILD_JOBS=2 cargo test -p margin-controller funded_prototype
```

Fixture setup reuses existing native mock-auth helpers. Before invoking the new
entrypoint, every test clears blanket authorization. Successful execution grants
only the exact liquidator → repayment → token-transfer tree. The controller's
authorization of receipt pToken transfer is exercised as a real contract call.

Coverage: funding plus refund; capped seizure and borrower surplus; controller
cash isolation; absent pool implementation; missing root authorization; missing
nested transfer authorization; failed fresh price with a valid cache; atomic
rollback when seizure fails after repayment; health, minimum payout, maximum
payment, zero-price, and pause rejection. These tests use native contracts and
mock oracle prices. They do not establish compiled-WASM resource use.

Results on 2026-10-31 (Rust 1.99.1, pinned Cargo.lock, SDK 25.3.1):

- Full native margin-controller suite: **117 passed, 0 failed, 2 ignored**,
  including seven new funded-liquidation tests. The two existing ignored tests
  require explicit live/new WASM paths; they were not run.
- `stellar contract build --package margin-controller --features
  funded-liquidation-prototype --locked` with Stellar CLI 28.1.0 reached the
  deliberate compile error: the prototype cannot be built as a release WASM.
  Compilation used the clearly test-only zero-account public key
  `GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF` for the four
  init-admin environment guards. No production configuration was changed.
- The default Stellar CLI build succeeded with those test-only admin values:
  86,091-byte optimized WASM, 72 exported functions, **no
  `liquidate_funded_v3` export**. Artifact hash:
  `6a27fab966cc51244ea9067137c1b93e1dd8d3d391382d7b4a1463a9394306a3`.
  This verifies exclusion of the prototype, not its execution as WASM.
- Formatting of the added modules and `git diff --check` passed.

## Still required before the product gate can pass

- Authenticated beneficial-owner/position context across receipt-market and
  controller custody, including every free-margin and pending-open transition.
- A pre-seizure reward checkpoint which retains the borrower's historical
  entitlement; an unknown boundary remains quarantined, without a liquidation
  waiver. Receipt pToken transfer alone does not move the share vault's
  beneficial-owner reward ledger.
- Delivery of underlying share tokens or the two LP assets through an authorized
  recovery path, independent of the facade and reward availability. This
  prototype delivers receipt pTokens only.
- Safe recovery from each staged position state; this prototype must not be
  substituted into the staged-liquidation keeper.
- Validity propagation for share NAV/backing, beyond a successful price read.
- Real compiled integration, mainnet-state resource measurements, adversarial
  review, and measured funded-liquidator acceptance and capacity. No capacity,
  launch maintenance margin, or production settings are chosen here.

Re-audit scope before activation: funded settlement, seizure valuation and
rounding, exact user/contract authorization, and the future reward/recovery
hooks. Receipt-vault code is unchanged in this checkpoint.
