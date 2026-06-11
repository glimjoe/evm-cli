// SPDX-License-Identifier: MIT
//
// evm_cli::keystore — encrypted wallet file storage.
//
// Per V8 §5 M2 DoD and V10 §19 (M2 deviation from V8 spec):
//   - Keystore file location: `<data_dir>/keystore/<alias>` (no extension;
//     the alias is the filename as written by eth-keystore).
//   - File format: standard Ethereum JSON keystore (EIP-2335 / EIP-1081).
//     Uses **scrypt** KDF + **AES-128-CTR** cipher + Keccak-256 MAC.
//     This is the format produced by `geth`, `ethers.js`, `MyEtherWallet`,
//     and `alloy_signer_local::PrivateKeySigner::encrypt_keystore`.
//   - Deviations from V8 §5 M2 original spec: V8 said Argon2id +
//     AES-256-GCM (non-interoperable); V10 (and this code) uses
//     standard eth-keystore for interoperability. See V10 §19.
//   - API: create / load / list / delete / rename
//   - Anti-side-channel: decrypt failure returns `InvalidPassword`
//     whether the file is missing or the password is wrong. (Per V8
//     §5 M2 DoD; anti-side-channel is more important than UX in V1.)
//   - File mode: 0600 (umask 0o077 set in main(); see ADR-0007 rev1)
//
// Per ADR-0003 (workspace split), this module depends only on:
//   - `types` (for Secret)
//   - external crates: alloy, eth-keystore, directories, serde, serde_json
//   - NOT `chain`, NOT `cli` (those come in M3+).
//
// File naming:
//   The eth-keystore 0.5.0 API writes the file at `dir/<name>` where
//   `name` is either a user-supplied string (when `Some(name)` is
//   passed) or a generated UUID. We pass `Some(alias)`, so the file
//   is literally named after the alias. This makes `rename` a
//   filesystem rename — clean and simple.

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests legitimately use `.expect()` / `.unwrap()` on
// fixed inputs (`tempdir()`, `Secret::new("...")`-style fakes). Production
// paths must not trip `clippy::disallowed_methods` (P0-4).

use std::fs;
use std::path::{Path, PathBuf};

use alloy_primitives::Address;
use alloy_signer_local::{coins_bip39::English, MnemonicBuilder, PrivateKeySigner};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

use crate::crypto::mnemonic::{self, MnemonicError, WordCount};
use crate::types::secret::Secret;

/// Errors from keystore operations. Per V8 §5 M2 DoD, decrypt failures
/// return `InvalidPassword` whether the file is missing or the password
/// is wrong. Other variants surface only for genuine internal errors
/// (corrupted JSON, file-system issues, etc.).
#[derive(Debug, Error)]
pub enum KeystoreError {
    /// Anti-side-channel: returned for both "file missing" and
    /// "password wrong". The caller cannot tell which.
    #[error("invalid password (or file missing — see ADR-0007 anti-side-channel)")]
    InvalidPassword,

    /// File exists but is not a valid JSON keystore (corruption).
    #[error("keystore file is corrupted (not a valid JSON keystore)")]
    FileCorrupted,

    /// Alias not found.
    #[error("alias not found: {0}")]
    AliasNotFound(String),

    /// Alias already in use (collision on create/rename).
    #[error("alias already exists: {0}")]
    AliasExists(String),

    /// I/O error other than file-missing (permission, disk full, ...).
    #[error("keystore I/O error: {0}")]
    Io(String),

    /// Other internal error (alloy / eth-keystore / BIP-39 / KDF).
    #[error("keystore internal error: {0}")]
    Internal(String),
}

impl From<std::io::Error> for KeystoreError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<alloy_signer_local::LocalSignerError> for KeystoreError {
    fn from(e: alloy_signer_local::LocalSignerError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<serde_json::Error> for KeystoreError {
    fn from(_e: serde_json::Error) -> Self {
        Self::FileCorrupted // EVMK-002
    }
}

impl From<MnemonicError> for KeystoreError {
    fn from(e: MnemonicError) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Stable error code mapping per ADR-0006 rev1 + ADR-0009 (forthcoming).
/// Codes are listed in `docs/code_allocation.md` and CI-enforced by the
/// `all_codes_are_documented_in_code_allocation` test in `src/error.rs`.
impl crate::error::CodeSource for KeystoreError {
    fn code(&self) -> &'static str {
        match self {
            Self::InvalidPassword => "EVMK-001",
            Self::FileCorrupted => "EVMK-002",
            Self::AliasNotFound(_) => "EVMK-009",
            Self::AliasExists(_) => "EVMK-010",
            Self::Io(_) => "EVMK-011",
            Self::Internal(_) => "EVMK-012",
        }
    }
}

/// Summary of a wallet entry, used by `list()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    /// User-assigned alias.
    pub alias: String,
    /// Derived Ethereum address (EIP-55 mixed case).
    pub address: Address,
}

/// A keystore bound to a specific directory. Construct with
/// `KeystoreStore::open(dir)` (uses `directories` crate) or
/// `KeystoreStore::open_at(path)` (for tests).
pub struct KeystoreStore {
    /// Directory holding `<alias>` keystore files.
    dir: PathBuf,
}

impl KeystoreStore {
    /// Open the default keystore directory
    /// (`~/.local/share/evm-cli/keystore` on Linux, per XDG Base Directory spec).
    pub fn open() -> Result<Self, KeystoreError> {
        let dir = directories::ProjectDirs::from("local", "evm-cli", "evm-cli")
            .ok_or_else(|| KeystoreError::Internal("cannot determine data directory".to_string()))?
            .data_dir()
            .join("keystore");
        Self::open_at(dir)
    }

    /// Open a specific directory. Creates it if it doesn't exist.
    pub fn open_at(dir: PathBuf) -> Result<Self, KeystoreError> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Directory backing this store.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    // ────────────────────────────────────────────────────────────────
    // Path helpers
    // ────────────────────────────────────────────────────────────────

    /// File path for a given alias. No extension per eth-keystore convention.
    fn path_for(&self, alias: &str) -> PathBuf {
        self.dir.join(alias)
    }

    /// True iff the alias's file exists.
    fn exists(&self, alias: &str) -> bool {
        self.path_for(alias).exists()
    }

    // ────────────────────────────────────────────────────────────────
    // CRUD
    // ────────────────────────────────────────────────────────────────

    /// Create a new wallet: generate a random mnemonic, derive the
    /// key, encrypt with `password`, save to a new file.
    ///
    /// Returns the `PrivateKeySigner` (already loaded) so the caller
    /// can immediately use it without re-decrypting.
    pub fn create(
        &self,
        alias: &str,
        password: &Secret<String>,
        word_count: WordCount,
    ) -> Result<PrivateKeySigner, KeystoreError> {
        if self.exists(alias) {
            return Err(KeystoreError::AliasExists(alias.to_string()));
        }
        // 1. Generate mnemonic.
        let phrase = mnemonic::generate(word_count)?;
        // 2. Build signer at index 0. Wrap the cloned phrase in
        //    `Zeroizing<String>` so our local copy is wiped on drop
        //    (the alloy `MnemonicBuilder` retains an internal
        //    `Option<String>` we cannot reach; see ADR-0009 §"Known
        //    residual risk").
        let z_phrase: Zeroizing<String> = Zeroizing::new(phrase.expose_secret().clone());
        let signer = MnemonicBuilder::<English>::default()
            .phrase(&*z_phrase)
            .derivation_path("m/44'/60'/0'/0/0")?
            .build()?;
        drop(z_phrase);
        // 3. Encrypt + save via alloy (file = alias, no extension).
        self.persist(&signer, alias, password)?;
        Ok(signer)
    }

    /// Import a wallet from an existing mnemonic.
    pub fn import(
        &self,
        alias: &str,
        password: &Secret<String>,
        phrase: &str,
    ) -> Result<PrivateKeySigner, KeystoreError> {
        if self.exists(alias) {
            return Err(KeystoreError::AliasExists(alias.to_string()));
        }
        let _validated = mnemonic::validate(phrase)?;
        // Same mitigation as `create`: wrap the caller-supplied phrase
        // in `Zeroizing<String>` so our local copy is wiped on drop.
        let z_phrase: Zeroizing<String> = Zeroizing::new(phrase.to_string());
        let signer = MnemonicBuilder::<English>::default()
            .phrase(&*z_phrase)
            .derivation_path("m/44'/60'/0'/0/0")?
            .build()?;
        drop(z_phrase);
        self.persist(&signer, alias, password)?;
        Ok(signer)
    }

    /// Load (unlock) a wallet by alias and password.
    ///
    /// **Anti-side-channel** (per V8 §5 M2 DoD): if the file is missing,
    /// this returns `InvalidPassword` — the same error as a wrong password.
    /// The caller cannot distinguish "no such alias" from "wrong password"
    /// by the error type. (For the same reason, `list()` is the only way
    /// to discover which aliases exist.)
    pub fn load(
        &self,
        alias: &str,
        password: &Secret<String>,
    ) -> Result<PrivateKeySigner, KeystoreError> {
        if !self.exists(alias) {
            return Err(KeystoreError::InvalidPassword); // file-missing → swallow
        }
        let path = self.path_for(alias);
        // Delegate to alloy's decrypt_keystore. It returns LocalSignerError
        // for both wrong-password (MacMismatch) and io errors. We collapse
        // all of them to InvalidPassword per the anti-side-channel rule.
        match PrivateKeySigner::decrypt_keystore(&path, password.expose_secret().as_bytes()) {
            Ok(signer) => Ok(signer),
            Err(_) => Err(KeystoreError::InvalidPassword),
        }
    }

    /// Strict variant of `load` that returns `AliasNotFound` for a
    /// missing alias. Used by internal flows that need to distinguish.
    /// For user-facing unlock, prefer `load` (anti-side-channel).
    pub fn load_strict(
        &self,
        alias: &str,
        password: &Secret<String>,
    ) -> Result<PrivateKeySigner, KeystoreError> {
        if !self.exists(alias) {
            return Err(KeystoreError::AliasNotFound(alias.to_string()));
        }
        let path = self.path_for(alias);
        let signer =
            PrivateKeySigner::decrypt_keystore(&path, password.expose_secret().as_bytes())?;
        Ok(signer)
    }

    /// List all wallets (alias, address).
    ///
    /// **No anti-side-channel here** — this is the canonical way to
    /// discover which aliases exist. The caller (CLI) chooses whether
    /// to call this.
    ///
    /// Reading the address requires decrypting each file, which
    /// needs a password. V1 returns `Address::ZERO` as a placeholder;
    /// M4 (CLI) will decrypt on demand. Alternatively, the address
    /// can be cached in a sidecar file (out of scope for M2).
    pub fn list(&self) -> Result<Vec<WalletInfo>, KeystoreError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // The eth-keystore writes a single JSON object per file.
            // We treat all non-`.json` extension files as keystores too
            // (eth-keystore writes with no extension by default).
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    continue; // skip hidden files
                }
                out.push(WalletInfo {
                    alias: name.to_string(),
                    address: Address::ZERO, // placeholder; see doc above
                });
            }
        }
        out.sort_by(|a, b| a.alias.cmp(&b.alias));
        Ok(out)
    }

    /// Delete a wallet by alias. Removes the keystore file. The
    /// function does NOT verify the password (the caller is assumed
    /// authorized; for password-protected delete, use `load + delete`).
    pub fn delete(&self, alias: &str) -> Result<(), KeystoreError> {
        if !self.exists(alias) {
            return Err(KeystoreError::AliasNotFound(alias.to_string()));
        }
        fs::remove_file(self.path_for(alias))?;
        Ok(())
    }

    /// Rename a wallet's alias. This is a filesystem rename of the
    /// keystore file. The underlying key (and address) is unchanged.
    /// NonceManager keys on Address, so this does NOT affect the
    /// nonce pool (per ADR-0003 rev1).
    pub fn rename(&self, old: &str, new: &str) -> Result<(), KeystoreError> {
        if old == new {
            return Ok(());
        }
        if !self.exists(old) {
            return Err(KeystoreError::AliasNotFound(old.to_string()));
        }
        if self.exists(new) {
            return Err(KeystoreError::AliasExists(new.to_string()));
        }
        // Atomic rename within the same filesystem.
        fs::rename(self.path_for(old), self.path_for(new))?;
        Ok(())
    }

    // ────────────────────────────────────────────────────────────────
    // Internals
    // ────────────────────────────────────────────────────────────────

    /// Persist a signer under the given alias. The file is written
    /// at `dir/<alias>` (no extension) by eth-keystore.
    fn persist(
        &self,
        signer: &PrivateKeySigner,
        alias: &str,
        password: &Secret<String>,
    ) -> Result<(), KeystoreError> {
        let mut rng = rand::thread_rng();
        let (_signer, _uuid) = alloy_signer_local::PrivateKeySigner::encrypt_keystore(
            &self.dir,
            &mut rng,
            signer.to_bytes(),
            password.expose_secret().as_bytes(),
            Some(alias),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, KeystoreStore) {
        let dir = tempfile::tempdir().expect("create tempdir");
        let store = KeystoreStore::open_at(dir.path().to_path_buf()).expect("open store");
        (dir, store)
    }

    fn pw() -> Secret<String> {
        Secret::new("correct horse battery staple".to_string())
    }

    /// M2 DoD: "create → simulate restart → unlock → same address recovered".
    #[test]
    fn create_then_load_returns_same_address() {
        let (dir, store) = temp_store();
        let signer1 = store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");
        let addr1 = signer1.address();

        // Simulate restart: drop the store, create a new one pointing
        // at the same directory.
        drop(store);
        let store2 = KeystoreStore::open_at(dir.path().to_path_buf()).expect("reopen");
        let signer2 = store2.load("main", &pw()).expect("load");
        assert_eq!(signer2.address(), addr1);
    }

    /// Anti-side-channel: wrong password returns `InvalidPassword`.
    #[test]
    fn wrong_password_returns_invalid_password() {
        let (_dir, store) = temp_store();
        store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");

        let bad = Secret::new("wrong password".to_string());
        match store.load("main", &bad) {
            Err(KeystoreError::InvalidPassword) => {} // expected
            other => panic!("expected InvalidPassword, got {other:?}"),
        }
    }

    /// Anti-side-channel: missing alias returns `InvalidPassword` (not
    /// `AliasNotFound` — those are the same from the caller's POV).
    #[test]
    fn missing_alias_returns_invalid_password() {
        let (_dir, store) = temp_store();
        // No create — alias doesn't exist.
        match store.load("ghost", &pw()) {
            Err(KeystoreError::InvalidPassword) => {} // expected
            other => panic!("expected InvalidPassword, got {other:?}"),
        }
    }

    /// List shows all created wallets.
    #[test]
    fn list_returns_created_wallets() {
        let (_dir, store) = temp_store();
        store
            .create("a", &pw(), WordCount::Twelve)
            .expect("create a");
        store
            .create("b", &pw(), WordCount::Twelve)
            .expect("create b");
        let list = store.list().expect("list");
        let aliases: Vec<&str> = list.iter().map(|w| w.alias.as_str()).collect();
        assert!(aliases.contains(&"a"));
        assert!(aliases.contains(&"b"));
        assert_eq!(list.len(), 2);
    }

    /// Delete removes the wallet file.
    #[test]
    fn delete_removes_wallet() {
        let (dir, store) = temp_store();
        store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");

        store.delete("main").expect("delete");
        // Verify the file is gone.
        assert!(!store
            .list()
            .expect("list")
            .iter()
            .any(|w| w.alias == "main"));
        // The file system should not have a "main" file.
        assert!(!dir.path().join("main").exists());
    }

    /// Rename changes the alias; the key (and address) is unchanged.
    #[test]
    fn rename_changes_alias_preserves_key() {
        let (_dir, store) = temp_store();
        let signer1 = store
            .create("old", &pw(), WordCount::Twelve)
            .expect("create");
        let addr1 = signer1.address();

        store.rename("old", "new").expect("rename");
        let signer2 = store.load("new", &pw()).expect("load new");
        assert_eq!(signer2.address(), addr1);
        // Old alias no longer works (anti-side-channel → InvalidPassword).
        match store.load("old", &pw()) {
            Err(KeystoreError::InvalidPassword) => {}
            other => panic!("expected InvalidPassword, got {other:?}"),
        }
    }

    /// Rename to an existing alias is rejected.
    #[test]
    fn rename_to_existing_alias_rejected() {
        let (_dir, store) = temp_store();
        store
            .create("a", &pw(), WordCount::Twelve)
            .expect("create a");
        store
            .create("b", &pw(), WordCount::Twelve)
            .expect("create b");
        match store.rename("a", "b") {
            Err(KeystoreError::AliasExists(_)) => {}
            other => panic!("expected AliasExists, got {other:?}"),
        }
    }

    /// Create with a duplicate alias is rejected.
    #[test]
    fn create_duplicate_alias_rejected() {
        let (_dir, store) = temp_store();
        store
            .create("dup", &pw(), WordCount::Twelve)
            .expect("create");
        match store.create("dup", &pw(), WordCount::Twelve) {
            Err(KeystoreError::AliasExists(_)) => {}
            other => panic!("expected AliasExists, got {other:?}"),
        }
    }

    /// Import an existing mnemonic.
    #[test]
    fn import_existing_mnemonic() {
        let (_dir, store) = temp_store();
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let signer = store.import("imported", &pw(), phrase).expect("import");
        // The well-known test vector derives to 0x9858E...
        let addr_lower = format!("{:?}", signer.address()).to_lowercase();
        assert!(addr_lower.starts_with("0x9858e"));
    }

    /// Delete a non-existent alias returns `AliasNotFound`.
    #[test]
    fn delete_missing_alias_returns_not_found() {
        let (_dir, store) = temp_store();
        match store.delete("ghost") {
            Err(KeystoreError::AliasNotFound(_)) => {}
            other => panic!("expected AliasNotFound, got {other:?}"),
        }
    }

    /// `load_strict` distinguishes missing alias from wrong password.
    /// Useful for internal flows (e.g. M3+ when surface "wallet not imported").
    #[test]
    fn load_strict_distinguishes_missing() {
        let (_dir, store) = temp_store();
        // Missing alias → AliasNotFound.
        match store.load_strict("ghost", &pw()) {
            Err(KeystoreError::AliasNotFound(_)) => {}
            other => panic!("expected AliasNotFound, got {other:?}"),
        }
        // Create then wrong password → Internal (alloy error), NOT InvalidPassword.
        store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");
        let bad = Secret::new("nope".to_string());
        match store.load_strict("main", &bad) {
            Err(KeystoreError::Internal(_)) => {} // alloy's MacMismatch
            other => panic!("expected Internal (MacMismatch), got {other:?}"),
        }
    }

    /// Filename has no extension (per eth-keystore convention).
    #[test]
    fn file_has_no_extension() {
        let (dir, store) = temp_store();
        store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");
        // The file is named exactly "main" (no ".json").
        assert!(dir.path().join("main").exists());
        assert!(!dir.path().join("main.json").exists());
    }

    /// P0-1: every `KeystoreError` variant must return a stable EVMK-NNN
    /// code via `CodeSource::code()`. This test pins the mapping per
    /// `docs/code_allocation.md` and is the regression guard for the
    /// M2 review's "two `KeystoreError` types" P0 finding.
    #[test]
    fn code_mapping_matches_code_allocation() {
        use crate::error::CodeSource;

        assert_eq!(KeystoreError::InvalidPassword.code(), "EVMK-001");
        assert_eq!(KeystoreError::FileCorrupted.code(), "EVMK-002");
        assert_eq!(
            KeystoreError::AliasNotFound("ghost".to_string()).code(),
            "EVMK-009"
        );
        assert_eq!(
            KeystoreError::AliasExists("dup".to_string()).code(),
            "EVMK-010"
        );
        assert_eq!(
            KeystoreError::Io("perm denied".to_string()).code(),
            "EVMK-011"
        );
        assert_eq!(
            KeystoreError::Internal("mac mismatch".to_string()).code(),
            "EVMK-012"
        );
    }

    /// `rename(old, old)` is a no-op: the alias is unchanged, the file
    /// is not touched, and `Ok(())` is returned. Covers the early-return
    /// branch at `rename()` (was untested per M2 review §B / §G).
    #[test]
    fn rename_to_same_alias_is_noop() {
        let (dir, store) = temp_store();
        let signer1 = store
            .create("main", &pw(), WordCount::Twelve)
            .expect("create");
        let addr1 = signer1.address();
        let mtime_before = std::fs::metadata(dir.path().join("main"))
            .expect("metadata")
            .modified()
            .expect("mtime");

        // Sleep 10ms to ensure mtime would change on a rewrite.
        std::thread::sleep(std::time::Duration::from_millis(10));

        store.rename("main", "main").expect("rename to same");

        let mtime_after = std::fs::metadata(dir.path().join("main"))
            .expect("metadata after")
            .modified()
            .expect("mtime");
        assert_eq!(
            mtime_before, mtime_after,
            "rename to same alias must not touch the file"
        );
        // Key is still loadable and address unchanged.
        let signer2 = store.load("main", &pw()).expect("load");
        assert_eq!(signer2.address(), addr1);
    }
}
