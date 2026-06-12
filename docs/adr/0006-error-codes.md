# ADR-0006: Error Code Naming Convention

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision)
> Deciders: evm-cli maintainers
> Supersedes: V2 §4 (no error codes), V3 §4 (variant list but no codes)

## Context and Problem Statement

V2's error model used `thiserror` enums without stable machine-readable codes. This breaks:

- Script integration (CI pipelines parsing `evm-cli` output cannot reliably detect "wrong password" vs "file missing")
- Log analysis (aggregating `tracing` output by error class)
- Future i18n (codes become i18n lookup keys)

P0-1 mandates a code system.

## Decision Drivers

- **Grep-friendly**: a developer searching the codebase for "EVMC-004" lands on the variant.
- **Layer-scoped**: codes should not require global coordination when adding a variant.
- **Stable across releases**: codes never change meaning; deprecated codes become aliases, not deletions.
- **Human-readable**: codes should be short and prefix-distinguishable.

## Considered Options

- **A. Per-layer prefix, 3-digit suffix** (chosen)
- **B. Single global counter**: `EVM-001`, `EVM-002`, ... — requires global coordination; two PRs adding variants conflict.
- **C. Hierarchical dotted**: `evm.chain.rpc.timeout` — longer, less script-friendly.

## Decision Outcome

**Chosen option: A**, with the following prefix table and full code allocation:

### Prefix table

| Prefix | Layer | Reserved suffix range |
|---|---|---|
| `EVMCR-NNN`  | `CryptoError`   | `001`–`099` |
| `EVMK-NNN`   | `KeystoreError` | `001`–`099` |
| `EVMC-NNN`   | `ChainError`    | `001`–`099` |
| `EVMCFG-NNN` | `ConfigError`   | `001`–`099` |
| `EVMIO-NNN`  | `IoError`       | `001`–`099` |
| `EVM-NNN`    | Unknown / unclassified (catch-all) | `900`–`999` |

The catch-all `EVM-9xx` is for errors that escape all typed layers (e.g. panics that recover, library-internal errors with no code). Default is `EVM-999` `Unknown`.

### Full code allocation (V1 baseline)

Codes marked `ASSIGNED` are required by V1; `RESERVED` are reserved for future variants in the same prefix.

**`EVMCR-NNN` — CryptoError**

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMCR-001 | ASSIGNED | `InvalidMnemonic { reason: &'static str }` | BIP-39 wordlist / checksum / length failures |
| EVMCR-002 | ASSIGNED | `InvalidDerivationPath { path: String }` | BIP-44 path validation |
| EVMCR-003 | ASSIGNED | `InvalidSignature { rec: Option<Address> }` | EIP-2 low-S / ecrecover mismatch |
| EVMCR-004 | ASSIGNED | `KdfFailure { backend: &'static str }` | Argon2 / HKDF failure (rare) |
| EVMCR-099 | RESERVED | (future) | |

**`EVMK-NNN` — KeystoreError**

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

**`EVMC-NNN` — ChainError**

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMC-001 | ASSIGNED | `Rpc(String)` | Underlying RPC failure (timeout, 429, server error, deserialization). The string is a free-form description (e.g. `"get_balance: connection refused"`). **rev2 simplification**: the structured `RpcErrorKind` sub-enum from the rev1 draft was collapsed into a single `String` payload to keep the public surface minimal (per the M3 audit fix for B1). Sub-categorization is preserved in the string content (`"get_balance: ..."`, `"send_raw_transaction: ..."`) for log filtering. |
| EVMC-002 | ASSIGNED | `NonceStuck { addr, stuck_for: Duration }` | NonceManager reports timeout |
| EVMC-003 | ASSIGNED | `FeeUnderpriced { required, offered }` | RPC rejected for fee |
| EVMC-004 | ASSIGNED | `InvalidAmount { value: String, reason: &'static str }` | U256 parse / overflow (P0-4) |
| EVMC-005 | ASSIGNED | `InvalidChainId { expected, actual }` | EIP-155 mismatch (B2) |
| EVMC-006 | ASSIGNED | `TxReverted { hash, reason: String }` | Receipt status = 0 |
| EVMC-007 | ASSIGNED | `TxNotFound { hash }` | RBF / Cancel: original hash unknown *(added per ADR-0008 rev1)* |
| EVMC-008 | ASSIGNED | `TxAlreadyMined { hash, block }` | RBF / Cancel: already in a block *(added per ADR-0008 rev1)* |
| EVMC-009 | ASSIGNED | `InsufficientFunds { required, available }` | Balance < value + fee |
| EVMC-010 | ASSIGNED | `GasEstimationFailed { reason: String }` | `eth_estimateGas` revert / timeout |
| EVMC-099 | ASSIGNED | `ReceiptTimeout(Duration)` | 120s receipt polling timed out (M3 receipt pipeline). 099 was previously RESERVED; promoted to ASSIGNED in rev2. |
| (catch-all) | — | `Internal(String)` | Other internal error (signing, internal invariant). Surfaces as `EVM-999` via the downcast-chain catch-all; not a `ChainError::code()` arm itself. |

**`EVMCFG-NNN` — ConfigError**

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMCFG-001 | ASSIGNED | `MissingRequiredField { field: &'static str }` | Required key absent |
| EVMCFG-002 | ASSIGNED | `InvalidValue { field, reason: String }` | Parse / range failure |
| EVMCFG-003 | ASSIGNED | `FileNotFound { path: PathBuf }` | Config file path missing |
| EVMCFG-004 | ASSIGNED | `PermissionDenied { path: PathBuf }` | Config file not readable |
| EVMCFG-099 | RESERVED | (future) | |

**`EVMIO-NNN` — IoError**

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVMIO-001 | ASSIGNED | `PermissionDenied { path }` | Filesystem permissions |
| EVMIO-002 | ASSIGNED | `DiskFull { path, needed: u64 }` | Write would exceed fs |
| EVMIO-003 | ASSIGNED | `PathNotFound { path }` | Parent dir missing |
| EVMIO-004 | ASSIGNED | `ReadWriteError { path, source: String }` | Generic I/O failure |
| EVMIO-099 | RESERVED | (future) | |

**`EVM-9NN` — Catch-all (untyped errors)**

| Code | Status | Variant | Notes |
|---|---|---|---|
| EVM-999 | ASSIGNED | `Unknown` | Default when downcast fails; never use deliberately |

### Stability rules

- A code, once assigned, **never changes meaning**.
- A code, once deprecated, **becomes an alias** for the replacement; the old variant stays in the enum (marked `#[allow(dead_code)]`) with a `#[deprecated]` attribute, and its `code()` match arm returns the **new** code. The old code string is preserved as a `pub const ALIAS: &str` on the variant for back-compat.
- The first reserved code in each prefix is `001`. New variants get the next free number (e.g. EVMCR-005 next).
- Adding a variant requires: (1) `code()` match arm, (2) unit test asserting the code, (3) `CHANGELOG.md` entry if the variant is user-visible, (4) update `code_allocation.md`.

### Surface area

- Each enum variant implements `pub const fn code(&self) -> &'static str` (zero-cost, no allocation).
- `CliError` is a **wrapper struct** around `anyhow::Error`, not an enum:
  ```rust
  pub struct CliError {
      inner: anyhow::Error,
  }

  impl CliError {
      pub fn code(&self) -> &'static str {
          // Linear downcast chain. Most common paths are listed first
          // so the average case is one or two downcasts.
          if let Some(e) = self.inner.downcast_ref::<ChainError>()    { return e.code(); }
          if let Some(e) = self.inner.downcast_ref::<KeystoreError>() { return e.code(); }
          if let Some(e) = self.inner.downcast_ref::<CryptoError>()   { return e.code(); }
          if let Some(e) = self.inner.downcast_ref::<ConfigError>()   { return e.code(); }
          if let Some(e) = self.inner.downcast_ref::<IoError>()       { return e.code(); }
          "EVM-999"
      }
  }
  ```
- The downcast order is documented as part of the API; changing it is a breaking change for callers that introspect codes.
- CLI `--json` flag emits `{code, message, cause: [...]}` (M4, P0-1).

### `code_allocation.md` deliverable (M0)

A file `docs/code_allocation.md` (or a top-of-file table in `src/error.rs` if the maintainer prefers single-source) is created at M0 with the full table above. The file is the **single source of truth** for code assignments; any new code MUST be added here in the same PR that adds the variant. CI fails if a `code()` match arm returns a code not in the allocation table.

### Consequences

* **Good**: per-prefix reservation means adding a `ChainError` variant never conflicts with someone adding a `KeystoreError` variant.
* **Good**: 3-digit suffix leaves headroom (≤ 999 variants per layer, ample for V1).
* **Good**: stable codes enable downstream tooling.
* **Bad**: every new variant requires a code — minor developer overhead, enforced by code review.
* **Bad**: if a code becomes misleading due to refactor, the rule says don't change it. We accept this; aliases for the old code can be added.

## Implementation

- PLAN-V4 §4 (Error Model — codes enumerated in tree; updated to match this ADR's full table)
- PLAN-V4 §5 M4 (CLI `--json` flag)
- PLAN-V4 §7 (self-audit "every variant exposes `code()`")
- `docs/code_allocation.md` created at M0 with the full table above
- CI check: a unit test enumerates all `code()` match arms across all error enums and asserts every returned string appears in `docs/code_allocation.md`. New codes require both.

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 4 issues in the initial Accepted draft. All addressed:

1. **EVMC-007 / EVMC-008 missing**: ADR-0008 rev1 introduced `TxNotFound` and `TxAlreadyMined` as new `ChainError` variants but did not update this ADR. Now both are ASSIGNED in the `EVMC-NNN` table.
2. **Code table severely incomplete**: the initial draft listed only one example per prefix; V1 actually needs ~30 codes across 5 layers. Now a full allocation table is published: 4 EVMCR, 8 EVMK, 10 EVMC (+ 6 RpcErrorKind sub-variants), 4 EVMCFG, 4 EVMIO, 1 catch-all. Each ASSIGNED / RESERVED status is explicit.
3. **`code_allocation.md` deliverable was a one-liner**: the initial draft mentioned the file but did not specify its location, format, or CI enforcement. Now explicit: it lives at `docs/code_allocation.md`, is the single source of truth, and a CI unit test enforces that every `code()` match arm returns a string from the table.
4. **CliError delegation was muddled**: the initial draft said "CliError delegates to its inner variant" but V4's `CliError` is `anyhow`-based, not an enum. Now explicit: `CliError` is a wrapper struct holding `anyhow::Error`, and `code()` is a linear downcast chain. The order of downcasts is part of the API contract.

The total ASSIGNED count is 31 codes (4 EVMCR + 8 EVMK + 10 EVMC + 4 EVMCFG + 4 EVMIO + 1 catch-all). The 999-per-layer headroom is comfortable for V2 additions.

### 2026-06-11 (revision 2)

M3 audit fix for B1 surfaced two corrections to the rev1 spec:

1. **`RpcError { kind: RpcErrorKind }` collapsed to `Rpc(String)`**: the rev1 sub-enum (`Timeout`, `ConnectionRefused`, `HttpStatus`, `RateLimited`, `ServerError`, `Deserialization`) was a speculative design that the M3 implementation simplified to a single `String` payload. The downcast-chain code regression test (`downcast_yields_chain_codes` in `src/error.rs:285-363`) now matches this simpler shape. ADR-0006 rev2 (this revision) removes the `RpcErrorKind` sub-enum description and the "10 EVMC (+ 6 RpcErrorKind sub-variants)" wording from the rev1 changelog. The total ASSIGNED count is unchanged.

2. **`EVMC-099` promoted from RESERVED to ASSIGNED**: the M3 receipt-polling timeout (`ChainError::ReceiptTimeout(Duration)`) lands in code EVMC-099 (was previously earmarked for "future use"). 099 was chosen to leave EVMC-011..098 free for V2 additions; the rev1 "10 EVMC" count is now 11 with EVMC-099.

**Stability impact:** EVMC-001 already has the string payload ("RpcError" or "Rpc" both map to EVMC-001 in `code()`); no scripts depending on the variant name were affected. The variant rename (`RpcError` → `Rpc`) is internal and not yet exposed via the `--json` flag (M4 is when it becomes user-visible).

## References

- PLAN-V4 §4
- PLAN-V4 §5 M4
- PLAN-V4 §7
- ADR-0002 (NonceManager: `NonceStuck` lives at EVMC-002)
- ADR-0007 (companion: memory hardening affects which fields are safe to log)
- ADR-0008 (companion: introduced EVMC-007 / EVMC-008)
- ADR-0004 (companion: code changes require CHANGELOG entry)
- `anyhow::Error::downcast_ref`: https://docs.rs/anyhow/latest/anyhow/struct.Error.html#method.downcast_ref
