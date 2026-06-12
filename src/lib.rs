// SPDX-License-Identifier: MIT
//
// evm-cli library root.
//
// M0: types, error. M1: crypto. M2: keystore. M3: chain. M4: cli.
// See PLAN-V9 §2 (Repository Layout) and ADR-0003 (Workspace Split)
// for the allowed module dependency graph.

#![allow(unused_crate_dependencies)] // M1+ deps declared in Cargo.toml per PLAN-V9 §11 step 4

pub mod chain;
pub mod cli;
pub mod crypto;
pub mod error;
pub mod keystore;
pub mod release;
pub mod types;

pub use error::CliError;
pub use types::secret::Secret;
