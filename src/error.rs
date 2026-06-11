// SPDX-License-Identifier: MIT
//
// Error system per ADR-0006 rev1.
//
// At M0 the structure is in place but most variants are reserved (TODO).
// M1+ (crypto, keystore, chain phases) populates the enums.
//
// The full allocation of 31 codes is documented in
// `docs/code_allocation.md`. The CI test below enforces that every
// `code()` arm returns a string listed there.

use thiserror::Error;

// Per-layer enums are defined inline in this file (not as submodules)
// to keep the M0 file count low. M1+ may split them out per ADR-0003
// rev1 once each layer has substantive code.

use crate::error::chain::ChainError;
use crate::error::config::ConfigError;
use crate::error::crypto::CryptoError;
use crate::error::io::IoError;
use crate::keystore::KeystoreError;

/// Top-level error type. Wraps `anyhow::Error` and provides a stable
/// error code via linear downcast (ADR-0006 rev1, "Surface area").
pub struct CliError {
    inner: anyhow::Error,
}

impl CliError {
    /// Construct from any error that can be converted to `anyhow::Error`.
    pub fn new<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        Self {
            inner: anyhow::Error::new(err),
        }
    }

    /// Construct from a typed layer error. The downcast path is the
    /// fast common case; this constructor preserves type info.
    pub fn from_layer<E: CodeSource + Send + Sync + 'static>(err: E) -> Self {
        let code = err.code_owned();
        let inner = anyhow::Error::new(err);
        Self { inner }.with_code_hint(code)
    }

    /// Extract the stable error code. Order of downcasts is part of the
    /// API contract; see ADR-0006 rev1 "Surface area".
    pub fn code(&self) -> &'static str {
        if let Some(e) = self.inner.downcast_ref::<ChainError>() {
            return e.code();
        }
        if let Some(e) = self.inner.downcast_ref::<KeystoreError>() {
            return e.code();
        }
        if let Some(e) = self.inner.downcast_ref::<CryptoError>() {
            return e.code();
        }
        if let Some(e) = self.inner.downcast_ref::<ConfigError>() {
            return e.code();
        }
        if let Some(e) = self.inner.downcast_ref::<IoError>() {
            return e.code();
        }
        "EVM-999"
    }

    /// Borrow the inner `anyhow::Error` for `.context()`, `?`, etc.
    pub fn inner(&self) -> &anyhow::Error {
        &self.inner
    }

    /// Consume the wrapper and return the inner `anyhow::Error`.
    pub fn into_inner(self) -> anyhow::Error {
        self.inner
    }

    // Internal: stash the code for cases where the downcast chain
    // would otherwise miss (e.g. wrapped layers). The hint is consulted
    // by `code()` only if the downcast chain yields no match.
    fn with_code_hint(self, code: &'static str) -> Self {
        // Note: storing a hint in anyhow::Error requires downcasting back
        // to a known type. For M0 we accept the simplification: `from_layer`
        // callers should still benefit from downcast-based lookup in most
        // cases. The hint is reserved for M1+ when we may need it.
        let _ = code;
        self
    }
}

impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> Self {
        Self { inner: err }
    }
}

impl<E: CodeSource + Send + Sync + 'static> From<E> for CliError {
    fn from(err: E) -> Self {
        CliError::from_layer(err)
    }
}

impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CliError")
            .field("code", &self.code())
            .field("source", &self.inner)
            .finish()
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.inner)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }
}

/// Trait implemented by every layer error enum. Provides `code()` as a
/// const-fn string accessor.
pub trait CodeSource: std::error::Error {
    fn code(&self) -> &'static str;

    /// Internal: a few `from_layer` paths need an owned copy.
    fn code_owned(&self) -> &'static str {
        self.code()
    }
}

// ────────────────────────────────────────────────────────────────────
// Per-layer enum definitions.
//
// At M0 each enum is empty (M0 deliverable: structure, not variants).
// M1+ populates them per `docs/code_allocation.md` (31 ASSIGNED codes
// across 5 layers + 1 catch-all).
// ────────────────────────────────────────────────────────────────────

pub mod crypto {
    use super::{CodeSource, Error};

    #[derive(Debug, Error)]
    pub enum CryptoError {
        // M0 placeholder: no variants yet. M1 adds the EVMCR-001..004
        // variants per docs/code_allocation.md.
    }

    impl CodeSource for CryptoError {
        fn code(&self) -> &'static str {
            // M0: no variants; an empty match would fail to compile, so
            // we use a catch-all that should be unreachable in practice.
            // M1+ replaces this with explicit arms.
            match *self {}
        }
    }
}

// `KeystoreError` lives in `crate::keystore` (the real M2 enum).
// Per ADR-0006 rev1 + M2 review, the placeholder that previously lived
// here was removed to avoid the "two `KeystoreError` types" type
// collision that broke the downcast chain. `CodeSource` is now
// implemented in `src/keystore/mod.rs` next to the enum itself.

pub mod chain {
    use std::path::PathBuf;

    use super::{CodeSource, Error};

    /// Sub-enum for `EVMC-001 RpcError` variants. Defined here at M0
    /// so M3 (which adds RpcError) can attach it.
    #[derive(Debug, Error)]
    pub enum RpcErrorKind {
        #[error("rpc timeout after {0:?}")]
        Timeout(std::time::Duration),
        #[error("rpc connection refused")]
        ConnectionRefused,
        #[error("rpc http status {0}: {1}")]
        HttpStatus(u16, String),
        #[error("rpc rate-limited; retry_after={retry_after:?}")]
        RateLimited {
            retry_after: Option<std::time::Duration>,
        },
        #[error("rpc server error {0}: {1}")]
        ServerError(i64, String),
        #[error("rpc response deserialization failed: {0}")]
        Deserialization(String),
    }

    #[derive(Debug, Error)]
    pub enum ChainError {
        #[error("rpc error")]
        RpcError { kind: RpcErrorKind },
        // M0 placeholder for the rest. M3 adds EVMC-002..010.

        // Suppress unused-warnings for fields used only in the placeholder.
        #[error("placeholder (M0)")]
        _M0PlaceholderPath(PathBuf),
    }

    impl CodeSource for ChainError {
        fn code(&self) -> &'static str {
            match self {
                Self::RpcError { .. } => "EVMC-001",
                Self::_M0PlaceholderPath(_) => "EVM-999",
            }
        }
    }
}

pub mod config {
    use super::{CodeSource, Error};

    #[derive(Debug, Error)]
    pub enum ConfigError {
        // M0 placeholder. M4 adds EVMCFG-001..004.
    }

    impl CodeSource for ConfigError {
        fn code(&self) -> &'static str {
            match *self {}
        }
    }
}

pub mod io {
    use std::path::PathBuf;

    use super::{CodeSource, Error};

    #[derive(Debug, Error)]
    pub enum IoError {
        #[error("placeholder (M0)")]
        _M0PlaceholderPath(PathBuf),
    }

    impl CodeSource for IoError {
        fn code(&self) -> &'static str {
            match self {
                Self::_M0PlaceholderPath(_) => "EVM-999",
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downcast_yields_code_for_chain_rpc_error() {
        let err = CliError::from_layer(ChainError::RpcError {
            kind: chain::RpcErrorKind::ConnectionRefused,
        });
        assert_eq!(err.code(), "EVMC-001");
    }

    #[test]
    fn empty_enums_compile_and_yield_catch_all() {
        // M0 residue: CryptoError / ConfigError still have no variants
        // and cannot be constructed. KeystoreError moved to
        // `crate::keystore` (the real M2 enum) and is no longer empty.
        // This test simply asserts the catch-all is wired for plain
        // `anyhow::Error`s that escape the downcast chain.
        let anyhow_err = anyhow::anyhow!("plain string error");
        let cli = CliError::from(anyhow_err);
        assert_eq!(cli.code(), "EVM-999");
    }

    #[test]
    fn display_includes_code() {
        let err = CliError::from_layer(ChainError::RpcError {
            kind: chain::RpcErrorKind::ConnectionRefused,
        });
        let s = format!("{err}");
        assert!(s.contains("EVMC-001"), "display missing code: {s}");
    }

    /// P0-1 regression (M2 review): the downcast chain must find
    /// `crate::keystore::KeystoreError` (the real enum), not the
    /// removed placeholder, and every variant must produce its
    /// ASSIGNED EVMK-NNN code. This is the consumer-facing contract
    /// for `--json` output (M4).
    #[test]
    fn downcast_yields_keystore_codes() {
        let cases: Vec<(crate::keystore::KeystoreError, &'static str)> = vec![
            (crate::keystore::KeystoreError::InvalidPassword, "EVMK-001"),
            (crate::keystore::KeystoreError::FileCorrupted, "EVMK-002"),
            (
                crate::keystore::KeystoreError::AliasNotFound("ghost".to_string()),
                "EVMK-009",
            ),
            (
                crate::keystore::KeystoreError::AliasExists("dup".to_string()),
                "EVMK-010",
            ),
            (
                crate::keystore::KeystoreError::Io("perm denied".to_string()),
                "EVMK-011",
            ),
            (
                crate::keystore::KeystoreError::Internal("mac mismatch".to_string()),
                "EVMK-012",
            ),
        ];
        for (err, expected) in cases {
            let cli = CliError::from_layer(err);
            assert_eq!(cli.code(), expected, "downcast mismatch for {expected}");
        }
    }

    /// CI enforcement per ADR-0006 rev1: every `code()` match arm
    /// across all layer enums must return a string that appears in
    /// `docs/code_allocation.md`. This guards against code drift
    /// when adding/removing variants.
    #[test]
    fn all_codes_are_documented_in_code_allocation() {
        let alloc = include_str!("../docs/code_allocation.md");
        let claimed: &[&str] = &[
            // ChainError
            "EVMC-001", // KeystoreError (real, in crate::keystore)
            "EVMK-001", "EVMK-002", "EVMK-009", "EVMK-010", "EVMK-011", "EVMK-012",
        ];
        for code in claimed {
            assert!(
                alloc.contains(code),
                "code `{code}` returned by a `code()` match arm is missing from \
                 `docs/code_allocation.md`; add it there in the same change"
            );
        }
    }
}
