use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
#[cfg(not(test))]
use soroban_sdk::String;
use soroban_sdk::{
    contract, contractimpl, token, Address, BytesN, Env, IntoVal, InvokeError, Symbol, Val, Vec,
};

use crate::constants::*;
use crate::helpers::*;
use crate::storage::*;

#[contract]
pub struct MarginController;

#[contractimpl]
impl MarginController {
    pub fn initialize(
        env: Env,
        admin: Address,
        peridottroller: Address,
        swap_adapter: Address,
        max_leverage: u128,
    ) {
        let persistent = env.storage().persistent();
        let already_initialized = persistent.has(&DataKey::Initialized)
            || persistent.has(&DataKey::Admin)
            || env.storage().instance().has(&DataKey::Initialized);
        if already_initialized {
            panic!("already initialized");
        }
        assert_expected_admin(&env, &admin);
        admin.require_auth();
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        Self::assert_valid_swap_adapter(&env, &swap_adapter);
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller);
        env.storage()
            .persistent()
            .set(&DataKey::SwapAdapter, &swap_adapter);
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
        env.storage()
            .persistent()
            .set(&DataKey::MaxSlippageBps, &DEFAULT_MAX_SLIPPAGE_BPS);
        env.storage()
            .persistent()
            .set(&DataKey::PositionCounter, &0u64);
        env.storage().persistent().set(&DataKey::Initialized, &true);
        bump_core_ttl(&env);
    }

    pub fn set_market(env: Env, admin: Address, asset: Address, vault: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Market(asset.clone()), &vault);
        bump_market_ttl(&env, &asset);
    }

    pub fn set_peridottroller(env: Env, admin: Address, peridottroller: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller);
    }

    pub fn set_params(env: Env, admin: Address, max_leverage: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
    }

    pub fn set_max_slippage_bps(env: Env, admin: Address, max_slippage_bps: u128) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_slippage_bps == 0 || max_slippage_bps > MAX_SLIPPAGE_BPS_CAP {
            panic!("invalid slippage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxSlippageBps, &max_slippage_bps);
    }

    pub fn set_swap_adapter(env: Env, admin: Address, swap_adapter: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        Self::assert_valid_swap_adapter(&env, &swap_adapter);
        env.storage()
            .persistent()
            .set(&DataKey::SwapAdapter, &swap_adapter);
    }

    pub fn deposit_collateral(env: Env, user: Address, asset: Address, amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        let vault = get_market(&env, &asset);
        ReceiptVaultClient::new(&env, &vault).deposit(&user, &amount);
    }

    pub fn withdraw_collateral(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        let vault = get_market(&env, &asset);
        let current_ptokens = ReceiptVaultClient::new(&env, &vault).get_ptoken_balance(&user);
        if current_ptokens < ptoken_amount {
            panic!("Insufficient pTokens");
        }
        let locked = Self::locked_ptokens_in_market(env.clone(), user.clone(), vault.clone());
        let remaining = current_ptokens.saturating_sub(ptoken_amount);
        if remaining < locked {
            panic!("collateral locked");
        }
        let vault_client = ReceiptVaultClient::new(&env, &vault);
        Self::begin_margin_withdraw_if_supported(&env, &vault, &user);
        vault_client.withdraw(&user, &ptoken_amount);
    }

    /// Move spot pTokens into margin custody.
    pub fn transfer_spot_to_margin(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        if ptoken_amount == 0 {
            panic!("bad amount");
        }
        let vault = get_market(&env, &asset);
        let controller = env.current_contract_address();
        let amount_i128: i128 = ptoken_amount.try_into().expect("amount too large");
        ReceiptVaultClient::new(&env, &vault).transfer(&user, &controller, &amount_i128);
        let current = get_margin_balance_ptokens(&env, &user, &vault);
        set_margin_balance_ptokens(&env, &user, &vault, current.saturating_add(ptoken_amount));
    }

    /// Move pTokens from margin custody back to spot wallet.
    pub fn transfer_margin_to_spot(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        bump_core_ttl(&env);
        user.require_auth();
        if ptoken_amount == 0 {
            panic!("bad amount");
        }
        let vault = get_market(&env, &asset);
        let current = get_margin_balance_ptokens(&env, &user, &vault);
        if current < ptoken_amount {
            panic!("insufficient margin balance");
        }
        set_margin_balance_ptokens(&env, &user, &vault, current.saturating_sub(ptoken_amount));
        let controller = env.current_contract_address();
        let amount_i128: i128 = ptoken_amount.try_into().expect("amount too large");
        let transfer_args: Vec<Val> =
            (controller.clone(), user.clone(), amount_i128).into_val(&env);
        Self::authorize_controller_subcall(&env, &vault, "transfer", transfer_args);
        ReceiptVaultClient::new(&env, &vault).transfer(&controller, &user, &amount_i128);
    }

    pub fn get_margin_balance_ptokens(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        get_margin_balance_ptokens(&env, &user, &vault)
    }

    pub fn get_margin_balance_underlying(env: Env, user: Address, asset: Address) -> u128 {
        bump_core_ttl(&env);
        let vault = get_market(&env, &asset);
        let pbal = get_margin_balance_ptokens(&env, &user, &vault);
        if pbal == 0 {
            return 0;
        }
        let rate = ReceiptVaultClient::new(&env, &vault).get_exchange_rate();
        pbal.saturating_mul(rate) / SCALE_1E6
    }

    pub fn propose_upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        let execute_after = env
            .ledger()
            .timestamp()
            .saturating_add(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::PendingUpgradeEta, &execute_after);
        bump_pending_upgrade_ttl(&env);
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        bump_pending_upgrade_ttl(&env);
        let pending_hash: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let execute_after: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending_hash != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < execute_after {
            panic!("upgrade timelocked");
        }
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeHash);
        env.storage()
            .persistent()
            .remove(&DataKey::PendingUpgradeEta);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }

    pub fn open_position(
        env: Env,
        user: Address,
        collateral_asset: Address,
        base_asset: Address,
        collateral_amount: u128,
        leverage: u128,
        side: PositionSide,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_amount == 0 {
            panic!("bad collateral");
        }
        if collateral_asset == base_asset {
            panic!("assets must differ");
        }
        let (debt_asset, position_asset) = match side {
            PositionSide::Long => (collateral_asset.clone(), base_asset.clone()),
            PositionSide::Short => (base_asset.clone(), collateral_asset.clone()),
        };
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &debt_asset,
            &position_asset,
        );
        let collateral_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        if collateral_price.0 == 0 || collateral_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let collateral_value =
            collateral_amount.saturating_mul(collateral_price.0) / collateral_price.1;
        let target_value = collateral_value.saturating_mul(leverage);
        let borrow_value = target_value.saturating_sub(collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value.saturating_mul(debt_price.1) / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let min_out_oracle =
            Self::oracle_min_out(&env, &debt_asset, &position_asset, borrow_amount);
        if amount_with_slippage < min_out_oracle {
            panic!("slippage too high");
        }

        // Deposit initial collateral
        let collateral_vault = get_market(&env, &collateral_asset);
        Self::assert_margin_lock_configured(&env, &collateral_vault);
        let p_before = ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &collateral_vault).deposit(&user, &collateral_amount);
        let peridottroller = get_peridottroller(&env);
        peridottroller.enter_market(&user, &collateral_vault);
        let p_after = ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        let initial_p_delta = p_after.saturating_sub(p_before);
        if initial_p_delta == 0 {
            panic!("no collateral minted");
        }

        // Borrow debt asset in bounded steps, swapping/depositing each step so newly
        // acquired collateral contributes to the next borrow check.
        let debt_vault = get_market(&env, &debt_asset);
        Self::assert_margin_lock_configured(&env, &debt_vault);
        let position_vault = get_market(&env, &position_asset);
        Self::assert_margin_lock_configured(&env, &position_vault);
        peridottroller.enter_market(&user, &debt_vault);
        peridottroller.enter_market(&user, &position_vault);
        let debt_vault_client = ReceiptVaultClient::new(&env, &debt_vault);
        let debt_before = debt_vault_client.get_user_borrow_balance(&user);
        let shares_before = get_debt_shares_total(&env, &user, &debt_asset);
        let mut remaining_borrow = borrow_amount;
        let mut total_received = 0u128;
        let p_before = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        const MAX_BORROW_STEPS: u32 = 32;
        for _ in 0..MAX_BORROW_STEPS {
            if remaining_borrow == 0 {
                break;
            }

            let step_borrow = Self::max_borrow_step_for_position(
                &peridottroller,
                &user,
                debt_price,
                remaining_borrow,
            );
            if step_borrow == 0 {
                break;
            }

            debt_vault_client.borrow(&user, &step_borrow);
            let step_min_out_oracle =
                Self::oracle_min_out(&env, &debt_asset, &position_asset, step_borrow);
            let received_step = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
                &user,
                &swaps_chain,
                &debt_asset,
                &step_borrow,
                &step_min_out_oracle,
            );
            if received_step < step_min_out_oracle {
                panic!("slippage too high");
            }
            if received_step == 0 {
                panic!("swap failed");
            }

            ReceiptVaultClient::new(&env, &position_vault).deposit(&user, &received_step);
            total_received = total_received.saturating_add(received_step);
            remaining_borrow = remaining_borrow.saturating_sub(step_borrow);
        }
        if remaining_borrow > 0 {
            panic!("leverage unsupported pre-swap");
        }
        if total_received < min_out_oracle || total_received < amount_with_slippage {
            panic!("slippage too high");
        }
        let p_after = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let debt_after = debt_vault_client.get_user_borrow_balance(&user);
        let actual_borrowed = debt_after.saturating_sub(debt_before);
        if actual_borrowed == 0 {
            panic!("zero borrow");
        }
        let new_shares =
            Self::calculate_new_debt_shares(actual_borrowed, shares_before, debt_before);
        set_debt_shares_total(
            &env,
            &user,
            &debt_asset,
            shares_before.saturating_add(new_shares),
        );

        let entry_price_scaled = actual_borrowed.saturating_mul(SCALE_1E6) / total_received;

        let id = next_position_id(&env);
        let position = Position {
            owner: user.clone(),
            side,
            collateral_asset: position_asset,
            debt_asset,
            collateral_ptokens: p_delta,
            debt_shares: new_shares,
            entry_price_scaled,
            opened_at: env.ledger().timestamp(),
            status: PositionStatus::Open,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        set_position_mode(&env, id, PositionMode::Legacy);
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &position_vault);
        set_position_initial_lock(&env, id, &collateral_vault, initial_p_delta);
        bump_position_ttl(&env, id);
        push_user_position(&env, &user, id);
        id
    }

    /// Margin V2 open path that consumes pTokens from margin balance and borrows
    /// against a position-scoped debt namespace in the vault.
    pub fn open_position_v2(
        env: Env,
        user: Address,
        collateral_asset: Address,
        base_asset: Address,
        collateral_ptokens: u128,
        leverage: u128,
        side: PositionSide,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_ptokens == 0 {
            panic!("bad collateral");
        }
        if collateral_asset == base_asset {
            panic!("assets must differ");
        }
        let (debt_asset, position_asset) = match side {
            PositionSide::Long => (collateral_asset.clone(), base_asset.clone()),
            PositionSide::Short => (base_asset.clone(), collateral_asset.clone()),
        };
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &debt_asset,
            &position_asset,
        );

        let collateral_vault = get_market(&env, &collateral_asset);
        let debt_vault = get_market(&env, &debt_asset);
        let position_vault = get_market(&env, &position_asset);
        Self::assert_margin_lock_configured(&env, &collateral_vault);
        Self::assert_margin_lock_configured(&env, &debt_vault);
        Self::assert_margin_lock_configured(&env, &position_vault);

        let free_collateral = get_margin_balance_ptokens(&env, &user, &collateral_vault);
        if free_collateral < collateral_ptokens {
            panic!("insufficient margin balance");
        }
        set_margin_balance_ptokens(
            &env,
            &user,
            &collateral_vault,
            free_collateral.saturating_sub(collateral_ptokens),
        );

        let coll_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let coll_rate = ReceiptVaultClient::new(&env, &collateral_vault).get_exchange_rate();
        let collateral_underlying = collateral_ptokens.saturating_mul(coll_rate) / SCALE_1E6;
        let collateral_value = collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        let target_value = collateral_value.saturating_mul(leverage);
        let borrow_value = target_value.saturating_sub(collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value.saturating_mul(debt_price.1) / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }
        let min_out_oracle =
            Self::oracle_min_out(&env, &debt_asset, &position_asset, borrow_amount);
        if amount_with_slippage < min_out_oracle {
            panic!("slippage too high");
        }

        let id = next_position_id(&env);
        let mut position = Position {
            owner: user.clone(),
            side,
            collateral_asset: position_asset.clone(),
            debt_asset: debt_asset.clone(),
            collateral_ptokens: 0u128,
            debt_shares: 0u128,
            entry_price_scaled: 0u128,
            opened_at: env.ledger().timestamp(),
            status: PositionStatus::Open,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        set_position_mode(&env, id, PositionMode::MarginV2);
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &position_vault);
        set_position_initial_lock(&env, id, &collateral_vault, collateral_ptokens);
        bump_position_ttl(&env, id);

        let debt_vault_client = ReceiptVaultClient::new(&env, &debt_vault);
        debt_vault_client.init_margin_borrow_state(&id);
        debt_vault_client.borrow_for_margin(&id, &user, &borrow_amount);
        let received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &debt_asset,
            &borrow_amount,
            &amount_with_slippage,
        );
        if received < min_out_oracle {
            panic!("slippage too high");
        }
        if received == 0 {
            panic!("swap failed");
        }

        let p_before = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &position_vault).deposit(&user, &received);
        let p_after = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }
        let controller = env.current_contract_address();
        let p_delta_i128: i128 = p_delta.try_into().expect("amount too large");
        ReceiptVaultClient::new(&env, &position_vault).transfer(&user, &controller, &p_delta_i128);

        let entry_price_scaled = borrow_amount.saturating_mul(SCALE_1E6) / received;
        position.collateral_ptokens = p_delta;
        position.entry_price_scaled = entry_price_scaled;
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        bump_position_ttl(&env, id);
        push_user_position(&env, &user, id);
        id
    }

    pub fn open_position_no_swap(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: u128,
        borrow_amount: u128,
        leverage: u128,
        side: PositionSide,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        if side != PositionSide::Long {
            panic!("no-swap only long");
        }
        Self::open_position_no_swap_inner(
            env,
            user,
            collateral_asset,
            debt_asset,
            collateral_amount,
            borrow_amount,
            leverage,
            side,
        )
    }

    pub fn open_position_no_swap_short(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: u128,
        borrow_amount: u128,
        leverage: u128,
    ) -> u64 {
        bump_core_ttl(&env);
        user.require_auth();
        Self::open_position_no_swap_inner(
            env,
            user,
            collateral_asset,
            debt_asset,
            collateral_amount,
            borrow_amount,
            leverage,
            PositionSide::Short,
        )
    }

    fn open_position_no_swap_inner(
        env: Env,
        user: Address,
        collateral_asset: Address,
        debt_asset: Address,
        collateral_amount: u128,
        borrow_amount: u128,
        leverage: u128,
        side: PositionSide,
    ) -> u64 {
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_asset == debt_asset {
            panic!("assets must differ");
        }
        let collateral_vault = get_market(&env, &collateral_asset);
        let collateral_cf =
            Self::assert_pre_swap_leverage_supported(&env, &collateral_vault, leverage);
        if collateral_amount == 0 || borrow_amount == 0 {
            panic!("bad amounts");
        }
        let collateral_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        if collateral_price.0 == 0 || collateral_price.1 == 0 {
            panic!("invalid collateral price");
        }
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let collateral_value =
            collateral_amount.saturating_mul(collateral_price.0) / collateral_price.1;
        let borrow_value = borrow_amount.saturating_mul(debt_price.0) / debt_price.1;
        let discounted_collateral_value =
            collateral_value.saturating_mul(collateral_cf) / SCALE_1E6;
        let target_value = discounted_collateral_value.saturating_mul(leverage);
        if borrow_value >= target_value {
            panic!("borrow exceeds leverage");
        }

        // Deposit initial collateral
        Self::assert_margin_lock_configured(&env, &collateral_vault);
        let p_before = ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &collateral_vault).deposit(&user, &collateral_amount);
        let peridottroller = get_peridottroller(&env);
        peridottroller.enter_market(&user, &collateral_vault);
        let p_after = ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }

        // Borrow debt asset
        let debt_vault = get_market(&env, &debt_asset);
        Self::assert_margin_lock_configured(&env, &debt_vault);
        peridottroller.enter_market(&user, &debt_vault);
        let debt_before = ReceiptVaultClient::new(&env, &debt_vault).get_user_borrow_balance(&user);
        let shares_before = get_debt_shares_total(&env, &user, &debt_asset);
        let new_shares = Self::calculate_new_debt_shares(borrow_amount, shares_before, debt_before);
        ReceiptVaultClient::new(&env, &debt_vault).borrow(&user, &borrow_amount);
        set_debt_shares_total(
            &env,
            &user,
            &debt_asset,
            shares_before.saturating_add(new_shares),
        );

        let entry_price_scaled = borrow_value
            .saturating_mul(SCALE_1E6)
            .saturating_mul(debt_price.1)
            / debt_price.0
            / collateral_amount;

        let id = next_position_id(&env);
        let position = Position {
            owner: user.clone(),
            side,
            collateral_asset,
            debt_asset,
            collateral_ptokens: p_delta,
            debt_shares: new_shares,
            entry_price_scaled,
            opened_at: env.ledger().timestamp(),
            status: PositionStatus::Open,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Position(id), &position);
        set_position_mode(&env, id, PositionMode::Legacy);
        set_position_vaults(&env, id, &collateral_vault, &debt_vault, &collateral_vault);
        bump_position_ttl(&env, id);
        push_user_position(&env, &user, id);
        id
    }

    pub fn close_position(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if get_position_mode(&env, position_id) == PositionMode::MarginV2 {
            panic!("use close_position_v2");
        }
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &position.collateral_asset,
            &position.debt_asset,
        );
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }

        let (debt_amount, total_shares, _total_debt) = debt_for_shares_in_vault(
            &env,
            &user,
            &position.debt_asset,
            &vaults.debt_vault,
            position.debt_shares,
        );
        if debt_amount == 0 {
            panic!("zero debt");
        }
        let initial_lock = get_position_initial_lock(&env, position_id);
        position.status = PositionStatus::Closed;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(&env, position_id);

        if position.collateral_asset == position.debt_asset {
            ReceiptVaultClient::new(&env, &vaults.debt_vault).repay(&user, &debt_amount);
            let vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
            Self::begin_margin_withdraw_if_supported(&env, &vaults.position_vault, &user);
            vault_client.withdraw(&user, &position.collateral_ptokens);

            let new_total_shares = total_shares.saturating_sub(position.debt_shares);
            set_debt_shares_total(&env, &user, &position.debt_asset, new_total_shares);
            Self::assert_no_residual_debt_when_all_shares_burned(
                &env,
                &user,
                &vaults.debt_vault,
                new_total_shares,
            );
            Self::withdraw_initial_collateral_if_any(&env, &user, initial_lock);
            clear_position_initial_lock(&env, position_id);
            clear_position_vaults(&env, position_id);
            clear_position_mode(&env, position_id);

            remove_user_position(&env, &user, position_id);
            return;
        }

        let underlying_token =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_underlying_token();
        let token_client = token::TokenClient::new(&env, &underlying_token);
        let bal_before = token_client.balance(&user);
        let vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
        Self::begin_margin_withdraw_if_supported(&env, &vaults.position_vault, &user);
        vault_client.withdraw(&user, &position.collateral_ptokens);
        let bal_after = token_client.balance(&user);
        let collateral_underlying = if bal_after <= bal_before {
            0u128
        } else {
            (bal_after - bal_before) as u128
        };
        let mut received = 0u128;
        if collateral_underlying > 0 {
            let min_out_oracle = Self::oracle_min_out(
                &env,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            if amount_with_slippage < min_out_oracle {
                panic!("slippage too high");
            }
            received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
                &user,
                &swaps_chain,
                &position.collateral_asset,
                &collateral_underlying,
                &amount_with_slippage,
            );
            if received < min_out_oracle {
                panic!("slippage too high");
            }
        }

        // Allow voluntary close of underwater positions by topping up debt asset from wallet.
        let debt_underlying =
            ReceiptVaultClient::new(&env, &vaults.debt_vault).get_underlying_token();
        let debt_token = token::TokenClient::new(&env, &debt_underlying);
        let user_debt_balance = debt_token.balance(&user);
        let debt_amount_i128: i128 = debt_amount.try_into().expect("debt too large");
        if user_debt_balance < debt_amount_i128 {
            panic!("insufficient funds to close");
        }
        ReceiptVaultClient::new(&env, &vaults.debt_vault).repay(&user, &debt_amount);

        let new_total_shares = total_shares.saturating_sub(position.debt_shares);
        set_debt_shares_total(&env, &user, &position.debt_asset, new_total_shares);
        Self::assert_no_residual_debt_when_all_shares_burned(
            &env,
            &user,
            &vaults.debt_vault,
            new_total_shares,
        );
        Self::withdraw_initial_collateral_if_any(&env, &user, initial_lock);
        clear_position_initial_lock(&env, position_id);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);

        remove_user_position(&env, &user, position_id);

        // Any remaining swap output stays with the user as profit.
        let _unused = received.saturating_sub(debt_amount);
    }

    pub fn close_position_v2(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        amount_with_slippage: u128,
    ) {
        bump_core_ttl(&env);
        user.require_auth();
        if amount_with_slippage == 0 {
            panic!("bad slippage");
        }
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let swap_adapter = get_swap_adapter(&env);
        validate_swaps_chain(
            &env,
            &swap_adapter,
            &swaps_chain,
            &position.collateral_asset,
            &position.debt_asset,
        );

        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        let debt_amount = debt_vault_client.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        let controller = env.current_contract_address();
        let underlying_token =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_underlying_token();
        let token_client = token::TokenClient::new(&env, &underlying_token);
        let bal_before = token_client.balance(&controller);
        let vault_client = ReceiptVaultClient::new(&env, &vaults.position_vault);
        Self::begin_margin_withdraw_if_supported(&env, &vaults.position_vault, &controller);
        let withdraw_args: Vec<Val> =
            (controller.clone(), position.collateral_ptokens).into_val(&env);
        Self::authorize_controller_subcall(&env, &vaults.position_vault, "withdraw", withdraw_args);
        vault_client.withdraw(&controller, &position.collateral_ptokens);
        let bal_after = token_client.balance(&controller);
        let collateral_underlying = if bal_after <= bal_before {
            0u128
        } else {
            (bal_after - bal_before) as u128
        };
        let mut received = 0u128;
        if collateral_underlying > 0 {
            let min_out_oracle = Self::oracle_min_out(
                &env,
                &position.collateral_asset,
                &position.debt_asset,
                collateral_underlying,
            );
            if amount_with_slippage < min_out_oracle {
                panic!("slippage too high");
            }
            // close_position_v2 swaps are executed from controller-held collateral,
            // so pre-authorize the controller for swap-adapter's user.require_auth().
            let swap_args: Vec<Val> = (
                controller.clone(),
                swaps_chain.clone(),
                position.collateral_asset.clone(),
                collateral_underlying,
                amount_with_slippage,
            )
                .into_val(&env);
            let mut auths = Vec::new(&env);
            auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: swap_adapter.clone(),
                    fn_name: Symbol::new(&env, "swap_chained"),
                    args: swap_args,
                },
                sub_invocations: Vec::new(&env),
            }));
            env.authorize_as_current_contract(auths);
            received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
                &controller,
                &swaps_chain,
                &position.collateral_asset,
                &collateral_underlying,
                &amount_with_slippage,
            );
            if received < min_out_oracle {
                panic!("slippage too high");
            }
        }
        if received < debt_amount {
            panic!("insufficient swap output");
        }
        let repay_for_margin_args: Vec<Val> =
            (position_id, controller.clone(), debt_amount).into_val(&env);
        Self::authorize_controller_subcall(
            &env,
            &vaults.debt_vault,
            "repay_for_margin",
            repay_for_margin_args,
        );
        debt_vault_client.repay_for_margin(&position_id, &controller, &debt_amount);

        let surplus = received.saturating_sub(debt_amount);
        if surplus > 0 {
            let p_before = debt_vault_client.get_ptoken_balance(&controller);
            let deposit_args: Vec<Val> = (controller.clone(), surplus).into_val(&env);
            Self::authorize_controller_subcall(&env, &vaults.debt_vault, "deposit", deposit_args);
            debt_vault_client.deposit(&controller, &surplus);
            let p_after = debt_vault_client.get_ptoken_balance(&controller);
            let p_delta = p_after.saturating_sub(p_before);
            if p_delta > 0 {
                let free = get_margin_balance_ptokens(&env, &user, &vaults.debt_vault);
                set_margin_balance_ptokens(
                    &env,
                    &user,
                    &vaults.debt_vault,
                    free.saturating_add(p_delta),
                );
            }
        }

        if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(&env, position_id)
        {
            if initial_ptokens > 0 {
                let free = get_margin_balance_ptokens(&env, &user, &initial_market);
                set_margin_balance_ptokens(
                    &env,
                    &user,
                    &initial_market,
                    free.saturating_add(initial_ptokens),
                );
            }
        }

        position.status = PositionStatus::Closed;
        position.collateral_ptokens = 0u128;
        position.debt_shares = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_position_initial_lock(&env, position_id);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);
        remove_user_position(&env, &user, position_id);
    }

    pub fn liquidate_position(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if get_position_mode(&env, position_id) == PositionMode::MarginV2 {
            panic!("use liquidate_position_v2");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let (debt_amount, total_shares, total_debt_before) = debt_for_shares_in_vault(
            &env,
            &position.owner,
            &position.debt_asset,
            &vaults.debt_vault,
            position.debt_shares,
        );
        if debt_amount == 0 {
            panic!("zero debt");
        }
        // Position-level guard: only liquidate when this position itself is underwater.
        let debt_price = get_price_usd(&env, &position.debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        let coll_price = get_price_usd(&env, &position.collateral_asset);
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        let collateral_cf = get_peridottroller(&env).get_market_cf(&vaults.position_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let exchange_rate =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_exchange_rate();
        let collateral_underlying =
            position.collateral_ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        let collateral_value_raw =
            collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        let collateral_value = collateral_value_raw.saturating_mul(collateral_cf) / SCALE_1E6;
        if collateral_value >= debt_value {
            panic!("not liquidatable");
        }

        let debt_vault_client = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        let position_shortfall_usd = debt_value.saturating_sub(collateral_value);
        let max_seize_ptokens = position.collateral_ptokens;
        let peridottroller_addr: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Peridottroller)
            .expect("peridottroller not set");
        let controller = env.current_contract_address();
        let liquidation_args: Vec<Val> = (
            controller.clone(),
            position.owner.clone(),
            vaults.debt_vault.clone(),
            vaults.position_vault.clone(),
            debt_amount,
            liquidator.clone(),
            position_shortfall_usd,
            max_seize_ptokens,
        )
            .into_val(&env);
        let mut auths = Vec::new(&env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: peridottroller_addr.clone(),
                fn_name: Symbol::new(&env, "liquidate_for_margin"),
                args: liquidation_args,
            },
            sub_invocations: Vec::new(&env),
        }));
        env.authorize_as_current_contract(auths);

        let seized_ptokens = get_peridottroller(&env).liquidate_for_margin(
            &controller,
            &position.owner,
            &vaults.debt_vault,
            &vaults.position_vault,
            &debt_amount,
            &liquidator,
            &position_shortfall_usd,
            &max_seize_ptokens,
        );
        let total_debt_after = debt_vault_client.get_user_borrow_balance(&position.owner);
        if total_debt_after >= total_debt_before {
            panic!("no liquidation progress");
        }
        let actual_repaid = total_debt_before - total_debt_after;
        let shares_burned = if actual_repaid >= debt_amount {
            position.debt_shares
        } else {
            let numerator = position
                .debt_shares
                .checked_mul(actual_repaid)
                .expect("share calc overflow");
            let mut burned = numerator
                .checked_add(debt_amount - 1)
                .expect("share calc overflow")
                / debt_amount;
            if burned == 0 {
                burned = 1;
            }
            burned.min(position.debt_shares)
        };
        let new_position_shares = position
            .debt_shares
            .checked_sub(shares_burned)
            .expect("share underflow");
        position.debt_shares = new_position_shares;
        position.collateral_ptokens = position.collateral_ptokens.saturating_sub(seized_ptokens);
        let new_total_shares = total_shares
            .checked_sub(shares_burned)
            .expect("share underflow");
        set_debt_shares_total(
            &env,
            &position.owner,
            &position.debt_asset,
            new_total_shares,
        );
        if new_position_shares == 0 {
            position.status = PositionStatus::Liquidated;
            clear_position_initial_lock(&env, position_id);
            clear_position_vaults(&env, position_id);
            clear_position_mode(&env, position_id);
            remove_user_position(&env, &position.owner, position_id);
        } else {
            position.status = PositionStatus::Open;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(&env, position_id);
    }

    pub fn liquidate_position_v2(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        let debt_vault = ReceiptVaultClient::new(&env, &vaults.debt_vault);
        let debt_amount = debt_vault.get_margin_borrow_balance(&position_id);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        let debt_price = get_price_usd(&env, &position.debt_asset);
        let coll_price = get_price_usd(&env, &position.collateral_asset);
        let exchange_rate =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_exchange_rate();
        let collateral_cf = get_peridottroller(&env).get_market_cf(&vaults.position_vault);
        if collateral_cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let collateral_underlying =
            position.collateral_ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        let collateral_value_raw =
            collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        let collateral_value = collateral_value_raw.saturating_mul(collateral_cf) / SCALE_1E6;
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        if collateral_value >= debt_value {
            panic!("not liquidatable");
        }

        debt_vault.repay_for_margin(&position_id, &liquidator, &debt_amount);

        let seize_underlying = debt_value
            .saturating_mul(DEFAULT_MARGIN_LIQ_BONUS_SCALED)
            .saturating_mul(coll_price.1)
            / coll_price.0
            / SCALE_1E6;
        let mut seize_ptokens = seize_underlying.saturating_mul(SCALE_1E6) / exchange_rate;
        if seize_ptokens == 0 {
            seize_ptokens = 1;
        }
        if seize_ptokens > position.collateral_ptokens {
            seize_ptokens = position.collateral_ptokens;
        }
        if seize_ptokens > 0 {
            let controller = env.current_contract_address();
            let seize_i128: i128 = seize_ptokens.try_into().expect("amount too large");
            let transfer_args: Vec<Val> =
                (controller.clone(), liquidator.clone(), seize_i128).into_val(&env);
            Self::authorize_controller_subcall(
                &env,
                &vaults.position_vault,
                "transfer",
                transfer_args,
            );
            ReceiptVaultClient::new(&env, &vaults.position_vault).transfer(
                &controller,
                &liquidator,
                &seize_i128,
            );
            position.collateral_ptokens = position.collateral_ptokens.saturating_sub(seize_ptokens);
        }

        if let Some((initial_market, initial_ptokens)) =
            get_position_initial_lock(&env, position_id)
        {
            if initial_ptokens > 0 {
                let controller = env.current_contract_address();
                let amt_i128: i128 = initial_ptokens.try_into().expect("amount too large");
                let transfer_args: Vec<Val> =
                    (controller.clone(), liquidator.clone(), amt_i128).into_val(&env);
                Self::authorize_controller_subcall(
                    &env,
                    &initial_market,
                    "transfer",
                    transfer_args,
                );
                ReceiptVaultClient::new(&env, &initial_market).transfer(
                    &controller,
                    &liquidator,
                    &amt_i128,
                );
            }
        }

        position.status = PositionStatus::Liquidated;
        position.collateral_ptokens = 0u128;
        position.debt_shares = 0u128;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        clear_position_initial_lock(&env, position_id);
        clear_position_vaults(&env, position_id);
        clear_position_mode(&env, position_id);
        remove_user_position(&env, &position.owner, position_id);
    }

    pub fn locked_ptokens_in_market(env: Env, user: Address, market: Address) -> u128 {
        bump_core_ttl(&env);
        let position_ids = compact_user_positions(&env, &user);
        let mut total_locked = 0u128;
        for position_id in position_ids.iter() {
            let position: Option<Position> = env
                .storage()
                .persistent()
                .get(&DataKey::Position(position_id));
            let Some(position) = position else {
                continue;
            };
            if position.status != PositionStatus::Open {
                continue;
            }
            let mode = get_position_mode(&env, position_id);
            if mode == PositionMode::MarginV2 {
                continue;
            }

            let vaults = get_position_vaults(&env, position_id, &position);
            if vaults.position_vault == market {
                total_locked = total_locked.saturating_add(position.collateral_ptokens);
            }

            if let Some((initial_market, initial_ptokens)) =
                get_position_initial_lock(&env, position_id)
            {
                if initial_market == market {
                    total_locked = total_locked.saturating_add(initial_ptokens);
                }
            }
        }
        total_locked
    }

    pub fn get_position(env: Env, position_id: u64) -> Option<Position> {
        bump_core_ttl(&env);
        bump_position_ttl(&env, position_id);
        env.storage()
            .persistent()
            .get(&DataKey::Position(position_id))
    }

    /// ReceiptVault defense-in-depth callback.
    /// Returns the owner for an open MarginV2 position and validates debt-vault binding.
    pub fn get_margin_position_owner(env: Env, position_id: u64, debt_vault: Address) -> Address {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open {
            panic!("position not open");
        }
        if get_position_mode(&env, position_id) != PositionMode::MarginV2 {
            panic!("not v2 position");
        }
        let vaults = get_position_vaults(&env, position_id, &position);
        if vaults.debt_vault != debt_vault {
            panic!("wrong debt vault");
        }
        position.owner
    }

    pub fn get_user_positions(env: Env, user: Address) -> Vec<u64> {
        bump_core_ttl(&env);
        compact_user_positions(&env, &user)
    }

    pub fn get_health_factor(env: Env, position_id: u64) -> u128 {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        let vaults = get_position_vaults(&env, position_id, &position);
        let mode = get_position_mode(&env, position_id);
        let debt_amount = if mode == PositionMode::MarginV2 {
            ReceiptVaultClient::new(&env, &vaults.debt_vault)
                .get_margin_borrow_balance(&position_id)
        } else {
            let (debt, _total_shares, _total_debt) = debt_for_shares_in_vault(
                &env,
                &position.owner,
                &position.debt_asset,
                &vaults.debt_vault,
                position.debt_shares,
            );
            debt
        };
        if debt_amount == 0 {
            return u128::MAX;
        }
        let debt_price = get_price_usd(&env, &position.debt_asset);
        if debt_price.0 == 0 || debt_price.1 == 0 {
            panic!("invalid debt price");
        }
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        let coll_price = get_price_usd(&env, &position.collateral_asset);
        if coll_price.0 == 0 || coll_price.1 == 0 {
            panic!("invalid collateral price");
        }
        let collateral_cf = get_peridottroller(&env).get_market_cf(&vaults.position_vault);
        if collateral_cf > SCALE_1E6 {
            return u128::MAX;
        }
        let exchange_rate =
            ReceiptVaultClient::new(&env, &vaults.position_vault).get_exchange_rate();
        let collateral_underlying =
            position.collateral_ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        let collateral_value_raw =
            collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        let collateral_value = collateral_value_raw.saturating_mul(collateral_cf) / SCALE_1E6;
        if collateral_value == 0 {
            return 0;
        }
        collateral_value.saturating_mul(SCALE_1E6) / debt_value
    }

    fn oracle_min_out(env: &Env, token_in: &Address, token_out: &Address, amount_in: u128) -> u128 {
        let in_price = get_price_usd(env, token_in);
        let out_price = get_price_usd(env, token_out);
        if in_price.0 == 0 || in_price.1 == 0 || out_price.0 == 0 || out_price.1 == 0 {
            panic!("invalid price");
        }
        let in_value_usd = amount_in.saturating_mul(in_price.0) / in_price.1;
        let expected_out = in_value_usd.saturating_mul(out_price.1) / out_price.0;
        if expected_out == 0 {
            panic!("swap amount too small");
        }
        let max_slippage_bps = get_max_slippage_bps(env);
        expected_out.saturating_mul(SCALE_1E6.saturating_sub(max_slippage_bps)) / SCALE_1E6
    }

    fn assert_margin_lock_configured(env: &Env, vault: &Address) {
        let configured = env.try_invoke_contract::<Option<Address>, InvokeError>(
            vault,
            &Symbol::new(env, "get_margin_controller"),
            ().into_val(env),
        );
        match configured {
            Ok(Ok(Some(controller))) if controller == env.current_contract_address() => {}
            Ok(Ok(_)) => panic!("margin lock not configured"),
            Err(_) => panic!("margin lock not configured"),
            Ok(Err(_)) => panic!("margin lock not configured"),
        }
    }

    fn assert_valid_swap_adapter(env: &Env, swap_adapter: &Address) {
        match env.try_invoke_contract::<bool, InvokeError>(
            swap_adapter,
            &Symbol::new(env, "is_pool_allowed"),
            (env.current_contract_address(),).into_val(env),
        ) {
            Ok(Ok(true)) => {}
            _ => panic!("invalid swap adapter"),
        }
    }

    fn max_borrow_step_for_position(
        peridottroller: &PeridottrollerClient<'_>,
        user: &Address,
        debt_price: (u128, u128),
        max_requested: u128,
    ) -> u128 {
        if max_requested == 0 {
            return 0;
        }
        let (liquidity, shortfall) = peridottroller.account_liquidity(user);
        if shortfall > 0 || liquidity == 0 || debt_price.0 == 0 {
            return 0;
        }
        let liquidity_in_debt = liquidity.saturating_mul(debt_price.1) / debt_price.0;
        if liquidity_in_debt == 0 {
            return 0;
        }
        if liquidity_in_debt < max_requested {
            liquidity_in_debt
        } else {
            max_requested
        }
    }

    fn calculate_new_debt_shares(
        borrow_amount: u128,
        shares_before: u128,
        debt_before: u128,
    ) -> u128 {
        if shares_before == 0 || debt_before == 0 {
            return borrow_amount;
        }
        let numerator = borrow_amount
            .checked_mul(shares_before)
            .expect("share calc overflow");
        numerator
            .checked_add(debt_before - 1)
            .expect("share calc overflow")
            / debt_before
    }

    fn assert_no_residual_debt_when_all_shares_burned(
        env: &Env,
        user: &Address,
        debt_vault: &Address,
        new_total_shares: u128,
    ) {
        if new_total_shares != 0 {
            return;
        }
        let remaining = ReceiptVaultClient::new(env, debt_vault).get_user_borrow_balance(user);
        if remaining > 0 {
            panic!("residual debt");
        }
    }

    fn withdraw_initial_collateral_if_any(
        env: &Env,
        user: &Address,
        initial_lock: Option<(Address, u128)>,
    ) {
        let Some((initial_market, initial_ptokens)) = initial_lock else {
            return;
        };
        if initial_ptokens == 0 {
            return;
        }
        let vault = ReceiptVaultClient::new(env, &initial_market);
        Self::begin_margin_withdraw_if_supported(env, &initial_market, user);
        vault.withdraw(user, &initial_ptokens);
    }

    fn begin_margin_withdraw_if_supported(env: &Env, vault: &Address, user: &Address) {
        let controller = env.current_contract_address();
        let begin_args: Vec<Val> = (controller.clone(), user.clone()).into_val(env);
        Self::authorize_controller_subcall(env, vault, "begin_margin_withdraw", begin_args);
        let _ = env.try_invoke_contract::<(), InvokeError>(
            vault,
            &Symbol::new(env, "begin_margin_withdraw"),
            (controller, user.clone()).into_val(env),
        );
    }

    fn authorize_controller_subcall(env: &Env, contract: &Address, fn_name: &str, args: Vec<Val>) {
        let mut auths = Vec::new(env);
        auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: contract.clone(),
                fn_name: Symbol::new(env, fn_name),
                args,
            },
            sub_invocations: Vec::new(env),
        }));
        env.authorize_as_current_contract(auths);
    }

    // No-swap borrow path performs health checks before any additional collateral is added.
    // This caps leverage to what the initial collateral can support on its own.
    fn assert_pre_swap_leverage_supported(
        env: &Env,
        collateral_market: &Address,
        leverage: u128,
    ) -> u128 {
        let cf = get_peridottroller(env).get_market_cf(collateral_market);
        if cf > SCALE_1E6 {
            panic!("invalid market cf");
        }
        let requested_scaled = leverage.checked_mul(SCALE_1E6).expect("leverage overflow");
        let max_supported_scaled = SCALE_1E6.checked_add(cf).expect("cf overflow");
        if requested_scaled > max_supported_scaled {
            panic!("leverage unsupported pre-swap");
        }
        cf
    }
}

fn assert_expected_admin(env: &Env, admin: &Address) {
    #[cfg(test)]
    {
        let _ = env;
        let _ = admin;
        return;
    }
    #[cfg(not(test))]
    {
        let expected_admin_str = option_env!("MARGIN_CONTROLLER_INIT_ADMIN")
            .expect("MARGIN_CONTROLLER_INIT_ADMIN not set");
        let expected_admin = Address::from_string(&String::from_str(env, expected_admin_str));
        if admin != &expected_admin {
            panic!("unexpected admin");
        }
    }
}
