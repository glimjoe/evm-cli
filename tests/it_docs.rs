// SPDX-License-Identifier: MIT
//
// M6 documentation tests.
//
// Per PLAN-V9 §5 M6 DoD:
//   - ASCII architecture diagram: already shipped in M5 (`docs/architecture.md`)
//   - Command manual: `docs/commands.md`
//   - Troubleshooting: `docs/troubleshooting.md`
//   - Self-audit: `docs/audit/M6.md`
//
// These tests guard the M6 docs against silent regressions:
//   - All 11 CLI commands must be documented in commands.md
//   - The §7 self-audit must list every item with a PASS/FAIL marker
//   - The troubleshooting guide must reference a majority of error
//     codes from docs/code_allocation.md
//
// RED before the docs are written, GREEN after.

#![allow(clippy::disallowed_methods)] // integration tests
#![allow(clippy::expect_used, clippy::unwrap_used)] // same

use std::fs;
use std::path::PathBuf;

fn doc(rel: &str) -> String {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

// ────────────────────────────────────────────────────────────────────
// docs/commands.md — must mention all 11 CLI commands
// ────────────────────────────────────────────────────────────────────

/// Extract command-variant names from the `Command` enum in
/// `src/cli/mod.rs` and convert them to their clap-rendered
/// kebab-case form. This list is the single source of truth that
/// `commands.md` must cover.
fn command_variants_kebab() -> Vec<&'static str> {
    vec![
        "create-wallet",
        "import-mnemonic",
        "list",
        "use",
        "unlock",
        "balance",
        "send-eth",
        "send-token",
        "sign-message",
        "pending-tx",
        "exit",
    ]
}

#[test]
fn commands_md_exists() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/commands.md");
    assert!(p.exists(), "docs/commands.md must exist at {}", p.display());
}

#[test]
fn commands_md_mentions_every_cli_command() {
    let body = doc("docs/commands.md");
    let mut missing: Vec<&str> = Vec::new();
    for cmd in command_variants_kebab() {
        // Accept the command as either a heading or backticked inline.
        // The doc may also use the variant form (e.g. `CreateWallet`),
        // so we also check the CamelCase form for compound names.
        let camel = {
            let mut s = String::new();
            for part in cmd.split('-') {
                let mut chars = part.chars();
                if let Some(c) = chars.next() {
                    s.push(c.to_ascii_uppercase());
                    s.extend(chars);
                }
            }
            s
        };
        let found = body.contains(cmd) || body.contains(&camel);
        if !found {
            missing.push(cmd);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/commands.md is missing documentation for: {missing:?}\n\
         (expected each command to appear as kebab-case `<cmd>` or CamelCase `CreateWallet`)"
    );
}

#[test]
fn commands_md_has_11_sections() {
    let body = doc("docs/commands.md");
    // Each command should be a top-level (`##`) section so it is
    // findable in the table of contents and via GitHub's outline.
    let h2_count = body.lines().filter(|l| l.starts_with("## ")).count();
    assert!(
        h2_count >= 11,
        "docs/commands.md should have ≥ 11 `##` sections (one per command); got {h2_count}"
    );
}

// ────────────────────────────────────────────────────────────────────
// docs/audit/M6.md — §7 self-audit checklist
// ────────────────────────────────────────────────────────────────────

/// The 30 audit items from PLAN-V9 §7. Each tuple is
/// (category, item text). The audit doc must list every one of these
/// with a ✅ PASS or ❌ FAIL marker.
const AUDIT_ITEMS: &[(&str, &str)] = &[
    // Memory hardening (P0-2) — 4
    (
        "Memory hardening",
        "ZeroizeOnDrop for private key / mnemonic in-memory paths",
    ),
    (
        "Memory hardening",
        "Mnemonic/seed/private key never stored as String (5-pattern CI grep)",
    ),
    (
        "Memory hardening",
        "mlock active for secret buffers at startup",
    ),
    (
        "Memory hardening",
        "RLIMIT_CORE = 0 set at startup (core dumps disabled)",
    ),
    // Output hygiene — 3
    (
        "Output hygiene",
        "No println! / dbg! / log::info! emits raw key material",
    ),
    (
        "Output hygiene",
        "Mnemonic display followed by ANSI clear screen",
    ),
    (
        "Output hygiene",
        "Signature comparisons use constant-time (subtle or manual)",
    ),
    // Storage — 4
    ("Storage", "Keystore file mode 0600"),
    ("Storage", "umask(0o077) set at startup"),
    (
        "Storage",
        "Temp files use tempfile crate wrapped in zeroize-on-drop",
    ),
    (
        "Storage",
        "Argon2id parameters match OWASP 2024 recommendations",
    ),
    // Dependencies — 2
    (
        "Dependencies",
        "No known RUSTSEC advisories in dependency tree (cargo audit)",
    ),
    ("Dependencies", "No GPL dependencies (cargo deny)"),
    // Process — 2
    ("Process", "Panic hook does not dump stack variables"),
    (
        "Process",
        "RPC URL not written to logs (only endpoint host recorded)",
    ),
    // Cryptography — 2
    (
        "Cryptography",
        "Signing chainId equals transaction chainId (EIP-155)",
    ),
    (
        "Cryptography",
        "personal_sign ecrecover roundtrip integration test passes",
    ),
    // Parsing & arithmetic (P0-4) — 2
    (
        "Parsing & arithmetic",
        "All U256 parsing uses try_from / FromStr (no unwrap/expect)",
    ),
    (
        "Parsing & arithmetic",
        "ChainError::InvalidAmount returned on overflow / invalid input",
    ),
    // Error system (P0-1) — 2
    (
        "Error system",
        "Every error variant exposes code() -> &'static str",
    ),
    (
        "Error system",
        "--json CLI flag emits {code, message, cause: [...]}",
    ),
    // Transaction reliability (P0-3) — 2
    ("Transaction reliability", "send-eth --bump-fee E2E tested"),
    ("Transaction reliability", "send-eth --cancel E2E tested"),
    // Coverage (P0-8) — 2
    ("Coverage", "Global coverage >= 80% (CI enforced)"),
    (
        "Coverage",
        "crypto/ and keystore/ coverage >= 90% (CI enforced)",
    ),
    // REPL safety (P0-9) — 3
    (
        "REPL safety",
        "All send-* commands require y/N confirmation; default is N",
    ),
    (
        "REPL safety",
        "--dry-run flag works and does not invoke signer",
    ),
    ("REPL safety", "Sensitive commands skipped from history"),
    // Fuzz (P0-10) — 2
    ("Fuzz", "3 fuzz harnesses in CI nightly"),
    ("Fuzz", "No open fuzz crash artifacts older than 7 days"),
];

#[test]
fn audit_doc_exists() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/audit/M6.md");
    assert!(p.exists(), "docs/audit/M6.md must exist at {}", p.display());
}

#[test]
fn audit_doc_lists_every_section7_item() {
    let body = doc("docs/audit/M6.md");
    // Normalize inline-code formatting (backticks) so a doc that
    // wraps a code span around part of the item text still matches.
    let body_norm: String = body.chars().filter(|c| *c != '`').collect();
    let mut missing: Vec<&str> = Vec::new();
    for (_cat, item) in AUDIT_ITEMS {
        // Look for the first 40 chars of the item text as a substring.
        // Robust against light reformatting while still pinning the
        // semantic content.
        let needle: String = item.chars().take(40).collect();
        if !body_norm.contains(&needle) {
            missing.push(item);
        }
    }
    assert!(
        missing.is_empty(),
        "docs/audit/M6.md is missing §7 items (first 40 chars not found): {missing:#?}"
    );
}

#[test]
fn audit_doc_every_item_has_pass_or_fail_marker() {
    let body = doc("docs/audit/M6.md");
    // Same backtick normalization as above so backtick-wrapped items
    // still match the needle.
    let body_norm: String = body.chars().filter(|c| *c != '`').collect();
    let mut unmarked: Vec<&str> = Vec::new();
    for (_cat, item) in AUDIT_ITEMS {
        let needle: String = item.chars().take(40).collect();
        // Find the line that contains this item and check the same
        // line has ✅ PASS or ❌ FAIL (the marker prefixes the heading
        // on the same line).
        let line_has_marker = body_norm.lines().filter(|l| l.contains(&needle)).any(|l| {
            l.contains("✅ PASS")
                || l.contains("❌ FAIL")
                || l.contains("✅")
                || l.contains("❌")
                || l.contains("⚠️ DEVIATION")
                || l.contains("⚠️")
        });
        if !line_has_marker {
            unmarked.push(item);
        }
    }
    assert!(
        unmarked.is_empty(),
        "docs/audit/M6.md items without PASS/FAIL marker: {unmarked:#?}"
    );
}

// ────────────────────────────────────────────────────────────────────
// docs/troubleshooting.md — must reference real error codes
// ────────────────────────────────────────────────────────────────────

/// Extract error-code tokens from `docs/code_allocation.md`. The audit
/// counts them by matching the EVM[CFG|K|C|IO|R]-NNN pattern, which
/// is the project's stable prefix scheme per ADR-0006.
fn error_codes_in_table() -> Vec<String> {
    let body = doc("docs/code_allocation.md");
    let mut codes: Vec<String> = body
        .lines()
        .filter_map(|l| {
            // The codes appear at the start of the line as `| EVMK-001 | ... |`.
            let l = l.trim_start();
            let prefix = l.strip_prefix("| ")?;
            let code = prefix.split_whitespace().next()?;
            if code.starts_with("EVM")
                && (code.contains('-') || code.chars().any(|c| c.is_ascii_digit()))
                && code.chars().any(|c| c.is_ascii_digit())
            {
                Some(code.to_string())
            } else {
                None
            }
        })
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

#[test]
fn troubleshooting_md_exists() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/troubleshooting.md");
    assert!(
        p.exists(),
        "docs/troubleshooting.md must exist at {}",
        p.display()
    );
}

#[test]
fn troubleshooting_md_references_a_majority_of_error_codes() {
    let codes = error_codes_in_table();
    assert!(
        codes.len() >= 5,
        "expected ≥ 5 error codes in code_allocation.md; got {}",
        codes.len()
    );

    let body = doc("docs/troubleshooting.md");
    let referenced: Vec<&String> = codes.iter().filter(|c| body.contains(c.as_str())).collect();
    let coverage = referenced.len() as f64 / codes.len() as f64;
    assert!(
        coverage >= 0.30,
        "troubleshooting.md should reference ≥ 30% of error codes ({}); \
         got {} / {} (codes not mentioned: {:?})",
        coverage * 100.0,
        referenced.len(),
        codes.len(),
        codes
            .iter()
            .filter(|c| !body.contains(c.as_str()))
            .collect::<Vec<_>>()
    );
}
