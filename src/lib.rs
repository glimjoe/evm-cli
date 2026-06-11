// SPDX-License-Identifier: MIT
//
// evm-cli library root.
//
// M0: types, error. M1: crypto. M2: keystore. M3+: chain, cli.
// See PLAN-V10 §2 (Repository Layout) and ADR-0003 (Workspace Split)
// for the allowed module dependency graph.

#![allow(unused_crate_dependencies)] // M1+ deps declared in Cargo.toml per V8 §11 step 4

pub mod crypto;
pub mod error;
pub mod keystore;
pub mod types;

pub use error::CliError;
pub use types::secret::Secret;
