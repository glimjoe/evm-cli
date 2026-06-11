// SPDX-License-Identifier: MIT
//
// Primitive newtype wrappers per V8 §3 (Type System).
//
// At M0 this module exposes only `Secret<T>`. M1+ adds Address, Amount,
// Nonce, ChainId, Signature, TxHash, BlockNumber. All depend only on
// `Secret` and external crates — no upstream deps to other evm_cli
// modules (see ADR-0003 rev1 explicit dependency edges).

pub mod secret;
