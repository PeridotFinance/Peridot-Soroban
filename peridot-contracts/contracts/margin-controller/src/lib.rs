#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Vec};

#[soroban_sdk::contractclient(name = "ReceiptVaultClient")]
pub trait ReceiptVaultContract {
    fn deposit(env: Env, user: Address, amount: u128);
    fn withdraw(env: Env, user: Address, ptoken_amount: u128);
    fn borrow(env: Env, user: Address, amount: u128);
    fn repay(env: Env, user: Address, amount: u128);
    fn get_exchange_rate(env: Env) -> u128;
    fn get_ptoken_balance(env: Env, user: Address) -> u128;
    fn get_user_borrow_balance(env: Env, user: Address) -> u128;
}

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait PeridottrollerContract {
    fn account_liquidity(env: Env, user: Address) -> (u128, u128);
    fn get_price_usd(env: Env, token: Address) -> Option<(u128, u128)>;
    fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        repay_market: Address,
        collateral_market: Address,
        repay_amount: u128,
    );
}

#[soroban_sdk::contractclient(name = "SwapAdapterClient")]
pub trait SwapAdapterContract {
    fn swap_chained(
        env: Env,
        user: Address,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        token_in: Address,
        in_amount: u128,
        out_min: u128,
    ) -> u128;
}

const SCALE_1E6: u128 = 1_000_000u128;

#[contracttype]
pub enum DataKey {
    Admin,
    Peridottroller,
    SwapAdapter,
    MaxLeverage,
    LiquidationBonus,
    Market(Address),
    PositionCounter,
    Position(u64),
    UserPositions(Address),
    DebtSharesTotal(Address, Address), // (user, debt_asset)
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionSide {
    Long,
    Short,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PositionStatus {
    Open,
    Closed,
    Liquidated,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Position {
    pub owner: Address,
    pub side: PositionSide,
    pub collateral_asset: Address,
    pub debt_asset: Address,
    pub collateral_ptokens: u128,
    pub debt_shares: u128,
    pub entry_price_scaled: u128,
    pub opened_at: u64,
    pub status: PositionStatus,
}

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
        if env.storage().persistent().get::<_, Address>(&DataKey::Admin).is_some() {
            panic!("already initialized");
        }
        admin.require_auth();
        if max_leverage < 1 {
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
        env.storage().persistent().set(&DataKey::PositionCounter, &0u64);
    }

    pub fn set_market(env: Env, admin: Address, asset: Address, vault: Address) {
        require_admin(&env, &admin);
        env.storage().persistent().set(&DataKey::Market(asset), &vault);
    }

    pub fn set_params(
        env: Env,
        admin: Address,
        max_leverage: u128,
        liquidation_bonus_scaled: u128,
    ) {
        require_admin(&env, &admin);
        if max_leverage < 1 {
            panic!("invalid leverage");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MaxLeverage, &max_leverage);
        env.storage()
            .persistent()
            .set(&DataKey::LiquidationBonus, &liquidation_bonus_scaled);
    }

    pub fn deposit_collateral(env: Env, user: Address, asset: Address, amount: u128) {
        user.require_auth();
        let vault = get_market(&env, &asset);
        ReceiptVaultClient::new(&env, &vault).deposit(&user, &amount);
    }

    pub fn withdraw_collateral(env: Env, user: Address, asset: Address, ptoken_amount: u128) {
        user.require_auth();
        let vault = get_market(&env, &asset);
        ReceiptVaultClient::new(&env, &vault).withdraw(&user, &ptoken_amount);
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
        out_min: u128,
    ) -> u64 {
        user.require_auth();
        let max_leverage = get_max_leverage(&env);
        if leverage < 1 || leverage > max_leverage {
            panic!("bad leverage");
        }
        if collateral_amount == 0 {
            panic!("bad collateral");
        }
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
        ReceiptVaultClient::new(&env, &collateral_vault).deposit(&user, &collateral_amount);

        // Borrow debt asset
        let debt_vault = get_market(&env, &debt_asset);
        let debt_before =
            ReceiptVaultClient::new(&env, &debt_vault).get_user_borrow_balance(&user);
        let shares_before = get_debt_shares_total(&env, &user, &debt_asset);
        let new_shares = if shares_before == 0 || debt_before == 0 {
            borrow_amount
        } else {
            borrow_amount.saturating_mul(shares_before) / debt_before
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
            &out_min,
        );
        if received == 0 {
            panic!("swap failed");
        }

        // Deposit swapped asset as collateral, track pTokens minted
        let position_vault = get_market(&env, &position_asset);
        let p_before = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
        ReceiptVaultClient::new(&env, &position_vault).deposit(&user, &received);
        let p_after = ReceiptVaultClient::new(&env, &position_vault).get_ptoken_balance(&user);
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
        push_user_position(&env, &user, id);
        id
    }

    pub fn close_position(
        env: Env,
        user: Address,
        position_id: u64,
        swaps_chain: Vec<(Vec<Address>, BytesN<32>, Address)>,
        out_min: u128,
    ) {
        user.require_auth();
        let mut position = get_position(&env, position_id);
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
            &out_min,
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
        remove_user_position(&env, &user, position_id);

        // Any remaining swap output stays with the user as profit
        let _unused = received.saturating_sub(debt_amount);
    }

    pub fn liquidate_position(
        env: Env,
        liquidator: Address,
        position_id: u64,
    ) {
        liquidator.require_auth();
        let mut position = get_position(&env, position_id);
        if position.status != PositionStatus::Open {
            panic!("not open");
        }
        let (liq, shortfall) = get_peridottroller(&env).account_liquidity(&position.owner);
        if shortfall == 0 || liq > 0 {
            panic!("not liquidatable");
        }

        let (debt_amount, total_shares, _total_debt) =
            debt_for_shares(&env, &position.owner, &position.debt_asset, position.debt_shares);
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
        set_debt_shares_total(&env, &position.owner, &position.debt_asset, new_total_shares);

        position.status = PositionStatus::Liquidated;
        env.storage()
            .persistent()
            .set(&DataKey::Position(position_id), &position);
        remove_user_position(&env, &position.owner, position_id);
    }

    pub fn get_position(env: Env, position_id: u64) -> Option<Position> {
        env.storage().persistent().get(&DataKey::Position(position_id))
    }

    pub fn get_user_positions(env: Env, user: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::UserPositions(user))
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_health_factor(env: Env, position_id: u64) -> u128 {
        let position = get_position(&env, position_id);
        let (debt_amount, total_shares, _total_debt) =
            debt_for_shares(&env, &position.owner, &position.debt_asset, position.debt_shares);
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
        let collateral_value = collateral_underlying.saturating_mul(coll_price.0) / coll_price.1;
        if collateral_value == 0 {
            return 0;
        }
        let _ = total_shares;
        collateral_value.saturating_mul(SCALE_1E6) / debt_value
    }
}

fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .expect("admin not set");
    if stored != *admin {
        panic!("not admin");
    }
    admin.require_auth();
}

fn get_market(env: &Env, asset: &Address) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::Market(asset.clone()))
        .expect("market not set")
}

fn get_peridottroller(env: &Env) -> PeridottrollerClient<'_> {
    let addr: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .expect("peridottroller not set");
    PeridottrollerClient::new(env, &addr)
}

fn get_swap_adapter(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::SwapAdapter)
        .expect("swap adapter not set")
}

fn get_max_leverage(env: &Env) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::MaxLeverage)
        .unwrap_or(1u128)
}

fn get_price_usd(env: &Env, asset: &Address) -> (u128, u128) {
    let peridottroller = get_peridottroller(env);
    peridottroller
        .get_price_usd(asset)
        .expect("price unavailable")
}

fn next_position_id(env: &Env) -> u64 {
    let mut id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::PositionCounter)
        .unwrap_or(0u64);
    id = id.saturating_add(1);
    env.storage().persistent().set(&DataKey::PositionCounter, &id);
    id
}

fn push_user_position(env: &Env, user: &Address, id: u64) {
    let mut positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    positions.push_back(id);
    env.storage()
        .persistent()
        .set(&DataKey::UserPositions(user.clone()), &positions);
}

fn remove_user_position(env: &Env, user: &Address, id: u64) {
    let positions: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::UserPositions(user.clone()))
        .unwrap_or(Vec::new(env));
    let mut out = Vec::new(env);
    for p in positions.iter() {
        if p != id {
            out.push_back(p);
        }
    }
    env.storage()
        .persistent()
        .set(&DataKey::UserPositions(user.clone()), &out);
}

fn get_debt_shares_total(env: &Env, user: &Address, debt_asset: &Address) -> u128 {
    env.storage()
        .persistent()
        .get(&DataKey::DebtSharesTotal(user.clone(), debt_asset.clone()))
        .unwrap_or(0u128)
}

fn set_debt_shares_total(env: &Env, user: &Address, debt_asset: &Address, value: u128) {
    env.storage()
        .persistent()
        .set(&DataKey::DebtSharesTotal(user.clone(), debt_asset.clone()), &value);
}

fn debt_for_shares(
    env: &Env,
    user: &Address,
    debt_asset: &Address,
    shares: u128,
) -> (u128, u128, u128) {
    let total_shares = get_debt_shares_total(env, user, debt_asset);
    if total_shares == 0 || shares == 0 {
        return (0, total_shares, 0);
    }
    let debt_vault = get_market(env, debt_asset);
    let total_debt = ReceiptVaultClient::new(env, &debt_vault).get_user_borrow_balance(user);
    let debt_amount = shares.saturating_mul(total_debt) / total_shares;
    (debt_amount, total_shares, total_debt)
}

fn get_position(env: &Env, position_id: u64) -> Position {
    env.storage()
        .persistent()
        .get(&DataKey::Position(position_id))
        .expect("position missing")
}

#[cfg(test)]
mod test;
