# CL share-vault accounting checkpoint

**Status: non-deployable accounting kernel. No Soroban contract or working vault
is implemented in this crate yet.** There is no `cdylib` target, and a Wasm build
is deliberately rejected. Nothing here changes the live `aquarius-lp-vault`,
controller configuration, collateral activation, or maintenance margin.

Source baseline: `PeridotFinance/Peridot-Soroban`, `leveraged-fix`, commit
`47858c1a13c2588970395faa2212e7b68bd33060`. Requirements: final handoffs 00 §8/8a
and 07 §3a. The kernel supports the first engineering checkpoint; it does not
satisfy the compiled V3/Aquarius integration gate by itself.

## Implemented and tested

- Accrual is allocated at the **old ownership boundary**, never by elapsed time.
  The emission-change example yields Alice 5 and Bob 95.
- Verified cumulative pending readings are differenced. Estimates are unresolved
  until a separate pinned-WASM comparison establishes their reliability.
- A missing boundary is not a zero reading. The next readable total quarantines
  its ambiguous delta; it cannot reconstruct the missing ownership split.
- Successful claims provide cash but still require old-interval reconciliation.
  An unresolved measurement gap quarantines the ambiguous receipt before the
  next live interval starts.
- Shortfalls are proportional to original proven reward units, including exited
  holders. Excess and division residue remain quarantined. Proven allocation
  cannot consume the unresolved-unit budget.
- Ordinary changes require a successful claim. Forced liquidation and
  repayment-based release have no waiver parameter. Only voluntary unproven
  emergency redemption can require the holder's explicit waiver.
- A partial principal withdrawal receives only its fraction of **all** fees
  swept by Aquarius. The remainder stays booked for the other holders.
- Depositors mint against their net post-zap contribution and pre-deposit booked
  NAV. Operation output excludes pre-existing balances and is not quote-capped.

The kernel uses checked `u128` multiplication and floor division. Inputs whose
intermediate product overflows are rejected. A reviewed wide-integer replacement
may expand that range; wrapping or saturating entitlement arithmetic is not safe.

## Integration boundary

These functions intentionally perform no external calls, authorization or
persistent accounting. Production code must record original entitlement owners,
prevent duplicate settlement of one entry, fund each claim once, and preserve
quarantine records after a holder exits. `ClaimSettlement::new` accepts only a
fully sized interval. If part of the interval has an unknown amount, use the
tracker's `settle_claim` (while its gap is open) or `entirely_unresolved`; do not
pretend the unknown amount is zero.

Future vault ABI agreed with the facade workstream (not implemented here):

```rust
deposit_underlying(from: Address, amount: i128, min_shares: i128) -> i128
redeem(from: Address, shares: i128, min_underlying_out: i128) -> i128
estimate_deposit(amount: u128) -> u128
estimate_redeem(shares: u128) -> u128
balance(id: Address) -> i128
transfer(from: Address, to: Address, amount: i128)
total_supply() -> i128
```

The facade's Aquarius `u128` amounts require checked conversion to the vault's
`i128` amounts. Quotes must return zero when the relevant execution direction is
unavailable. The final SEP-41 surface also needs allowances, `transfer_from`,
token metadata, events and TTL behavior; the list above is not a complete token
implementation.

## Work still required before the compiled checkpoint

1. Fork venue math/range/oracle logic into a sibling contract without changing
   the live crate. Replace raw-balance NAV with booked principal/fee inventory;
   the current single-holder withdrawal code spends entire paired inventory and
   cannot simply be made transferable.
2. Implement the persistent reward index and interval entitlement ledger,
   including dust, TTL, bounded resource use and governed quarantine attribution.
   Normal claims must reconcile old entries before mint/burn/transfer.
3. Add receipt-market and V3 **per-position** reward hooks. Pooling shares under
   the controller or receipt address loses owner identity. A pToken-only seizure
   needs a borrower checkpoint even if no underlying share token moves. The
   `swap(controller, ...)` facade ABI cannot infer the position owner by itself.
4. Implement authorized recovery entrypoints and custody transitions. A public
   transfer bypass or a boolean called `recovery` is not sufficient authorization.
5. Implement real in-kind redemption, directional pause behavior, donation
   isolation, reentrancy checks, deposit/remaining-balance limits and delayed
   upgrades. Unencumbered principal recovery must not require the oracle or an
   AQUA conversion.
6. Prove pending-view/claim equivalence, fee sweeps and zero-claim behavior against
   the pinned Aquarius Wasm. The existing repository pool test is explicitly a
   native/controlled-mock rehearsal and uses blanket auth mocks; it is not proof
   for the new facade authorization tree.
7. Compile and run the actual V3 → facade → vault → Aquarius route with real auth,
   reward failures, donated underlying **and shares**, resource measurement and
   the funded liquidation path. No native arithmetic test replaces this gate.

## Local validation

```sh
CARGO_BUILD_JOBS=2 cargo test -p cl-share-vault
```

The tests are deterministic accounting/policy tests, including a bounded
conservation sweep. They do not measure Soroban resources, invoke the real venue,
or establish production deployment readiness.

Validated on 2026-10-31 with Rust 1.99.1:

- `CARGO_BUILD_JOBS=2 /root/.cargo/bin/cargo test -p cl-share-vault`: **27 passed,
  0 failed**, including 45,000 bounded settlement partitions.
- `/root/.cargo/bin/cargo fmt -p cl-share-vault`: completed.
- `git diff --check`: passed.
- Clippy was not run successfully: this environment did not have the
  `cargo-clippy` component installed at validation time.

Error handling remains an integration obligation: a rejected or inconsistent
pending reading must not make principal recovery depend on reward availability.
The recovery caller records unresolved ownership and conservatively quarantines
the affected interval; it cannot turn a failed reading into a zero reward.
