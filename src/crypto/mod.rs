// SPDX-License-Identifier: MIT
//
// evm_cli::crypto — BIP-39 / BIP-44 / Keccak-256 / signing.
//
// Per PLAN-V9 §5 M1 DoD and ADR-0003 (workspace split), this module
// depends only on `types` and external crates (alloy, tiny-keccak). It
// does NOT depend on `keystore`, `chain`, or `cli` (those come in M2+).
//
// Submodules:
//   keccak   — Keccak-256 wrapper around `tiny-keccak` (not `sha3`)
//   mnemonic — BIP-39 12/24-word generation, validation, conversion to seed
//   address  — BIP-44 derivation (m/44'/60'/0'/0/index), EIP-55 checksum
//   sign     — EIP-2 low-S check, personal_sign (EIP-191), ecrecover

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests legitimately use `.expect()` / `.unwrap()` on
// fixed inputs (`tempdir()`, `Secret::new("...")`-style fakes). Production
// paths must not trip `clippy::disallowed_methods` (P0-4). Same narrow
// form as `src/keystore/mod.rs:33` per the M2 review's §B.

pub mod address;
pub mod keccak;
pub mod mnemonic;
pub mod sign;
