// SPDX-License-Identifier: MIT
//
// `should_skip_history` predicate (PLAN-V9 §5 M4 DoD, P0-9).
//
// Per the plan: "Sensitive commands (lines containing `mnemonic`,
// `password`, `--private-key`, `import-mnemonic`) skipped from
// `rustyline` history via `should_skip_history(line: &str) -> bool`
// predicate".
//
// The predicate is intentionally name-based and case-insensitive
// (lowercase comparison) so that `Mnemonic = "..."`, `import-mnemonic ...`,
// or any whitespace interleaving is caught. A multi-line entry
// (e.g. an unbalanced quote) is treated as one line — the caller
// is expected to pass each input line independently.

/// Returns `true` if the given input line contains a sensitive
/// token and MUST NOT be added to the rustyline history file.
///
/// Matching is case-insensitive substring. The four patterns
/// (per PLAN-V9 §5 M4 DoD):
///   - `mnemonic`
///   - `password`
///   - `--private-key`
///   - `import-mnemonic`
///
/// The check is conservative: any false positive (a benign line
/// not in history) is a UX cost; a false negative (a sensitive
/// line in history) is a security cost. We err on the side of
/// skipping.
pub fn should_skip_history(line: &str) -> bool {
    let lower = line.to_lowercase();
    SENSITIVE_TOKENS.iter().any(|tok| lower.contains(tok))
}

const SENSITIVE_TOKENS: &[&str] = &["mnemonic", "password", "--private-key", "import-mnemonic"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_mnemonic() {
        assert!(should_skip_history(
            "import-mnemonic main \"my phrase here\""
        ));
        assert!(should_skip_history("MNEMONIC test"));
        assert!(should_skip_history("create-wallet  my-mnemonic-stuff"));
    }

    #[test]
    fn skips_password() {
        // The REPL prompts for the password interactively (not via
        // stdin line) but if a user pastes `password=foo` into a
        // heredoc it must be skipped.
        assert!(should_skip_history("set password hunter2"));
        assert!(should_skip_history("password=hunter2"));
    }

    #[test]
    fn skips_private_key_flag() {
        assert!(should_skip_history("import --private-key 0xdeadbeef"));
        assert!(should_skip_history("--PRIVATE-KEY=0xabc"));
    }

    #[test]
    fn skips_import_mnemonic_subcommand() {
        assert!(should_skip_history("import-mnemonic imported-2026"));
    }

    #[test]
    fn keeps_benign_lines() {
        assert!(!should_skip_history("balance"));
        assert!(!should_skip_history("send-eth 0xabc 0.001"));
        assert!(!should_skip_history("list"));
        assert!(!should_skip_history("use main"));
        assert!(!should_skip_history("exit"));
        // "key" without "private-key" prefix is OK.
        assert!(!should_skip_history("set api-key sk-12345"));
    }

    #[test]
    fn empty_line_kept() {
        assert!(!should_skip_history(""));
    }
}
