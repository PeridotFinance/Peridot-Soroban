use crate::accounting::*;

#[test]
fn alice_rewards_are_settled_before_bob_mints() {
    // Alice is the only holder when 10 units accrue. Bob's later deposit cannot
    // alter the recorded amount even though he owns half when payment is made.
    let alice = earned_at_boundary(10, 100, 100).unwrap();
    let mut settlement = ClaimSettlement::new(10, alice, 0).unwrap();
    assert_eq!(settlement.owner_amount(alice), Ok(10));
    assert_eq!(settlement.owner_amount(0), Ok(0)); // Bob
    assert_eq!(settlement.quarantine(), 0);
}

#[test]
fn emission_change_produces_five_and_ninety_five() {
    let alice = earned_at_boundary(10, 50, 100).unwrap();
    let bob_before = earned_at_boundary(10, 50, 100).unwrap();
    // Alice exits. The later 90 belong entirely to Bob regardless of duration.
    let bob_after = earned_at_boundary(90, 50, 50).unwrap();
    assert_eq!((alice, bob_before + bob_after), (5, 95));
    let mut settlement = ClaimSettlement::new(100, 100, 0).unwrap();
    assert_eq!(settlement.owner_amount(alice), Ok(5));
    assert_eq!(settlement.owner_amount(bob_before + bob_after), Ok(95));
    assert_eq!(settlement.quarantine(), 0);
}

#[test]
fn pending_totals_are_differenced_instead_of_counted_twice() {
    let mut tracker = AccrualBoundaries::default();
    assert_eq!(
        tracker.observe(PendingReading::Verified(10)),
        Ok(BoundaryDelta::Proven(10))
    );
    assert_eq!(
        tracker.observe(PendingReading::Verified(100)),
        Ok(BoundaryDelta::Proven(90))
    );
    assert_eq!(
        tracker.observe(PendingReading::Verified(100)),
        Ok(BoundaryDelta::Proven(0))
    );
}

#[test]
fn a_later_reading_does_not_reconstruct_a_missing_boundary() {
    let mut tracker = AccrualBoundaries::default();
    assert_eq!(
        tracker.observe(PendingReading::Verified(10)),
        Ok(BoundaryDelta::Proven(10))
    );
    // Ownership changes now; the missing amount must not be recorded as zero.
    assert_eq!(
        tracker.observe(PendingReading::Unavailable),
        Ok(BoundaryDelta::Unresolved)
    );
    assert_eq!(
        tracker.observe(PendingReading::Verified(100)),
        Ok(BoundaryDelta::Quarantine(90))
    );
    // Only accrual AFTER that new verified boundary can be attributed normally.
    assert_eq!(
        tracker.observe(PendingReading::Verified(130)),
        Ok(BoundaryDelta::Proven(30))
    );
}

#[test]
fn an_unverified_estimate_is_not_a_proven_boundary() {
    let mut tracker = AccrualBoundaries::default();
    assert_eq!(
        tracker.observe(PendingReading::Estimate(80)),
        Ok(BoundaryDelta::Unresolved)
    );
    assert_eq!(
        tracker.observe(PendingReading::Verified(100)),
        Ok(BoundaryDelta::Quarantine(100))
    );
}

#[test]
fn repeated_failed_reads_preserve_the_gap() {
    let mut tracker = AccrualBoundaries::default();
    tracker.observe(PendingReading::Unavailable).unwrap();
    tracker.observe(PendingReading::Estimate(10)).unwrap();
    assert_eq!(
        tracker.observe(PendingReading::Verified(40)),
        Ok(BoundaryDelta::Quarantine(40))
    );
}

#[test]
fn a_backwards_read_is_rejected_without_mutating_the_tracker() {
    let mut tracker = AccrualBoundaries::default();
    tracker.observe(PendingReading::Verified(100)).unwrap();
    let before = tracker;
    assert_eq!(
        tracker.observe(PendingReading::Verified(99)),
        Err(Error::NonMonotonicReading)
    );
    assert_eq!(tracker, before);
}

#[test]
fn a_new_claim_interval_resets_cumulative_measurement_only() {
    let mut tracker = AccrualBoundaries::after_successful_claim();
    assert_eq!(
        tracker.observe(PendingReading::Verified(7)),
        Ok(BoundaryDelta::Proven(7))
    );
}

#[test]
fn exited_alice_takes_her_shortfall_without_touching_bob() {
    let mut settlement = ClaimSettlement::new(80, 100, 0).unwrap();
    assert_eq!(settlement.owner_amount(100), Ok(80)); // Alice's original record
    assert_eq!(settlement.owner_amount(0), Ok(0)); // Bob's current balance irrelevant
    assert_eq!(settlement.quarantine(), 0);
}

#[test]
fn shortfall_is_proportional_between_original_owners() {
    let mut settlement = ClaimSettlement::new(80, 100, 0).unwrap();
    assert_eq!(settlement.owner_amount(50), Ok(40));
    assert_eq!(settlement.owner_amount(50), Ok(40));
    assert_eq!(settlement.quarantine(), 0);
}

#[test]
fn excess_has_no_implicit_current_holder() {
    let mut settlement = ClaimSettlement::new(120, 100, 0).unwrap();
    assert_eq!(settlement.owner_amount(100), Ok(100));
    assert_eq!(settlement.quarantine(), 20);
}

#[test]
fn unresolved_units_keep_backing_during_a_shortfall() {
    let mut settlement = ClaimSettlement::new(80, 50, 50).unwrap();
    assert_eq!(settlement.owner_amount(50), Ok(40));
    assert_eq!(settlement.quarantine(), 40);
    // Proven owners cannot consume the unresolved portion by supplying its units.
    assert_eq!(settlement.owner_amount(50), Err(Error::InvalidFraction));
    assert_eq!(settlement.quarantine(), 40);
}

#[test]
fn proven_allocation_cannot_spend_the_quarantine_budget() {
    let mut settlement = ClaimSettlement::new(100, 50, 50).unwrap();
    assert_eq!(settlement.owner_amount(100), Err(Error::InvalidFraction));
    assert_eq!(settlement.owner_amount(50), Ok(50));
    assert_eq!(settlement.quarantine(), 50);
}

#[test]
fn successful_claim_after_a_missing_boundary_still_requires_quarantine() {
    let mut tracker = AccrualBoundaries::default();
    tracker.observe(PendingReading::Verified(10)).unwrap();
    tracker.observe(PendingReading::Unavailable).unwrap();
    assert_eq!(
        treatment(OwnershipChange::Ordinary, true, false),
        Ok(RewardTreatment::ReconcileClaim)
    );
    let mut settlement = tracker.settle_claim(100, 10, 0).unwrap();
    assert_eq!(settlement.owner_amount(10), Err(Error::InvalidFraction));
    assert_eq!(settlement.quarantine(), 100);
    // Resetting the live tracker does not resolve the persisted old interval.
    let _new_interval = AccrualBoundaries::after_successful_claim();
    assert_eq!(settlement.quarantine(), 100);
}

#[test]
fn an_unknown_size_claim_is_entirely_quarantined() {
    let mut settlement = ClaimSettlement::entirely_unresolved(100);
    assert_eq!(settlement.owner_amount(1), Err(Error::InvalidFraction));
    assert_eq!(settlement.quarantine(), 100);
}

#[test]
fn rounding_is_conserved_in_the_original_interval() {
    let mut settlement = ClaimSettlement::new(10, 3, 0).unwrap();
    // All expected entitlement units are paid at par; unexplained excess stays.
    let paid = settlement.owner_amount(1).unwrap()
        + settlement.owner_amount(1).unwrap()
        + settlement.owner_amount(1).unwrap();
    assert_eq!(paid + settlement.quarantine(), 10);
    assert_eq!(settlement.quarantine(), 7);
    let mut shortfall = ClaimSettlement::new(2, 3, 0).unwrap();
    assert_eq!(shortfall.owner_amount(1), Ok(0));
    assert_eq!(shortfall.owner_amount(1), Ok(0));
    assert_eq!(shortfall.owner_amount(1), Ok(0));
    assert_eq!(shortfall.quarantine(), 2);
}

#[test]
fn successful_zero_claims_allow_repeated_normal_movements() {
    for _ in 0..4 {
        assert_eq!(
            treatment(OwnershipChange::Ordinary, true, false),
            Ok(RewardTreatment::ReconcileClaim)
        );
        assert_eq!(earned_at_boundary(0, 100, 100), Ok(0));
    }
}

#[test]
fn normal_operations_never_use_receivables() {
    assert_eq!(
        treatment(OwnershipChange::Ordinary, false, true),
        Err(Error::ClaimRequired)
    );
    assert_eq!(
        treatment(OwnershipChange::Ordinary, false, false),
        Err(Error::ClaimRequired)
    );
}

#[test]
fn forced_liquidation_has_no_waiver_and_preserves_unknown_rewards() {
    assert_eq!(
        treatment(OwnershipChange::ForcedLiquidation, false, false),
        Ok(RewardTreatment::Quarantine)
    );
    assert_eq!(
        treatment(OwnershipChange::ForcedLiquidation, false, true),
        Ok(RewardTreatment::ProvenReceivable)
    );
    assert_eq!(
        treatment(OwnershipChange::RepayAndRelease, false, false),
        Ok(RewardTreatment::Quarantine)
    );
}

#[test]
fn only_voluntary_unproven_redemption_requires_the_holders_waiver() {
    assert_eq!(
        treatment(
            OwnershipChange::VoluntaryEmergencyRedemption {
                accept_reward_waiver: false
            },
            false,
            false
        ),
        Err(Error::RewardWaiverRequired)
    );
    assert_eq!(
        treatment(
            OwnershipChange::VoluntaryEmergencyRedemption {
                accept_reward_waiver: true
            },
            false,
            false
        ),
        Ok(RewardTreatment::Waived)
    );
    assert_eq!(
        treatment(
            OwnershipChange::VoluntaryEmergencyRedemption {
                accept_reward_waiver: false
            },
            false,
            true
        ),
        Ok(RewardTreatment::ProvenReceivable)
    );
}

#[test]
fn partial_withdrawal_keeps_other_holders_fees() {
    let split = split_collected_fees(20, 60, 100).unwrap();
    assert_eq!(
        split,
        FeeSplit {
            withdrawing_holder: 12,
            remaining_holders: 8
        }
    );
    let last = split_collected_fees(split.remaining_holders, 40, 40).unwrap();
    assert_eq!(last.withdrawing_holder, 8);
    assert_eq!(last.remaining_holders, 0);
    assert_eq!(split.withdrawing_holder + last.withdrawing_holder, 20);
}

#[test]
fn fee_rounding_does_not_overpay_the_first_exiter() {
    assert_eq!(
        split_collected_fees(1, 1, 3),
        Ok(FeeSplit {
            withdrawing_holder: 0,
            remaining_holders: 1
        })
    );
}

#[test]
fn the_depositor_bears_zap_cost() {
    // Alice has 100 shares backed by 100. Bob transfers 100 but adds only 98
    // after his zap. He receives 98 shares, preserving Alice's unit NAV.
    assert_eq!(shares_for_net_deposit(98, 100, 100), Ok(98));
    assert_eq!(198 * 100 / 198, 100);
}

#[test]
fn donations_do_not_subsidize_outputs_or_change_booked_nav() {
    assert_eq!(operation_output(5, 105), Ok(100));
    assert_eq!(operation_output(500, 600), Ok(100));
    // Raw balances intentionally are not accepted by the minting function.
    assert_eq!(shares_for_net_deposit(100, 100, 100), Ok(100));
}

#[test]
fn no_supply_cannot_assign_rewards_or_inherit_unowned_nav() {
    assert_eq!(earned_at_boundary(10, 0, 0), Ok(0));
    assert_eq!(shares_for_net_deposit(100, 0, 1), Err(Error::UnownedAssets));
    assert_eq!(shares_for_net_deposit(100, 0, 0), Ok(100));
}

#[test]
fn invalid_amounts_and_overflow_fail_closed() {
    assert_eq!(
        earned_at_boundary(10, 101, 100),
        Err(Error::InvalidFraction)
    );
    assert_eq!(shares_for_net_deposit(1, 1, 0), Err(Error::InsolventSupply));
    assert_eq!(shares_for_net_deposit(1, 1, 2), Err(Error::ZeroShares));
    assert_eq!(shares_for_net_deposit(0, 1, 1), Err(Error::EmptyDeposit));
    assert_eq!(split_collected_fees(10, 2, 1), Err(Error::InvalidFraction));
    assert_eq!(mul_div_floor(u128::MAX, 2, 1), Err(Error::Overflow));
    assert_eq!(mul_div_floor(1, 1, 0), Err(Error::ZeroDenominator));
    assert_eq!(operation_output(2, 1), Err(Error::BalanceDecreased));
    assert_eq!(ClaimSettlement::new(1, u128::MAX, 1), Err(Error::Overflow));
}

#[test]
fn settlement_conservation_holds_over_bounded_adversarial_inputs() {
    // Crosses zero, shortfall, exact and excess claims with two proven owners
    // and an unresolved bucket; this checks conservation across many partitions.
    for alice in 0..15 {
        for bob in 0..15 {
            for unknown in 0..5 {
                for received in 0..40 {
                    let mut s = ClaimSettlement::new(received, alice + bob, unknown).unwrap();
                    let a = s.owner_amount(alice).unwrap();
                    let b = s.owner_amount(bob).unwrap();
                    assert!(a <= alice && b <= bob);
                    assert_eq!(a + b + s.quarantine(), received);
                }
            }
        }
    }
}
