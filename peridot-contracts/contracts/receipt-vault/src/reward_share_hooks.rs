//! Native-only ownership hooks for supply-neutral lending share movements.
//! No production entrypoint calls these. The caller MUST first checkpoint every
//! pool emission with the complete coordinator, then call the authenticated
//! lending operation between begin/finish in ONE atomic invocation. This helper
//! cannot discover claims and confers no authorization. Never wrap a public
//! caller-selected arbitrary mutation with it.

use crate::{reward_backing as backing, reward_ledger as ledger};
use soroban_sdk::{Address, Env, Vec};
use stellar_tokens::fungible::Base as TokenBase;

/// Opaque, non-serializable and non-cloneable same-invocation checkpoint.
pub struct ShareMove {
    receipt: Address,
    owners: Vec<Address>,
    before: Vec<u128>,
    assets: Vec<Address>,
    streams: Vec<ledger::Stream>,
    supply: i128,
    backing: backing::BackingState,
}

fn weight(env: &Env, owner: &Address) -> u128 {
    TokenBase::balance(env, owner)
        .try_into()
        .expect("negative shares")
}

pub fn begin(env: &Env, affected: Vec<Address>) -> ShareMove {
    assert!(
        !affected.is_empty() && affected.len() <= 3,
        "invalid affected owners"
    );
    let receipt = env.current_contract_address();
    let mut owners = Vec::new(env);
    let mut before = Vec::new(env);
    for owner in affected.iter() {
        assert_ne!(owner, receipt, "escrow is not movable collateral");
        if !owners.contains(owner.clone()) {
            before.push_back(weight(env, &owner));
            owners.push_back(owner);
        }
    }
    let supply = TokenBase::total_supply(env);
    let backing = backing::state(env);
    assert_eq!(weight(env, &receipt), backing.ptokens, "escrow mismatch");
    let ordinary = u128::try_from(supply)
        .expect("negative supply")
        .checked_sub(backing.ptokens)
        .expect("escrow exceeds supply");
    let assets = ledger::assets(env);
    assert!(
        !assets.is_empty() && assets.len() <= 4,
        "invalid reward registry"
    );
    let mut streams = Vec::new(env);
    for asset in assets.iter() {
        let stream = ledger::stream(env, &asset);
        assert_eq!(stream.total_weight, ordinary, "missed share checkpoint");
        for (owner, old) in owners.iter().zip(before.iter()) {
            ledger::checkpoint(env, &asset, &owner, old);
        }
        streams.push_back(stream);
    }
    ShareMove {
        receipt,
        owners,
        before,
        assets,
        streams,
        supply,
        backing,
    }
}

pub fn finish(env: &Env, snapshot: ShareMove) {
    assert_eq!(snapshot.receipt, env.current_contract_address());
    assert_eq!(
        snapshot.supply,
        TokenBase::total_supply(env),
        "share move changed supply"
    );
    assert_eq!(
        snapshot.backing,
        backing::state(env),
        "share move changed backing"
    );
    assert_eq!(
        weight(env, &snapshot.receipt),
        snapshot.backing.ptokens,
        "escrow changed"
    );
    assert_eq!(
        snapshot.assets,
        ledger::assets(env),
        "reward registry changed"
    );
    let mut after = Vec::new(env);
    let mut old_sum = 0u128;
    let mut new_sum = 0u128;
    for (owner, old) in snapshot.owners.iter().zip(snapshot.before.iter()) {
        let new = weight(env, &owner);
        old_sum = old_sum.checked_add(old).expect("share sum overflow");
        new_sum = new_sum.checked_add(new).expect("share sum overflow");
        after.push_back(new);
    }
    assert_eq!(old_sum, new_sum, "unaccounted share recipient");
    for (asset, stream) in snapshot.assets.iter().zip(snapshot.streams.iter()) {
        assert_eq!(ledger::stream(env, &asset), stream, "reward stream changed");
        // Debit before credit so intermediate aggregate weights remain valid.
        // Duplicate borrower/liquidator/fee recipients were coalesced above.
        for debit in [true, false] {
            for i in 0..snapshot.owners.len() {
                let old = snapshot.before.get(i).unwrap();
                let new = after.get(i).unwrap();
                if old != new && (new < old) == debit {
                    ledger::change_weight(env, &asset, &snapshot.owners.get(i).unwrap(), old, new);
                }
            }
        }
        assert_eq!(
            ledger::stream(env, &asset).total_weight,
            stream.total_weight
        );
    }
}
