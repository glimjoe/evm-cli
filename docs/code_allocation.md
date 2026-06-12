# Error Code Allocation

> Source of truth: `docs/adr/0006-error-codes.md`. This file is generated
> from that ADR at M0 kickoff and is **CI-enforced**: any `code()` match
> arm that returns a string not listed here fails the build.

**Status legend:**
- **ASSIGNED** = currently used by a variant in source
- **RESERVED** = prefix allocated, no variant yet

**Code stability:** a code, once ASSIGNED, never changes meaning. Deprecated codes
become aliases (the old variant stays in the enum marked `#[deprecated]`, and
its `code()` arm returns the new code; the old string is preserved as a
`pub const ALIAS` for back-compat).

---

## EVMCR-NNN — CryptoError

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMCR-001 | ASSIGNED | `InvalidMnemonic { reason: &'static str }` | BIP-39 wordlist / checksum / length failures |
| EVMCR-002 | ASSIGNED | `InvalidDerivationPath { path: String }` | BIP-44 path validation |
| EVMCR-003 | ASSIGNED | `InvalidSignature { rec: Option<Address> }` | EIP-2 low-S / ecrecover mismatch |
| EVMCR-004 | ASSIGNED | `KdfFailure { backend: &'static str }` | Argon2 / HKDF failure (rare) |
| EVMCR-099 | RESERVED | (future) | |

## EVMK-NNN — KeystoreError

> Realigned 2026-06-11 per M2 review: the M0 placeholder code list
> (EVMK-001..008) was a speculative design that did not match the
> implemented enum in `src/keystore/mod.rs`. EVMK-001 and EVMK-002
> are kept (signatures adjusted to match actual variants). EVMK-003..008
> are demoted to RESERVED (the speculative variants Argon2Failed,
> EncryptionFailed, etc. are out of scope for M2 — we use
> `eth-keystore`'s built-in crypto, see ADR-0009). EVMK-009..012
> are the new codes that cover the real M2 variants.

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMK-001 | ASSIGNED | `InvalidPassword` | Anti-side-channel: covers wrong password **and** file missing in unlock path |
| EVMK-002 | ASSIGNED | `FileCorrupted` | JSON parse fail / non-keystore content (no `path` field; the path is captured in the surrounding log context) |
| EVMK-003 | RESERVED | (former `UnsupportedVersion`) | Keystore format version mismatch — unused in M2; reserved for V2 if we drop `eth-keystore` |
| EVMK-004 | RESERVED | (former `Argon2Failed`) | KDF computation failed — unused; `eth-keystore` is scrypt-based (ADR-0009) |
| EVMK-005 | RESERVED | (former `EncryptionFailed`) | AES-GCM seal failed — unused; encryption is `eth-keystore` internal |
| EVMK-006 | RESERVED | (former `DecryptionFailed`) | AES-GCM open failed — collapsed into `InvalidPassword` (EVMK-001) per anti-side-channel |
| EVMK-007 | RESERVED | (former `FileMissing { path }`) | Collapsed into `InvalidPassword` (EVMK-001) per anti-side-channel |
| EVMK-008 | RESERVED | (former `PermissionDenied { path }`) | Surfaced as `Io(String)` (EVMK-011) instead, with the path in the string |
| EVMK-009 | ASSIGNED | `AliasNotFound(String)` | Alias does not exist (used by `delete` / `load_strict` / `rename`) |
| EVMK-010 | ASSIGNED | `AliasExists(String)` | Alias collision on `create` / `rename` |
| EVMK-011 | ASSIGNED | `Io(String)` | I/O error other than file-missing (permission, disk full, …) |
| EVMK-012 | ASSIGNED | `Internal(String)` | Other internal error (alloy / eth-keystore / BIP-39 / KDF). Includes `LocalSignerError::MacMismatch` from `load_strict` on wrong password |
| EVMK-099 | RESERVED | (future) | |

## EVMC-NNN — ChainError

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMC-001 | ASSIGNED | `Rpc(String)` | Underlying RPC failure (timeout, 429, server error, deserialization). The string is a free-form description (e.g. `"get_balance: ..."`); the structured `RpcErrorKind` sub-enum from the M0 placeholder was collapsed into a single string in M3 to keep the API surface minimal. |
| EVMC-002 | ASSIGNED | `NonceStuck { addr, stuck_for: Duration }` | NonceManager reports timeout |
| EVMC-003 | ASSIGNED | `FeeUnderpriced { required, offered }` | RPC rejected for fee |
| EVMC-004 | ASSIGNED | `InvalidAmount { value: String, reason: &'static str }` | U256 parse / overflow (P0-4) |
| EVMC-005 | ASSIGNED | `InvalidChainId { expected, actual }` | EIP-155 mismatch (B2) |
| EVMC-006 | ASSIGNED | `TxReverted { hash, reason: String }` | Receipt status = 0 |
| EVMC-007 | ASSIGNED | `TxNotFound { hash }` | RBF / Cancel: original hash unknown *(ADR-0008 rev1)* |
| EVMC-008 | ASSIGNED | `TxAlreadyMined { hash, block }` | RBF / Cancel: already in a block *(ADR-0008 rev1)* |
| EVMC-009 | ASSIGNED | `InsufficientFunds { required, available }` | Balance < value + fee |
| EVMC-010 | ASSIGNED | `GasEstimationFailed { reason: String }` | `eth_estimateGas` revert / timeout |
| EVMC-099 | ASSIGNED | `ReceiptTimeout(Duration)` | 120s receipt polling timed out (M3 receipt pipeline) |
| EVMC-999 (via `EVM-999`) | ASSIGNED | `Internal(String)` | Other internal error (signing, internal invariant) — surfaces as the catch-all `EVM-999` |

## EVMCFG-NNN — ConfigError

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMCFG-001 | ASSIGNED | `MissingRequiredField { field: &'static str }` | Required key absent |
| EVMCFG-002 | ASSIGNED | `InvalidValue { field, reason: String }` | Parse / range failure |
| EVMCFG-003 | ASSIGNED | `FileNotFound { path: PathBuf }` | Config file path missing |
| EVMCFG-004 | ASSIGNED | `PermissionDenied { path: PathBuf }` | Config file not readable |
| EVMCFG-099 | RESERVED | (future) | |

## EVMIO-NNN — IoError

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMIO-001 | ASSIGNED | `PermissionDenied { path }` | Filesystem permissions |
| EVMIO-002 | ASSIGNED | `DiskFull { path, needed: u64 }` | Write would exceed fs |
| EVMIO-003 | ASSIGNED | `PathNotFound { path }` | Parent dir missing |
| EVMIO-004 | ASSIGNED | `ReadWriteError { path, source: String }` | Generic I/O failure |
| EVMIO-099 | RESERVED | (future) | |

## EVM-9NN — Catch-all (untyped errors)

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVM-999 | ASSIGNED | `Unknown` | Default when downcast fails; never use deliberately |

---

## CI enforcement (per ADR-0006 rev1)

A unit test in `src/error.rs` enumerates all `code()` match arms and asserts
each returned string is present in this file. Adding a new code requires:

1. `code()` match arm in the appropriate enum
2. Unit test for the new code
3. Entry in this file (status ASSIGNED)
4. `CHANGELOG.md` entry if user-visible
5. All in the same PR
