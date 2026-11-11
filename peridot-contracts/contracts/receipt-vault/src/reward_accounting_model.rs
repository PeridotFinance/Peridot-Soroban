//! Experimental single-reward accounting model, deliberately absent from WASM.
//!
//! `units` mean separately backed compounded-reward claim units, NOT ordinary
//! pTokens and NOT an extra claim on assets already included in ordinary NAV.
//! A production escrow/share-pricing adapter does not exist yet. This model takes
//! its measured output as input and proves only allocation/conservation, not the
//! adapter's valuation, reinvestment, authorization, storage or execution budget.
//!
//! Every balance boundary first recognizes all pending pool rewards. Closed-epoch
//! indices let an inactive account catch up using one historical epoch record,
//! without visiting holders during compounding or replaying every missed epoch.

extern crate std;

use std::collections::BTreeMap;

const SCALE: u128 = 1_000_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Error {
    Arithmetic,
    Insufficient,
    NoHolders,
    ZeroAmount,
    MissingEpoch,
}

type Result<T> = core::result::Result<T, Error>;

fn add(a: u128, b: u128) -> Result<u128> {
    a.checked_add(b).ok_or(Error::Arithmetic)
}

fn sub(a: u128, b: u128) -> Result<u128> {
    a.checked_sub(b).ok_or(Error::Insufficient)
}

fn mul(a: u128, b: u128) -> Result<u128> {
    a.checked_mul(b).ok_or(Error::Arithmetic)
}

fn ratio(a: u128, b: u128, denominator: u128) -> Result<u128> {
    mul(a, b)?.checked_div(denominator).ok_or(Error::Arithmetic)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Account {
    weight: u128,
    epoch: u64,
    raw_index: u128,
    // Fractions are retained at checkpoints; do not round each accrual to tokens.
    raw_scaled: u128,
    units_scaled: u128,
    reserved_raw: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClosedEpoch {
    raw_index: u128,
    units_per_raw: u128,
    cumulative_units_index: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Model {
    accounts: BTreeMap<u32, Account>,
    closed: BTreeMap<u64, ClosedEpoch>,
    epoch: u64,
    total_weight: u128,
    raw_index: u128,
    cumulative_units_index: u128,
    // Simulated pool accrual, not yet recognized by the allocation ledger.
    unclaimed: u128,
    raw_inventory: u128,
    reserved_inventory: u128,
    unit_inventory: u128,
    raw_received: u128,
    raw_compounded: u128,
    raw_paid: u128,
    units_minted: u128,
    units_paid: u128,
}

impl Model {
    // Model Soroban root-call rollback, including errors after partial work.
    fn atomic<T>(&mut self, action: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let mut next = self.clone();
        let value = action(&mut next)?;
        *self = next;
        Ok(value)
    }

    fn recognize(&mut self) -> Result<()> {
        if self.unclaimed == 0 {
            return Ok(());
        }
        if self.total_weight == 0 {
            return Err(Error::NoHolders);
        }
        self.raw_index = add(
            self.raw_index,
            ratio(self.unclaimed, SCALE, self.total_weight)?,
        )?;
        self.raw_inventory = add(self.raw_inventory, self.unclaimed)?;
        self.raw_received = add(self.raw_received, self.unclaimed)?;
        self.unclaimed = 0;
        Ok(())
    }

    fn checkpoint(&mut self, id: u32) -> Result<()> {
        let mut account = self.accounts.get(&id).cloned().unwrap_or(Account {
            epoch: self.epoch,
            raw_index: self.raw_index,
            ..Account::default()
        });
        if account.epoch == self.epoch {
            account.raw_scaled = add(
                account.raw_scaled,
                mul(account.weight, sub(self.raw_index, account.raw_index)?)?,
            )?;
        } else {
            // Exactly one historical lookup, even after thousands of harvests.
            let closed = self.closed.get(&account.epoch).ok_or(Error::MissingEpoch)?;
            let historical_raw = add(
                account.raw_scaled,
                mul(account.weight, sub(closed.raw_index, account.raw_index)?)?,
            )?;
            let first_epoch_units = ratio(historical_raw, closed.units_per_raw, SCALE)?;
            let later_epoch_units = mul(
                account.weight,
                sub(self.cumulative_units_index, closed.cumulative_units_index)?,
            )?;
            account.units_scaled = add(
                account.units_scaled,
                add(first_epoch_units, later_epoch_units)?,
            )?;
            account.raw_scaled = mul(account.weight, self.raw_index)?;
        }
        account.epoch = self.epoch;
        account.raw_index = self.raw_index;
        self.accounts.insert(id, account);
        Ok(())
    }

    fn deposit(&mut self, id: u32, weight: u128) -> Result<()> {
        self.atomic(|model| {
            if weight == 0 {
                return Err(Error::ZeroAmount);
            }
            model.recognize()?;
            model.checkpoint(id)?;
            let account = model.accounts.get_mut(&id).unwrap();
            account.weight = add(account.weight, weight)?;
            model.total_weight = add(model.total_weight, weight)?;
            Ok(())
        })
    }

    fn transfer(&mut self, from: u32, to: u32, weight: u128) -> Result<()> {
        self.atomic(|model| {
            if weight == 0 {
                return Ok(());
            }
            model.recognize()?;
            model.checkpoint(from)?;
            model.checkpoint(to)?;
            let sender = model.accounts.get_mut(&from).unwrap();
            sender.weight = sub(sender.weight, weight)?;
            let receiver = model.accounts.get_mut(&to).unwrap();
            receiver.weight = add(receiver.weight, weight)?;
            Ok(())
        })
    }

    /// Reserve only the withdrawing account's proportional, whole raw rewards.
    /// Successful swaps debit this reserve; blocked swaps leave it claimable.
    /// Fractions stay with the same account, including after a final exit.
    fn withdraw(&mut self, id: u32, weight: u128) -> Result<u128> {
        self.atomic(|model| {
            if weight == 0 {
                return Err(Error::ZeroAmount);
            }
            model.recognize()?;
            model.checkpoint(id)?;
            let account = model.accounts.get_mut(&id).unwrap();
            if weight > account.weight {
                return Err(Error::Insufficient);
            }
            let raw = ratio(account.raw_scaled, weight, account.weight)? / SCALE;
            account.raw_scaled = sub(account.raw_scaled, mul(raw, SCALE)?)?;
            account.reserved_raw = add(account.reserved_raw, raw)?;
            account.weight = sub(account.weight, weight)?;
            model.total_weight = sub(model.total_weight, weight)?;
            model.raw_inventory = sub(model.raw_inventory, raw)?;
            model.reserved_inventory = add(model.reserved_inventory, raw)?;
            Ok(raw)
        })
    }

    fn settle_reserved(&mut self, id: u32, raw: u128, swap_succeeds: bool) -> Result<()> {
        self.atomic(|model| {
            model.checkpoint(id)?;
            if raw == 0 {
                return Err(Error::ZeroAmount);
            }
            let account = model.accounts.get_mut(&id).unwrap();
            if raw > account.reserved_raw {
                return Err(Error::Insufficient);
            }
            if swap_succeeds {
                account.reserved_raw = sub(account.reserved_raw, raw)?;
                model.reserved_inventory = sub(model.reserved_inventory, raw)?;
                model.raw_paid = add(model.raw_paid, raw)?;
            }
            Ok(())
        })
    }

    /// `minted_units` must come from actual separately backed compounding output.
    /// This model intentionally cannot price or authorize a production swap.
    fn compound(&mut self, minted_units: u128) -> Result<()> {
        self.atomic(|model| {
            model.recognize()?;
            if model.raw_inventory == 0 || minted_units == 0 {
                return Err(Error::ZeroAmount);
            }
            let rate = ratio(minted_units, SCALE, model.raw_inventory)?;
            if rate == 0 {
                return Err(Error::ZeroAmount);
            }
            model.cumulative_units_index = add(
                model.cumulative_units_index,
                ratio(model.raw_index, rate, SCALE)?,
            )?;
            model.closed.insert(
                model.epoch,
                ClosedEpoch {
                    raw_index: model.raw_index,
                    units_per_raw: rate,
                    cumulative_units_index: model.cumulative_units_index,
                },
            );
            model.epoch = model.epoch.checked_add(1).ok_or(Error::Arithmetic)?;
            model.raw_index = 0;
            model.raw_compounded = add(model.raw_compounded, model.raw_inventory)?;
            model.raw_inventory = 0;
            model.unit_inventory = add(model.unit_inventory, minted_units)?;
            model.units_minted = add(model.units_minted, minted_units)?;
            Ok(())
        })
    }

    fn claim_units(&mut self, id: u32) -> Result<u128> {
        self.atomic(|model| {
            model.checkpoint(id)?;
            let account = model.accounts.get_mut(&id).unwrap();
            let units = account.units_scaled / SCALE;
            account.units_scaled = sub(account.units_scaled, mul(units, SCALE)?)?;
            model.unit_inventory = sub(model.unit_inventory, units)?;
            model.units_paid = add(model.units_paid, units)?;
            Ok(units)
        })
    }

    fn accrual(&mut self, amount: u128) {
        self.unclaimed = add(self.unclaimed, amount).unwrap();
    }

    // Test oracle only; production operations above never enumerate holders.
    fn assert_conserved(&self) {
        let mut snapshot = self.clone();
        let ids: std::vec::Vec<_> = snapshot.accounts.keys().copied().collect();
        for id in ids {
            snapshot.checkpoint(id).unwrap();
        }
        assert_eq!(
            self.raw_received,
            self.raw_inventory + self.reserved_inventory + self.raw_compounded + self.raw_paid
        );
        assert_eq!(self.units_minted, self.unit_inventory + self.units_paid);
        assert_eq!(
            self.total_weight,
            self.accounts
                .values()
                .map(|account| account.weight)
                .sum::<u128>()
        );
        assert_eq!(
            self.reserved_inventory,
            self.accounts
                .values()
                .map(|account| account.reserved_raw)
                .sum::<u128>()
        );
        assert!(
            snapshot
                .accounts
                .values()
                .map(|account| account.raw_scaled)
                .sum::<u128>()
                <= self.raw_inventory * SCALE
        );
        assert!(
            snapshot
                .accounts
                .values()
                .map(|account| account.units_scaled)
                .sum::<u128>()
                <= self.unit_inventory * SCALE
        );
    }
}

#[test]
fn late_deposit_cannot_capture_unclaimed_rewards() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(40);
    model.deposit(2, 100).unwrap(); // Recognize the old 40 before changing supply.
    model.accrual(20);
    model.compound(120).unwrap();
    assert_eq!(model.claim_units(1).unwrap(), 100);
    assert_eq!(model.claim_units(2).unwrap(), 20);
    model.assert_conserved();
}

#[test]
fn transfer_keeps_historical_claims_with_earner() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(40);
    model.transfer(1, 2, 100).unwrap();
    model.accrual(20);
    model.compound(120).unwrap();
    assert_eq!(model.claim_units(1).unwrap(), 80);
    assert_eq!(model.claim_units(2).unwrap(), 40);
    assert_eq!(model.accounts[&1].weight, 0);
    model.assert_conserved();
}

#[test]
fn closed_rewards_are_not_paid_again_on_withdrawal() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(20);
    model.compound(40).unwrap();
    assert_eq!(model.withdraw(1, 100).unwrap(), 0);
    assert_eq!(model.claim_units(1).unwrap(), 40);
    assert_eq!(model.claim_units(1).unwrap(), 0);
    model.assert_conserved();
}

#[test]
fn blocked_partial_exit_reserves_only_its_portion() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.deposit(2, 100).unwrap();
    model.accrual(80);
    assert_eq!(model.withdraw(1, 50).unwrap(), 20);
    model.settle_reserved(1, 20, false).unwrap();
    assert_eq!(model.reserved_inventory, 20);
    assert_eq!(model.raw_inventory, 60);
    model.compound(120).unwrap(); // Must exclude the reserved 20 raw tokens.
    assert_eq!(model.claim_units(1).unwrap(), 40);
    assert_eq!(model.claim_units(2).unwrap(), 80);
    model.settle_reserved(1, 20, true).unwrap();
    assert_eq!(model.raw_paid, 20);
    model.assert_conserved();
}

#[test]
fn deferred_final_exit_survives_zero_supply_and_reentry() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(30);
    assert_eq!(model.withdraw(1, 100).unwrap(), 30);
    model.settle_reserved(1, 30, false).unwrap();
    assert_eq!(model.total_weight, 0);
    model.deposit(2, 100).unwrap();
    model.accrual(10);
    model.compound(20).unwrap();
    assert_eq!(model.claim_units(1).unwrap(), 0);
    assert_eq!(model.claim_units(2).unwrap(), 20);
    model.settle_reserved(1, 30, true).unwrap();
    model.assert_conserved();
}

#[test]
fn dormant_holder_catches_up_across_many_epochs_without_replay() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.deposit(2, 100).unwrap();
    for _ in 0..1000 {
        model.accrual(20);
        model.compound(40).unwrap();
        assert_eq!(model.claim_units(2).unwrap(), 20);
    }
    // Only epoch 0 is needed for account 1: all middle history can be unavailable.
    model.closed.retain(|epoch, _| *epoch == 0);
    assert_eq!(model.claim_units(1).unwrap(), 20_000);
    assert_eq!(model.claim_units(1).unwrap(), 0);
    model.assert_conserved();
}

#[test]
fn zero_and_self_transfers_do_not_create_claims() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(40);
    model.transfer(1, 1, 50).unwrap();
    model.transfer(1, 2, 0).unwrap();
    model.compound(80).unwrap();
    assert_eq!(model.claim_units(1).unwrap(), 80);
    assert_eq!(model.claim_units(2).unwrap(), 0);
    model.assert_conserved();
}

#[test]
fn failed_boundary_and_overflow_roll_back_recognition() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(40);
    let before = model.clone();
    assert_eq!(model.transfer(1, 2, 101), Err(Error::Insufficient));
    assert_eq!(model, before);
    assert_eq!(model.withdraw(1, 101), Err(Error::Insufficient));
    assert_eq!(model, before);
    assert_eq!(model.deposit(2, u128::MAX), Err(Error::Arithmetic));
    assert_eq!(model, before);
}

#[test]
fn zero_supply_rewards_cannot_be_assigned_to_next_depositor() {
    let mut model = Model::default();
    model.accrual(1);
    let before = model.clone();
    assert_eq!(model.deposit(1, 100), Err(Error::NoHolders));
    assert_eq!(model, before);
}

#[test]
fn fractional_exit_claim_stays_with_old_holder() {
    let mut model = Model::default();
    model.deposit(1, 3).unwrap();
    model.deposit(2, 3).unwrap();
    model.accrual(1);
    assert_eq!(model.withdraw(1, 3).unwrap(), 0);
    assert!(model.accounts[&1].raw_scaled > 0);
    model.deposit(3, 3).unwrap();
    model.compound(12).unwrap();
    assert_eq!(model.claim_units(3).unwrap(), 0);
    assert_eq!(model.claim_units(1).unwrap(), 5); // Fractional remainder retained.
    assert_eq!(model.claim_units(2).unwrap(), 5);
    model.assert_conserved();
}

#[test]
fn missing_epoch_fails_closed_without_erasing_claims() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(20);
    model.compound(40).unwrap();
    model.closed.clear();
    let before = model.clone();
    assert_eq!(model.claim_units(1), Err(Error::MissingEpoch));
    assert_eq!(model, before);
}

#[test]
fn independently_versioned_reward_stream_retains_old_claims() {
    let mut old_token = Model::default();
    let mut new_token = Model::default();
    old_token.deposit(1, 100).unwrap();
    old_token.accrual(30);
    old_token.withdraw(1, 100).unwrap();
    new_token.deposit(1, 100).unwrap();
    new_token.accrual(50);
    new_token.compound(100).unwrap();
    assert_eq!(new_token.claim_units(1).unwrap(), 100);
    old_token.settle_reserved(1, 30, true).unwrap();
    old_token.assert_conserved();
    new_token.assert_conserved();
}

#[test]
fn deterministic_mixed_timelines_never_exceed_actual_backing() {
    // Reproducible state-machine regression, not a replacement for fuzzing or
    // independent differential tests against real contracts and pool WASMs.
    for seed in 1..=16u64 {
        let mut random = seed;
        let mut model = Model::default();
        model.deposit(0, 100).unwrap();
        for _ in 0..500 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let id = ((random >> 8) % 4) as u32;
            let other = ((random >> 16) % 4) as u32;
            let amount = u128::from((random >> 24) % 19 + 1);
            let balance = model.accounts.get(&id).map_or(0, |account| account.weight);
            match random % 7 {
                0 => {
                    model.deposit(id, amount).unwrap();
                }
                1 if model.total_weight > 0 => model.accrual(amount),
                2 if balance > 0 => {
                    model.transfer(id, other, amount.min(balance)).unwrap();
                }
                3 if balance > 0 => {
                    model.withdraw(id, amount.min(balance)).unwrap();
                }
                4 if model.raw_inventory + model.unclaimed > 0 => {
                    model.compound(amount).unwrap();
                }
                5 => {
                    model.claim_units(id).unwrap();
                }
                6 => {
                    let reserved = model.accounts.get(&id).map_or(0, |a| a.reserved_raw);
                    if reserved > 0 {
                        model
                            .settle_reserved(id, amount.min(reserved), random & 256 == 0)
                            .unwrap();
                    }
                }
                _ => {}
            }
            model.assert_conserved();
        }
    }
}

#[test]
fn compounding_overflow_and_zero_output_do_not_consume_rewards() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.accrual(30);
    let before = model.clone();
    assert_eq!(model.compound(0), Err(Error::ZeroAmount));
    assert_eq!(model, before);
    assert_eq!(model.compound(u128::MAX), Err(Error::Arithmetic));
    assert_eq!(model, before);
}

#[test]
fn four_holder_timeline_has_exact_expected_ownership_across_epochs() {
    let mut model = Model::default();
    model.deposit(1, 100).unwrap();
    model.deposit(2, 100).unwrap();
    model.accrual(20); // 10 each to Alice and Bob.
    model.transfer(1, 3, 40).unwrap();
    model.accrual(20); // Alice 6, Bob 10, Carol 4.
    model.compound(120).unwrap(); // Three claim units per raw reward.
    model.accrual(30); // Alice 9, Bob 15, Carol 6.
    assert_eq!(model.withdraw(2, 50).unwrap(), 7); // 7.5 rounds down; Bob keeps 8.
    model.deposit(4, 100).unwrap();
    model.accrual(25); // Alice 6, Bob 5, Carol 4, Dave 10.
    model.compound(96).unwrap(); // Two units per remaining raw reward.
    assert_eq!(model.claim_units(1).unwrap(), 78); // (10+6)*3 + (9+6)*2
    assert_eq!(model.claim_units(2).unwrap(), 86); // (10+10)*3 + (8+5)*2
    assert_eq!(model.claim_units(3).unwrap(), 32); // 4*3 + (6+4)*2
    assert_eq!(model.claim_units(4).unwrap(), 20); // No share of older rewards.
    model.settle_reserved(2, 7, true).unwrap();
    assert_eq!(model.unit_inventory, 0);
    model.assert_conserved();
}

#[test]
fn repeated_withdrawals_cannot_round_up_or_spend_another_holders_rewards() {
    let mut model = Model::default();
    model.deposit(1, 10).unwrap();
    model.deposit(2, 10).unwrap();
    model.accrual(6);
    let mut reserved = 0;
    for _ in 0..10 {
        reserved += model.withdraw(1, 1).unwrap();
        model.assert_conserved();
    }
    assert_eq!(reserved, 3);
    model.compound(30).unwrap();
    assert_eq!(model.claim_units(1).unwrap(), 0);
    assert_eq!(model.claim_units(2).unwrap(), 30);
    assert_eq!(model.settle_reserved(1, 4, true), Err(Error::Insufficient));
    model.settle_reserved(1, 3, true).unwrap();
    model.assert_conserved();
}
