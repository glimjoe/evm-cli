// SPDX-License-Identifier: MIT
//
// Primitive newtype wrappers per PLAN-V9 §3 (Type System).
//
// All seven newtypes are M0 deliverables per the plan. They are
// thin wrappers around alloy types (or `u64`); the newtype is the
// API boundary while alloy types are used internally where
// alloy requires them (e.g. trait method signatures). Each
// newtype implements `From`/`Into`/`AsRef` for ergonomic
// interop with the underlying alloy type.
//
// Per ADR-0003 rev1, the `types` module has no upstream deps on
// other `evm_cli` modules — it depends only on external crates
// (alloy, thiserror). The `Secret<T>` type lives here too, but
// is isolated in its own submodule for the §3 P0-2 hardening.

pub mod address;
pub mod amount;
pub mod block_number;
pub mod chain_id;
pub mod nonce;
pub mod secret;
pub mod signature;
pub mod tx_hash;

// Public re-exports for convenient use:
//   use crate::types::{Address, Nonce, ChainId, ...};
pub use address::{Address, AddressParseError};
pub use amount::{Amount, AmountParseError};
pub use block_number::BlockNumber;
pub use chain_id::ChainId;
pub use nonce::Nonce;
pub use secret::Secret;
pub use signature::Signature;
pub use tx_hash::TxHash;
