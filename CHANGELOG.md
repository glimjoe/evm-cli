# Changelog

All notable changes to `evm-cli` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- M0 scaffolding per PLAN-V8 §11:
  - License (MIT), README, CHANGELOG, SECURITY
  - 8 Architecture Decision Records (`docs/adr/0001..0008*.md`)
  - Error code allocation table (`docs/code_allocation.md`, 31 codes)
  - Core `Secret<T>` type with `ZeroizeOnDrop` (`src/types/secret.rs`)
  - `CliError` wrapper with stable error codes (`src/error.rs`)
  - `human_panic::setup!()` first-line panic hook
  - Process hardening: `umask(0o077)` + `setrlimit(RLIMIT_CORE, 0)`
  - Integration test for panic hook (`tests/it_panic_hook.rs`)

### Security

- See `docs/adr/0007-secret-memory.md` for memory hardening rationale.

[Unreleased]: https://github.com/<org>/evm-cli/compare/v0.0.0...HEAD
