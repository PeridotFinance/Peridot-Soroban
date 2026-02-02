#![no_std]

use soroban_sdk::{contract, contractevent, contractimpl, contracttype, Address, BytesN, Env};

#[soroban_sdk::contractclient(name = "BasicSmartAccountClient")]
pub trait BasicSmartAccount {
    fn initialize(
        env: Env,
        owner: Address,
        signer: BytesN<32>,
        peridottroller: Address,
        margin_controller: Address,
    );
}

#[contract]
pub struct SmartAccountFactory;

#[contracttype]
pub enum AccountType {
    Basic,
}

#[contracttype]
pub struct AccountConfig {
    pub account_type: AccountType,
    pub owner: Address,
    pub signer: BytesN<32>,
    pub peridottroller: Address,
    pub margin_controller: Address,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    WasmHash(AccountType),
    UserAccount(Address),
    AccountCount,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountCreated {
    pub owner: Address,
    pub account: Address,
}

const TTL_THRESHOLD: u32 = 100_000_000;
const TTL_EXTEND_TO: u32 = 200_000_000;

#[contractimpl]
impl SmartAccountFactory {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Initialized, &true);
        bump_ttl(&env);
    }

    pub fn set_wasm_hash(env: Env, admin: Address, account_type: AccountType, hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::WasmHash(account_type), &hash);
    }

    pub fn create_account(env: Env, config: AccountConfig, salt: BytesN<32>) -> Address {
        bump_ttl(&env);
        config.owner.require_auth();
        if env
            .storage()
            .instance()
            .has(&DataKey::UserAccount(config.owner.clone()))
        {
            panic!("account exists");
        }
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::WasmHash(config.account_type))
            .expect("wasm hash not set");

        let deployed_address = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, ());

        match config.account_type {
            AccountType::Basic => {
                BasicSmartAccountClient::new(&env, &deployed_address).initialize(
                    &config.owner,
                    &config.signer,
                    &config.peridottroller,
                    &config.margin_controller,
                );
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::UserAccount(config.owner.clone()), &deployed_address);

        let mut count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AccountCount)
            .unwrap_or(0u64);
        count = count.saturating_add(1);
        env.storage().instance().set(&DataKey::AccountCount, &count);
        AccountCreated {
            owner: config.owner,
            account: deployed_address.clone(),
        }
        .publish(&env);

        deployed_address
    }

    pub fn get_account(env: Env, user: Address) -> Option<Address> {
        bump_ttl(&env);
        env.storage().instance().get(&DataKey::UserAccount(user))
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

fn require_admin(env: &Env, admin: &Address) {
    let stored: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .expect("admin not set");
    if stored != *admin {
        panic!("not admin");
    }
    bump_ttl(env);
    admin.require_auth();
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
    fn test_initialize_and_set_wasm_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let contract_id = env.register(SmartAccountFactory, ());
        let client = SmartAccountFactoryClient::new(&env, &contract_id);
        client.initialize(&admin);

        let fake_hash = BytesN::from_array(&env, &[1u8; 32]);
        client.set_wasm_hash(&admin, &AccountType::Basic, &fake_hash);
    }

    #[test]
    #[should_panic(expected = "not admin")]
    fn test_set_wasm_hash_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let non_admin = Address::generate(&env);

        let contract_id = env.register(SmartAccountFactory, ());
        let client = SmartAccountFactoryClient::new(&env, &contract_id);
        client.initialize(&admin);

        let fake_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.set_wasm_hash(&non_admin, &AccountType::Basic, &fake_hash);
    }
}
