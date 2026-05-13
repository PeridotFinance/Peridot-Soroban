#![no_std]

use soroban_sdk::auth::{Context, ContractContext, CustomAccountInterface};
use soroban_sdk::crypto::Hash;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal,
    String, Symbol, TryIntoVal, Vec,
};

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait Peridottroller {
    fn hypothetical_liquidity(
        env: Env,
        user: Address,
        market: Address,
        borrow_amount: u128,
        underlying: Address,
    ) -> (u128, u128);

    fn preview_redeem_max(env: Env, user: Address, market: Address) -> u128;
}

#[soroban_sdk::contractclient(name = "ReceiptVaultClient")]
pub trait ReceiptVault {
    fn get_underlying_token(env: Env) -> Address;
}

/// Basic smart account that enforces policy via `__check_auth`.
///
/// # Example (doctest, no_run)
/// ```no_run
/// use soroban_sdk::{Env, Address, BytesN};
/// use soroban_sdk::testutils::Address as _;
/// use smart_account_basic::{BasicSmartAccount, BasicSmartAccountClient};
///
/// let env = Env::default();
/// env.mock_all_auths();
/// let owner = Address::generate(&env);
/// let signer = BytesN::from_array(&env, &[1u8; 32]);
/// let peridottroller = Address::generate(&env);
/// let margin = Address::generate(&env);
///
/// let contract_id = env.register(BasicSmartAccount, ());
/// let client = BasicSmartAccountClient::new(&env, &contract_id);
/// client.initialize(&owner, &signer, &peridottroller, &margin);
/// ```
#[contract]
pub struct BasicSmartAccount;

#[contracttype]
pub enum DataKey {
    Factory,
    Owner,
    Signer(BytesN<32>),
    SignerCount,
    Peridottroller,
    MarginController,
    AllowedContract(Address),
    AllowedContractUnderlying(Address),
    Initialized,
    PendingUpgradeHash,
    PendingUpgradeEta,
}

#[contracttype]
pub struct Signature {
    pub public_key: BytesN<32>,
    pub signature: BytesN<64>,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unauthorized = 1,
    InvalidSignature = 2,
    InsufficientHealth = 3,
    NotInitialized = 4,
}

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const UPGRADE_TIMELOCK_SECS: u64 = 24 * 60 * 60;
const MAX_SIGNERS: u32 = 8;
const DEFAULT_FACTORY_ADDRESS: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";

#[contractimpl]
impl BasicSmartAccount {
    pub fn __constructor(env: Env, factory: Address) {
        assert_expected_factory(&env, &factory);
        if env.storage().persistent().has(&DataKey::Factory) {
            panic!("already constructed");
        }
        env.storage().persistent().set(&DataKey::Factory, &factory);
        bump_ttl(&env);
    }

    pub fn initialize(
        env: Env,
        owner: Address,
        signer: BytesN<32>,
        peridottroller: Address,
        margin_controller: Address,
    ) {
        if env.storage().persistent().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        let factory: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Factory)
            .expect("factory not set");
        assert_expected_factory(&env, &factory);
        factory.require_auth();
        let persistent = env.storage().persistent();
        persistent.set(&DataKey::Owner, &owner);
        persistent.set(&DataKey::Signer(signer.clone()), &true);
        persistent.set(&DataKey::SignerCount, &1u32);
        persistent.set(&DataKey::Peridottroller, &peridottroller);
        persistent.set(&DataKey::MarginController, &margin_controller);
        persistent.set(&DataKey::Initialized, &true);
        bump_signer_ttl(&env, &signer);
        bump_ttl(&env);
    }

    pub fn get_owner(env: Env) -> Address {
        bump_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Owner)
            .expect("owner not set")
    }

    pub fn has_signer(env: Env, signer: BytesN<32>) -> bool {
        bump_ttl(&env);
        bump_signer_ttl(&env, &signer);
        env.storage()
            .persistent()
            .get(&DataKey::Signer(signer))
            .unwrap_or(false)
    }

    pub fn add_signer(env: Env, owner: Address, signer: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        let persistent = env.storage().persistent();
        let key = DataKey::Signer(signer.clone());
        if persistent.get::<_, bool>(&key).unwrap_or(false) {
            bump_signer_ttl(&env, &signer);
            return;
        }
        let count: u32 = persistent.get(&DataKey::SignerCount).unwrap_or(0u32);
        if count >= MAX_SIGNERS {
            panic!("too many signers");
        }
        persistent.set(&key, &true);
        persistent.set(&DataKey::SignerCount, &(count + 1));
        bump_signer_ttl(&env, &signer);
    }

    pub fn remove_signer(env: Env, owner: Address, signer: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        let persistent = env.storage().persistent();
        let key = DataKey::Signer(signer.clone());
        if !persistent.get::<_, bool>(&key).unwrap_or(false) {
            return;
        }
        let count: u32 = persistent.get(&DataKey::SignerCount).unwrap_or(0u32);
        if count <= 1 {
            panic!("cannot remove last signer");
        }
        persistent.remove(&key);
        persistent.set(&DataKey::SignerCount, &(count - 1));
    }

    pub fn set_peridottroller(env: Env, owner: Address, peridottroller: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .persistent()
            .set(&DataKey::Peridottroller, &peridottroller);
    }

    pub fn set_margin_controller(env: Env, owner: Address, margin_controller: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .persistent()
            .set(&DataKey::MarginController, &margin_controller);
    }

    pub fn add_allowed_contract(env: Env, owner: Address, contract: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .persistent()
            .set(&DataKey::AllowedContract(contract.clone()), &true);
        bump_allowed_contract_ttl(&env, &contract);
    }

    pub fn add_allowed_vault(env: Env, owner: Address, vault: Address, underlying: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        let reported = ReceiptVaultClient::new(&env, &vault).get_underlying_token();
        if reported != underlying {
            panic!("underlying mismatch");
        }
        env.storage()
            .persistent()
            .set(&DataKey::AllowedContract(vault.clone()), &true);
        env.storage().persistent().set(
            &DataKey::AllowedContractUnderlying(vault.clone()),
            &underlying,
        );
        bump_allowed_contract_ttl(&env, &vault);
    }

    pub fn remove_allowed_contract(env: Env, owner: Address, contract: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .persistent()
            .remove(&DataKey::AllowedContract(contract.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::AllowedContractUnderlying(contract));
    }

    pub fn is_allowed_contract(env: Env, contract: Address) -> bool {
        bump_ttl(&env);
        bump_allowed_contract_ttl(&env, &contract);
        env.storage()
            .persistent()
            .get(&DataKey::AllowedContract(contract))
            .unwrap_or(false)
    }

    pub fn bump_ttl(env: Env) {
        bump_ttl(&env);
    }

    pub fn propose_upgrade_wasm(env: Env, owner: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
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

    pub fn upgrade_wasm(env: Env, owner: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
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
}

#[contractimpl]
impl CustomAccountInterface for BasicSmartAccount {
    type Signature = Vec<Signature>;
    type Error = Error;

    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signatures: Self::Signature,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        bump_ttl(&env);
        verify_signatures(&env, &signature_payload, &signatures)?;
        enforce_policies(&env, &auth_contexts)?;
        Ok(())
    }
}

fn enforce_policies(env: &Env, auth_contexts: &Vec<Context>) -> Result<(), Error> {
    for i in 0..auth_contexts.len() {
        let ctx = auth_contexts.get(i).unwrap();
        match ctx {
            Context::Contract(contract_ctx) => {
                enforce_contract_policy(env, &contract_ctx)?;
            }
            _ => return Err(Error::Unauthorized),
        }
    }
    Ok(())
}

fn enforce_contract_policy(env: &Env, ctx: &ContractContext) -> Result<(), Error> {
    let fn_name = ctx.fn_name.clone();
    let is_vault = is_allowed_vault_contract(env, &ctx.contract);
    let is_margin = is_margin_controller_contract(env, &ctx.contract);
    let is_token_auth_fn = is_token_auth_function(env, &fn_name);
    let is_sensitive_vault_fn = is_sensitive_vault_function(env, &fn_name);
    let is_sensitive_margin_fn = is_sensitive_margin_function(env, &fn_name);

    // Allow token auth methods only when this account is the token source/owner.
    // This keeps repay/deposit token flows functional while still rejecting arbitrary auth.
    if is_token_auth_fn && !is_vault && !is_margin {
        return enforce_token_auth_policy(env, ctx, &fn_name);
    }

    // Fail closed: if a sensitive protocol method is requested on an
    // unrecognized contract, reject authorization.
    if is_sensitive_vault_fn && !is_vault {
        return Err(Error::Unauthorized);
    }
    if is_sensitive_margin_fn && !is_margin {
        return Err(Error::Unauthorized);
    }

    if is_vault && fn_name == Symbol::new(env, "borrow") {
        check_borrow_policy(env, ctx)?;
    } else if is_vault
        && (fn_name == Symbol::new(env, "deposit") || fn_name == Symbol::new(env, "repay"))
    {
        check_first_address_is_self(env, ctx, 0)?;
    } else if is_vault
        && (fn_name == Symbol::new(env, "borrow_for_margin")
            || fn_name == Symbol::new(env, "repay_for_margin"))
    {
        // Margin debt helpers are called by the margin controller, but the
        // vault still requires the receiver/payer to authorize at arg index 1.
        check_first_address_is_self(env, ctx, 1)?;
    } else if is_vault && fn_name == Symbol::new(env, "withdraw") {
        check_redeem_policy(env, ctx, 0, 1)?;
    } else if is_vault && fn_name == Symbol::new(env, "transfer") {
        check_redeem_policy(env, ctx, 0, 2)?;
        let to = get_address_arg(env, ctx, 1)?;
        if !is_protocol_recipient(env, &to) {
            return Err(Error::Unauthorized);
        }
    } else if is_margin
        && (fn_name == Symbol::new(env, "deposit_collateral")
            || fn_name == Symbol::new(env, "withdraw_collateral")
            || fn_name == Symbol::new(env, "transfer_spot_to_margin")
            || fn_name == Symbol::new(env, "transfer_margin_to_spot")
            || fn_name == Symbol::new(env, "open_position")
            || fn_name == Symbol::new(env, "open_position_v2")
            || fn_name == Symbol::new(env, "open_position_no_swap")
            || fn_name == Symbol::new(env, "open_position_no_swap_short")
            || fn_name == Symbol::new(env, "open_position_no_swap_v2")
            || fn_name == Symbol::new(env, "close_position")
            || fn_name == Symbol::new(env, "close_position_v2")
            || fn_name == Symbol::new(env, "close_position_no_swap_v2")
            || fn_name == Symbol::new(env, "liquidate_position")
            || fn_name == Symbol::new(env, "liquidate_position_v2"))
    {
        check_first_address_is_self(env, ctx, 0)?;
    } else if is_vault || is_margin {
        return Err(Error::Unauthorized);
    }
    Ok(())
}

fn is_sensitive_vault_function(env: &Env, fn_name: &Symbol) -> bool {
    *fn_name == Symbol::new(env, "borrow")
        || *fn_name == Symbol::new(env, "deposit")
        || *fn_name == Symbol::new(env, "repay")
        || *fn_name == Symbol::new(env, "borrow_for_margin")
        || *fn_name == Symbol::new(env, "repay_for_margin")
        || *fn_name == Symbol::new(env, "withdraw")
        || *fn_name == Symbol::new(env, "transfer")
}

fn is_sensitive_margin_function(env: &Env, fn_name: &Symbol) -> bool {
    *fn_name == Symbol::new(env, "deposit_collateral")
        || *fn_name == Symbol::new(env, "withdraw_collateral")
        || *fn_name == Symbol::new(env, "transfer_spot_to_margin")
        || *fn_name == Symbol::new(env, "transfer_margin_to_spot")
        || *fn_name == Symbol::new(env, "open_position")
        || *fn_name == Symbol::new(env, "open_position_v2")
        || *fn_name == Symbol::new(env, "open_position_no_swap")
        || *fn_name == Symbol::new(env, "open_position_no_swap_short")
        || *fn_name == Symbol::new(env, "open_position_no_swap_v2")
        || *fn_name == Symbol::new(env, "close_position")
        || *fn_name == Symbol::new(env, "close_position_v2")
        || *fn_name == Symbol::new(env, "close_position_no_swap_v2")
        || *fn_name == Symbol::new(env, "liquidate_position")
        || *fn_name == Symbol::new(env, "liquidate_position_v2")
}

fn is_token_auth_function(env: &Env, fn_name: &Symbol) -> bool {
    *fn_name == Symbol::new(env, "transfer")
        || *fn_name == Symbol::new(env, "transfer_from")
        || *fn_name == Symbol::new(env, "approve")
}

fn enforce_token_auth_policy(
    env: &Env,
    ctx: &ContractContext,
    fn_name: &Symbol,
) -> Result<(), Error> {
    if *fn_name == Symbol::new(env, "transfer") {
        check_first_address_is_self(env, ctx, 0)?;
        let to = get_address_arg(env, ctx, 1)?;
        if !is_protocol_recipient(env, &to) {
            return Err(Error::Unauthorized);
        }
        return Ok(());
    }
    if *fn_name == Symbol::new(env, "approve") {
        check_first_address_is_self(env, ctx, 0)?;
        let spender = get_address_arg(env, ctx, 1)?;
        if !is_protocol_recipient(env, &spender) {
            return Err(Error::Unauthorized);
        }
        return Ok(());
    }
    if *fn_name == Symbol::new(env, "transfer_from") {
        let owner = get_address_arg(env, ctx, 1)?;
        if owner != env.current_contract_address() {
            return Err(Error::Unauthorized);
        }
        let to = get_address_arg(env, ctx, 2)?;
        if !is_protocol_recipient(env, &to) {
            return Err(Error::Unauthorized);
        }
        return Ok(());
    }
    Err(Error::Unauthorized)
}

fn is_protocol_recipient(env: &Env, addr: &Address) -> bool {
    is_allowed_vault_contract(env, addr) || is_margin_controller_contract(env, addr)
}

fn check_borrow_policy(env: &Env, ctx: &ContractContext) -> Result<(), Error> {
    let user = get_address_arg(env, ctx, 0)?;
    require_self_address(env, &user)?;
    let borrow_amount: u128 = ctx
        .args
        .get(1)
        .ok_or(Error::Unauthorized)?
        .try_into_val(env)
        .map_err(|_| Error::Unauthorized)?;

    let peridottroller: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .ok_or(Error::NotInitialized)?;

    let expected_underlying: Address = env
        .storage()
        .persistent()
        .get(&DataKey::AllowedContractUnderlying(ctx.contract.clone()))
        .ok_or(Error::Unauthorized)?;
    let underlying = ReceiptVaultClient::new(env, &ctx.contract).get_underlying_token();
    if underlying != expected_underlying {
        return Err(Error::Unauthorized);
    }
    let (_liq, shortfall) = PeridottrollerClient::new(env, &peridottroller).hypothetical_liquidity(
        &user,
        &ctx.contract,
        &borrow_amount,
        &underlying,
    );
    if shortfall > 0 {
        return Err(Error::InsufficientHealth);
    }
    Ok(())
}

fn check_redeem_policy(
    env: &Env,
    ctx: &ContractContext,
    user_index: u32,
    amount_index: u32,
) -> Result<(), Error> {
    let user = get_address_arg(env, ctx, user_index)?;
    require_self_address(env, &user)?;
    let amount = get_u128_arg(env, ctx, amount_index)?;
    let peridottroller: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Peridottroller)
        .ok_or(Error::NotInitialized)?;
    let max_redeem =
        PeridottrollerClient::new(env, &peridottroller).preview_redeem_max(&user, &ctx.contract);
    if amount > max_redeem {
        return Err(Error::InsufficientHealth);
    }
    Ok(())
}

fn check_first_address_is_self(env: &Env, ctx: &ContractContext, index: u32) -> Result<(), Error> {
    let address = get_address_arg(env, ctx, index)?;
    require_self_address(env, &address)
}

fn get_address_arg(env: &Env, ctx: &ContractContext, index: u32) -> Result<Address, Error> {
    ctx.args
        .get(index)
        .ok_or(Error::Unauthorized)?
        .try_into_val(env)
        .map_err(|_| Error::Unauthorized)
}

fn get_u128_arg(env: &Env, ctx: &ContractContext, index: u32) -> Result<u128, Error> {
    let value = ctx.args.get(index).ok_or(Error::Unauthorized)?;
    let unsigned: Result<u128, _> = value.try_into_val(env);
    if let Ok(amount) = unsigned {
        return Ok(amount);
    }
    let signed: i128 = value.try_into_val(env).map_err(|_| Error::Unauthorized)?;
    if signed < 0 {
        return Err(Error::Unauthorized);
    }
    Ok(signed as u128)
}

fn require_self_address(env: &Env, address: &Address) -> Result<(), Error> {
    if *address != env.current_contract_address() {
        return Err(Error::Unauthorized);
    }
    Ok(())
}

fn is_allowed_vault_contract(env: &Env, contract: &Address) -> bool {
    bump_allowed_contract_ttl(env, contract);
    env.storage()
        .persistent()
        .get(&DataKey::AllowedContract(contract.clone()))
        .unwrap_or(false)
}

fn is_margin_controller_contract(env: &Env, contract: &Address) -> bool {
    let expected: Option<Address> = env.storage().persistent().get(&DataKey::MarginController);
    matches!(expected, Some(addr) if addr == *contract)
}

fn verify_signatures(
    env: &Env,
    signature_payload: &Hash<32>,
    signatures: &Vec<Signature>,
) -> Result<(), Error> {
    if signatures.len() == 0 || signatures.len() > MAX_SIGNERS {
        return Err(Error::Unauthorized);
    }
    for i in 0..signatures.len() {
        let sig = signatures.get(i).unwrap();
        for j in (i + 1)..signatures.len() {
            let other = signatures.get(j).unwrap();
            if sig.public_key == other.public_key {
                return Err(Error::Unauthorized);
            }
        }
    }
    let msg: Bytes = signature_payload.to_bytes().into_val(env);
    for i in 0..signatures.len() {
        let sig = signatures.get(i).unwrap();
        let allowed: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Signer(sig.public_key.clone()))
            .unwrap_or(false);
        if !allowed {
            return Err(Error::Unauthorized);
        }
        bump_signer_ttl(env, &sig.public_key);
        env.crypto()
            .ed25519_verify(&sig.public_key, &msg, &sig.signature);
    }
    Ok(())
}

fn require_owner(env: &Env, owner: &Address) {
    let stored: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Owner)
        .expect("owner not set");
    if stored != *owner {
        panic!("not owner");
    }
    bump_ttl(env);
    owner.require_auth();
}

fn expected_factory_config() -> Option<&'static str> {
    if cfg!(test) {
        option_env!("SMART_ACCOUNT_BASIC_FACTORY").or(Some(DEFAULT_FACTORY_ADDRESS))
    } else {
        Some(
            option_env!("SMART_ACCOUNT_BASIC_FACTORY")
                .expect("SMART_ACCOUNT_BASIC_FACTORY must be set at build time"),
        )
    }
}

fn assert_expected_factory(env: &Env, factory: &Address) {
    if let Some(expected) = expected_factory_config() {
        let expected_factory = Address::from_string(&String::from_str(env, expected));
        if *factory != expected_factory {
            panic!("unexpected factory");
        }
    }
}

fn bump_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::Initialized) {
        persistent.extend_ttl(&DataKey::Initialized, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Factory) {
        persistent.extend_ttl(&DataKey::Factory, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Owner) {
        persistent.extend_ttl(&DataKey::Owner, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::SignerCount) {
        persistent.extend_ttl(&DataKey::SignerCount, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::Peridottroller) {
        persistent.extend_ttl(&DataKey::Peridottroller, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::MarginController) {
        persistent.extend_ttl(&DataKey::MarginController, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_pending_upgrade_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::PendingUpgradeHash) {
        persistent.extend_ttl(&DataKey::PendingUpgradeHash, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    if persistent.has(&DataKey::PendingUpgradeEta) {
        persistent.extend_ttl(&DataKey::PendingUpgradeEta, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_signer_ttl(env: &Env, signer: &BytesN<32>) {
    let persistent = env.storage().persistent();
    let key = DataKey::Signer(signer.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_allowed_contract_ttl(env: &Env, contract: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::AllowedContract(contract.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
    let underlying_key = DataKey::AllowedContractUnderlying(contract.clone());
    if persistent.has(&underlying_key) {
        persistent.extend_ttl(&underlying_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

#[cfg(test)]
mod test;
