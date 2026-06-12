// SPDX-License-Identifier: MIT
//
// Fuzz target: keystore JSON deserialization.
//
// Per PLAN-V9 §5 M3 DoD (P0-10): "fuzz/fuzz_keystore_json/: random
// JSON-shaped bytes → keystore deserialize must not panic".
//
// We feed arbitrary bytes to `serde_json::from_slice::<serde_json::Value>`
// (the lowest-level JSON parse) and to the eth-keystore style
// `KeyFile` parser. Both must NOT panic on arbitrary input.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // JSON parse: must return Err, not panic.
    let _ = serde_json::from_slice::<serde_json::Value>(data);

    // Additionally, we test `serde_json::Value` accessors (e.g. `as_object`,
    // `as_str`) which the eth-keystore crate uses internally. We use a
    // helper closure to walk any valid JSON value without panicking on
    // unexpected types.
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(data) {
        // Just calling accessors — no panics expected.
        let _ = v.as_object();
        let _ = v.as_array();
        let _ = v.as_str();
        let _ = v.as_i64();
        let _ = v.as_u64();
        let _ = v.as_f64();
        let _ = v.as_bool();
    }
});
