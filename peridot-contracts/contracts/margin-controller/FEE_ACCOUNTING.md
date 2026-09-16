# Margin V3 fee ownership

Opening fees are reserved pTokens; closing fees are reserved underlying after
debt repayment. Both belong to the free-margin providers present when the fee is
charged. Locked position collateral is not part of the free-margin denominator.
Conversion time and the caller of `distribute_margin_fees` do not decide ownership.

## Deferred conversion

Each vault has a fee epoch with separate pToken and underlying indices. A charge
adds `floor(fee * 1e18 / total_free_ptokens)` to the respective index. Zero-weight
charges are recorded as orphans immediately, never assigned to later depositors.
Every free-balance mutation first checkpoints both indices using the old balance.
Thus moving out before conversion preserves already-earned entitlements, and
moving in cannot obtain past fees. Claims and returned position collateral also
checkpoint before increasing free balances.

Permissionless distribution deposits only reserved underlying. The actual minted
pToken delta determines conversion, not an estimated exchange rate. The completed
epoch records its final indices and conversion ratio. A cumulative converted
index lets an untouched account skip intervening completed epochs in constant
work: only its first unfinished epoch needs an individual history read. Its
balance cannot have changed in skipped epochs because mutations checkpoint first.
All entitlement operations round down. Unallocated rounding dust remains backed;
it is not reassigned to new entrants. Products use checked U256 intermediates and
checked u128 results. No participant or epoch scan is performed on a hot path.

If underlying cannot mint one pToken, distribution returns zero and retains the
whole epoch, including reserved opening shares. Principal transfers, position
settlement and withdrawal do not depend on conversion succeeding. A failed
deposit reverts atomically without clearing inventory or entitlements.

## Compatibility and operations

Existing stored structs and legacy fee-index/accrued keys are unchanged. New
epoch and account keys are separate. Upgrades from the fee-free V3 release need
no reward migration. **Before upgrading a deployment of the earlier experimental
batch-fee release, settle every old pending fee batch.** Historical weights cannot
be reconstructed from those batches; the new code rejects converting them or
mixing new fees into them without entitlement state. This is a deployment
precondition, not a way to repair historical misallocation.

Epoch, account and required closed-history reads renew persistent TTL. Closed
history is never deleted by distribution. Missing required history fails closed;
archived persistent data must be restored, not recreated with empty/default
entitlements. A newly created account starts at epoch zero: existing free capital
earns since activation, while every later balance increase has checkpointed first.

`get_claimable_margin_fees` includes settled pTokens only, including legacy claims.
Unconverted fee inventory is not immediately claimable. `get_margin_fee_index`
returns the legacy index plus the cumulative converted epoch index, still scaled
1e18. It is useful for historical fee-yield reporting, not a promise of forward
APY or a substitute for the user-specific claim getter. The public transaction
signatures and optional keeper distribution call are unchanged.

This accounting prevents retroactive fee capture. It does not impose a holding
period or prevent someone supplying capital before a future trade earns fees.
