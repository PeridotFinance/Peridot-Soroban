//! Direct-holder exit assessment only. Existing in-kind redemption is unchanged;
//! the owner's subsequent paired-token swap is a separate authorized transaction.
//! This is NOT a staged V3 close, and funded repayment/seizure remain unchanged.
use super::*;

const EXIT_SCALE: u128 = 1_000_000_000_000_000_000;

#[contracttype]
#[derive(Clone)]
struct ExitRange {
    tick_lower: i32,
    tick_upper: i32,
}
#[contracttype]
#[derive(Clone)]
struct ExitPoolPosition {
    ranges: Vec<ExitRange>,
    raw_liquidity: u128,
    weighted_liquidity: u128,
}

fn exit_reward_conservation(f: &F, owners: &[Address]) {
    let e = &f.e;
    let total: RewardTotals = call(e, &f.share, "reward_totals", ().into_val(e));
    let mut pending = 0u128;
    let mut fractions = 0u128;
    for owner in owners {
        let h: RewardHolder = call(e, &f.share, "reward_account", (owner.clone(),).into_val(e));
        pending = pending.checked_add(h.pending).unwrap();
        fractions = fractions.checked_add(h.fraction).unwrap();
    }
    assert_eq!(
        total.received * EXIT_SCALE,
        (total.paid + pending + total.quarantine) * EXIT_SCALE + fractions + total.rounding_scaled
    );
    assert_eq!(
        token::Client::new(e, &f.reward).balance(&f.share) as u128,
        total.received - total.paid
    );
    assert_eq!(
        total.quarantine, 0,
        "working claim path must not manufacture quarantine"
    );
    std::println!("DIRECT_EXIT_REWARD_CONSERVATION received={} paid={} pending={pending} fractions={fractions} quarantine={} rounding={}",
        total.received, total.paid, total.quarantine, total.rounding_scaled);
}

#[test]
fn compiled_exit_assessment_direct_deposit_baseline() {
    let (f, _) = setup();
    let e = &f.e;
    next(e);
    let amount = 100_000_000i128;
    let before = token::Client::new(e, &f.share).balance(&f.user);
    let transfers = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.share.clone(), amount).into_val(e),
        sub_invokes: &[],
    }];
    let minted: i128 = auth(
        e,
        &f.user,
        &f.share,
        "deposit_underlying",
        (f.user.clone(), amount, 1i128).into_val(e),
        &transfers,
    );
    measure(e, "exit_assessment_direct_deposit", true);
    assert!(minted > 0);
    assert_eq!(
        token::Client::new(e, &f.share).balance(&f.user),
        before + minted
    );
    std::println!("DIRECT_DEPOSIT_BASELINE minted={minted} unchanged_atomic_path=true");
}

#[test]
fn compiled_exit_assessment_normal_redeem_baseline() {
    let (f, _) = setup();
    let e = &f.e;
    let shares = token::Client::new(e, &f.share).balance(&f.user) / 3;
    let before = token::Client::new(e, &f.under).balance(&f.user);
    next(e);
    let out: i128 = auth(
        e,
        &f.user,
        &f.share,
        "redeem",
        (f.user.clone(), shares, 1i128).into_val(e),
        &[],
    );
    measure(e, "exit_assessment_normal_redeem", true);
    assert!(out > 0);
    assert_eq!(
        token::Client::new(e, &f.under).balance(&f.user),
        before + out
    );
    std::println!(
        "DIRECT_REDEEM_BASELINE burned={shares} underlying_out={out} unchanged_atomic_path=true"
    );
}

#[test]
fn compiled_exit_assessment_v3_swap_close_baseline() {
    let (f, _) = setup();
    let e = &f.e;
    let selected = std::env::var("CL_RESOURCE_PROFILE").unwrap();
    let construction = std::env::var("CL_SPLIT_SETUP_PROFILE").unwrap();
    std::println!("EXIT_FIXTURE_CONSTRUCTION legacy open and close-preparation under {construction}; not selected-profile opening/prepare acceptance");
    std::env::set_var("CL_RESOURCE_PROFILE", &construction);
    crate::cl_resource_limits::configure(e);
    MEASURE.with(|m| m.set(false));
    let id = begin(&f);
    for name in [
        "swap_open_position_v3",
        "activate_open_position_v3",
        "prepare_close_position_v3",
    ] {
        auth::<()>(
            e,
            &f.user,
            &f.margin,
            name,
            (f.user.clone(), id).into_val(e),
            &[],
        );
    }
    std::env::set_var("CL_RESOURCE_PROFILE", selected);
    crate::cl_resource_limits::configure(e);
    MEASURE.with(|m| m.set(true));
    next(e);
    let shares = token::Client::new(e, &f.share).balance(&f.margin);
    assert!(shares > 0);
    let quote: u128 = call(
        e,
        &f.facade,
        "estimate_swap",
        (1u32, 0u32, shares as u128).into_val(e),
    );
    auth::<()>(
        e,
        &f.user,
        &f.margin,
        "swap_close_position_v3",
        (f.user.clone(), id, quote).into_val(e),
        &[],
    );
    measure(e, "exit_assessment_v3_swap_close", true);
    call::<()>(e, &f.margin, "finish_close_position_v3", (id,).into_val(e));
    measure(e, "exit_assessment_v3_finish_close", true);
    assert_eq!(
        call::<u128>(e, &f.cash, "get_margin_borrow_balance", (id,).into_val(e)),
        0
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&f.margin), 0);
    std::println!(
        "V3_CLOSE_BASELINE canonical_swap_and_finish_complete=true split_exit_implemented=false"
    );
}

#[test]
fn compiled_exit_assessment_standalone_owner_conversion() {
    let (f, _) = setup();
    let e = &f.e;
    let tokens: Vec<Address> = call(e, &f.pool, "get_tokens", ().into_val(e));
    let u_idx = if tokens.get(0).unwrap() == f.under {
        0u32
    } else {
        1u32
    };
    let paired = 100_000_000u128;
    let quote: u128 = call(
        e,
        &f.pool,
        "estimate_swap",
        (1 - u_idx, u_idx, paired).into_val(e),
    );
    let before = token::Client::new(e, &f.under).balance(&f.user);
    let sub = [MockAuthInvoke {
        contract: &f.paired,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), paired as i128).into_val(e),
        sub_invokes: &[],
    }];
    let out: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (f.user.clone(), 1 - u_idx, u_idx, paired, quote * 99 / 100).into_val(e),
        &sub,
    );
    measure(e, "exit_assessment_standalone_owner_conversion", true);
    assert_eq!(
        token::Client::new(e, &f.under).balance(&f.user),
        before + out as i128
    );
    std::println!("OWNER_CONVERSION_INDEPENDENT input={paired} output={out} preexisting_wallet_asset=true not_exit_lifecycle_proof=true");
}

#[test]
fn compiled_exit_assessment_in_kind_then_owner_conversion() {
    let (f, _) = setup();
    let e = &f.e;
    let supply: i128 = call(e, &f.share, "total_supply", ().into_val(e));
    let bob = Address::generate(e);
    // A remaining co-owner makes incorrect all-fees-to-exiter allocation visible.
    auth::<()>(
        e,
        &f.user,
        &f.share,
        "transfer",
        (f.user.clone(), bob.clone(), supply / 2).into_val(e),
        &[],
    );
    for (asset, dust) in [(&f.under, 33i128), (&f.paired, 55i128)] {
        auth::<()>(
            e,
            &f.user,
            asset,
            "transfer",
            (f.user.clone(), f.share.clone(), dust).into_val(e),
            &[],
        );
    }
    let tokens: Vec<Address> = call(e, &f.pool, "get_tokens", ().into_val(e));
    assert_eq!(tokens.len(), 2);
    let u_idx = if tokens.get(0).unwrap() == f.under {
        0u32
    } else {
        1u32
    };
    assert_eq!(tokens.get(1 - u_idx).unwrap(), f.paired);
    // Actual venue trade creates incumbent trading fees before the partial exit.
    let fee_trade = 5_000_000u128;
    let quote: u128 = call(
        e,
        &f.pool,
        "estimate_swap",
        (u_idx, 1 - u_idx, fee_trade).into_val(e),
    );
    let transfer = [MockAuthInvoke {
        contract: &f.under,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), fee_trade as i128).into_val(e),
        sub_invokes: &[],
    }];
    let _: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (
            f.user.clone(),
            u_idx,
            1 - u_idx,
            fee_trade,
            quote * 99 / 100,
        )
            .into_val(e),
        &transfer,
    );
    auth::<()>(
        e,
        &f.user,
        &f.reward,
        "transfer",
        (f.user.clone(), f.pool.clone(), 100_000_000i128).into_val(e),
        &[],
    );
    auth::<()>(
        e,
        &f.admin,
        &f.pool,
        "set_rewards_config",
        (f.admin.clone(), 3000u64, 10_003u128).into_val(e),
        &[],
    );
    e.ledger().with_mut(|l| {
        l.timestamp += 100;
        l.sequence_number += 20;
    });

    let burn = (supply / 3) as u128;
    let position: ExitPoolPosition = call(
        e,
        &f.pool,
        "get_user_position_snapshot",
        (f.share.clone(),).into_val(e),
    );
    assert_eq!(position.ranges.len(), 1);
    let range = position.ranges.get(0).unwrap();
    let burn_liquidity = position.raw_liquidity * burn / supply as u128;
    let expected_principal: Vec<u128> = call(
        e,
        &f.pool,
        "estimate_withdraw_position",
        (
            f.share.clone(),
            range.tick_lower,
            range.tick_upper,
            burn_liquidity,
        )
            .into_val(e),
    );
    let fees: Vec<u128> = call(
        e,
        &f.pool,
        "get_all_position_fees",
        (f.share.clone(),).into_val(e),
    );
    assert!(
        fees.get(0).unwrap() + fees.get(1).unwrap() > 0,
        "actual trading fees required"
    );
    let mut idle = Vec::<u128>::new(e);
    let mut wallets = Vec::<i128>::new(e);
    for asset in tokens.iter() {
        // The vault explicitly excludes unsolicited dust. The fixture has no
        // older excluded balances, so remove precisely the donations above.
        let donated = if asset == f.under { 33u128 } else { 55u128 };
        idle.push_back(
            (token::Client::new(e, &asset).balance(&f.share) as u128)
                .checked_sub(donated)
                .unwrap(),
        );
        wallets.push_back(token::Client::new(e, &asset).balance(&f.user));
    }
    let bob_before = token::Client::new(e, &f.share).balance(&bob);
    // Existing emergency ABI with explicit waiver=false; claim works and is real.
    let out: Vec<u128> = auth(
        e,
        &f.user,
        &f.share,
        "emergency_redeem",
        (f.user.clone(), burn, false).into_val(e),
        &[],
    );
    measure(e, "exit_assessment_in_kind_redeem", true);
    std::println!("EXIT_VIEW_DIAGNOSTIC fee_inclusive_withdraw_quote={expected_principal:?} fees={fees:?} booked_idle={idle:?} actual_out={out:?}");
    assert_eq!(out.len(), 2);
    for i in 0..2u32 {
        let old_idle = idle.get(i).unwrap() + fees.get(i).unwrap();
        // Actual pinned bytecode's estimate includes ALL claimable fees even
        // for a partial liquidity burn. The vault first collects those fees
        // for the whole share supply, then gives the exiter only its fraction.
        let principal = expected_principal
            .get(i)
            .unwrap()
            .checked_sub(fees.get(i).unwrap())
            .unwrap();
        let expected = principal + old_idle * burn / supply as u128;
        assert_eq!(
            out.get(i).unwrap(),
            expected,
            "partial exit principal + pro-rata prior idle/fees"
        );
        assert_eq!(
            token::Client::new(e, &tokens.get(i).unwrap()).balance(&f.user),
            wallets.get(i).unwrap() + expected as i128
        );
    }
    assert_eq!(token::Client::new(e, &f.share).balance(&bob), bob_before);
    assert_eq!(
        call::<i128>(e, &f.share, "total_supply", ().into_val(e)),
        supply - burn as i128
    );
    let owner_booked: RewardHolder =
        call(e, &f.share, "reward_account", (f.user.clone(),).into_val(e));
    let bob_booked: RewardHolder = call(e, &f.share, "reward_account", (bob.clone(),).into_val(e));
    assert!(owner_booked.pending > 0 && bob_booked.pending > 0);
    exit_reward_conservation(&f, &[f.user.clone(), bob.clone()]);

    // Pair tokens are already in the user's wallet. Only that exact exit leg is
    // converted; this second transaction cannot seize/release borrowed collateral.
    next(e);
    let paired = out.get(1 - u_idx).unwrap();
    assert!(paired > 0);
    let estimate: u128 = call(
        e,
        &f.pool,
        "estimate_swap",
        (1 - u_idx, u_idx, paired).into_val(e),
    );
    let min_out = estimate * 99 / 100;
    assert!(min_out > 0);
    let u_before = token::Client::new(e, &f.under).balance(&f.user);
    let p_before = token::Client::new(e, &f.paired).balance(&f.user);
    let sub = [MockAuthInvoke {
        contract: &f.paired,
        fn_name: "transfer",
        args: (f.user.clone(), f.pool.clone(), paired as i128).into_val(e),
        sub_invokes: &[],
    }];
    let converted: u128 = auth(
        e,
        &f.user,
        &f.pool,
        "swap",
        (f.user.clone(), 1 - u_idx, u_idx, paired, min_out).into_val(e),
        &sub,
    );
    measure(e, "exit_assessment_owner_paired_conversion", true);
    assert!(converted >= min_out);
    assert_eq!(
        token::Client::new(e, &f.under).balance(&f.user),
        u_before + converted as i128
    );
    assert_eq!(
        token::Client::new(e, &f.paired).balance(&f.user),
        p_before - paired as i128
    );
    assert_eq!(token::Client::new(e, &f.share).balance(&bob), bob_before);
    exit_reward_conservation(&f, &[f.user.clone(), bob.clone()]);
    std::println!("DIRECT_IN_KIND_EXIT burned={burn} raw_underlying={} raw_paired={paired} converted_underlying={converted} total_underlying={} waiver=false separate_owner_swap=true v3_close=false",
        out.get(u_idx).unwrap(), out.get(u_idx).unwrap() + converted);
}
