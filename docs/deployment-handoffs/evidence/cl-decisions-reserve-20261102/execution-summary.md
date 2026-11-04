# Completed CL execution evidence

Compiled controlled state; synthetic external authorization; unsigned target-runtime simulation. Not live state, real signatures or release approval.

Runner exit: **1**. Stage 1 remains incomplete; release approval: **false**.

| Profile | Invocation | Lifecycle | Transaction | Checkpoint |
|---|---|---|---|---|
| internal-policy | FAIL | FAIL | FAIL | FAIL |
| captured-network-diagnostic | PASS | PASS | PASS | PASS |

| Profile | Suite | Passed | Failed | Ignored |
|---|---|---:|---:|---:|
| internal-policy | integrity | 1 | 0 | 0 |
| internal-policy | harness | 4 | 32 | 0 |
| internal-policy | archive | 0 | 3 | 0 |
| internal-policy | history-scale-archive | 0 | 2 | 0 |
| internal-policy | native-funded | 5 | 4 | 0 |
| captured-network-diagnostic | integrity | 1 | 0 | 0 |
| captured-network-diagnostic | harness | 36 | 0 | 0 |
| captured-network-diagnostic | archive | 3 | 0 | 0 |
| captured-network-diagnostic | history-scale-archive | 2 | 0 | 0 |
| captured-network-diagnostic | native-funded | 9 | 0 | 0 |

| Profile | SDK CPU max | SDK memory max | SDK entries max | Writes max |
|---|---:|---:|---:|---:|
| internal-policy | 100003731 | 16847283 | 137 | 30 |
| captured-network-diagnostic | 235456689 | 24514601 | 213 | 56 |

## internal-policy

Unsigned transactions: 214 succeeded, 17 expected rejections, 28 unexpected failures; 0 restoration variants. 28 failures lack execution resource measurements.

Fee component maxima (stroops; independent maxima, not one transaction):

| Inclusion | Non-refundable | Refundable | Rent | Total |
|---:|---:|---:|---:|---:|
| 100 | 72352 | 5577 | 4599374 | 4651047 |

History-scale SDK rows: 0; unsigned restoration variants: 0. Exact restored key XDR, decoded historical names, resource numbers and fee deltas are preserved in the JSON summary.

Recorded gate errors:

- missing_required_flows: 0
- lifecycle_errors: 106
  - test process did not succeed: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-harness.log
  - successful Cargo suite summary missing: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-harness.log
  - test process did not succeed: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-archive.log
  - successful Cargo suite summary missing: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-archive.log
  - test process did not succeed: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-history-scale-archive.log
  - successful Cargo suite summary missing: /Users/joshua/Peridot/Peridot-Soroban/peridot-contracts/target/cl-decisions-corrected/logs/internal-policy-history-scale-archive.log
  - proof test not completed successfully: deposit -> test::compiled_vault_core_flows
  - proof has no successful measured invocation: redeem -> redeem
  - proof test not completed successfully: redeem -> test::compiled_vault_core_flows
  - proof test not completed successfully: transfer -> test::compiled_vault_core_flows
  - proof has no successful measured invocation: emergency_redeem -> emergency_redeem
  - proof test not completed successfully: emergency_redeem -> test::compiled_vault_core_flows
  - proof has no successful measured invocation: claim_rewards -> claim_rewards
  - proof test not completed successfully: claim_rewards -> test::compiled_vault_core_flows
  - proof test not completed successfully: begin_open -> test::compiled_v3_checkpoint
  - proof test not completed successfully: swap_open_position_v3 -> test::compiled_v3_checkpoint
  - proof has no successful measured invocation: activate_open_position_v3 -> activate_open_position_v3
  - proof test not completed successfully: activate_open_position_v3 -> test::compiled_v3_checkpoint
  - proof has no successful measured invocation: prepare_close_position_v3 -> prepare_close_position_v3
  - proof test not completed successfully: prepare_close_position_v3 -> test::compiled_v3_checkpoint
  - proof has no successful measured invocation: swap_close -> swap_close
  - proof test not completed successfully: swap_close -> test::compiled_v3_checkpoint
  - proof has no successful measured invocation: finish_close -> finish_close
  - proof test not completed successfully: finish_close -> test::compiled_v3_checkpoint
  - proof has no successful measured invocation: funded_liquidation -> funded_liquidation
  - proof test not completed successfully: funded_liquidation -> test::funded_tests::compiled_funded_liquidation
  - proof has no successful measured invocation: repay_release -> repay_release
  - proof test not completed successfully: repay_release -> test::funded_tests::compiled_repay_release
  - proof has no successful measured invocation: claim_failure_recovery -> claim_failure_recovery
  - proof test not completed successfully: claim_failure_recovery -> test::compiled_venue_rewards_and_claim_failure
  - proof has no successful measured invocation: cancel_after_execute -> cancel_after_execute_rejection
  - proof has no successful measured invocation: cancel_after_execute -> execute_open_position_v3
  - proof test not completed successfully: cancel_after_execute -> test::recovery_state_tests::compiled_cancel_after_execute_is_rejection_then_repay_release
  - proof has no successful measured invocation: cancel_after_swap -> cancel_after_swap
  - proof test not completed successfully: cancel_after_swap -> test::funded_tests::compiled_interrupted_open_recovery_claim_outage
  - proof has no successful measured invocation: receipt_custody_ownership_change -> receipt_custody_ownership_change
  - proof test not completed successfully: receipt_custody_ownership_change -> test::compiled_receipt_margin_reward_custody
  - proof has no successful measured invocation: allowance_transfer_from -> allowance_transfer_from
  - proof test not completed successfully: allowance_transfer_from -> test::lifecycle_tests::compiled_allowance_reward_boundary_and_claim_rollback
  - proof has no successful measured invocation: quarantine_reconciliation -> quarantine_reconciliation
  - proof test not completed successfully: quarantine_reconciliation -> test::historical_tests::compiled_automatic_history_exited_owner_and_later_accrual
  - proof has no successful measured invocation: waiver_redistribution -> waiver_redistribution
  - proof test not completed successfully: waiver_redistribution -> test::historical_tests::compiled_governed_waiver_original_custody_coowner_exits_before_receipt
  - proof has no successful measured invocation: pending_claim_boundary_matrix -> pending_boundary_fractional_first_transfer
  - proof has no successful measured invocation: pending_claim_boundary_matrix -> pending_boundary_emission_change
  - proof has no successful measured invocation: pending_claim_boundary_matrix -> pending_boundary_shifted_range
  - proof has no successful measured invocation: pending_claim_boundary_matrix -> pending_boundary_returned_range
  - proof has no successful measured invocation: pending_claim_boundary_matrix -> pending_boundary_partial_exit
  - proof test not completed successfully: pending_claim_boundary_matrix -> test::venue_boundary_tests::compiled_pending_claim_boundary_matrix
  - proof has no successful measured invocation: failed_activation_recovery -> failed_activation_atomic
  - proof test not completed successfully: failed_activation_recovery -> test::recovery_state_tests::compiled_failed_activation_preserves_pending_recovery
  - proof has no successful measured invocation: expired_pending_resolution -> expired_pending_resolution
  - proof test not completed successfully: expired_pending_resolution -> test::recovery_state_tests::compiled_expired_swapped_permissionless_resolution
  - proof has no successful measured invocation: both_reward_calls_failed_funded_liquidation -> both_reward_calls_failed_funded_liquidation
  - proof test not completed successfully: both_reward_calls_failed_funded_liquidation -> test::adversarial_tests::compiled_both_reward_calls_fail_forced_settlement_preserves_history
  - proof has no successful measured invocation: funded_insufficient_liquidator_rollback -> funded_insufficient_liquidator_rollback
  - proof test not completed successfully: funded_insufficient_liquidator_rollback -> test::adversarial_tests::compiled_funded_adversarial_atomicity
  - proof has no successful measured invocation: funded_invalid_valuation_rollback -> funded_invalid_valuation_rollback
  - proof test not completed successfully: funded_invalid_valuation_rollback -> test::adversarial_tests::compiled_funded_adversarial_atomicity
  - proof has no successful measured invocation: funded_failed_collateral_transfer_rollback -> funded_failed_collateral_transfer_rollback
  - proof test not completed successfully: funded_failed_collateral_transfer_rollback -> test::adversarial_tests::compiled_funded_adversarial_atomicity
  - proof has no successful measured invocation: funded_unauthorized_rollback -> funded_unauthorized_rollback
  - proof test not completed successfully: funded_unauthorized_rollback -> test::adversarial_tests::compiled_funded_adversarial_atomicity
  - proof has no successful measured invocation: funded_repeated_execution_rollback -> funded_repeated_execution_rollback
  - proof test not completed successfully: funded_repeated_execution_rollback -> test::adversarial_tests::compiled_funded_adversarial_atomicity
  - proof has no successful measured invocation: historical_multiple_intervals -> historical_multiple_intervals
  - proof test not completed successfully: historical_multiple_intervals -> test::historical_tests::compiled_multiple_unsettled_intervals_exited_beneficiary
  - proof has no successful measured invocation: historical_shortfall -> governed_proportional_shortfall
  - proof test not completed successfully: historical_shortfall -> test::historical_shortfall_tests::compiled_governed_shortfall_proportional_exited_custody_owner
  - proof has no successful measured invocation: historical_unresolved_intervals -> historical_unresolved_intervals
  - proof test not completed successfully: historical_unresolved_intervals -> test::unresolved_stress_tests::compiled_multiple_unresolved_intervals_after_funded_exits
  - proof has no successful measured invocation: history_scale_64_owners -> history_scale_64_owner_exit
  - proof has no successful measured invocation: history_scale_64_owners -> history_scale_64_owner_settle
  - proof has no successful measured invocation: history_scale_64_owners -> history_scale_8_outages_claim
  - proof test not completed successfully: history_scale_64_owners -> test::history_scale_tests::compiled_history_scale_64_owners_128_changes_8_outages
  - proof has no successful measured invocation: history_scale_8_unresolved -> history_scale_8_funded_fault_exit
  - proof has no successful measured invocation: history_scale_8_unresolved -> history_scale_governed_attribute
  - proof has no successful measured invocation: history_scale_8_unresolved -> history_scale_governed_exited_settle
  - proof test not completed successfully: history_scale_8_unresolved -> test::history_scale_tests::compiled_history_scale_8_unresolved_custody_outages_governed_one
  - proof has no successful measured invocation: reserve_complete_open -> reserve_configured_swap
  - proof has no successful measured invocation: reserve_complete_open -> reserve_configured_activate
  - proof has no successful measured invocation: reserve_complete_open -> reserve_blocked_active_repayment
  - proof has no successful measured invocation: reserve_complete_open -> reserve_blocked_active_release
  - proof test not completed successfully: reserve_complete_open -> test::reserve_open_tests::compiled_reserve_complete_open_positive
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_floor_activate_open_position_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_floor_execute_open_position_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_floor_resolve_pending_open_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_disabled_activate_open_position_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_disabled_execute_open_position_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_disabled_resolve_pending_open_v3_rollback
  - proof has no successful measured invocation: reserve_complete_open_rollback -> reserve_blocked_pending_funded_recovery
  - proof test not completed successfully: reserve_complete_open_rollback -> test::reserve_open_tests::compiled_reserve_all_finalizers_reject_interstage_exit_or_pause_then_recover
  - proof has no successful measured invocation: reserve_no_liquidation_bypass -> reserve_healthy_liquidation_conversion_rejected
  - proof test not completed successfully: reserve_no_liquidation_bypass -> test::reserve_open_tests::compiled_reserve_liquidation_conversion_cannot_activate_healthy_pending
  - proof has no successful measured invocation: default_reserve_active_repay_release -> default_activate_open_position_v3
  - proof has no successful measured invocation: default_reserve_active_repay_release -> default_active_full_repayment_paused_shortfall
  - proof has no successful measured invocation: default_reserve_active_repay_release -> default_release_debt_free_position_v3
  - proof test not completed successfully: default_reserve_active_repay_release -> test::default_reserve_tests::compiled_default_reserve_active_repay_release
  - proof has no successful measured invocation: default_reserve_finalization_rollback -> default_post_cost_reserve_reject
  - proof has no successful measured invocation: default_reserve_finalization_rollback -> default_pending_recovery_repeat_reject
  - proof test not completed successfully: default_reserve_finalization_rollback -> test::default_reserve_tests::compiled_default_reserve_post_cost_finalizers_rollback_recover
  - proof test not completed successfully: default_short_pending_recovery -> test::default_reserve_tests::compiled_default_short_pending_recovery_paused_shortfall
  - proof has no successful measured invocation: default_missing_reserve_abi -> default_missing_reserve_abi_reject
  - proof test not completed successfully: default_missing_reserve_abi -> test::default_reserve_tests::compiled_default_missing_receipt_abi_fails_closed
  - proof has no successful measured invocation: default_pending_fee_refund_conservation -> default_intervening_fee_epoch_distribution
  - proof test not completed successfully: default_pending_fee_refund_conservation -> test::default_reserve_tests::compiled_default_pending_fee_refund_preserves_historical_fee_owner
- transaction_errors: 303
  - transaction row 2: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 2: transaction evidence mismatch: base_fee_source
  - transaction row 2: transaction evidence mismatch: limits_pass
  - transaction row 2: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 2: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 2: transaction evidence mismatch: signature_verification_measured
  - transaction row 2: transaction evidence mismatch: live_state_validation
  - transaction row 2: transaction outcome/success classification mismatch
  - transaction row 5: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 5: transaction evidence mismatch: base_fee_source
  - transaction row 5: transaction evidence mismatch: limits_pass
  - transaction row 5: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 5: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 5: transaction evidence mismatch: signature_verification_measured
  - transaction row 5: transaction evidence mismatch: live_state_validation
  - transaction row 5: transaction outcome/success classification mismatch
  - transaction row 14: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 14: transaction evidence mismatch: base_fee_source
  - transaction row 14: transaction evidence mismatch: limits_pass
  - transaction row 14: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 14: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 14: transaction evidence mismatch: signature_verification_measured
  - transaction row 14: transaction evidence mismatch: live_state_validation
  - transaction row 14: transaction outcome/success classification mismatch
  - transaction row 18: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 18: transaction evidence mismatch: base_fee_source
  - transaction row 18: transaction evidence mismatch: limits_pass
  - transaction row 18: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 18: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 18: transaction evidence mismatch: signature_verification_measured
  - transaction row 18: transaction evidence mismatch: live_state_validation
  - transaction row 18: transaction outcome/success classification mismatch
  - transaction row 19: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 19: transaction evidence mismatch: base_fee_source
  - transaction row 19: transaction evidence mismatch: limits_pass
  - transaction row 19: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 19: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 19: transaction evidence mismatch: signature_verification_measured
  - transaction row 19: transaction evidence mismatch: live_state_validation
  - transaction row 19: transaction outcome/success classification mismatch
  - transaction row 20: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 20: transaction evidence mismatch: base_fee_source
  - transaction row 20: transaction evidence mismatch: limits_pass
  - transaction row 20: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 20: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 20: transaction evidence mismatch: signature_verification_measured
  - transaction row 20: transaction evidence mismatch: live_state_validation
  - transaction row 20: transaction outcome/success classification mismatch
  - transaction row 25: transaction evidence mismatch: limits_pass
  - transaction row 40: transaction evidence mismatch: limits_pass
  - transaction row 66: transaction evidence mismatch: limits_pass
  - transaction row 77: transaction evidence mismatch: limits_pass
  - transaction row 84: transaction evidence mismatch: limits_pass
  - transaction row 87: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 87: transaction evidence mismatch: base_fee_source
  - transaction row 87: transaction evidence mismatch: limits_pass
  - transaction row 87: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 87: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 87: transaction evidence mismatch: signature_verification_measured
  - transaction row 87: transaction evidence mismatch: live_state_validation
  - transaction row 87: transaction outcome/success classification mismatch
  - transaction row 90: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 90: transaction evidence mismatch: base_fee_source
  - transaction row 90: transaction evidence mismatch: limits_pass
  - transaction row 90: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 90: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 90: transaction evidence mismatch: signature_verification_measured
  - transaction row 90: transaction evidence mismatch: live_state_validation
  - transaction row 90: transaction outcome/success classification mismatch
  - transaction row 93: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 93: transaction evidence mismatch: base_fee_source
  - transaction row 93: transaction evidence mismatch: limits_pass
  - transaction row 93: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 93: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 93: transaction evidence mismatch: signature_verification_measured
  - transaction row 93: transaction evidence mismatch: live_state_validation
  - transaction row 93: transaction outcome/success classification mismatch
  - transaction row 96: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 96: transaction evidence mismatch: base_fee_source
  - transaction row 96: transaction evidence mismatch: limits_pass
  - transaction row 96: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 96: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 96: transaction evidence mismatch: signature_verification_measured
  - transaction row 96: transaction evidence mismatch: live_state_validation
  - transaction row 96: transaction outcome/success classification mismatch
  - transaction row 97: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 97: transaction evidence mismatch: base_fee_source
  - transaction row 97: transaction evidence mismatch: limits_pass
  - transaction row 97: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 97: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 97: transaction evidence mismatch: signature_verification_measured
  - transaction row 97: transaction evidence mismatch: live_state_validation
  - transaction row 97: transaction outcome/success classification mismatch
  - transaction row 100: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 100: transaction evidence mismatch: base_fee_source
  - transaction row 100: transaction evidence mismatch: limits_pass
  - transaction row 100: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 100: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 100: transaction evidence mismatch: signature_verification_measured
  - transaction row 100: transaction evidence mismatch: live_state_validation
  - transaction row 100: transaction outcome/success classification mismatch
  - transaction row 101: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 101: transaction evidence mismatch: base_fee_source
  - transaction row 101: transaction evidence mismatch: limits_pass
  - transaction row 101: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 101: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 101: transaction evidence mismatch: signature_verification_measured
  - transaction row 101: transaction evidence mismatch: live_state_validation
  - transaction row 101: transaction outcome/success classification mismatch
  - transaction row 102: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 102: transaction evidence mismatch: base_fee_source
  - transaction row 102: transaction evidence mismatch: limits_pass
  - transaction row 102: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 102: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 102: transaction evidence mismatch: signature_verification_measured
  - transaction row 102: transaction evidence mismatch: live_state_validation
  - transaction row 102: transaction outcome/success classification mismatch
  - transaction row 103: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 103: transaction evidence mismatch: base_fee_source
  - transaction row 103: transaction evidence mismatch: limits_pass
  - transaction row 103: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 103: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 103: transaction evidence mismatch: signature_verification_measured
  - transaction row 103: transaction evidence mismatch: live_state_validation
  - transaction row 103: transaction outcome/success classification mismatch
  - transaction row 139: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 139: transaction evidence mismatch: base_fee_source
  - transaction row 139: transaction evidence mismatch: limits_pass
  - transaction row 139: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 139: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 139: transaction evidence mismatch: signature_verification_measured
  - transaction row 139: transaction evidence mismatch: live_state_validation
  - transaction row 139: transaction outcome/success classification mismatch
  - transaction row 142: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 142: transaction evidence mismatch: base_fee_source
  - transaction row 142: transaction evidence mismatch: limits_pass
  - transaction row 142: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 142: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 142: transaction evidence mismatch: signature_verification_measured
  - transaction row 142: transaction evidence mismatch: live_state_validation
  - transaction row 142: transaction outcome/success classification mismatch
  - transaction row 145: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 145: transaction evidence mismatch: base_fee_source
  - transaction row 145: transaction evidence mismatch: limits_pass
  - transaction row 145: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 145: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 145: transaction evidence mismatch: signature_verification_measured
  - transaction row 145: transaction evidence mismatch: live_state_validation
  - transaction row 145: transaction outcome/success classification mismatch
  - transaction row 221: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 221: transaction evidence mismatch: base_fee_source
  - transaction row 221: transaction evidence mismatch: limits_pass
  - transaction row 221: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 221: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 221: transaction evidence mismatch: signature_verification_measured
  - transaction row 221: transaction evidence mismatch: live_state_validation
  - transaction row 221: transaction outcome/success classification mismatch
  - transaction row 226: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 226: transaction evidence mismatch: base_fee_source
  - transaction row 226: transaction evidence mismatch: limits_pass
  - transaction row 226: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 226: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 226: transaction evidence mismatch: signature_verification_measured
  - transaction row 226: transaction evidence mismatch: live_state_validation
  - transaction row 226: transaction outcome/success classification mismatch
  - transaction row 231: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 231: transaction evidence mismatch: base_fee_source
  - transaction row 231: transaction evidence mismatch: limits_pass
  - transaction row 231: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 231: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 231: transaction evidence mismatch: signature_verification_measured
  - transaction row 231: transaction evidence mismatch: live_state_validation
  - transaction row 231: transaction outcome/success classification mismatch
  - transaction row 248: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 248: transaction evidence mismatch: base_fee_source
  - transaction row 248: transaction evidence mismatch: limits_pass
  - transaction row 248: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 248: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 248: transaction evidence mismatch: signature_verification_measured
  - transaction row 248: transaction evidence mismatch: live_state_validation
  - transaction row 248: transaction outcome/success classification mismatch
  - transaction row 249: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 249: transaction evidence mismatch: base_fee_source
  - transaction row 249: transaction evidence mismatch: limits_pass
  - transaction row 249: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 249: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 249: transaction evidence mismatch: signature_verification_measured
  - transaction row 249: transaction evidence mismatch: live_state_validation
  - transaction row 249: transaction outcome/success classification mismatch
  - transaction row 252: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 252: transaction evidence mismatch: base_fee_source
  - transaction row 252: transaction evidence mismatch: limits_pass
  - transaction row 252: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 252: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 252: transaction evidence mismatch: signature_verification_measured
  - transaction row 252: transaction evidence mismatch: live_state_validation
  - transaction row 252: transaction outcome/success classification mismatch
  - transaction row 253: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 253: transaction evidence mismatch: base_fee_source
  - transaction row 253: transaction evidence mismatch: limits_pass
  - transaction row 253: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 253: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 253: transaction evidence mismatch: signature_verification_measured
  - transaction row 253: transaction evidence mismatch: live_state_validation
  - transaction row 253: transaction outcome/success classification mismatch
  - transaction row 254: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 254: transaction evidence mismatch: base_fee_source
  - transaction row 254: transaction evidence mismatch: limits_pass
  - transaction row 254: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 254: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 254: transaction evidence mismatch: signature_verification_measured
  - transaction row 254: transaction evidence mismatch: live_state_validation
  - transaction row 254: transaction outcome/success classification mismatch
  - transaction row 257: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 257: transaction evidence mismatch: base_fee_source
  - transaction row 257: transaction evidence mismatch: limits_pass
  - transaction row 257: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 257: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 257: transaction evidence mismatch: signature_verification_measured
  - transaction row 257: transaction evidence mismatch: live_state_validation
  - transaction row 257: transaction outcome/success classification mismatch
  - transaction row 258: transaction evidence mismatch: contract_authorization_enforced
  - transaction row 258: transaction evidence mismatch: base_fee_source
  - transaction row 258: transaction evidence mismatch: limits_pass
  - transaction row 258: transaction evidence mismatch: diagnostic_shadow_ok
  - transaction row 258: transaction evidence mismatch: exact_captured_cost_model
  - transaction row 258: transaction evidence mismatch: signature_verification_measured
  - transaction row 258: transaction evidence mismatch: live_state_validation
  - transaction row 258: transaction outcome/success classification mismatch
  - unsigned transaction proof missing: activate_open_position_v3 -> v3.activate_open_position_v3 (success)
  - unsigned transaction proof missing: allowance_transfer_from -> vault.transfer_from (success)
  - unsigned transaction proof missing: allowance_transfer_from -> vault.transfer_from (rejected, proof_case=allowance_missing_spender_auth)
  - unsigned transaction proof missing: allowance_transfer_from -> vault.transfer_from (rejected, proof_case=allowance_wrong_recipient_auth, auth_mode=enforcing-mismatched-root, mismatched_argument_indices=[2])
  - unsigned transaction proof missing: allowance_transfer_from -> vault.transfer_from (rejected, proof_case=allowance_failed_claim_rollback)
  - unsigned transaction proof missing: both_reward_calls_failed_funded_liquidation -> v3.liquidate_funded_v3 (success)
  - unsigned transaction proof missing: cancel_after_execute -> v3.execute_open_position_v3 (success)
  - unsigned transaction proof missing: cancel_after_execute -> v3.repay_margin_position_v3 (success)
  - unsigned transaction proof missing: cancel_after_execute -> v3.release_debt_free_position_v3 (success)
  - unsigned transaction proof missing: cancel_after_execute -> v3.cancel_pending_open_v3 (rejected, proof_case=cancel_after_execute_rejection)
  - unsigned transaction proof missing: cancel_after_swap -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: claim_failure_recovery -> vault.emergency_redeem (success)
  - unsigned transaction proof missing: claim_rewards -> vault.claim_rewards (success)
  - unsigned transaction proof missing: default_missing_reserve_abi -> v3.activate_open_position_v3 (rejected, proof_case=default_missing_reserve_abi_reject)
  - unsigned transaction proof missing: default_missing_reserve_abi -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: default_pending_fee_refund_conservation -> v3.distribute_margin_fees (success)
  - unsigned transaction proof missing: default_pending_fee_refund_conservation -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: default_pending_fee_refund_conservation -> v3.claim_margin_fees (success)
  - unsigned transaction proof missing: default_reserve_active_repay_release -> v3.activate_open_position_v3 (success)
  - unsigned transaction proof missing: default_reserve_active_repay_release -> v3.repay_margin_position_v3 (success)
  - unsigned transaction proof missing: default_reserve_active_repay_release -> v3.release_debt_free_position_v3 (success)
  - unsigned transaction proof missing: default_reserve_finalization_rollback -> v3.activate_open_position_v3 (rejected, proof_case=default_post_cost_reserve_reject)
  - unsigned transaction proof missing: default_reserve_finalization_rollback -> v3.execute_open_position_v3 (rejected, proof_case=default_post_cost_reserve_reject)
  - unsigned transaction proof missing: default_reserve_finalization_rollback -> v3.resolve_pending_open_v3 (rejected, proof_case=default_post_cost_reserve_reject)
  - unsigned transaction proof missing: default_reserve_finalization_rollback -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: default_reserve_finalization_rollback -> v3.recover_pending_open_funded (rejected, proof_case=default_pending_recovery_repeat_reject)
  - unsigned transaction proof missing: default_short_pending_recovery -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: deposit -> vault.deposit_underlying (success)
  - unsigned transaction proof missing: emergency_redeem -> vault.emergency_redeem (success)
  - unsigned transaction proof missing: expired_pending_resolution -> v3.resolve_pending_open_v3 (success)
  - unsigned transaction proof missing: expired_pending_resolution -> v3.release_debt_free_position_v3 (success)
  - unsigned transaction proof missing: failed_activation_recovery -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: failed_activation_recovery -> v3.activate_open_position_v3 (rejected, proof_case=failed_activation_atomic)
  - unsigned transaction proof missing: finish_close -> v3.finish_close_position_v3 (success)
  - unsigned transaction proof missing: funded_failed_collateral_transfer_rollback -> v3.liquidate_funded_v3 (rejected, proof_case=funded_failed_collateral_transfer_rollback)
  - unsigned transaction proof missing: funded_insufficient_liquidator_rollback -> v3.liquidate_funded_v3 (rejected, proof_case=funded_insufficient_liquidator_rollback)
  - unsigned transaction proof missing: funded_invalid_valuation_rollback -> v3.liquidate_funded_v3 (rejected, proof_case=funded_invalid_valuation_rollback)
  - unsigned transaction proof missing: funded_liquidation -> v3.liquidate_funded_v3 (success)
  - unsigned transaction proof missing: funded_repeated_execution_rollback -> v3.liquidate_funded_v3 (rejected, proof_case=funded_repeated_execution_rollback)
  - unsigned transaction proof missing: funded_unauthorized_rollback -> v3.liquidate_funded_v3 (rejected, proof_case=funded_unauthorized_rollback)
  - unsigned transaction proof missing: historical_multiple_intervals -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: historical_shortfall -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: historical_unresolved_intervals -> v3.liquidate_funded_v3 (success)
  - unsigned transaction proof missing: history_scale_64_owners -> vault.emergency_redeem (success)
  - unsigned transaction proof missing: history_scale_64_owners -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: history_scale_8_unresolved -> v3.liquidate_funded_v3 (success)
  - unsigned transaction proof missing: history_scale_8_unresolved -> vault.attribute_reward_segment (success)
  - unsigned transaction proof missing: history_scale_8_unresolved -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: pending_claim_boundary_matrix -> vault.transfer (success)
  - unsigned transaction proof missing: pending_claim_boundary_matrix -> vault.emergency_redeem (success)
  - unsigned transaction proof missing: prepare_close_position_v3 -> v3.prepare_close_position_v3 (success)
  - unsigned transaction proof missing: quarantine_reconciliation -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: quarantine_reconciliation -> vault.settle_reward_segment (success)
  - unsigned transaction proof missing: receipt_custody_ownership_change -> share_receipt.transfer (success)
  - unsigned transaction proof missing: redeem -> vault.redeem (success)
  - unsigned transaction proof missing: repay_release -> v3.repay_margin_position_v3 (success)
  - unsigned transaction proof missing: repay_release -> v3.release_debt_free_position_v3 (success)
  - unsigned transaction proof missing: reserve_complete_open -> v3.swap_open_position_v3 (success)
  - unsigned transaction proof missing: reserve_complete_open -> v3.activate_open_position_v3 (success)
  - unsigned transaction proof missing: reserve_complete_open -> v3.repay_margin_position_v3 (success)
  - unsigned transaction proof missing: reserve_complete_open -> v3.release_debt_free_position_v3 (success)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.activate_open_position_v3 (rejected, proof_case=reserve_floor_activate_open_position_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.execute_open_position_v3 (rejected, proof_case=reserve_floor_execute_open_position_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.resolve_pending_open_v3 (rejected, proof_case=reserve_floor_resolve_pending_open_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.activate_open_position_v3 (rejected, proof_case=reserve_disabled_activate_open_position_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.execute_open_position_v3 (rejected, proof_case=reserve_disabled_execute_open_position_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.resolve_pending_open_v3 (rejected, proof_case=reserve_disabled_resolve_pending_open_v3_rollback)
  - unsigned transaction proof missing: reserve_complete_open_rollback -> v3.recover_pending_open_funded (success)
  - unsigned transaction proof missing: reserve_no_liquidation_bypass -> v3.liquidate_position_v3 (rejected, proof_case=reserve_healthy_liquidation_conversion_rejected)
  - unsigned transaction proof missing: swap_close -> v3.swap_close_position_v3 (success)
  - unsigned transaction proof missing: swap_open_position_v3 -> v3.swap_open_position_v3 (success)
  - unsigned transaction proof missing: transfer -> vault.transfer (success)
  - unsigned transaction proof missing: waiver_redistribution -> vault.attribute_reward_segment (success)
  - unsigned transaction proof missing: waiver_redistribution -> vault.settle_reward_segment (success)

## captured-network-diagnostic

Unsigned transactions: 2655 succeeded, 86 expected rejections, 0 unexpected failures; 733 restoration variants. 0 failures lack execution resource measurements.

Fee component maxima (stroops; independent maxima, not one transaction):

| Inclusion | Non-refundable | Refundable | Rent | Total |
|---:|---:|---:|---:|---:|
| 100 | 162160 | 15147 | 22686852 | 22845642 |

History-scale SDK rows: 746; unsigned restoration variants: 436. Exact restored key XDR, decoded historical names, resource numbers and fee deltas are preserved in the JSON summary.

Recorded gate errors:

- missing_required_flows: 0
- lifecycle_errors: 0
- transaction_errors: 0

## Separate WASM hashes

| Artifact | Bytes | SHA256 |
|---|---:|---|
| cl_integration_fixtures.wasm | 3168 | `bff4b6856c0199893515ca688c674847c23eedd097fe143c1b7734691bcd7f7f` |
| cl_share_vault.wasm | 85750 | `5e52ff4e87cfc70b8f2ff5839a7625b0ef510d0433806661a41b998805bf4e1f` |
| cl_share_pool_facade.wasm | 7298 | `3209295684c883f829949d7258a01031818fe6d2da2911d8e2517b7783706ef5` |
| margin_controller.wasm | 91657 | `f59f592b1013d4e1ec49975f0cb2bb0ff81e9ab0f88f92bd76c3b7a1d6de7a36` |
| swap_adapter.wasm | 10876 | `55c8a55f27fb5f41d4eea8ed5f0090ad8258289a99886aad9a7c940b9b95674d` |
| receipt_vault.wasm | 87197 | `db995c2ee1614eb90660c8ab79b0780358ed91e0e686aa1ec1c95dc56cc11f0e` |
| simple_peridottroller.wasm | 63289 | `d24e9b05cfa8d1d438dd58c2dda67c954b1cf80016da7c85e0fbf698ae29d3a2` |
| jump_rate_model.wasm | 7644 | `b00c583659ebc1d3322cb52acf5ecc620aaaeacc5141a829cce4c60db944c800` |
| default_margin (separate fixture) | 87123 | `3ff76a19dabcd20e5c432aee6757e4561537f991dfcdd4d81926691c34cf4bb9` |
| legacy_receipt (separate fixture) | 80262 | `dddbadfc2bfa8d8dd24867e9b8e1db138506eeb7225f1301c9b68135948d3264` |
