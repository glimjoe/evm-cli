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

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMK-001 | ASSIGNED | `InvalidPassword` | Anti-side-channel: covers wrong password **and** file missing in unlock path |
| EVMK-002 | ASSIGNED | `FileCorrupted { path: PathBuf }` | JSON parse fail / non-keystore content |
| EVMK-003 | ASSIGNED | `UnsupportedVersion { found: u32 }` | Keystore format version mismatch |
| EVMK-004 | ASSIGNED | `Argon2Failed` | KDF computation failed |
| EVMK-005 | ASSIGNED | `EncryptionFailed` | AES-GCM seal failed |
| EVMK-006 | ASSIGNED | `DecryptionFailed` | AES-GCM open failed (distinct from EVMK-001 because this is a binary-level error, not a user-input error) |
| EVMK-007 | ASSIGNED | `FileMissing { path: PathBuf }` | Only surfaced in `create-wallet` path (per V2 §5 L111 anti-side-channel rule) |
| EVMK-008 | ASSIGNED | `PermissionDenied { path: PathBuf }` | File mode 0600 not satisfied or umask issue |
| EVMK-099 | RESERVED | (future) | |

## EVMC-NNN — ChainError

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMC-001 | ASSIGNED | `RpcError { kind: RpcErrorKind }` | See RpcErrorKind sub-enum below |
| EVMC-002 | ASSIGNED | `NonceStuck { addr, stuck_for: Duration }` | NonceManager reports timeout |
| EVMC-003 | ASSIGNED | `FeeUnderpriced { required, offered }` | RPC rejected for fee |
| EVMC-004 | ASSIGNED | `InvalidAmount { value: String, reason: &'static str }` | U256 parse / overflow (P0-4) |
| EVMC-005 | ASSIGNED | `InvalidChainId { expected, actual }` | EIP-155 mismatch (B2) |
| EVMC-006 | ASSIGNED | `TxReverted { hash, reason: String }` | Receipt status = 0 |
| EVMC-007 | ASSIGNED | `TxNotFound { hash }` | RBF / Cancel: original hash unknown *(ADR-0008 rev1)* |
| EVMC-008 | ASSIGNED | `TxAlreadyMined { hash, block }` | RBF / Cancel: already in a block *(ADR-0008 rev1)* |
| EVMC-009 | ASSIGNED | `InsufficientFunds { required, available }` | Balance < value + fee |
| EVMC-010 | ASSIGNED | `GasEstimationFailed { reason: String }` | `eth_estimateGas` revert / timeout |
| EVMC-099 | RESERVED | (future) | |

**RpcErrorKind sub-variants** (all under EVMC-001):
- `Timeout(Duration)` — HTTP timeout
- `ConnectionRefused` — RPC unreachable
- `HttpStatus(u16, String)` — non-200 HTTP
- `RateLimited { retry_after: Option<Duration> }` — 429
- `ServerError(i64, String)` — JSON-RPC error code + message
- `Deserialization(String)` — response body parse fail

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
