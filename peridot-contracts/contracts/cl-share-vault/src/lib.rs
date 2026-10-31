#![no_std]
//! Non-deployable accounting kernel for the proposed transferable CL vault.
//!
//! These functions take verified measurements as inputs. They do not provide
//! authorization, persistence, token custody, oracle verification or a pool
//! implementation. In particular, a successful model test is not evidence that
//! the V3/Aquarius execution path works. See the crate README for integration gaps.

#[cfg(target_arch = "wasm32")]
compile_error!("cl-share-vault is an accounting kernel, not a deployable contract");

pub mod accounting;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
