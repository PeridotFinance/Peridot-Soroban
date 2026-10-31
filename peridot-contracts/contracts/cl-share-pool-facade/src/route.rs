use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractclient, contractimpl, contracttype, token, vec, Address, Bytes, Env, IntoVal,
    Symbol, Vec,
};

#[contractclient(name = "ShareVaultClient")]
pub trait ShareVault {
    fn estimate_deposit(env: Env, amount: u128) -> u128;
    fn estimate_redeem(env: Env, shares: u128) -> u128;
    fn deposit_underlying(env: Env, from: Address, amount: i128, min_shares: i128) -> i128;
    fn redeem(env: Env, from: Address, shares: i128, min_underlying_out: i128) -> i128;
}

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admin: Address,
    pub controller: Address,
    pub vault: Address,
    pub underlying: Address,
    pub treasury: Address,
}

#[contracttype]
enum Key {
    Config,
    DepositPaused,
    FullPaused,
    InOperation,
}

#[contract]
pub struct ClSharePoolFacade;

#[contractimpl]
impl ClSharePoolFacade {
    pub fn __constructor(env: Env, config: Config) {
        // This prototype has no production recovery context, rewards custody
        // accounting, or delayed controller rotation. Fail closed on Mainnet.
        require_validation_network(&env);
        assert!(config.underlying != config.vault, "tokens must differ");
        env.storage().instance().set(&Key::Config, &config);
    }

    pub fn get_tokens(env: Env) -> Vec<Address> {
        let cfg = config(&env);
        vec![&env, cfg.underlying, cfg.vault]
    }

    pub fn set_pauses(env: Env, deposits: bool, full: bool) {
        let cfg = config(&env);
        cfg.admin.require_auth();
        assert!(!flag(&env, &Key::InOperation), "operation in progress");
        env.storage().instance().set(&Key::DepositPaused, &deposits);
        env.storage().instance().set(&Key::FullPaused, &full);
    }

    pub fn estimate_swap(env: Env, in_idx: u32, out_idx: u32, amount_in: u128) -> u128 {
        let deposit = direction(in_idx, out_idx);
        if amount_in == 0 || amount_in > i128::MAX as u128 || paused(&env, deposit) {
            return 0;
        }
        let cfg = config(&env);
        let vault = ShareVaultClient::new(&env, &cfg.vault);
        // The real share vault must enforce NAV freshness, required swap
        // availability, oracle valuation and zap costs in these estimates.
        let quote = if deposit {
            vault.estimate_deposit(&amount_in)
        } else {
            vault.estimate_redeem(&amount_in)
        };
        if quote > i128::MAX as u128 {
            0
        } else {
            quote
        }
    }

    pub fn swap(
        env: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128 {
        let cfg = config(&env);
        assert!(user == cfg.controller, "wrong controller");
        user.require_auth();
        let deposit = direction(in_idx, out_idx);
        assert!(!paused(&env, deposit), "paused direction");
        assert!(!flag(&env, &Key::InOperation), "operation in progress");
        let input = positive_i128(amount_in);
        let minimum = positive_i128(amount_out_min);
        assert!(
            Self::estimate_swap(env.clone(), in_idx, out_idx, amount_in) > 0,
            "quote unavailable"
        );
        let me = env.current_contract_address();
        let (token_in, token_out) = if deposit {
            (cfg.underlying.clone(), cfg.vault.clone())
        } else {
            (cfg.vault.clone(), cfg.underlying.clone())
        };
        let input_token = token::TokenClient::new(&env, &token_in);
        let output_token = token::TokenClient::new(&env, &token_out);
        let input_before = input_token.balance(&me);
        let output_before = output_token.balance(&me);
        env.storage().instance().set(&Key::InOperation, &true);

        // This exact transfer consumes V3's nested input authorization.
        input_token.transfer(&user, &me, &input);
        assert!(
            input_token.balance(&me) == input_before.checked_add(input).unwrap(),
            "input mismatch"
        );
        let vault = ShareVaultClient::new(&env, &cfg.vault);
        if deposit {
            authorize_deposit(&env, &cfg, &me, input, minimum);
            let _reported = vault.deposit_underlying(&me, &input, &minimum);
        } else {
            // Direct invocation authorizes the facade's own share burn. The
            // vault, not this contract, authorizes its payout token transfer.
            let _reported = vault.redeem(&me, &input, &minimum);
        }
        assert!(input_token.balance(&me) == input_before, "input residual");
        let produced = output_token
            .balance(&me)
            .checked_sub(output_before)
            .unwrap();
        assert!(produced >= minimum, "minimum not met");
        // Quotes are minimum guidance, never an output cap. Preserve donations.
        output_token.transfer(&me, &user, &produced);
        assert!(
            output_token.balance(&me) == output_before,
            "output residual"
        );
        env.storage().instance().set(&Key::InOperation, &false);
        produced as u128
    }

    /// Prototype donation sweep. The finished facade must additionally check
    /// any introduced recovery/reward liabilities before enabling this method.
    pub fn sweep_donation(env: Env, asset: Address) -> i128 {
        let cfg = config(&env);
        cfg.admin.require_auth();
        assert!(!flag(&env, &Key::InOperation), "operation in progress");
        // This principal-only prototype creates no between-call liabilities.
        // Restrict to its two known assets; do not sweep unknown reward tokens.
        assert!(
            asset == cfg.underlying || asset == cfg.vault,
            "unknown asset"
        );
        let token = token::TokenClient::new(&env, &asset);
        let me = env.current_contract_address();
        let amount = token.balance(&me);
        if amount > 0 {
            token.transfer(&me, &cfg.treasury, &amount);
        }
        amount
    }
}

fn config(env: &Env) -> Config {
    require_validation_network(env);
    env.storage()
        .instance()
        .get(&Key::Config)
        .expect("not initialized")
}

fn require_validation_network(env: &Env) {
    let mainnet = env.crypto().sha256(&Bytes::from_slice(
        env,
        b"Public Global Stellar Network ; September 2015",
    ));
    assert!(
        env.ledger().network_id() != mainnet.to_bytes(),
        "validation only"
    );
}

fn flag(env: &Env, key: &Key) -> bool {
    env.storage().instance().get(key).unwrap_or(false)
}

fn paused(env: &Env, deposit: bool) -> bool {
    flag(env, &Key::FullPaused) || (deposit && flag(env, &Key::DepositPaused))
}

fn direction(input: u32, output: u32) -> bool {
    assert!(
        (input == 0 && output == 1) || (input == 1 && output == 0),
        "bad indices"
    );
    input == 0
}

fn positive_i128(amount: u128) -> i128 {
    assert!(amount > 0, "zero amount");
    amount.try_into().expect("amount too large")
}

fn authorize_deposit(env: &Env, cfg: &Config, me: &Address, amount: i128, minimum: i128) {
    let transfer = SubContractInvocation {
        context: ContractContext {
            contract: cfg.underlying.clone(),
            fn_name: Symbol::new(env, "transfer"),
            args: (me.clone(), cfg.vault.clone(), amount).into_val(env),
        },
        sub_invocations: Vec::new(env),
    };
    env.authorize_as_current_contract(vec![
        env,
        InvokerContractAuthEntry::Contract(transfer.clone()),
        InvokerContractAuthEntry::Contract(SubContractInvocation {
            context: ContractContext {
                contract: cfg.vault.clone(),
                fn_name: Symbol::new(env, "deposit_underlying"),
                args: (me.clone(), amount, minimum).into_val(env),
            },
            sub_invocations: vec![env, InvokerContractAuthEntry::Contract(transfer)],
        }),
    ]);
}
