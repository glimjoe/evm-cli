// SPDX-License-Identifier: MIT
//
// Fuzz target: RLP decode.
//
// Per PLAN-V9 §5 M3 DoD (P0-10): "fuzz/fuzz_rlp_decode/: feed random
// bytes to `alloy::rlp::Decodable`; must not panic".
//
// We try a handful of consensus tx types (TxLegacy, TxEip1559, TxEip2930,
// TxEip7702). Each is decoded from the raw fuzz input; panics are
// surfaced by `cargo-fuzz` and uploaded as crash artifacts. The CI
// nightly job (`.github/workflows/nightly-fuzz.yml`) runs this for
// 5 minutes per harness (per PLAN-V9 §6 nightly row).

#![no_main]

use alloy_consensus::{TxEip1559, TxEip2930, TxEip7702, TxLegacy};
use alloy_rlp::Decodable;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Try each tx type. The `Decodable` impls must NOT panic on
    // arbitrary input — only return `Err`. The buffer is mutated in
    // place; each `decode` call is independent.
    let mut buf = data;
    let _ = TxLegacy::decode(&mut buf);
    let mut buf = data;
    let _ = TxEip1559::decode(&mut buf);
    let mut buf = data;
    let _ = TxEip2930::decode(&mut buf);
    let mut buf = data;
    let _ = TxEip7702::decode(&mut buf);
});
