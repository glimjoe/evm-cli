// SPDX-License-Identifier: MIT
//
// M5 release-workflow file validation.
//
// Reads `.github/workflows/release.yml` at test time and asserts
// that it passes `evm_cli::release::validate_release_workflow_yaml`.
// This is the "test the artifact" gate: if a human edits the workflow
// and accidentally drops a required step, this test fails in CI.

#![allow(clippy::disallowed_methods)] // integration tests
#![allow(clippy::expect_used, clippy::unwrap_used)] // same

use std::fs;
use std::path::PathBuf;

use evm_cli::release;

fn workflow_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is set by Cargo to the crate root (the
    // directory containing Cargo.toml), which is `evm-cli/`.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(".github");
    p.push("workflows");
    p.push("release.yml");
    p
}

#[test]
fn release_yml_exists() {
    let p = workflow_path();
    assert!(
        p.exists(),
        "release workflow file must exist at {}",
        p.display()
    );
}

#[test]
fn release_yml_passes_schema_validator() {
    let p = workflow_path();
    let yaml = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    release::validate_release_workflow_yaml(&yaml)
        .unwrap_or_else(|e| panic!("release.yml failed schema validation: {e}"));
}

#[test]
fn release_yml_mentions_all_required_targets() {
    // Beyond the step-name check, the workflow should declare the two
    // target platforms we promised in the M5 DoD.
    let p = workflow_path();
    let yaml = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    assert!(yaml.contains("linux-x86_64"), "linux-x86_64 missing");
    assert!(yaml.contains("linux-aarch64"), "linux-aarch64 missing");
}

#[test]
fn release_yml_uses_sha256sum() {
    let p = workflow_path();
    let yaml = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    assert!(
        yaml.contains("sha256sum"),
        "release.yml must compute SHA256 with sha256sum"
    );
}
