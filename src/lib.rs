#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

//! ALICE-VM: Bytecode virtual machine with stack machine, register machine,
//! instruction set, memory management, and call frames.

pub(crate) mod constants;
pub mod error;
pub(crate) mod frame;
pub(crate) mod heap;
pub mod op;
pub mod prelude;
pub mod vm;

#[cfg(test)]
mod integration_tests;

// Backward-compat re-exports.
pub use crate::error::*;
pub use crate::op::*;
pub use crate::vm::*;
