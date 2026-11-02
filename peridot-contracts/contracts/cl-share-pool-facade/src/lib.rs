#![no_std]
use soroban_sdk::{
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, contracttype, token, Address, Env, IntoVal, Symbol, Vec,
};
#[contracttype]
#[derive(Clone)]
struct Config {
    admin: Address,
    controller: Address,
    underlying: Address,
    share: Address,
    treasury: Address,
    deposits_paused: bool,
    paused: bool,
}
#[contracttype]
#[derive(Clone)]
enum Key {
    Config,
    Busy,
    NextController,
    ReadyAt,
}
#[soroban_sdk::contractclient(name = "ShareClient")]
trait Share {
    fn deposit_underlying(env: Env, from: Address, amount: i128, min_shares: i128) -> i128;
    fn redeem(env: Env, from: Address, shares: i128, min_underlying_out: i128) -> i128;
    fn quote(env: Env, deposit: bool, amount: u128) -> u128;
    fn get_underlying(env: Env) -> Address;
}
fn cfg(e: &Env) -> Config {
    e.storage().instance().extend_ttl(500_000, 1_000_000);
    e.storage().instance().get(&Key::Config).unwrap()
}
fn amount(n: u128) -> i128 {
    n.try_into().expect("amount too large")
}
#[contract]
pub struct ClSharePoolFacade;
#[contractimpl]
impl ClSharePoolFacade {
    pub fn initialize(
        e: Env,
        admin: Address,
        controller: Address,
        underlying: Address,
        share: Address,
        treasury: Address,
    ) {
        let public = e.crypto().sha256(&soroban_sdk::Bytes::from_slice(
            &e,
            b"Public Global Stellar Network ; September 2015",
        ));
        assert_ne!(
            e.ledger().network_id(),
            public.to_bytes(),
            "CL validation: Mainnet disabled"
        );
        assert!(!e.storage().instance().has(&Key::Config));
        admin.require_auth();
        assert_ne!(underlying, share);
        assert_eq!(ShareClient::new(&e, &share).get_underlying(), underlying);
        e.storage().instance().set(
            &Key::Config,
            &Config {
                admin,
                controller,
                underlying,
                share,
                treasury,
                deposits_paused: false,
                paused: false,
            },
        );
    }
    pub fn get_tokens(e: Env) -> Vec<Address> {
        let c = cfg(&e);
        soroban_sdk::vec![&e, c.underlying, c.share]
    }
    pub fn estimate_swap(e: Env, in_idx: u32, out_idx: u32, amount_in: u128) -> u128 {
        let c = cfg(&e);
        if in_idx > 1 || out_idx != 1 - in_idx || c.paused || (in_idx == 0 && c.deposits_paused) {
            return 0;
        }
        ShareClient::new(&e, &c.share)
            .try_quote(&(in_idx == 0), &amount_in)
            .ok()
            .and_then(Result::ok)
            .unwrap_or(0)
    }
    pub fn swap(
        e: Env,
        user: Address,
        in_idx: u32,
        out_idx: u32,
        amount_in: u128,
        amount_out_min: u128,
    ) -> u128 {
        let c = cfg(&e);
        assert_eq!(user, c.controller, "wrong controller");
        user.require_auth();
        assert!(in_idx < 2 && out_idx == 1 - in_idx && amount_in > 0 && amount_out_min > 0);
        assert!(Self::estimate_swap(e.clone(),in_idx,out_idx,amount_in)>0,"quote unavailable");
        assert!(
            !c.paused && !(in_idx == 0 && c.deposits_paused),
            "direction paused"
        );
        assert!(
            !e.storage().instance().get(&Key::Busy).unwrap_or(false),
            "busy"
        );
        e.storage().instance().set(&Key::Busy, &true);
        let me = e.current_contract_address();
        let (input, output) = if in_idx == 0 {
            (c.underlying.clone(), c.share.clone())
        } else {
            (c.share.clone(), c.underlying.clone())
        };
        let tin = token::Client::new(&e, &input);
        let tout = token::Client::new(&e, &output);
        let before_in = tin.balance(&me);
        let before_out = tout.balance(&me);
        // First state-changing external call consumes V3's exact nested transfer.
        tin.transfer(&user, &me, &amount(amount_in));
        assert_eq!(tin.balance(&me),before_in.checked_add(amount(amount_in)).unwrap(),"input mismatch");
        let vault = ShareClient::new(&e, &c.share);
        if in_idx == 0 {
            let transfer = InvokerContractAuthEntry::Contract(SubContractInvocation {
                context: ContractContext {
                    contract: input.clone(),
                    fn_name: Symbol::new(&e, "transfer"),
                    args: (me.clone(), c.share.clone(), amount(amount_in)).into_val(&e),
                },
                sub_invocations: Vec::new(&e),
            });
            // Vault require_auth is immediate-invoker auth; the deeper token
            // transfer requires its own authorization from the facade.
            e.authorize_as_current_contract(soroban_sdk::vec![&e, transfer.clone(),
                InvokerContractAuthEntry::Contract(SubContractInvocation {
                    context: ContractContext {contract:c.share.clone(),fn_name:Symbol::new(&e,"deposit_underlying"),args:(me.clone(),amount(amount_in),amount(amount_out_min)).into_val(&e)},
                    sub_invocations:soroban_sdk::vec![&e,transfer]
                })
            ]);
            vault.deposit_underlying(&me, &amount(amount_in), &amount(amount_out_min));
        } else {
            vault.redeem(&me, &amount(amount_in), &amount(amount_out_min));
        }
        let delta = tout.balance(&me).checked_sub(before_out).unwrap();
        assert!(delta >= amount(amount_out_min), "output below minimum");
        tout.transfer(&me, &user, &delta);
        assert_eq!(tin.balance(&me), before_in, "input residual");
        assert_eq!(tout.balance(&me), before_out, "output residual");
        e.storage().instance().set(&Key::Busy, &false);
        delta.try_into().unwrap()
    }
    pub fn set_paused(e: Env, deposits: bool, all: bool) {
        let mut c = cfg(&e);
        c.admin.require_auth();
        c.deposits_paused = deposits;
        c.paused = all;
        e.storage().instance().set(&Key::Config, &c);
    }
    pub fn propose_controller(e: Env, next: Address) {
        cfg(&e).admin.require_auth();
        e.storage().instance().set(&Key::NextController, &next);
        e.storage().instance().set(
            &Key::ReadyAt,
            &e.ledger().timestamp().checked_add(86400).unwrap(),
        );
    }
    pub fn accept_controller(e: Env) {
        let mut c = cfg(&e);
        c.admin.require_auth();
        let ready: u64 = e
            .storage()
            .instance()
            .get(&Key::ReadyAt)
            .expect("no proposal");
        assert!(e.ledger().timestamp() >= ready);
        c.controller = e.storage().instance().get(&Key::NextController).unwrap();
        e.storage().instance().set(&Key::Config, &c);
        e.storage().instance().remove(&Key::ReadyAt);
    }
    pub fn sweep(e: Env, asset: Address) {
        let c = cfg(&e);
        c.admin.require_auth();
        assert!(!e.storage().instance().get(&Key::Busy).unwrap_or(false));
        // Only dust in the two supported assets. AQUA entitlement is never swept.
        assert!(asset == c.underlying || asset == c.share);
        let t = token::Client::new(&e, &asset);
        let me = e.current_contract_address();
        let n = t.balance(&me);
        if n > 0 {
            t.transfer(&me, &c.treasury, &n);
        }
    }
}
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod test;

#[cfg(test)]
mod pr74_auth_tests;
