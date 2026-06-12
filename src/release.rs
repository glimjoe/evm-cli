// SPDX-License-Identifier: MIT
//
// M5 release-engineering helpers.
//
// Per PLAN-V9 §5 M5 DoD:
//   - artifacts named `evm-cli-v0.2.0-linux-x86_64.tar.gz` + `.sha256`
//   - release notes include CHANGELOG + security disclaimer + known limits
//   - `.github/workflows/release.yml` carries the build / sha256 / upload
//     / release steps
//
// This module is the testable surface for the release pipeline. The
// GitHub Actions YAML at `.github/workflows/release.yml` is validated
// against this module's `validate_release_workflow_yaml` so that a
// human edit to the workflow cannot silently drop a required step
// (build / sha256 / upload / release).
//
// TDD status: GREEN. 20 integration tests (`tests/it_release.rs`) +
// 4 artifact-file tests (`tests/it_release_workflow.rs`) + 4 lib unit
// tests for `ReleaseVersion` all pass. See CHANGELOG 0.2.0 M5 entry.

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests legitimately use `.expect()` / `.unwrap()` on
// fixed inputs (e.g. `ReleaseVersion::new("0.2.0")` round-trips).
// Production paths must not trip `clippy::disallowed_methods` (P0-4).

use std::fmt;

/// A versioned release (e.g. `0.2.0`). Lightweight newtype so that
/// callers can't accidentally pass arbitrary strings to artifact-name
/// helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseVersion(String);

impl ReleaseVersion {
    pub fn new(v: &str) -> Result<Self, VersionError> {
        let s = v.trim();
        if s.is_empty() {
            return Err(VersionError::Empty);
        }
        // Must look like X.Y.Z with optional pre-release / build suffix
        // (SemVer 2.0.0 core form: major.minor.patch).
        let mut parts = s.split('.');
        let major = parts.next().ok_or(VersionError::Malformed)?;
        let minor = parts.next().ok_or(VersionError::Malformed)?;
        let patch = parts.next().ok_or(VersionError::Malformed)?;
        if parts.next().is_some() {
            return Err(VersionError::Malformed);
        }
        for part in [major, minor, patch] {
            if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
                return Err(VersionError::Malformed);
            }
        }
        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VersionError {
    #[error("version string is empty")]
    Empty,
    #[error("version string is not in X.Y.Z form")]
    Malformed,
}

/// Errors from changelog extraction.
#[derive(Debug, thiserror::Error)]
pub enum ChangelogError {
    #[error("version `{0}` not found in changelog")]
    VersionNotFound(String),
    #[error("changelog has no `## [{0}]` section header (only `Unreleased` is present)")]
    NotAVersion(String),
}

/// Errors from release-workflow YAML validation.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow is missing required step: `{0}`")]
    MissingStep(String),
}

/// Build the artifact file name: `evm-cli-v<ver>-<platform>.tar.gz`.
pub fn artifact_name(version: &str, target: &str) -> String {
    format!("evm-cli-v{}-{}.tar.gz", version, platform_tag(target))
}

/// Build the SHA256 sidecar file name: `evm-cli-v<ver>-<platform>.sha256`.
pub fn sha256_sidecar(version: &str, target: &str) -> String {
    format!("evm-cli-v{}-{}.sha256", version, platform_tag(target))
}

/// Normalize a Rust target triple to the release archive's platform tag.
/// `x86_64-unknown-linux-gnu` -> `linux-x86_64`
/// `aarch64-unknown-linux-musl` -> `linux-aarch64`
pub fn platform_tag(target: &str) -> String {
    // Target triple shape: <arch>-<vendor>-<os>-<abi>. We only care
    // about the first and third dash-separated components; vendor and
    // abi are irrelevant to the artifact's platform tag.
    let parts: Vec<&str> = target.split('-').collect();
    if parts.len() < 3 {
        return target.to_string();
    }
    format!("{}-{}", parts[2], parts[0])
}

/// Extract the body of the `## [<version>]` section from a changelog.
/// Returns the body (everything after the heading line, up to the
/// next `## [...]` heading), trimmed.
pub fn extract_changelog_section(changelog: &str, version: &str) -> Result<String, ChangelogError> {
    if version.eq_ignore_ascii_case("Unreleased") {
        return Err(ChangelogError::NotAVersion(version.to_string()));
    }
    let header = format!("## [{version}]");
    let lines: Vec<&str> = changelog.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&header))
        .ok_or_else(|| ChangelogError::VersionNotFound(version.to_string()))?;
    let body_start = start + 1;
    let body_end = lines[body_start..]
        .iter()
        .position(|l| l.trim_start().starts_with("## ["))
        .map(|i| body_start + i)
        .unwrap_or(lines.len());
    Ok(lines[body_start..body_end].join("\n").trim().to_string())
}

/// Render the release notes that go into the GitHub Release body.
/// Sections are concatenated in the order: changelog, disclaimer, limits.
pub fn render_release_notes(
    changelog_section: &str,
    security_disclaimer: &str,
    known_limitations: &str,
) -> String {
    format!("{changelog_section}\n\n{security_disclaimer}\n\n{known_limitations}")
}

/// Required step names in `.github/workflows/release.yml`. The
/// validator asserts that each appears as a `name: <Step>` line so
/// that a human edit cannot silently drop a critical step.
const REQUIRED_WORKFLOW_STEPS: &[&str] = &["Build", "Sha256", "Upload", "Release"];

/// Validate that a release workflow YAML contains the four required
/// step names: `Build`, `Sha256`, `Upload`, `Release`.
pub fn validate_release_workflow_yaml(yaml: &str) -> Result<(), WorkflowError> {
    for required in REQUIRED_WORKFLOW_STEPS {
        // Match the canonical step-item form: `- name: <Step>`.
        // (Workflows' `steps:` block is a YAML list, so step items
        // are list items and must carry the `- ` prefix. Workflow-
        // level `name:` headers don't.) This avoids false positives
        // when a workflow's own `name: Release` header happens to
        // share a string with a required step name.
        let needle = format!("- name: {required}");
        let found = yaml.lines().any(|l| l.trim_start().contains(&needle));
        if !found {
            return Err(WorkflowError::MissingStep((*required).to_string()));
        }
    }
    Ok(())
}

// ─── Tests for the newtypes (white-box, also RED) ──────────────────

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn release_version_accepts_canonical() {
        assert_eq!(ReleaseVersion::new("0.2.0").unwrap().as_str(), "0.2.0");
        assert_eq!(ReleaseVersion::new("1.0.0").unwrap().as_str(), "1.0.0");
    }

    #[test]
    fn release_version_rejects_empty() {
        assert_eq!(ReleaseVersion::new("").unwrap_err(), VersionError::Empty);
        assert_eq!(ReleaseVersion::new("   ").unwrap_err(), VersionError::Empty);
    }

    #[test]
    fn release_version_rejects_malformed() {
        assert_eq!(
            ReleaseVersion::new("0.2").unwrap_err(),
            VersionError::Malformed
        );
        assert_eq!(
            ReleaseVersion::new("0.2.0.0").unwrap_err(),
            VersionError::Malformed
        );
        assert_eq!(
            ReleaseVersion::new("0.2.0-beta").unwrap_err(),
            VersionError::Malformed
        );
        assert_eq!(
            ReleaseVersion::new("v0.2.0").unwrap_err(),
            VersionError::Malformed
        );
        assert_eq!(
            ReleaseVersion::new("0.2.x").unwrap_err(),
            VersionError::Malformed
        );
    }

    #[test]
    fn release_version_display() {
        let v = ReleaseVersion::new("0.2.0").expect("canonical version must parse");
        assert_eq!(v.to_string(), "0.2.0");
    }
}
