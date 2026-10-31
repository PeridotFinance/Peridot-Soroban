#![no_std]

mod constants;
mod contract;
mod events;
mod fee_entitlements;
mod fees;
#[cfg(any(test, feature = "funded-liquidation-prototype"))]
mod funded_liquidation;
#[cfg(all(feature = "funded-liquidation-prototype", target_arch = "wasm32"))]
compile_error!("funded-liquidation-prototype is native integration work, not a deployable release");
mod helpers;
mod perps;
mod storage;

pub use constants::*;
pub use contract::*;
pub use events::*;
pub use helpers::*;
pub use storage::*;

#[cfg(test)]
mod test;
