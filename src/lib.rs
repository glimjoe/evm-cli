// SPDX-License-Identifier: MIT
//
// evm-cli library root.
//
// At M0 this exposes only the `Secret<T>` type and the `error` module.
// Subsequent milestones add `crypto`, `keystore`, `chain`, and `cli`.
// See PLAN-V8 §2 (Repository Layout) and ADR-0003 (Workspace Split) for
// the allowed module dependency graph.

#![allow(unused_crate_dependencies)] // M1+ deps declared in Cargo.toml per V8 §11 step 4

pub mod error;
pub mod types;

pub use error::CliError;
pub use types::secret::Secret;
