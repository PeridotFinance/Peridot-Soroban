#![no_std]

//! Principal-route research only. This is not the completed handoff-07 facade.
//! The feature and network guards prevent accidentally exposing this incomplete
//! adapter as a Mainnet contract. Beneficial-owner recovery hooks are not built.

#[cfg(any(test, feature = "validation-harness"))]
mod route;
#[cfg(any(test, feature = "validation-harness"))]
pub use route::*;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod test;
