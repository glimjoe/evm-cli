// SPDX-License-Identifier: MIT
//
// evm_cli::crypto — BIP-39 / BIP-44 / Keccak-256 / signing.
//
// Per V8 §5 M1 DoD and ADR-0003 (workspace split), this module depends
// only on `types` and external crates (alloy, tiny-keccak). It does
// NOT depend on `keystore`, `chain`, or `cli` (those come in M2+).
//
// Submodules:
//   keccak   — Keccak-256 wrapper around `tiny-keccak` (not `sha3`)
//   mnemonic — BIP-39 12/24-word generation, validation, conversion to seed
//   address  — BIP-44 derivation (m/44'/60'/0'/0/index), EIP-55 checksum
//   sign     — EIP-2 low-S check, personal_sign (EIP-191), ecrecover

#![allow(clippy::disallowed_methods)]
// We use Vec<u8>::from(...) and String::from(...) in tests for fixed
// byte strings; these are not the "leak secret into a String" patterns
// banned by ADR-0007 (which targets *secret material* specifically).

pub mod address;
pub mod keccak;
pub mod mnemonic;
pub mod sign;
