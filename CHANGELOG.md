# Changelog

All notable changes to `evm-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- M0 scaffolding per PLAN-V9 §11:
  - License (MIT), README, CHANGELOG, SECURITY
  - 9 Architecture Decision Records (`docs/adr/0001..0009*.md` —
    0009 is the M2 keystore deviation, Accepted 2026-06-11)
  - Error code allocation table (`docs/code_allocation.md`, 31 codes)
  - Core `Secret<T: Zeroize>` type with explicit `Drop` impl
    (`src/types/secret.rs`)
  - `CliError` wrapper with stable error codes (`src/error.rs`)
  - `human_panic::setup_panic!()` first-line panic hook
    (V9 §18 correction; not `setup!()`)
  - Process hardening: `umask(0o077)` + `setrlimit(RLIMIT_CORE, 0)`
  - Integration test for panic hook (`tests/it_panic_hook.rs`)

- M1 — Crypto Layer (BIP-39/44, Keccak-256, EIP-2 low-S, EIP-191
  `personal_sign`): see `src/crypto/{mnemonic,address,keccak,sign}.rs`.

- M2 — Keystore Layer (AES-128-CTR + scrypt KDF, deviated from
  PLAN-V9 §5 M2 DoD per ADR-0009): `src/keystore/mod.rs` provides
  create / load / list / delete / rename / import with anti-side-channel
  on the unlock path.

- M3 — Chain Layer (Chain trait + AlloyChain, NonceManager 4-state,
  RBF / Cancel, ERC-20 encode, anvil integration test):
  `src/chain/{mod,alloy_chain,client,erc20,nonce,rbf}.rs` +
  `tests/it_eth_transfer.rs` + `tests/e2e_sepolia_bump.rs #[ignore]`.

- M3 fuzz harness scaffolding (P0-10): `fuzz/fuzz_targets/{fuzz_rlp_decode,
  fuzz_signature_recover,fuzz_keystore_json}.rs` + separate
  `.github/workflows/nightly-fuzz.yml`.

### Changed

- V9 §18 corrections (MSRV 1.96, ADR-0005 macro `setup_panic!()`,
  ADR-0007 bound `T: Zeroize` + explicit `Drop`, `os_memlock` crate
  not `mlock`).

- `Secret<T>` bound corrected from `T: ZeroizeOnDrop` to `T: Zeroize`
  with explicit `Drop` impl. Functionally equivalent for `Vec<u8>`,
  `String`, and other concrete types; widens the API to types that
  impl `Zeroize` but not `ZeroizeOnDrop` (e.g. `B256` from alloy).

- `static_assertions` removed from `Cargo.toml` (V9 §18 — the
  assertion that motivated it is no longer applicable).

- `deny.toml`: `unmaintained = "deny"` added to `[advisories]`
  (PLAN-V9 §5 M0 DoD "forbids… unmaintained crates" was previously
  un-enforced).

- `scripts/secret_string_grep.sh`: explicit `command -v rg` guard
  (was previously silently passing when `rg` was missing).

- `alloy-node-bindings` moved from `[dependencies]` to
  `[dev-dependencies]` (it's only needed for the anvil integration
  test in `tests/`).

### Security

- See `docs/adr/0007-secret-memory.md` for memory hardening rationale.
- See `docs/adr/0009-eth-keystore-deviation.md` for the scrypt N=8192
  KDF cost (16× below OWASP 2024 baseline) — accepted V1 limitation.

[Unreleased]: https://github.com/<org>/evm-cli/compare/v0.0.0...HEAD
