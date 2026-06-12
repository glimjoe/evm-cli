// SPDX-License-Identifier: MIT
//
// M5 integration tests for the `release` module.
//
// Per PLAN-V9 §5 M5 DoD:
//   - artifacts named `evm-cli-v0.2.0-linux-x86_64.tar.gz` + `.sha256`
//   - release notes include CHANGELOG, security disclaimer, known limitations
//   - `.github/workflows/release.yml` is well-formed and contains the
//     build / sha256 / upload / release steps
//
// These tests exercise the public surface of `evm_cli::release`. They
// are RED before the module is implemented (compile error or panic).

// SPDX-License-Identifier: MIT
//
// M5 integration tests for the `release` module.
//
// Per PLAN-V9 §5 M5 DoD:
//   - artifacts named `evm-cli-v0.2.0-linux-x86_64.tar.gz` + `.sha256`
//   - release notes include CHANGELOG, security disclaimer, known limitations
//   - `.github/workflows/release.yml` is well-formed and contains the
//     build / sha256 / upload / release steps
//
// These tests exercise the public surface of `evm_cli::release`. They
// are RED before the module is implemented (compile error or panic).

#![allow(clippy::disallowed_methods)] // integration tests
#![allow(clippy::expect_used, clippy::unwrap_used)] // same

use evm_cli::release;

// ────────────────────────────────────────────────────────────────────
// artifact naming
// ────────────────────────────────────────────────────────────────────

#[test]
fn artifact_name_linux_x86_64() {
    // Canonical case from M5 DoD.
    assert_eq!(
        release::artifact_name("0.2.0", "x86_64-unknown-linux-gnu"),
        "evm-cli-v0.2.0-linux-x86_64.tar.gz"
    );
}

#[test]
fn artifact_name_linux_aarch64_musl() {
    // Static-linked ARM build (the other target the workflow is
    // expected to produce per PLAN-V9 §6 multi-arch build).
    assert_eq!(
        release::artifact_name("0.2.0", "aarch64-unknown-linux-musl"),
        "evm-cli-v0.2.0-linux-aarch64.tar.gz"
    );
}

#[test]
fn artifact_name_v_prefix_is_required() {
    // The `v` prefix in the artifact name must be present (matches
    // GitHub tag convention v0.2.0, not the semver 0.2.0).
    let name = release::artifact_name("1.2.3", "x86_64-unknown-linux-gnu");
    assert!(
        name.starts_with("evm-cli-v1.2.3-"),
        "artifact must start with `evm-cli-v<ver>-`, got: {name}"
    );
}

#[test]
fn sha256_sidecar_uses_same_naming_convention() {
    assert_eq!(
        release::sha256_sidecar("0.2.0", "x86_64-unknown-linux-gnu"),
        "evm-cli-v0.2.0-linux-x86_64.sha256"
    );
}

#[test]
fn sha256_sidecar_is_distinct_from_tarball() {
    // A common bug: returning the tarball name here by accident.
    let side = release::sha256_sidecar("0.2.0", "x86_64-unknown-linux-gnu");
    let art = release::artifact_name("0.2.0", "x86_64-unknown-linux-gnu");
    assert_ne!(side, art);
    assert!(side.ends_with(".sha256"));
    assert!(art.ends_with(".tar.gz"));
}

// ────────────────────────────────────────────────────────────────────
// platform tag normalization
// ────────────────────────────────────────────────────────────────────

#[test]
fn platform_tag_strips_vendor_and_abi_for_linux() {
    assert_eq!(
        release::platform_tag("x86_64-unknown-linux-gnu"),
        "linux-x86_64"
    );
    assert_eq!(
        release::platform_tag("aarch64-unknown-linux-musl"),
        "linux-aarch64"
    );
}

#[test]
fn platform_tag_falls_back_for_short_target() {
    // A target with fewer than 3 dash-separated parts is malformed;
    // we pass it through unchanged rather than panicking. The artifact
    // would be misnamed, but the call site (CI) only ever uses valid
    // triples. This test pins the fall-back behavior so that an
    // accidental panic-on-invalid-input regression is caught.
    assert_eq!(release::platform_tag(""), "");
    assert_eq!(release::platform_tag("x86_64"), "x86_64");
    assert_eq!(release::platform_tag("linux-gnu"), "linux-gnu");
}

#[test]
fn platform_tag_keeps_dash_separator_and_arch_underscore() {
    // The release-archive format is `-` between platform and arch
    // (e.g. `linux-x86_64`). The arch's own underscores are preserved
    // — that is what differentiates `x86_64` from `x86-64` (which is
    // not a valid Rust target). Important for case sensitivity on
    // case-insensitive filesystems (macOS APFS, Windows NTFS): the
    // `linux-x86_64` form matches the M5 DoD example.
    let tag = release::platform_tag("x86_64-unknown-linux-gnu");
    assert_eq!(tag, "linux-x86_64");
    // Separator is a single dash between `linux` and `x86_64`.
    assert!(
        tag.starts_with("linux-"),
        "platform tag must start with `linux-`: {tag}"
    );
    // Arch retains its underscore form.
    assert!(
        tag.ends_with("x86_64"),
        "arch must keep `x86_64` (not `x86-64`): {tag}"
    );
}

// ────────────────────────────────────────────────────────────────────
// changelog extraction
// ────────────────────────────────────────────────────────────────────

const SAMPLE_CHANGELOG: &str = "\
# Changelog

## [Unreleased]

### Planned
- thing

## [0.2.0] — 2026-06-12

### Added
- release workflow (M5)
- SHA256 sidecar artifacts

### Changed
- version bumped to 0.2.0

## [0.1.0] — 2026-06-12

### Added
- initial release
";

#[test]
fn extract_changelog_finds_matching_version() {
    let body = release::extract_changelog_section(SAMPLE_CHANGELOG, "0.2.0")
        .expect("0.2.0 section must be found");
    assert!(body.contains("release workflow (M5)"));
    assert!(body.contains("SHA256 sidecar artifacts"));
    // Must NOT include the next version's section
    assert!(
        !body.contains("initial release"),
        "extraction must stop at next version heading; got: {body}"
    );
}

#[test]
fn extract_changelog_finds_first_version_when_no_next() {
    let body = release::extract_changelog_section(SAMPLE_CHANGELOG, "0.1.0")
        .expect("0.1.0 section must be found");
    assert!(body.contains("initial release"));
}

#[test]
fn extract_changelog_returns_error_for_missing_version() {
    let err = release::extract_changelog_section(SAMPLE_CHANGELOG, "9.9.9")
        .expect_err("9.9.9 does not exist; must error");
    let msg = err.to_string();
    assert!(
        msg.contains("9.9.9"),
        "error message should mention the missing version, got: {msg}"
    );
}

#[test]
fn extract_changelog_returns_error_for_unreleased() {
    // The `## [Unreleased]` heading is not a versioned entry; trying
    // to extract it as a version must error.
    let err = release::extract_changelog_section(SAMPLE_CHANGELOG, "Unreleased")
        .expect_err("Unreleased is not a real version");
    let _ = err; // presence of Err is the assertion
}

// ────────────────────────────────────────────────────────────────────
// release-notes rendering
// ────────────────────────────────────────────────────────────────────

#[test]
fn render_release_notes_contains_all_three_sections() {
    let notes = release::render_release_notes(
        "### Added\n- release workflow (M5)",
        "**SECURITY:** This is a PoC. Do not use on mainnet with real assets.",
        "**Known limitation:** coverage gate does not pass (61.46% lines).",
    );
    assert!(notes.contains("release workflow (M5)"));
    assert!(notes.contains("SECURITY"));
    assert!(notes.contains("Do not use on mainnet"));
    assert!(notes.contains("Known limitation"));
    assert!(notes.contains("61.46%"));
}

#[test]
fn render_release_notes_sections_are_separated_by_blank_lines() {
    // Markdown parsers treat a single newline as a soft break; a
    // blank line is a paragraph break. All three sections must be
    // paragraph-separated.
    let notes =
        release::render_release_notes("CHANGELOG_BODY", "DISCLAIMER_BODY", "LIMITATIONS_BODY");
    assert!(notes.contains("CHANGELOG_BODY\n\nDISCLAIMER_BODY"));
    assert!(notes.contains("DISCLAIMER_BODY\n\nLIMITATIONS_BODY"));
}

// ────────────────────────────────────────────────────────────────────
// release workflow YAML validation
// ────────────────────────────────────────────────────────────────────

const MINIMAL_VALID_WORKFLOW: &str = r#"
name: Release
on:
  push:
    tags: ['v*']
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Build
        run: cargo build --release
      - name: Tar
        run: tar -czf evm-cli-v0.2.0-linux-x86_64.tar.gz target/release/evm-cli
      - name: Sha256
        run: sha256sum evm-cli-v0.2.0-linux-x86_64.tar.gz > evm-cli-v0.2.0-linux-x86_64.sha256
      - name: Upload
        uses: actions/upload-artifact@v4
      - name: Release
        uses: softprops/action-gh-release@v2
"#;

#[test]
fn validate_release_workflow_yaml_passes_on_minimal_valid() {
    release::validate_release_workflow_yaml(MINIMAL_VALID_WORKFLOW)
        .expect("minimal workflow must validate");
}

#[test]
fn validate_release_workflow_yaml_rejects_missing_build_step() {
    let bad = r#"
name: Release
on: { push: { tags: ['v*'] } }
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Tar
        run: tar -czf x.tar.gz target/release/evm-cli
      - name: Sha256
        run: sha256sum x.tar.gz > x.sha256
      - name: Upload
        uses: actions/upload-artifact@v4
      - name: Release
        uses: softprops/action-gh-release@v2
"#;
    let err =
        release::validate_release_workflow_yaml(bad).expect_err("missing `Build` step must error");
    assert!(
        err.to_string().to_lowercase().contains("build"),
        "error should mention Build, got: {err}"
    );
}

#[test]
fn validate_release_workflow_yaml_rejects_missing_sha256_step() {
    let bad = r#"
name: Release
on: { push: { tags: ['v*'] } }
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Build
        run: cargo build --release
      - name: Tar
        run: tar -czf x.tar.gz target/release/evm-cli
      - name: Upload
        uses: actions/upload-artifact@v4
      - name: Release
        uses: softprops/action-gh-release@v2
"#;
    let err =
        release::validate_release_workflow_yaml(bad).expect_err("missing `Sha256` step must error");
    assert!(
        err.to_string().to_lowercase().contains("sha256"),
        "error should mention Sha256, got: {err}"
    );
}

#[test]
fn validate_release_workflow_yaml_rejects_missing_release_step() {
    let bad = r#"
name: Release
on: { push: { tags: ['v*'] } }
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Build
        run: cargo build --release
      - name: Tar
        run: tar -czf x.tar.gz target/release/evm-cli
      - name: Sha256
        run: sha256sum x.tar.gz > x.sha256
      - name: Upload
        uses: actions/upload-artifact@v4
"#;
    let err = release::validate_release_workflow_yaml(bad)
        .expect_err("missing `Release` step must error");
    assert!(
        err.to_string().to_lowercase().contains("release"),
        "error should mention Release, got: {err}"
    );
}

#[test]
fn validate_release_workflow_yaml_rejects_missing_upload_step() {
    let bad = r#"
name: Release
on: { push: { tags: ['v*'] } }
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - name: Build
        run: cargo build --release
      - name: Tar
        run: tar -czf x.tar.gz target/release/evm-cli
      - name: Sha256
        run: sha256sum x.tar.gz > x.sha256
      - name: Release
        uses: softprops/action-gh-release@v2
"#;
    let err =
        release::validate_release_workflow_yaml(bad).expect_err("missing `Upload` step must error");
    assert!(
        err.to_string().to_lowercase().contains("upload"),
        "error should mention Upload, got: {err}"
    );
}

#[test]
fn validate_release_workflow_yaml_rejects_garbage() {
    let err = release::validate_release_workflow_yaml("not: [valid: yaml")
        .expect_err("garbage must error");
    let _ = err;
}
