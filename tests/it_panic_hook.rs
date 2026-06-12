// SPDX-License-Identifier: MIT
//
// Integration test: panic hook does not leak secret material or
// panic location (ADR-0005 verification, PLAN-V9 §11 step 12).
//
// Strategy: spawn the **release** binary with the
// `EVMC_TEST_TRIGGER_PANIC=1` env var (gated in
// `src/main.rs::trigger_test_panic` so it cannot fire in
// production). The panic payload contains a recognizable fake
// private key + BIP-39 mnemonic; capture stderr and assert:
//   1. The fake private key is NOT in stderr.
//   2. The fake mnemonic is NOT in stderr.
//   3. The default panic location line ("panicked at <file>:<line>")
//      is NOT in stderr (human-panic replaces the default hook).
//   4. The binary exits with non-zero (panic abort).
//
// **Why release and not dev:** human-panic 2.0.8's `setup_panic!()`
// is a no-op under `cfg(debug_assertions)` (this is by design — the
// crate is intended for shipping binaries). The test therefore runs
// against the release binary, building it on-demand if missing.
//
// M3 audit C8: this test was previously a documented placeholder. It
// is now a real test (the panic trigger is wired in `src/main.rs`).
// The dev profile deviance is now self-documented above.

#![allow(unused_crate_dependencies)] // assert_cmd is in dev-deps
#![allow(clippy::disallowed_methods)] // String::from_utf8_lossy is OK in test code (inspecting captured output)

use std::path::PathBuf;
use std::process::Command;

const EVMC_TEST_TRIGGER: &str = "EVMC_TEST_TRIGGER_PANIC";
const FAKE_PRIVATE_KEY_HEX: &str =
    "0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const FAKE_MNEMONIC: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

/// Locate (and build on demand) the **release** binary. We use the
/// release profile because human-panic 2.0.8's `setup_panic!()` is a
/// no-op under `cfg(debug_assertions)`.
fn release_bin() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `cargo build --release` outputs to `<workspace_root>/target/release/`
    // (or `target/release/` when the crate itself is the workspace root,
    // which is our case). The CLI binary name is `evm-cli`.
    let target_dir = manifest_dir.join("target").join("release").join("evm-cli");
    if !target_dir.exists() {
        eprintln!("release binary not found; building (this may take a moment)...");
        let status = Command::new("cargo")
            .args(["build", "--release", "--bin", "evm-cli"])
            .current_dir(&manifest_dir)
            .env("RUST_BACKTRACE", "0")
            .status()
            .expect("spawn cargo build --release");
        assert!(
            status.success(),
            "cargo build --release failed (status {status:?})"
        );
    }
    target_dir
}

#[test]
fn panic_hook_does_not_leak_secret_in_stderr() {
    let bin = release_bin();
    if !bin.exists() {
        panic!(
            "release binary not found at {} even after build attempt",
            bin.display()
        );
    }

    // Critical: human-panic 2.0.8's `PanicStyle::default()` checks
    // `RUST_BACKTRACE`: if the var is set to ANY value (including "0",
    // which is what CI sets globally per ADR-0005), human-panic falls
    // back to `PanicStyle::Debug` (no-op, default hook remains). The
    // test must explicitly REMOVE the var so human-panic fires.
    let output = Command::new(&bin)
        .env(EVMC_TEST_TRIGGER, "1")
        .env_remove("RUST_BACKTRACE")
        .env_remove("CI")
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            panic!("could not execute binary {}: {e}", bin.display());
        }
    };

    // Panic must produce a non-zero exit code.
    assert!(
        !output.status.success(),
        "expected panic to produce non-zero exit; got {:?}",
        output.status
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // (1) No leak: secret material must not appear in stderr.
    assert!(
        !stderr.contains(FAKE_PRIVATE_KEY_HEX),
        "stderr leaked fake private key: {:?}",
        &stderr[..stderr.len().min(400)]
    );
    assert!(
        !stderr.contains(FAKE_MNEMONIC),
        "stderr leaked fake BIP-39 mnemonic: {:?}",
        &stderr[..stderr.len().min(400)]
    );

    // (2) The default panic-location line must NOT appear
    // (human-panic replaces the default hook).
    assert!(
        !stderr.contains("panicked at"),
        "default panic location leaked; human-panic hook not active: {:?}",
        &stderr[..stderr.len().min(400)]
    );

    // (3) A human-panic friendly message is present.
    assert!(
        stderr.contains("internal error")
            || stderr.contains("report this")
            || stderr.contains("panicked")
            || stderr.contains("Oops")
            || stderr.contains("embarrassing")
            || stderr.contains("had a problem"),
        "expected human-panic friendly message; got: {:?}",
        &stderr[..stderr.len().min(400)]
    );
}
