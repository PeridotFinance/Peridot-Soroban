//! Supplemental complete recovery transactions; captured prestate is explicit.
use super::*;

fn from_phase(delivered: bool) {
    let (f, _) = setup();
    let mut id = 0;
    prestate(&f, || {
        id = begin(&f);
        fund(&f, id);
        prepare(&f, id);
        mint(&f, id);
        if delivered {
            let _: u128 = stage(&f, "split_deliver", (f.user.clone(), id).into_val(&f.e));
        }
        policy(&f, 10000, false);
    });
    recover(&f, id);
    assert_eq!(state(&f, id).phase, 6);
}

#[test]
fn compiled_split_late_recovery_minted() { from_phase(false); }

#[test]
fn compiled_split_late_recovery_delivered() { from_phase(true); }
