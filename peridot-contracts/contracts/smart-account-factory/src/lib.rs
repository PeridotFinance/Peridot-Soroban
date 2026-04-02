#![no_std]

use soroban_sdk::{
    contract, contractevent, contractimpl, contracttype, vec, Address, Bytes, BytesN, Env,
    IntoVal, String, Symbol,
};

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

/// Factory for deploying smart accounts.
///
/// # Example (doctest, no_run)
/// ```no_run
/// use soroban_sdk::{Env, Address, BytesN};
/// use soroban_sdk::testutils::Address as _;
/// use smart_account_factory::{SmartAccountFactory, SmartAccountFactoryClient, AccountType};
///
/// let env = Env::default();
/// env.mock_all_auths();
/// let admin = Address::generate(&env);
/// let contract_id = env.register(SmartAccountFactory, ());
/// let client = SmartAccountFactoryClient::new(&env, &contract_id);
/// client.initialize(&admin);
///
/// let wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
/// client.approve_hash(&admin, &wasm_hash);
/// client.set_wasm_hash(&admin, &AccountType::Basic, &wasm_hash);
/// ```
#[contract]
pub struct SmartAccountFactory;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
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
    ApprovedHash(BytesN<32>),
    PendingWasmHash(AccountType),
    PendingWasmEta(AccountType),
    PendingUpgradeHash,
    PendingUpgradeEta,
    UserAccount(Address),
    AccountCount,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountCreated {
    pub owner: Address,
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildWasmHashProposed {
    pub account_type: AccountType,
    pub hash: BytesN<32>,
    pub execute_after: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildWasmHashApplied {
    pub account_type: AccountType,
    pub hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FactoryUpgradeProposed {
    pub hash: BytesN<32>,
    pub execute_after: u64,
}

pub const DEFAULT_INIT_ADMIN: &str = "GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ";

const TTL_THRESHOLD: u32 = 500_000;
const TTL_EXTEND_TO: u32 = 1_000_000;
const HASH_CHANGE_DELAY_SECS: u64 = 24 * 60 * 60;

#[contractimpl]
impl SmartAccountFactory {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        let expected_admin_str =
            option_env!("SMART_ACCOUNT_FACTORY_INIT_ADMIN").unwrap_or(DEFAULT_INIT_ADMIN);
        let expected_admin = Address::from_string(&String::from_str(&env, expected_admin_str));
        if admin != expected_admin {
            panic!("unexpected admin");
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Initialized, &true);
        bump_ttl(&env);
    }

    pub fn approve_hash(env: Env, admin: Address, hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ApprovedHash(hash), &true);
    }

    pub fn revoke_hash(env: Env, admin: Address, hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        env.storage().instance().remove(&DataKey::ApprovedHash(hash));
    }

    pub fn set_wasm_hash(env: Env, admin: Address, account_type: AccountType, hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        require_approved_hash(&env, &hash);
        let execute_after = env.ledger().timestamp().saturating_add(HASH_CHANGE_DELAY_SECS);
        env.storage()
            .instance()
            .set(&DataKey::PendingWasmHash(account_type.clone()), &hash);
        env.storage()
            .instance()
            .set(&DataKey::PendingWasmEta(account_type.clone()), &execute_after);
        ChildWasmHashProposed {
            account_type,
            hash,
            execute_after,
        }
        .publish(&env);
    }

    pub fn apply_wasm_hash(env: Env, admin: Address, account_type: AccountType) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        let hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingWasmHash(account_type.clone()))
            .expect("pending hash not set");
        let execute_after: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PendingWasmEta(account_type.clone()))
            .expect("pending eta not set");
        if env.ledger().timestamp() < execute_after {
            panic!("hash timelocked");
        }
        require_approved_hash(&env, &hash);
        env.storage()
            .instance()
            .set(&DataKey::WasmHash(account_type.clone()), &hash);
        env.storage()
            .instance()
            .remove(&DataKey::PendingWasmHash(account_type.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::PendingWasmEta(account_type.clone()));
        ChildWasmHashApplied { account_type, hash }.publish(&env);
    }

    pub fn create_account(env: Env, config: AccountConfig, salt: BytesN<32>) -> Address {
        bump_ttl(&env);
        config.owner.require_auth();
        if env
            .storage()
            .persistent()
            .has(&DataKey::UserAccount(config.owner.clone()))
        {
            panic!("account exists");
        }
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::WasmHash(config.account_type))
            .expect("wasm hash not set");

        // Derive a unique, owner-scoped salt to prevent address squatting.
        let owner_str = config.owner.to_string();
        let owner_bytes: Bytes = owner_str.to_bytes();
        let derived_hash = env.crypto().sha256(&owner_bytes);
        let derived_salt = BytesN::from_array(&env, &derived_hash.to_array());
        if salt != derived_salt {
            panic!("bad salt");
        }
        let deployed_address = env
            .deployer()
            .with_current_contract(derived_salt)
            .deploy_v2(wasm_hash, (env.current_contract_address(),));

        match config.account_type {
            AccountType::Basic => {
                let auths = vec![&env, soroban_sdk::auth::InvokerContractAuthEntry::Contract(
                    soroban_sdk::auth::SubContractInvocation {
                        context: soroban_sdk::auth::ContractContext {
                            contract: deployed_address.clone(),
                            fn_name: Symbol::new(&env, "initialize"),
                            args: (
                                config.owner.clone(),
                                config.signer.clone(),
                                config.peridottroller.clone(),
                                config.margin_controller.clone(),
                            )
                                .into_val(&env),
                        },
                        sub_invocations: vec![&env],
                    },
                )];
                env.authorize_as_current_contract(auths);
                BasicSmartAccountClient::new(&env, &deployed_address).initialize(
                    &config.owner,
                    &config.signer,
                    &config.peridottroller,
                    &config.margin_controller,
                );
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::UserAccount(config.owner.clone()), &deployed_address);
        bump_user_account_ttl(&env, &config.owner);

        let mut count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::AccountCount)
            .unwrap_or(0u64);
        count = count.saturating_add(1);
        env.storage().persistent().set(&DataKey::AccountCount, &count);
        bump_account_count_ttl(&env);
        AccountCreated {
            owner: config.owner,
            account: deployed_address.clone(),
        }
        .publish(&env);

        deployed_address
    }

    pub fn get_account(env: Env, user: Address) -> Option<Address> {
        bump_ttl(&env);
        bump_user_account_ttl(&env, &user);
        env.storage().persistent().get(&DataKey::UserAccount(user))
    }

    pub fn propose_upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        require_approved_hash(&env, &new_wasm_hash);
        let execute_after = env.ledger().timestamp().saturating_add(HASH_CHANGE_DELAY_SECS);
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeEta, &execute_after);
        FactoryUpgradeProposed {
            hash: new_wasm_hash,
            execute_after,
        }
        .publish(&env);
    }

    pub fn upgrade_wasm(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        bump_ttl(&env);
        require_admin(&env, &admin);
        let pending_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .expect("pending upgrade not set");
        let execute_after: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeEta)
            .expect("pending upgrade eta not set");
        if pending_hash != new_wasm_hash {
            panic!("upgrade hash mismatch");
        }
        if env.ledger().timestamp() < execute_after {
            panic!("upgrade timelocked");
        }
        require_approved_hash(&env, &new_wasm_hash);
        env.storage().instance().remove(&DataKey::PendingUpgradeHash);
        env.storage().instance().remove(&DataKey::PendingUpgradeEta);
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

fn require_approved_hash(env: &Env, hash: &BytesN<32>) {
    let approved: bool = env
        .storage()
        .instance()
        .get(&DataKey::ApprovedHash(hash.clone()))
        .unwrap_or(false);
    if !approved {
        panic!("hash not approved");
    }
}

fn bump_ttl(env: &Env) {
    if env.storage().instance().has(&DataKey::Initialized) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_user_account_ttl(env: &Env, user: &Address) {
    let persistent = env.storage().persistent();
    let key = DataKey::UserAccount(user.clone());
    if persistent.has(&key) {
        persistent.extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

fn bump_account_count_ttl(env: &Env) {
    let persistent = env.storage().persistent();
    if persistent.has(&DataKey::AccountCount) {
        persistent.extend_ttl(&DataKey::AccountCount, TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    #[test]
    fn test_initialize_and_set_wasm_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let admin =
            Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));

        let contract_id = env.register(SmartAccountFactory, ());
        let client = SmartAccountFactoryClient::new(&env, &contract_id);
        client.initialize(&admin);

        let fake_hash = BytesN::from_array(&env, &[1u8; 32]);
        client.approve_hash(&admin, &fake_hash);
        client.set_wasm_hash(&admin, &AccountType::Basic, &fake_hash);
        env.ledger().with_mut(|l| l.timestamp += HASH_CHANGE_DELAY_SECS + 1);
        client.apply_wasm_hash(&admin, &AccountType::Basic);
    }

    #[test]
    #[should_panic(expected = "not admin")]
    fn test_set_wasm_hash_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin =
            Address::from_string(&String::from_str(&env, DEFAULT_INIT_ADMIN));
        let non_admin = Address::generate(&env);

        let contract_id = env.register(SmartAccountFactory, ());
        let client = SmartAccountFactoryClient::new(&env, &contract_id);
        client.initialize(&admin);

        let fake_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.approve_hash(&admin, &fake_hash);
        client.set_wasm_hash(&non_admin, &AccountType::Basic, &fake_hash);
    }
}
