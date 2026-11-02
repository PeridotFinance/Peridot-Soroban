#![no_std]

mod constants;
mod contract;
mod events;
mod fee_entitlements;
mod fees;
#[cfg(any(test, feature = "funded-liquidation-prototype"))]
mod funded_liquidation;
#[cfg(all(
    feature = "funded-liquidation-prototype",
    target_arch = "wasm32",
    not(feature = "funded-liquidation-validation")
))]
compile_error!("funded-liquidation-prototype is native-only; use guarded funded-liquidation-validation for local WASM integration");
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

#[cfg(feature = "funded-liquidation-validation")]
mod cl_custody;
