//! Arithmetic and state transitions with no external calls or holder iteration.
//!
//! A caller invokes the allocation functions for each original entitlement owner.
//! Production storage must preserve that owner across receipt custody, transfers,
//! exit and seizure; substituting the current share holder is not valid.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Overflow,
    ZeroDenominator,
    InvalidFraction,
    EmptyDeposit,
    InsolventSupply,
    UnownedAssets,
    ZeroShares,
    NonMonotonicReading,
    ClaimRequired,
    RewardWaiverRequired,
    BalanceDecreased,
}

/// Checked multiplication deliberately fails closed rather than overflowing.
/// Integration may replace this with reviewed wide-integer arithmetic while
/// retaining the same floor rounding; do not silently truncate either operand.
pub fn mul_div_floor(value: u128, numerator: u128, denominator: u128) -> Result<u128, Error> {
    if denominator == 0 {
        return Err(Error::ZeroDenominator);
    }
    value
        .checked_mul(numerator)
        .ok_or(Error::Overflow)
        .map(|v| v / denominator)
}

/// Reward amount demonstrably earned by a holder at the OLD ownership weights.
/// Any division remainder remains separately accounted, never credited at the
/// next ownership weights by carrying it through a shared per-share accumulator.
pub fn earned_at_boundary(
    accrual: u128,
    old_shares: u128,
    old_supply: u128,
) -> Result<u128, Error> {
    if old_shares > old_supply {
        return Err(Error::InvalidFraction);
    }
    if old_supply == 0 {
        // No holder owns this accrual. The caller quarantines the whole amount.
        return Ok(0);
    }
    mul_div_floor(accrual, old_shares, old_supply)
}

/// Only Verified is an attribution input. The integration must establish
/// pending-view/claim equivalence against the pinned venue WASM before it may
/// construct Verified. An arbitrary readable estimate is insufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingReading {
    Verified(u128),
    Estimate(u128),
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryDelta {
    /// Known accrual since the previous verified boundary, owned at old weights.
    Proven(u128),
    /// Missing boundary, with an unknown amount. Persist an entitlement record
    /// with the original owners. This is NOT a zero-reward observation.
    Unresolved,
    /// Amount now measurable, but its split across earlier ownership changes is
    /// still unknown. Keep all of it in the affected interval's quarantine.
    Quarantine(u128),
}

/// Cumulative readings belong to ONE interval between successful claims.
/// A gap taints attribution until the next verified observation establishes a
/// new boundary; that later observation does not repair the missing history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AccrualBoundaries {
    last_verified: u128,
    gap: bool,
}

impl AccrualBoundaries {
    pub fn observe(&mut self, reading: PendingReading) -> Result<BoundaryDelta, Error> {
        let PendingReading::Verified(total) = reading else {
            self.gap = true;
            return Ok(BoundaryDelta::Unresolved);
        };
        let delta = total
            .checked_sub(self.last_verified)
            .ok_or(Error::NonMonotonicReading)?;
        let outcome = if self.gap {
            BoundaryDelta::Quarantine(delta)
        } else {
            BoundaryDelta::Proven(delta)
        };
        self.last_verified = total;
        self.gap = false;
        Ok(outcome)
    }

    /// Call only after the old interval has been settled or fully quarantined.
    /// Persisted interval records are not erased by this new-interval tracker.
    pub fn after_successful_claim() -> Self {
        Self::default()
    }

    /// A successful claim supplies backing, not missing attribution. Reconcile
    /// BEFORE resetting this tracker. A still-open measurement gap makes the
    /// entire ambiguous claim unavailable for distribution pending evidence.
    pub fn settle_claim(
        &self,
        received: u128,
        proven_total: u128,
        unresolved_known_units: u128,
    ) -> Result<ClaimSettlement, Error> {
        if self.gap {
            Ok(ClaimSettlement::entirely_unresolved(received))
        } else {
            ClaimSettlement::new(received, proven_total, unresolved_known_units)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipChange {
    Ordinary,
    VoluntaryEmergencyRedemption { accept_reward_waiver: bool },
    ForcedLiquidation,
    RepayAndRelease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewardTreatment {
    /// Cash arrived; reconcile the OLD interval before changing ownership.
    /// This is never permission to credit the whole receipt at current weights.
    ReconcileClaim,
    ProvenReceivable,
    Quarantine,
    Waived,
}

/// A policy decision only: the contract must separately authorize each recovery
/// caller AND check its permitted position state. There is intentionally no
/// waiver argument on forced liquidation or repayment-based collateral release.
pub fn treatment(
    change: OwnershipChange,
    claim_succeeded: bool,
    boundary_proven: bool,
) -> Result<RewardTreatment, Error> {
    if claim_succeeded {
        return Ok(RewardTreatment::ReconcileClaim);
    }
    if change == OwnershipChange::Ordinary {
        return Err(Error::ClaimRequired);
    }
    if boundary_proven {
        return Ok(RewardTreatment::ProvenReceivable);
    }
    match change {
        OwnershipChange::VoluntaryEmergencyRedemption {
            accept_reward_waiver: true,
        } => Ok(RewardTreatment::Waived),
        OwnershipChange::VoluntaryEmergencyRedemption {
            accept_reward_waiver: false,
        } => Err(Error::RewardWaiverRequired),
        OwnershipChange::ForcedLiquidation | OwnershipChange::RepayAndRelease => {
            Ok(RewardTreatment::Quarantine)
        }
        OwnershipChange::Ordinary => Err(Error::ClaimRequired),
    }
}

/// Reconciliation of one upstream claim against one interval's expected reward
/// units, including known-size unresolved units. Owners supply their ORIGINAL
/// proven units, not current shares. Excess and rounding are never assigned to
/// current holders implicitly. If any interval portion still has unknown size,
/// the entire unresolved claim must be quarantined before calling this routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimSettlement {
    received: u128,
    proven_total: u128,
    expected_total: u128,
    allocated: u128,
    allocated_units: u128,
}

impl ClaimSettlement {
    pub fn new(
        received: u128,
        proven_total: u128,
        unresolved_known_units: u128,
    ) -> Result<Self, Error> {
        Ok(Self {
            received,
            proven_total,
            expected_total: proven_total
                .checked_add(unresolved_known_units)
                .ok_or(Error::Overflow)?,
            allocated: 0,
            allocated_units: 0,
        })
    }

    /// Allocate one original owner's proven units. The storage caller must
    /// ensure each entitlement is visited once and cannot claim in two epochs.
    pub fn owner_amount(&mut self, proven_units: u128) -> Result<u128, Error> {
        let next_units = self
            .allocated_units
            .checked_add(proven_units)
            .ok_or(Error::Overflow)?;
        if next_units > self.proven_total {
            return Err(Error::InvalidFraction);
        }
        let available = self.received.min(self.expected_total);
        let amount = if self.expected_total == 0 {
            0
        } else {
            mul_div_floor(available, proven_units, self.expected_total)?
        };
        let next_allocated = self.allocated.checked_add(amount).ok_or(Error::Overflow)?;
        self.allocated_units = next_units;
        self.allocated = next_allocated;
        Ok(amount)
    }

    /// All undistributed backing: unresolved units, unexplained excess and
    /// rounding residue. Keep it under the interval's quarantine identifier.
    pub fn quarantine(&self) -> u128 {
        self.received - self.allocated
    }

    /// No bounded attributable split is available. Preserve all received tokens
    /// until venue evidence or the governed attribution path resolves ownership.
    pub fn entirely_unresolved(received: u128) -> Self {
        Self {
            received,
            proven_total: 0,
            expected_total: 0,
            allocated: 0,
            allocated_units: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeSplit {
    pub withdrawing_holder: u128,
    pub remaining_holders: u128,
}

/// Aquarius may collect ALL fees on a partial principal withdrawal. Only the
/// withdrawing holder's fraction is paid; the remainder becomes BOOKED idle
/// assets for existing shares. The underlying principal is accounted separately.
pub fn split_collected_fees(
    all_fees: u128,
    burned_shares: u128,
    supply_before: u128,
) -> Result<FeeSplit, Error> {
    if supply_before == 0 || burned_shares > supply_before {
        return Err(Error::InvalidFraction);
    }
    let withdrawing_holder = mul_div_floor(all_fees, burned_shares, supply_before)?;
    Ok(FeeSplit {
        withdrawing_holder,
        remaining_holders: all_fees - withdrawing_holder,
    })
}

/// Share minting uses BOOKED pre-deposit NAV after fee collection. Execution
/// costs belong to the depositor: value_added is the post-zap net value, never
/// the original transfer amount. Raw token donations must not enter either NAV.
pub fn shares_for_net_deposit(
    value_added: u128,
    supply_before: u128,
    booked_nav_before: u128,
) -> Result<u128, Error> {
    if value_added == 0 {
        return Err(Error::EmptyDeposit);
    }
    if supply_before == 0 {
        if booked_nav_before != 0 {
            return Err(Error::UnownedAssets);
        }
        return Ok(value_added);
    }
    if booked_nav_before == 0 {
        return Err(Error::InsolventSupply);
    }
    let shares = mul_div_floor(value_added, supply_before, booked_nav_before)?;
    if shares == 0 {
        return Err(Error::ZeroShares);
    }
    Ok(shares)
}

/// Full output produced by an operation, independent of any previous donation.
/// A quote is a minimum guard elsewhere, not a cap on this measured output.
pub fn operation_output(balance_before: u128, balance_after: u128) -> Result<u128, Error> {
    balance_after
        .checked_sub(balance_before)
        .ok_or(Error::BalanceDecreased)
}
