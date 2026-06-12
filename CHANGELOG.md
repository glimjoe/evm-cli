# Changelog

All notable changes to `evm-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned (M4 — CLI layer)
- `src/cli/` module: clap derive + rustyline REPL
- 11 commands (`create-wallet`, `import-mnemonic`, `list`, `use`,
  `unlock`, `balance`, `send-eth`, `send-token`, `sign-message`,
  `pending-tx`, `exit`)
- `--json` global flag, y/N confirmation, `--dry-run` /
  `--bump-fee` / `--cancel` flags, EIP-55 display, history filter
  (`should_skip_history` predicate)
- This work is the next milestone per PLAN-V9 §5 M4 DoD.

## [0.1.0] — 2026-06-12

First M0–M3 release. Sepolia PoC; not for mainnet.

### Added

- **M0 — Scaffolding** (per PLAN-V9 §11):
  - License (MIT), README, CHANGELOG, SECURITY
  - 9 Architecture Decision Records (`docs/adr/0001..0009*.md` —
    0009 is the M2 keystore deviation, Accepted 2026-06-11)
  - Error code allocation table (`docs/code_allocation.md`, 31 codes)
  - Core `Secret<T: Zeroize>` type with explicit `Drop` impl
    (`src/types/secret.rs`)
  - `CliError` wrapper with stable error codes (`src/error.rs`)
  - `human_panic::setup_panic!()` first-line panic hook
  - Process hardening: `umask(0o077)` + `setrlimit(RLIMIT_CORE, 0)`
  - mlock on `Secret<Vec<u8>>` buffers ≥ 32 bytes (via `os-memlock` 0.2.0)
  - Integration test for panic hook (`tests/it_panic_hook.rs`)
  - 5-pattern `secret_string_grep.sh` (ADR-0007 rev1; now includes
    `phrase` in SENSITIVE per the M3 audit H-6 fix)

- **M1 — Crypto Layer** (BIP-39/44, Keccak-256, EIP-2 low-S, EIP-191
  `personal_sign`): see `src/crypto/{mnemonic,address,keccak,sign}.rs`.
  5 ethereumbook test vectors + proptest (empty / 1-byte / non-UTF-8
  / 0x-hex / 1 KiB / 1 MiB manual).

- **M2 — Keystore Layer** (AES-128-CTR + scrypt KDF, deviated from
  PLAN-V9 §5 M2 DoD per ADR-0009): `src/keystore/mod.rs` provides
  create / load / list / delete / rename / import with anti-side-channel
  on the unlock path. Keystore format is the standard Ethereum JSON
  keystore (EIP-1081) used by `geth` / `ethers.js` / `MyEtherWallet`.

- **M3 — Chain Layer** (Chain trait + AlloyChain, NonceManager 4-state,
  RBF / Cancel, ERC-20 encode, anvil integration test):
  `src/chain/{mod,alloy_chain,client,erc20,nonce,rbf}.rs` +
  `tests/it_eth_transfer.rs` (3 tests, anvil-spawned) +
  `tests/e2e_sepolia_bump.rs` (`#[ignore]`, real Sepolia).

- **M3 fuzz harness scaffolding** (P0-10): `fuzz/fuzz_targets/{fuzz_rlp_decode,
  fuzz_signature_recover,fuzz_keystore_json}.rs` + separate
  `.github/workflows/nightly-fuzz.yml` (cron `0 3 * * *`).

- **7 newtypes** per PLAN-V9 §3 (`src/types/{address,amount,nonce,
  chain_id,signature,tx_hash,block_number}.rs`) with `From`/`Into`/
  `AsRef` interop with the underlying alloy types. The `Chain`
  trait uses the newtypes at the API boundary.

### Changed

- V9 §18 corrections (MSRV 1.96, ADR-0005 macro `setup_panic!()`,
  ADR-0007 bound `T: Zeroize` + explicit `Drop`, `os_memlock` crate
  not `mlock`).
- `Secret<T>` bound corrected from `T: ZeroizeOnDrop` to `T: Zeroize`
  with explicit `Drop` impl. Functionally equivalent for `Vec<u8>`,
  `String`, and other concrete types.
- `static_assertions` removed from `Cargo.toml` (V9 §18 — the
  assertion that motivated it is no longer applicable).
- `deny.toml`: `unmaintained = "deny"` added to `[advisories]`
  (PLAN-V9 §5 M0 DoD "forbids… unmaintained crates").
- `scripts/secret_string_grep.sh`: explicit `command -v rg` guard
  (was previously silently passing when `rg` was missing).
- `alloy-node-bindings` moved from `[dependencies]` to
  `[dev-dependencies]` (only needed for the anvil integration test).

### Security

- See `docs/adr/0007-secret-memory.md` for memory hardening rationale.
- See `docs/adr/0009-eth-keystore-deviation.md` for the scrypt N=8192
  KDF cost (16× below OWASP 2024 baseline) — **accepted V1 limitation**.

### Test results

- `cargo build --all-targets`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean
- `cargo test --all-targets`: **89 passing** (84 unit + 3 anvil
  integration + 1 panic hook + 1 Sepolia E2E `#[ignore]`)
- `bash scripts/secret_string_grep.sh`: 0 matches

[0.1.0]: https://github.com/glimjoe/evm-cli/releases/tag/v0.1.0
[Unreleased]: https://github.com/glimjoe/evm-cli/compare/v0.1.0...HEAD
