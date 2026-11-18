//! D3: the liquidation-only fallback valuation becomes available only after the NORMAL
//! checkpoint has been blocked CONTINUOUSLY for `BLOCK_WAIT` seconds.
//!
//! On-chain code cannot watch the pool between transactions, so "continuous" is defined as a
//! heartbeat chain of blocked observations:
//!
//! * every blocked `checkpoint_nav` attempt (a named cause is present: venue valuation fails,
//!   depeg guard, pool out of band / out of range) is an observation. It EXTENDS the chain when
//!   the previous observation is at most `BLOCK_GAP` seconds old; otherwise it RESTARTS it;
//! * a successful `checkpoint_nav` DELETES the chain. It is permissionless, so any healthy
//!   moment the borrower, a keeper or anyone else catches resets the clock;
//! * `checkpoint_fallback` first re-proves a cause NOW (that proof is itself an observation),
//!   then requires the chain to be alive (`now - last <= BLOCK_GAP`) and old enough
//!   (`now - since >= BLOCK_WAIT`);
//! * a stale/missing oracle records nothing (the caller reverts first), so the chain lapses and
//!   the account becomes repay-only (D4).
//!
//! Guarantee: at the fallback, no two consecutive blocked observations in the last
//! `BLOCK_WAIT` seconds were more than `BLOCK_GAP` apart, and no healthy NAV checkpoint landed
//! in between. Residual: a healthy interval shorter than `BLOCK_GAP` that nobody sampled is not
//! seen; the protocol keeper therefore refreshes `checkpoint_nav` more often than `BLOCK_GAP`
//! while a chain is open (see D3-CONTINUITY.md).

/// Minimum continuous-block age before the fallback is admitted (user decision: 10 minutes).
pub const BLOCK_WAIT: u64 = 600;
/// Maximum spacing between consecutive blocked observations of one chain (= checkpoint max age).
pub const BLOCK_GAP: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chain {
    /// Time of the first observation of the current chain.
    pub since: u64,
    /// Time of the latest observation.
    pub last: u64,
    /// Observations in the chain (diagnostic only).
    pub n: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No chain: nothing was observed blocked, or a healthy checkpoint cleared it.
    NotBlocked,
    /// The latest observation is older than `BLOCK_GAP`: continuity not shown.
    ChainBroken,
    /// The chain is alive but shorter than `BLOCK_WAIT`.
    BlockTooRecent,
}

/// A blocked normal-checkpoint observation at `now`.
pub fn observe(prev: Option<Chain>, now: u64) -> Chain {
    match prev {
        Some(c) if now >= c.last && now - c.last <= BLOCK_GAP => Chain { since: c.since, last: now, n: c.n.saturating_add(1) },
        _ => Chain { since: now, last: now, n: 1 },
    }
}

/// Fallback admission at `now`; the caller has ALREADY proven a blocking cause at `now`.
/// On success returns the extended chain to store.
pub fn admit_fallback(prev: Option<Chain>, now: u64) -> Result<Chain, Refusal> {
    let c = prev.ok_or(Refusal::NotBlocked)?;
    if now < c.last || now - c.last > BLOCK_GAP {
        return Err(Refusal::ChainBroken);
    }
    if now - c.since < BLOCK_WAIT {
        return Err(Refusal::BlockTooRecent);
    }
    Ok(Chain { since: c.since, last: now, n: c.n.saturating_add(1) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Observations at t0, t0+step, ... up to and including t0+span.
    fn beat(t0: u64, step: u64, span: u64) -> Option<Chain> {
        let mut c = None;
        let mut t = t0;
        while t <= t0 + span {
            c = Some(observe(c, t));
            t += step;
        }
        c
    }

    #[test]
    fn at_9_59_refused_at_10_00_admitted() {
        let c = beat(1_000, 60, 540); // since 1000, last 1540
        assert_eq!(admit_fallback(c, 1_599), Err(Refusal::BlockTooRecent), "9:59");
        let ok = admit_fallback(c, 1_600).expect("10:00");
        assert_eq!((ok.since, ok.last, ok.n), (1_000, 1_600, 11));
    }

    #[test]
    fn healthy_checkpoint_clears() {
        // A successful checkpoint_nav deletes the chain (the caller stores None).
        let c = beat(1_000, 60, 300);
        assert!(c.is_some());
        let cleared: Option<Chain> = None;
        assert_eq!(admit_fallback(cleared, 1_600), Err(Refusal::NotBlocked));
        // Blocking resumes after the clear: a fresh chain, 10 minutes from its own start.
        let c2 = beat(1_360, 60, 240); // restarts at 1360
        assert_eq!(admit_fallback(c2, 1_600), Err(Refusal::BlockTooRecent));
        let c3 = beat(1_360, 60, 540);
        assert!(admit_fallback(c3, 1_960).is_ok());
    }

    #[test]
    fn gap_of_61_s_restarts_the_chain() {
        let c = beat(1_000, 60, 240); // last 1240
        let restarted = observe(c, 1_301); // 61 s later
        assert_eq!((restarted.since, restarted.n), (1_301, 1));
        let mut d = Some(restarted);
        let mut t = 1_301;
        while t + 60 <= 1_600 { t += 60; d = Some(observe(d, t)); }
        assert_eq!(admit_fallback(d, 1_600), Err(Refusal::BlockTooRecent), "first-block age alone is not enough");
        // Positive control: exactly 60 s spacing keeps one chain.
        let e = observe(c, 1_300);
        assert_eq!(e.since, 1_000);
    }

    #[test]
    fn lapsed_chain_is_broken() {
        let c = beat(1_000, 60, 540); // last 1540
        assert_eq!(admit_fallback(c, 1_601), Err(Refusal::ChainBroken), "61 s since last observation");
        assert!(admit_fallback(c, 1_600).is_ok(), "60 s is still alive");
    }

    #[test]
    fn two_momentary_pushes_ten_minutes_apart_are_not_continuous() {
        // The sleeve-d3 first-block rule would admit this; the chain does not.
        let c = Some(observe(None, 1_000));
        assert_eq!(admit_fallback(c, 1_600), Err(Refusal::ChainBroken));
        let c2 = observe(c, 1_600);
        assert_eq!(c2.since, 1_600);
    }

    #[test]
    fn fallback_extends_and_repeats() {
        let c = beat(1_000, 60, 540);
        let a = admit_fallback(c, 1_600).unwrap();
        let b = admit_fallback(Some(a), 1_660).unwrap();
        assert_eq!((b.since, b.last), (1_000, 1_660));
        assert_eq!(admit_fallback(Some(b), 1_721), Err(Refusal::ChainBroken));
    }

    #[test]
    fn clock_never_runs_backwards_into_a_chain() {
        let c = Some(Chain { since: 1_000, last: 2_000, n: 5 });
        assert_eq!(admit_fallback(c, 1_999), Err(Refusal::ChainBroken));
        assert_eq!(observe(c, 1_999).since, 1_999);
    }
}
