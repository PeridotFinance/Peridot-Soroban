#![no_std]
use soroban_sdk::auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation};
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, IntoVal, Symbol, Vec};

#[contracttype]
enum DataKey {
    Token,
    Initialized,
}

#[contract]
pub struct MockLendingVault;

#[contractimpl]
impl MockLendingVault {
    pub fn initialize(env: Env, token: Address) {
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Initialized)
            .is_some()
        {
            panic!("already initialized");
        }
        env.storage().persistent().set(&DataKey::Token, &token);
        env.storage().persistent().set(&DataKey::Initialized, &true);
    }

    pub fn deposit(env: Env, user: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        let token = get_token(&env);
        let vault = env.current_contract_address();
        token::Client::new(&env, &token).transfer(&user, &vault, &amount);
    }

    pub fn withdraw_to(env: Env, user: Address, to: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 {
            panic!("bad amount");
        }
        let token = get_token(&env);
        let vault = env.current_contract_address();
        authorize_transfer_from_self(&env, &token, &vault, &to, amount);
        token::Client::new(&env, &token).transfer(&vault, &to, &amount);
    }
}

fn get_token(env: &Env) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey::Token)
        .expect("token not set")
}

fn authorize_transfer_from_self(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
    let args = (from.clone(), to.clone(), amount).into_val(env);
    let ctx = ContractContext {
        contract: token.clone(),
        fn_name: Symbol::new(env, "transfer"),
        args,
    };
    let mut auths = Vec::new(env);
    auths.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ctx,
        sub_invocations: Vec::new(env),
    }));
    env.authorize_as_current_contract(auths);
}
