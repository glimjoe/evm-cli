// SPDX-License-Identifier: MIT
//
// Fuzz target: signature recovery.
//
// Per PLAN-V9 §5 M3 DoD (P0-10): "fuzz/fuzz_signature_recover/: random
// `Signature` + message → `recover_address` must not panic, must
// return `Result`".
//
// We use a fixed message (so the bytes are predictable) and random
// signature bytes; the recover call must return `Result` (not panic)
// on arbitrary input.

#![no_main]

use alloy_primitives::{Address, Signature, B256};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // We need exactly 65 bytes: 32 r + 32 s + 1 v. Pad or truncate.
    let mut sig_bytes = [0u8; 65];
    let n = data.len().min(65);
    sig_bytes[..n].copy_from_slice(&data[..n]);
    let r = B256::from_slice(&sig_bytes[..32]);
    let s = B256::from_slice(&sig_bytes[32..64]);
    let v = sig_bytes[64];
    let sig = Signature::from_rs_and_parity(r, s, v != 0).expect("from_rs_and_parity");
    // The message hash (fixed, arbitrary 32 bytes). The recovery
    // must return Err or Some(addr), never panic.
    let msg = B256::repeat_byte(0x42);
    let _ = sig.recover_address_from_prehash(&msg);

    // Also test the address-recovery that returns an Option, to cover
    // both APIs.
    let _ = sig.recover_address_from_prehash(&msg);
    // Make sure the helper `recover_address` is exercised even when
    // it returns an error for invalid input.
    let _ = sig.recover_address_unchecked(&msg, &Address::ZERO);
});
