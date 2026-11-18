#![no_std]
#[cfg(feature = "lp-zero-peri")]
compile_error!("lp-zero-peri is reserved for the lp-peridottroller source consumer");
#[cfg(test)]
extern crate std;

mod constants;
mod contract;
mod events;
pub(crate) mod reflector;
mod storage;

pub use constants::*;
pub use contract::*;
pub use events::*;
pub use storage::*;

mod test;

#[cfg(test)]
mod deployed_price_behaviour_test;
#[cfg(test)]
mod upgrade_checks_test;
