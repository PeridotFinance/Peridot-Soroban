use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::IntoVal;

fn register_account<'a>(env: &'a Env, factory: &Address) -> (Address, BasicSmartAccountClient<'a>) {
    let contract_id = env.register(BasicSmartAccount, (factory.clone(),));
    let client = BasicSmartAccountClient::new(env, &contract_id);
    (contract_id, client)
}

fn expected_factory(env: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(env, DEFAULT_FACTORY_ADDRESS))
}

#[test]
fn test_constructor_and_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);

    let (_contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);

    assert_eq!(client.get_owner(), owner);
    assert!(client.has_signer(&signer));
}

#[test]
#[should_panic(expected = "not owner")]
fn test_add_signer_requires_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);

    let (_contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);

    let other = Address::generate(&env);
    let new_signer = BytesN::from_array(&env, &[2u8; 32]);
    client.add_signer(&other, &new_signer);
}

#[test]
#[should_panic(expected = "too many signers")]
fn test_add_signer_respects_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);

    let (_contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);

    for i in 2..=9u8 {
        client.add_signer(&owner, &BytesN::from_array(&env, &[i; 32]));
    }
}

#[test]
fn test_verify_signatures_rejects_duplicate_public_keys() {
    let env = Env::default();
    let payload = env.crypto().sha256(&Bytes::from_array(&env, &[7u8; 32]));
    let public_key = BytesN::from_array(&env, &[3u8; 32]);
    let signature = BytesN::from_array(&env, &[4u8; 64]);
    let mut signatures = Vec::new(&env);
    signatures.push_back(Signature {
        public_key: public_key.clone(),
        signature: signature.clone(),
    });
    signatures.push_back(Signature {
        public_key,
        signature,
    });

    assert_eq!(
        verify_signatures(&env, &payload, &signatures),
        Err(Error::Unauthorized)
    );
}

#[test]
fn test_verify_signatures_rejects_too_many_signatures() {
    let env = Env::default();
    let payload = env.crypto().sha256(&Bytes::from_array(&env, &[9u8; 32]));
    let mut signatures = Vec::new(&env);
    for i in 0..(MAX_SIGNERS + 1) {
        signatures.push_back(Signature {
            public_key: BytesN::from_array(&env, &[i as u8; 32]),
            signature: BytesN::from_array(&env, &[i as u8; 64]),
        });
    }

    assert_eq!(
        verify_signatures(&env, &payload, &signatures),
        Err(Error::Unauthorized)
    );
}

#[test]
fn test_vault_deposit_policy_accepts_self() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let allowed_vault = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);
    client.add_allowed_contract(&owner, &allowed_vault);
    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: allowed_vault,
            fn_name: Symbol::new(&env, "deposit"),
            args: (contract_id.clone(), 123u128).into_val(&env),
        };

        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Ok(()));
    });
}

#[test]
fn test_margin_open_policy_rejects_other_user() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);
    let other = Address::generate(&env);
    let swaps_chain = Vec::<(Vec<Address>, BytesN<32>, Address)>::new(&env);
    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: margin,
            fn_name: Symbol::new(&env, "open_position"),
            args: (
                other,
                Address::generate(&env),
                Address::generate(&env),
                100u128,
                2u128,
                Symbol::new(&env, "Long"),
                swaps_chain,
                90u128,
            )
                .into_val(&env),
        };

        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Err(Error::Unauthorized));
    });
}

#[test]
fn test_margin_open_v2_policy_rejects_other_user() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);
    let other = Address::generate(&env);
    let swaps_chain = Vec::<(Vec<Address>, BytesN<32>, Address)>::new(&env);
    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: margin,
            fn_name: Symbol::new(&env, "open_position_v2"),
            args: (
                other,
                Address::generate(&env),
                Address::generate(&env),
                100u128,
                2u128,
                Symbol::new(&env, "Long"),
                swaps_chain,
                90u128,
            )
                .into_val(&env),
        };

        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Err(Error::Unauthorized));
    });
}

#[test]
fn test_margin_transfer_policy_accepts_self() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);
    let asset = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let spot_to_margin = ContractContext {
            contract: margin.clone(),
            fn_name: Symbol::new(&env, "transfer_spot_to_margin"),
            args: (contract_id.clone(), asset.clone(), 10u128).into_val(&env),
        };
        assert_eq!(enforce_contract_policy(&env, &spot_to_margin), Ok(()));

        let margin_to_spot = ContractContext {
            contract: margin.clone(),
            fn_name: Symbol::new(&env, "transfer_margin_to_spot"),
            args: (contract_id.clone(), asset.clone(), 5u128).into_val(&env),
        };
        assert_eq!(enforce_contract_policy(&env, &margin_to_spot), Ok(()));
    });
}

#[test]
fn test_transfer_from_policy_is_rejected_for_allowed_vaults() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner_account = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let allowed_vault = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner_account, &signer, &peridottroller, &margin);
    client.add_allowed_contract(&owner_account, &allowed_vault);
    let owner = Address::generate(&env);
    let to = Address::generate(&env);
    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: allowed_vault,
            fn_name: Symbol::new(&env, "transfer_from"),
            args: (contract_id.clone(), owner, to, 50u128).into_val(&env),
        };

        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Err(Error::Unauthorized));
    });
}

#[test]
fn test_sensitive_call_on_unlisted_vault_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner_account = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let unlisted_vault = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner_account, &signer, &peridottroller, &margin);

    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: unlisted_vault,
            fn_name: Symbol::new(&env, "borrow"),
            args: (contract_id.clone(), 1u128).into_val(&env),
        };
        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Err(Error::Unauthorized));
    });
}

#[test]
fn test_non_contract_auth_context_is_rejected() {
    let env = Env::default();
    let wasm_hash = BytesN::from_array(&env, &[7u8; 32]);
    let mut contexts = Vec::new(&env);
    contexts.push_back(Context::CreateContractHostFn(
        soroban_sdk::auth::CreateContractHostFnContext {
            executable: soroban_sdk::auth::ContractExecutable::Wasm(wasm_hash),
            salt: BytesN::from_array(&env, &[9u8; 32]),
        },
    ));
    let res = enforce_policies(&env, &contexts);
    assert_eq!(res, Err(Error::Unauthorized));
}

#[test]
fn test_token_transfer_context_requires_protocol_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = expected_factory(&env);
    let owner = Address::generate(&env);
    let signer = BytesN::from_array(&env, &[1u8; 32]);
    let peridottroller = Address::generate(&env);
    let margin = Address::generate(&env);
    let token_contract = Address::generate(&env);
    let allowed_vault = Address::generate(&env);
    let (contract_id, client) = register_account(&env, &factory);
    client.initialize(&owner, &signer, &peridottroller, &margin);
    client.add_allowed_contract(&owner, &allowed_vault);

    env.as_contract(&contract_id, || {
        let ctx = ContractContext {
            contract: token_contract,
            fn_name: Symbol::new(&env, "transfer"),
            args: (contract_id.clone(), allowed_vault, 10i128).into_val(&env),
        };
        let res = enforce_contract_policy(&env, &ctx);
        assert_eq!(res, Ok(()));
    });
}
