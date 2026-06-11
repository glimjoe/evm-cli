# ADR-0003: Workspace Split Decision

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision)
> Deciders: evm-cli maintainers
> Supersedes: V2 §2 L28 ("workspace split evaluated at M5")

## Context and Problem Statement

V2 deferred the workspace-split decision to M5 (Release Engineering stage). This is too late: any split requires touching `Cargo.toml`, `mod` paths, and the module visibility graph, all of which are stable by M5. The decision must be made at M0, even if the answer is "no".

This ADR records the M0 decision and the conditions under which it should be revisited.

## Decision Drivers

- **Build time**: workspace splits enable per-crate incremental builds.
- **Code organization**: separating `crypto`, `keystore`, `chain`, `cli`, `types` clarifies dependency direction.
- **Distribution flexibility**: a workspace allows publishing sub-crates independently.
- **Single-binary constraint**: V1 is `no published lib`; internal modules are enough.
- **Cost of split**: mod path changes, dependency duplication risk, CI complexity.

## Considered Options

- **A. Single package, internal modules** (chosen for V1)
- **B. Workspace: `evm-cli-core` (crypto/keystore/types) + `evm-cli-chain` (chain) + `evm-cli` (cli/main)**: would force the same layer split as internal modules but with separate `Cargo.toml`s.
- **C. Workspace: 5 crates matching §2 layout (crypto, keystore, chain, cli, types)**: one crate per directory; maximum isolation, maximum build matrix cost.

## Decision Outcome

**Chosen option: A (single package)** for V1, with a clear revisit trigger:

- **Revisit at**: M5 (Release Engineering) **or** when any of these triggers fires:
  - A sub-crate is desired as a published library (e.g. for downstream tooling)
  - `cargo build` time for the full crate exceeds **60 s on the reference machine** — defined as a **GitHub Actions standard Linux runner** (2 vCPU, 7 GB RAM, 14 GB SSD). This baseline is reproducible and free; if a maintainer's local machine is faster, that's fine, the trigger fires when CI is slow.
  - Internal module-level dependency cycles are detected (sign that physical separation is needed)
- **Module layout under single package**:
  ```
  src/
  ├── types/       # Bottom: Newtype primitives (Address, Amount, Nonce,
  │                #         ChainId, TxHash, BlockNumber, **Secret<T>**)
  │                # NO upstream deps; depended on by all other modules.
  ├── crypto/      # depends only on types
  │                #   mnemonic, BIP-39/44, Keccak-256, EIP-2/personal_sign
  ├── keystore/    # depends on types AND crypto
  │                #   encrypted storage; uses crypto for address derivation
  ├── chain/       # depends on types AND crypto
  │                #   Chain trait, alloy impl, nonce mgmt, RBF/Cancel
  │                #   uses crypto for signing
  ├── cli/         # depends on types, crypto, keystore, chain (top)
  │                #   clap + rustyline REPL; calls into all lower layers
  └── main.rs      # entry point; calls cli::run() after process hardening
  ```

  **Explicit dependency edges (no others allowed):**
  - `types` → (no internal deps)
  - `crypto` → `types`
  - `keystore` → `types`, `crypto`
  - `chain` → `types`, `crypto`
  - `cli` → `types`, `crypto`, `keystore`, `chain`

  **Why `Secret<T>` lives in `types/`:** it is a generic wrapper (`Secret<T: ZeroizeOnDrop>(T)`) used by `crypto` (for mnemonic/seed), `keystore` (for encrypted payloads), and `chain` (for in-flight signed tx material). It has no crypto-specific logic, so it is a primitive, not a domain module.

  **No cycles allowed.** Mechanically enforced by a CI job that runs `cargo-modules deps --no-externals` (added in M0) and fails if the internal graph contains a cycle. `cargo-deny` does not natively detect intra-crate cycles, so `cargo-modules` is the chosen tool.

### Consequences

* **Good**: minimal `Cargo.toml` overhead, simple CI, fast path to first commit.
* **Good**: explicit module dependency direction acts as a "poor man's workspace" — split later is mostly mechanical.
* **Bad**: full rebuild on changes that touch many modules. Mitigated by Rust's incremental compilation and the small V1 codebase.
* **Bad**: if M5 actually splits, every `use` path changes. Mitigated by keeping the module names identical to the eventual crate names (e.g. `mod chain;` not `mod evm_cli_chain;`).

## Implementation

- PLAN-V4 §2 (Repository Layout)
- `Cargo.toml` declares a single `[package]`, no `[workspace]`
- Module dependency direction enforced by `cargo-modules` in CI (added at M0)
- `Secret<T>` lives in `src/types/secret.rs`; re-exported as `crate::types::Secret`
- Revisit condition is checked at M5 kickoff

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 4 issues in the initial Accepted draft. All addressed:

1. **Dependency graph explicit**: the initial diagram said `types ← {crypto, keystore, chain}` which hid the fact that `keystore` depends on `crypto` (for BIP-44 derivation) and `chain` depends on `crypto` (for signing). Revised "Explicit dependency edges" section lists all 5 modules and their allowed upstream edges.
2. **`Secret<T>` attribution**: was implicit (could have been in `types/` or `crypto/`); now explicit in `types/` with rationale.
3. **Reference machine defined**: "60 s threshold" was vague; now pinned to GitHub Actions standard Linux runner (2 vCPU / 7 GB RAM / 14 GB SSD) for reproducibility.
4. **Cycle detection in CI**: was "code review only"; now `cargo-modules deps --no-externals` in M0 CI job.

No semantic change to the "single package for V1" decision. All changes are documentation precision + automated enforcement.

## References

- PLAN-V4 §2
- PLAN-V4 §5 M0 DoD (ADR-0003 is a deliverable)
- Rust workspaces: https://doc.rust-lang.org/cargo/reference/workspaces.html
- `cargo-modules`: https://crates.io/crates/cargo-modules
- GitHub Actions runner specs: https://docs.github.com/en/actions/using-github-hosted-runners/using-github-hosted-runners/about-github-hosted-runners#standard-github-hosted-runners
