#![no_std]

use soroban_sdk::auth::{Context, ContractContext, CustomAccountInterface};
use soroban_sdk::crypto::Hash;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal,
    Symbol, TryIntoVal, Vec,
};
#[cfg(not(test))]
use soroban_sdk::String;

#[soroban_sdk::contractclient(name = "PeridottrollerClient")]
pub trait Peridottroller {
    fn hypothetical_liquidity(
        env: Env,
        user: Address,
        market: Address,
        borrow_amount: u128,
        underlying: Address,
    ) -> (u128, u128);
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
    Owner,
    Signer(BytesN<32>),
    Peridottroller,
    MarginController,
    Initialized,
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

const TTL_THRESHOLD: u32 = 100_000;
const TTL_EXTEND_TO: u32 = 200_000;

#[contractimpl]
impl BasicSmartAccount {
    pub fn __constructor(env: Env) {
        bump_ttl(&env);
    }

    pub fn initialize(
        env: Env,
        owner: Address,
        signer: BytesN<32>,
        peridottroller: Address,
        margin_controller: Address,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        // Require factory authorization for initialization (prevents takeover).
        #[cfg(not(test))]
        {
            if let Some(factory_str) = option_env!("SMART_ACCOUNT_FACTORY_ID") {
                let factory = Address::from_string(&String::from_str(&env, factory_str));
                factory.require_auth();
            }
        }
        owner.require_auth();
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::Signer(signer), &true);
        env.storage()
            .instance()
            .set(&DataKey::Peridottroller, &peridottroller);
        env.storage()
            .instance()
            .set(&DataKey::MarginController, &margin_controller);
        env.storage().instance().set(&DataKey::Initialized, &true);
        bump_ttl(&env);
    }

    pub fn get_owner(env: Env) -> Address {
        bump_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::Owner)
            .expect("owner not set")
    }

    pub fn has_signer(env: Env, signer: BytesN<32>) -> bool {
        bump_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::Signer(signer))
            .unwrap_or(false)
    }

    pub fn add_signer(env: Env, owner: Address, signer: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .instance()
            .set(&DataKey::Signer(signer), &true);
    }

    pub fn remove_signer(env: Env, owner: Address, signer: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage().instance().remove(&DataKey::Signer(signer));
    }

    pub fn set_peridottroller(env: Env, owner: Address, peridottroller: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .instance()
            .set(&DataKey::Peridottroller, &peridottroller);
    }

    pub fn set_margin_controller(env: Env, owner: Address, margin_controller: Address) {
        bump_ttl(&env);
        require_owner(&env, &owner);
        env.storage()
            .instance()
            .set(&DataKey::MarginController, &margin_controller);
    }

    pub fn bump_ttl(env: Env) {
        bump_ttl(&env);
    }

    pub fn upgrade_wasm(env: Env, owner: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_owner(&env, &owner);
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
        if let Context::Contract(contract_ctx) = ctx {
            enforce_contract_policy(env, &contract_ctx)?;
        }
    }
    Ok(())
}

fn enforce_contract_policy(env: &Env, ctx: &ContractContext) -> Result<(), Error> {
    let borrow_sym = Symbol::new(env, "borrow");
    if ctx.fn_name == borrow_sym {
        check_borrow_policy(env, ctx)?;
    }
    Ok(())
}

fn check_borrow_policy(env: &Env, ctx: &ContractContext) -> Result<(), Error> {
    let user: Address = ctx
        .args
        .get(0)
        .ok_or(Error::Unauthorized)?
        .try_into_val(env)
        .map_err(|_| Error::Unauthorized)?;
    if user != env.current_contract_address() {
        return Err(Error::Unauthorized);
    }
    let borrow_amount: u128 = ctx
        .args
        .get(1)
        .ok_or(Error::Unauthorized)?
        .try_into_val(env)
        .map_err(|_| Error::Unauthorized)?;

    let peridottroller: Address = env
        .storage()
        .instance()
        .get(&DataKey::Peridottroller)
        .ok_or(Error::NotInitialized)?;

    let underlying =
        ReceiptVaultClient::new(env, &ctx.contract).get_underlying_token();
    let (_liq, shortfall) = PeridottrollerClient::new(env, &peridottroller)
        .hypothetical_liquidity(&user, &ctx.contract, &borrow_amount, &underlying);
    if shortfall > 0 {
        return Err(Error::InsufficientHealth);
    }
    Ok(())
}

fn verify_signatures(
    env: &Env,
    signature_payload: &Hash<32>,
    signatures: &Vec<Signature>,
) -> Result<(), Error> {
    if signatures.len() == 0 {
        return Err(Error::Unauthorized);
    }
    for i in 0..signatures.len() {
        let sig = signatures.get(i).unwrap();
        let allowed: bool = env
            .storage()
            .instance()
            .get(&DataKey::Signer(sig.public_key.clone()))
            .unwrap_or(false);
        if !allowed {
            return Err(Error::Unauthorized);
        }
        let msg: Bytes = signature_payload.to_bytes().into_val(env);
        env.crypto()
            .ed25519_verify(&sig.public_key, &msg, &sig.signature);
    }
    Ok(())
}

fn require_owner(env: &Env, owner: &Address) {
    let stored: Address = env
        .storage()
        .instance()
        .get(&DataKey::Owner)
        .expect("owner not set");
    if stored != *owner {
        panic!("not owner");
    }
    bump_ttl(env);
    owner.require_auth();
}

fn bump_ttl(env: &Env) {
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_constructor_and_signers() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let signer = BytesN::from_array(&env, &[1u8; 32]);
        let peridottroller = Address::generate(&env);
        let margin = Address::generate(&env);

        let contract_id = env.register(BasicSmartAccount, ());
        let client = BasicSmartAccountClient::new(&env, &contract_id);
        client.initialize(&owner, &signer, &peridottroller, &margin);

        assert_eq!(client.get_owner(), owner);
        assert!(client.has_signer(&signer));
    }

    #[test]
    #[should_panic(expected = "not owner")]
    fn test_add_signer_requires_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let signer = BytesN::from_array(&env, &[1u8; 32]);
        let peridottroller = Address::generate(&env);
        let margin = Address::generate(&env);

        let contract_id = env.register(BasicSmartAccount, ());
        let client = BasicSmartAccountClient::new(&env, &contract_id);
        client.initialize(&owner, &signer, &peridottroller, &margin);

        let other = Address::generate(&env);
        let new_signer = BytesN::from_array(&env, &[2u8; 32]);
        client.add_signer(&other, &new_signer);
    }
}
