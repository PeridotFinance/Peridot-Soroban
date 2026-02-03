use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec};

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
        liquidation_bonus_scaled: u128,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        if env
            .storage()
            .persistent()
            .get::<_, Address>(&DataKey::Admin)
            .is_some()
        {
            panic!("already initialized");
        }
        admin.require_auth();
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
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
            .set(&DataKey::LiquidationBonus, &liquidation_bonus_scaled);
        env.storage()
            .persistent()
            .set(&DataKey::PositionCounter, &0u64);
        env.storage().instance().set(&DataKey::Initialized, &true);
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

    pub fn set_params(
        env: Env,
        admin: Address,
        max_leverage: u128,
        liquidation_bonus_scaled: u128,
    ) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
        if max_leverage < 1 || max_leverage > MAX_LEVERAGE_CAP {
            panic!("invalid leverage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationBonus, &liquidation_bonus_scaled);
    }

    pub fn set_swap_adapter(env: Env, admin: Address, swap_adapter: Address) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
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
        ReceiptVaultClient::new(&env, &vault).withdraw(&user, &ptoken_amount);
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_core_ttl(&env);
        require_admin(&env, &admin);
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
        validate_swaps_chain(&swaps_chain);
        let (debt_asset, position_asset) = match side {
            PositionSide::Long => (collateral_asset.clone(), base_asset.clone()),
            PositionSide::Short => (base_asset.clone(), collateral_asset.clone()),
        };
        let collateral_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        let collateral_value = collateral_amount
            .saturating_mul(collateral_price.0)
            / collateral_price.1;
        let target_value = collateral_value.saturating_mul(leverage);
        let borrow_value = target_value.saturating_sub(collateral_value);
        if borrow_value == 0 {
            panic!("zero borrow");
        }
        let borrow_amount = borrow_value.saturating_mul(debt_price.1) / debt_price.0;
        if borrow_amount == 0 {
            panic!("borrow too small");
        }

        // Deposit initial collateral
        let collateral_vault = get_market(&env, &collateral_asset);
        let p_before =
            ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &collateral_vault).deposit(&user, &collateral_amount);
        let p_after =
            ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }

        // Borrow debt asset
        let debt_vault = get_market(&env, &debt_asset);
        let debt_before =
            ReceiptVaultClient::new(&env, &debt_vault).get_user_borrow_balance(&user);
        let shares_before = get_debt_shares_total(&env, &user, &debt_asset);
        let new_shares = if shares_before == 0 || debt_before == 0 {
            borrow_amount
        } else {
            let numerator = borrow_amount.saturating_mul(shares_before);
            let mut shares = numerator / debt_before;
            if numerator > 0 && shares == 0 {
                shares = 1;
            }
            shares
        };
        ReceiptVaultClient::new(&env, &debt_vault).borrow(&user, &borrow_amount);
        set_debt_shares_total(
            &env,
            &user,
            &debt_asset,
            shares_before.saturating_add(new_shares),
        );

        // Swap borrowed debt asset to position asset via Aquarius
        let swap_adapter = get_swap_adapter(&env);
        let received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &debt_asset,
            &borrow_amount,
            &amount_with_slippage,
        );
        if received == 0 {
            panic!("swap failed");
        }

        // Deposit swapped asset as collateral, track pTokens minted
        let position_vault = get_market(&env, &position_asset);
        let p_before =
            ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &position_vault).deposit(&user, &received);
        let p_after =
            ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }

        let entry_price_scaled = borrow_value
            .saturating_mul(SCALE_1E6)
            .saturating_mul(debt_price.1)
            / debt_price.0
            / received;

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
        if collateral_amount == 0 || borrow_amount == 0 {
            panic!("bad amounts");
        }
        let collateral_price = get_price_usd(&env, &collateral_asset);
        let debt_price = get_price_usd(&env, &debt_asset);
        let collateral_value = collateral_amount
            .saturating_mul(collateral_price.0)
            / collateral_price.1;
        let borrow_value = borrow_amount.saturating_mul(debt_price.0) / debt_price.1;
        let target_value = collateral_value.saturating_mul(leverage);
        if borrow_value >= target_value {
            panic!("borrow exceeds leverage");
        }

        // Deposit initial collateral
        let collateral_vault = get_market(&env, &collateral_asset);
        let p_before =
            ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &collateral_vault).deposit(&user, &collateral_amount);
        let p_after =
            ReceiptVaultClient::new(&env, &collateral_vault).get_ptoken_balance(&user);
        let p_delta = p_after.saturating_sub(p_before);
        if p_delta == 0 {
            panic!("no collateral minted");
        }

        // Borrow debt asset
        let debt_vault = get_market(&env, &debt_asset);
        let debt_before =
            ReceiptVaultClient::new(&env, &debt_vault).get_user_borrow_balance(&user);
        let shares_before = get_debt_shares_total(&env, &user, &debt_asset);
        let new_shares = if shares_before == 0 || debt_before == 0 {
            borrow_amount
        } else {
            let numerator = borrow_amount.saturating_mul(shares_before);
            let mut shares = numerator / debt_before;
            if numerator > 0 && shares == 0 {
                shares = 1;
            }
            shares
        };
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
        validate_swaps_chain(&swaps_chain);
        let mut position = get_position_or_panic(&env, position_id);
        if position.owner != user {
            panic!("not owner");
        }
        if position.status != PositionStatus::Open {
            panic!("not open");
        }

        let (debt_amount, total_shares, _total_debt) =
            debt_for_shares(&env, &user, &position.debt_asset, position.debt_shares);
        if debt_amount == 0 {
            panic!("zero debt");
        }

        if position.collateral_asset == position.debt_asset {
            let debt_vault = get_market(&env, &position.debt_asset);
            ReceiptVaultClient::new(&env, &debt_vault).repay(&user, &debt_amount);
            let vault = get_market(&env, &position.collateral_asset);
            ReceiptVaultClient::new(&env, &vault)
                .withdraw(&user, &position.collateral_ptokens);

            let new_total_shares = total_shares.saturating_sub(position.debt_shares);
            set_debt_shares_total(&env, &user, &position.debt_asset, new_total_shares);

            position.status = PositionStatus::Closed;
            env.storage()
                .persistent()
                .set(&DataKey::Position(position_id), &position);
            bump_position_ttl(&env, position_id);
            remove_user_position(&env, &user, position_id);
            return;
        }

        let vault = get_market(&env, &position.collateral_asset);
        let exchange_rate = ReceiptVaultClient::new(&env, &vault).get_exchange_rate();
        let collateral_underlying =
            position.collateral_ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        ReceiptVaultClient::new(&env, &vault).withdraw(&user, &position.collateral_ptokens);

        let swap_adapter = get_swap_adapter(&env);
        let received = SwapAdapterClient::new(&env, &swap_adapter).swap_chained(
            &user,
            &swaps_chain,
            &position.collateral_asset,
            &collateral_underlying,
            &amount_with_slippage,
        );
        if received < debt_amount {
            panic!("insufficient swap output");
        }

        let debt_vault = get_market(&env, &position.debt_asset);
        ReceiptVaultClient::new(&env, &debt_vault).repay(&user, &debt_amount);

        let new_total_shares = total_shares.saturating_sub(position.debt_shares);
        set_debt_shares_total(&env, &user, &position.debt_asset, new_total_shares);

        position.status = PositionStatus::Closed;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(&env, position_id);
        remove_user_position(&env, &user, position_id);

        // Any remaining swap output stays with the user as profit
        let _unused = received.saturating_sub(debt_amount);
    }

    pub fn liquidate_position(env: Env, liquidator: Address, position_id: u64) {
        bump_core_ttl(&env);
        liquidator.require_auth();
        let mut position = get_position_or_panic(&env, position_id);
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        if liquidator == position.owner {
            panic!("self liquidation");
        }
        let (liq, shortfall) =
            get_peridottroller(&env).account_liquidity(&position.owner);
        if shortfall == 0 || liq > 0 {
            panic!("not liquidatable");
        }

        let (debt_amount, total_shares, _total_debt) = debt_for_shares(
            &env,
            &position.owner,
            &position.debt_asset,
            position.debt_shares,
        );
        if debt_amount == 0 {
            panic!("zero debt");
        }
        let debt_vault = get_market(&env, &position.debt_asset);
        let collateral_vault = get_market(&env, &position.collateral_asset);
        get_peridottroller(&env).liquidate(
            &liquidator,
            &position.owner,
            &debt_vault,
            &collateral_vault,
            &debt_amount,
        );

        let new_total_shares = total_shares.saturating_sub(position.debt_shares);
        set_debt_shares_total(
            &env,
            &position.owner,
            &position.debt_asset,
            new_total_shares,
        );

        position.status = PositionStatus::Liquidated;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        bump_position_ttl(&env, position_id);
        remove_user_position(&env, &position.owner, position_id);
    }

    pub fn get_position(env: Env, position_id: u64) -> Option<Position> {
        bump_core_ttl(&env);
        bump_position_ttl(&env, position_id);
        env.storage()
            .persistent()
            .get(&DataKey::Position(position_id))
    }

    pub fn get_user_positions(env: Env, user: Address) -> Vec<u64> {
        bump_core_ttl(&env);
        bump_user_positions_ttl(&env, &user);
        env.storage()
            .persistent()
            .get(&DataKey::UserPositions(user))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_health_factor(env: Env, position_id: u64) -> u128 {
        bump_core_ttl(&env);
        let position = get_position_or_panic(&env, position_id);
        let (debt_amount, total_shares, _total_debt) = debt_for_shares(
            &env,
            &position.owner,
            &position.debt_asset,
            position.debt_shares,
        );
        if debt_amount == 0 {
            return u128::MAX;
        }
        let debt_price = get_price_usd(&env, &position.debt_asset);
        let debt_value = debt_amount.saturating_mul(debt_price.0) / debt_price.1;
        let coll_price = get_price_usd(&env, &position.collateral_asset);
        let vault = get_market(&env, &position.collateral_asset);
        let exchange_rate = ReceiptVaultClient::new(&env, &vault).get_exchange_rate();
        let collateral_underlying =
            position.collateral_ptokens.saturating_mul(exchange_rate) / SCALE_1E6;
        let collateral_value =
            collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        if collateral_value == 0 {
            return 0;
        }
        let _ = total_shares;
        collateral_value.saturating_mul(SCALE_1E6) / debt_value
    }
}
