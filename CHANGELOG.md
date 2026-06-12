# Changelog

All notable changes to `evm-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned (M6 — Self-Audit + Documentation Wrap, per PLAN-V9 §5)
- Security self-audit per PLAN-V9 §7 (every item verified with evidence)
- Refinement: live REPL `json`/`human` toggle (P0-1 stretch)
- Optional: clipboard support (P0-7 stretch)
- Optional: ERC-20 broadcast via calldata path (currently V1 builds a
  value=0 envelope; calldata computed but not broadcast — see ADR-0010
  future)

## [0.2.0] — 2026-06-12

### Added

- **M5 — Release Engineering (PLAN-V9 §5 M5 DoD)**:
  - `.github/workflows/release.yml` — hand-written GitHub Actions
    workflow (option A from the M5 branch decision; rationale: matches
    the project's "no-magic single-package" philosophy, ADR-0003).
  - Multi-arch release artifacts per tag:
    - `evm-cli-v0.2.0-linux-x86_64.tar.gz` (primary, CI-tested)
    - `evm-cli-v0.2.0-linux-aarch64.tar.gz` (cross-compiled, best-effort)
  - SHA256 sidecar files alongside each tarball.
  - `scripts/build_release_artifact.sh` — local-build helper that
    mirrors the CI steps (so contributors can dry-run a release
    before pushing a tag).
  - `docs/architecture.md` — ASCII architecture diagram (M5 +
    architecture-diagram DoD rolled into one atomic commit, per the
    user's M5 branch decision).
  - `evm_cli::release` module (testable surface for the release
    pipeline; see `src/release.rs`):
    - `ReleaseVersion` newtype (validates `X.Y.Z` form, rejects
      `Unreleased` / `v`-prefixed / pre-release tags)
    - `artifact_name(version, target) -> "evm-cli-v0.2.0-linux-x86_64.tar.gz"`
    - `sha256_sidecar(version, target) -> "evm-cli-v0.2.0-linux-x86_64.sha256"`
    - `platform_tag(target) -> "linux-x86_64"` (normalizes Rust triples)
    - `extract_changelog_section(changelog, version) -> body`
    - `render_release_notes(changelog, disclaimer, limits) -> String`
    - `validate_release_workflow_yaml(yaml) -> Result<(), MissingStep>`
      (guards the workflow against silent step drops)
  - 20 new integration tests in `tests/it_release.rs` + 4 new artifact
    tests in `tests/it_release_workflow.rs` (validate the on-disk
    `release.yml` against `validate_release_workflow_yaml`). All green.
  - 4 new unit tests for `ReleaseVersion` in `src/release.rs` (all green).

### Changed

- **Version bump 0.1.0 → 0.2.0** (MINOR; PLAN-V9 §1.1 release policy).
  Triggers: new release-engineering capability, not runtime change.
  No CLI surface change. `cargo install --git ...` continues to work.
- `Cargo.toml` `version = "0.2.0"`.
- `src/main.rs` startup banner: `evm-cli v0.2.0 starting`.

### Security

- No security-critical changes. All hardening (P0-2 memory, P0-7
  output, scrypt KDF) is unchanged from 0.1.0 — see ADR-0007 rev1,
  ADR-0009.

### Known V1 limitations (carried forward from 0.1.0)

- **P0-8 coverage gate**: per-module gate **passes** (crypto/ ≥
  97%, keystore/ = 90.56%, both ≥ 90%). **Global gate does not
  pass** (61.46% lines; plan requires ≥ 80%). The shortfall is
  concentrated in `cli/commands.rs` (0% — requires anvil) and
  `chain/alloy_chain.rs` (57% — requires anvil) and
  `chain/rbf.rs` (45% — requires anvil). The 3 anvil-driven
  integration tests in `tests/it_eth_transfer.rs` give behavioral
  coverage of these paths but do not count toward `cargo
  llvm-cov` line coverage (different processes). Accepting this
  for V1; deferred to M5/V2 to either:
    (a) refactor `cli/commands.rs` to extract testable pure
        functions (estimated 50–100 LoC, target 70–75% global);
    (b) write a SEPOLIA-fork anvil integration test that exercises
        the full REPL flow (significant investment; defer to V2).
- **Keystore KDF**: `eth-keystore` 0.5.0 uses scrypt N=8192, 16× below
  OWASP 2024 baseline. Accepted for V1 (interop with geth/ethers.js).
  See ADR-0009.
- **PoC — do not use on mainnet with real assets.** This release is a
  proof-of-concept for the Sepolia testnet. Mainnet use is out of
  scope (PLAN-V9 §9).

### Test results

- `cargo build --all-targets`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo fmt --all -- --check`: clean
- `cargo test --tests --lib`: **153 passing** (118 lib unit + 7 e2e
  Sepolia + 3 anvil + 1 panic hook + 20 release + 4 release_workflow
  integration) + 1 `#[ignore]` Sepolia bump test
- `bash scripts/secret_string_grep.sh`: 0 matches

## [0.1.0] — 2026-06-12

### Added

- **M4 — CLI Layer** (this commit, per PLAN-V9 §5 M4 DoD):
  - `src/cli/` module: clap derive + rustyline REPL.
  - 11 commands: `create-wallet`, `import-mnemonic`, `list`, `use`,
    `unlock`, `balance`, `send-eth` (with `--bump-fee`/`--cancel`/
    `--dry-run`), `send-token` (with `--decimals` and `--dry-run`),
    `sign-message`, `pending-tx`, `exit`. Plus `help` for in-REPL use.
  - 12-factor config (CLI > env > TOML > default):
    `EVMCLI_RPC_URL`, `EVMCLI_KEYSTORE_DIR`, `EVMCLI_DATA_DIR`,
    `EVMCLI_JSON`, `EVMCLI_NO_HISTORY`, `EVMCLI_CHAIN_ID`,
    `EVMCLI_RPC_TIMEOUT`. Optional TOML at
    `~/.config/evm-cli/config.toml`.
  - `--json` global flag: NDJSON output (`{"ok": true, "data": ...}`
    / `{"ok": false, "code": ..., "message": ..., "cause": [...]}`)
  - y/N confirmation (default N) for `send-eth`, `send-token`, RBF,
    Cancel.
  - `--dry-run` flag for `send-eth` and `send-token`.
  - EIP-55 mixed-case display for all addresses (`types::Address`
    Display impl).
  - `should_skip_history` predicate; lines containing `mnemonic`,
    `password`, `--private-key`, or `import-mnemonic` are NOT added
    to the rustyline history file.
  - `--no-history` global flag to disable history entirely.
  - Startup validation: keystore directory writability (probe
    write + remove) and chain id match against expected (Sepolia).
  - `main.rs` reworked: still installs `human_panic::setup_panic!()`
    + `harden_process()` (per ADR-0005 + ADR-0007), then dispatches
    to `cli::run()` via a current-thread tokio runtime.
  - PoC warning printed to stderr at startup (skipped only when
    `EVMCLI_JSON=true`).

- **M0 — Scaffolding** (per PLAN-V9 §11):

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

[0.2.0]: https://github.com/glimjoe/evm-cli/releases/tag/v0.2.0
[0.1.0]: https://github.com/glimjoe/evm-cli/releases/tag/v0.1.0
[Unreleased]: https://github.com/glimjoe/evm-cli/compare/v0.2.0...HEAD
