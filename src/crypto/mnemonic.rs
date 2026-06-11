// SPDX-License-Identifier: MIT
//
// BIP-39 mnemonic generation and validation.
//
// V8 §5 M1 DoD: "12/24-word mnemonic generation".
//
// We delegate to `coins_bip39::Mnemonic` (transitive via alloy, also
// a direct dep) for the wordlist and entropy. The thin wrapper here:
//   1. Returns a `Secret<String>` (per ADR-0007 — mnemonic is secret
//      material; never stored as plain `String`).
//   2. Validates the user-supplied phrase with a clear error.
//   3. Applies our ANSI-clear-on-display policy (P0-7).
//
// The Secret wrapper means the mnemonic is zeroized on drop. Callers
// must use `.expose_secret()` to display it, and should follow the
// display policy immediately.

use std::fmt;

use thiserror::Error;

use crate::types::secret::Secret;

/// Word-count options supported by V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCount {
    /// 12-word mnemonic (128 bits of entropy).
    Twelve = 12,
    /// 24-word mnemonic (256 bits of entropy).
    TwentyFour = 24,
}

impl WordCount {
    fn as_usize(self) -> usize {
        self as usize
    }
}

/// Errors from mnemonic operations.
#[derive(Debug, Error)]
pub enum MnemonicError {
    #[error("invalid mnemonic phrase: {0}")]
    InvalidPhrase(String),
    #[error("unsupported word count: {0} (must be 12 or 24)")]
    UnsupportedWordCount(usize),
    #[error("alloy internal error: {0}")]
    Alloy(String),
}

/// Generate a fresh random mnemonic with the given word count.
///
/// The returned `Secret<String>` zeroizes on drop (per ADR-0007). Use
/// `.expose_secret()` to read the phrase, and follow the display
/// policy (`display_and_clear`) immediately after.
pub fn generate(count: WordCount) -> Result<Secret<String>, MnemonicError> {
    use coins_bip39::{English, Mnemonic};

    let mnemonic = Mnemonic::<English>::new_with_count(&mut rand::thread_rng(), count.as_usize())
        .map_err(|e| MnemonicError::Alloy(e.to_string()))?;
    Ok(Secret::new(mnemonic.to_phrase()))
}

/// Validate a user-supplied phrase and return it wrapped in `Secret`.
///
/// Returns `Err(MnemonicError::InvalidPhrase)` if the phrase is not a
/// valid BIP-39 mnemonic (wrong word count, invalid checksum, words
/// not in the English wordlist, etc.).
pub fn validate(phrase: &str) -> Result<Secret<String>, MnemonicError> {
    use coins_bip39::{English, Mnemonic};

    Mnemonic::<English>::new_from_phrase(phrase)
        .map(|_| Secret::new(phrase.to_string()))
        .map_err(|e| MnemonicError::InvalidPhrase(e.to_string()))
}

/// Word count extracted from a phrase (helper for validation).
pub fn word_count(phrase: &str) -> usize {
    phrase.split_whitespace().count()
}

impl fmt::Display for WordCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Twelve => f.write_str("12 words"),
            Self::TwentyFour => f.write_str("24 words"),
        }
    }
}

/// Display a mnemonic to stderr, followed by an ANSI clear screen
/// (P0-7 hardening). The mnemonic is read from `Secret` via
/// `expose_secret()` but is wiped from the terminal scrollback.
///
/// This function never returns the mnemonic to a caller — it is a
/// "fire and forget" display. Callers wanting the mnemonic for
/// re-import should use `Secret::expose_secret()` directly.
pub fn display_and_clear(secret: &Secret<String>) {
    // SAFETY: displaying on stderr to avoid stdout being piped/recorded.
    // The ANSI clear screen below is best-effort: terminal emulators
    // respect it, dumb terminals ignore it. Plain `script` recordings
    // may still capture the phrase; that is a known limitation of P0-7.
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════╗");
    eprintln!("║              NEW MNEMONIC — WRITE THIS DOWN               ║");
    eprintln!("╠══════════════════════════════════════════════════════════╣");
    eprintln!("║  {}", secret.expose_secret());
    eprintln!("╚══════════════════════════════════════════════════════════╝");
    eprintln!();
    eprintln!("请立即抄写并关闭窗口 (write this down and close the window).");
    eprintln!("The next ANSI escape will clear the screen.");

    // ANSI clear screen + cursor home.
    // \x1b[2J  — clear entire screen
    // \x1b[H   — move cursor to home (row 0, col 0)
    eprint!("\x1b[2J\x1b[H");
    // Flush so the escape reaches the terminal before main exits.
    use std::io::Write;
    let _ = std::io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_12_words() {
        let m = generate(WordCount::Twelve).expect("generate 12-word");
        let s = m.expose_secret();
        assert_eq!(word_count(s), 12);
    }

    #[test]
    fn generate_24_words() {
        let m = generate(WordCount::TwentyFour).expect("generate 24-word");
        let s = m.expose_secret();
        assert_eq!(word_count(s), 24);
    }

    #[test]
    fn validate_well_known_phrase() {
        // BIP-39 official test vector #1.
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let m = validate(phrase).expect("well-known phrase should validate");
        assert_eq!(m.expose_secret(), phrase);
    }

    #[test]
    fn validate_rejects_garbage() {
        let phrase = "not a real mnemonic phrase at all just some words";
        assert!(validate(phrase).is_err());
    }

    #[test]
    fn validate_rejects_wrong_word_count() {
        // 11 words — too short.
        let phrase =
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        assert!(validate(phrase).is_err());
    }

    #[test]
    fn word_count_helper() {
        assert_eq!(
            word_count("abandon abandon abandon abandon abandon abandon"),
            6
        );
    }

    /// The 5 official ethereumbook / Trezor BIP-39 test vectors.
    /// Each must parse as a valid BIP-39 phrase. The corresponding
    /// derived addresses are checked in `address::tests`.
    #[test]
    fn ethereumbook_5_vectors_validate() {
        let vectors = [
            // 1.
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            // 2.
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            // 3.
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
            // 4.
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
            // 5. (24 words)
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
        ];
        for v in &vectors {
            let m = validate(v).unwrap_or_else(|e| panic!("vector should validate: {v}\n  error: {e}"));
            assert_eq!(m.expose_secret(), v, "round-trip preserves phrase");
        }
    }
}
