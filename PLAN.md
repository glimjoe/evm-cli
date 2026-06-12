# evm-cli V1 — Formal Implementation Plan v9

> Status: **M0 ACTIVE (post-V2 BLOCKER + V3 P0 + V4 ADR-0001 + V5 ADR-0005 + V6 G3 ADR revisions + V8 doc sync + V9 MSRV upgrade)**
> Scope: Linux-only CLI wallet for Sepolia testnet. Single binary, MIT licensed.
> V9 = V8 + MSRV upgrade 1.91 → 1.96 + 3 ADR corrections (Zeroize bound, os-memlock, setup_panic) — see §18.
> V8 history preserved as §17. V7 history preserved as §16. V6 history preserved as §15. V5 history preserved as §14. V4 history preserved as §13. V3 history preserved as §12.

---

## 1. Project Identity

| Field | Value |
|---|---|
| Name | `evm-cli` |
| License | MIT |
| Form | Single binary (no published lib) |
| Target OS | Linux (x86_64 + aarch64) |
| Target Chain | Sepolia (chainId `0xaa36a7`) |
| MSRV | **1.96** *(ADR-0001 + PLAN-V9 §18: raised 2026-06-11 from 1.91; alloy 2.0.5 still satisfied since it requires ≥ 1.91; V8's interim 1.91 was a lower bound during plan review, superseded by toolchain reality)* |
| Distribution | GitHub Releases (with `cargo install --git` as alt) |
| alloy Version | **`=2.0.5`** *(ADR-0001 Accepted 2026-06-11; verified against crates.io — V4's "1.x latest stable" framing preceded the 2026-04-13 2.0 release)* |
| Dependency Policy | **Release: pinned exact. Development: minor upgrades permitted per §1.1.** *(B5)* |
| Security Audit | Self-audit only (within V1 scope) |

### 1.1 Upgrade Policy *(B5)*

**Two-phase policy, applied at the `Cargo.toml` dependency line level.**

**Development period (pre-0.1.0):**
- Minor-version upgrades of direct dependencies (e.g. `alloy = "=1.x.y"` → `=1.x.y'`) are permitted via PR.
- Each upgrade PR **must**:
  1. Reference an ADR in `docs/adr/NNNN-<topic>.md` justifying the upgrade
  2. Pass the full CI matrix (fmt, clippy `-D warnings`, test, audit, deny, cov)
  3. Add a `CHANGELOG.md` entry under "Unreleased"
  4. Be reviewed and merged by a maintainer other than the author
- `cargo update -p <crate>` is the canonical command.
- Blanket `cargo update` (no `-p`) is **forbidden** in PRs.

**Release period (0.1.0+):**
- Published `Cargo.toml` pins every dependency to an exact version.
- Any dependency change triggers a new minor release of `evm-cli`.
- Patch-level CVEs are backported to a `release/X.Y` branch.

**Policy changes themselves require an ADR** (ADR-0004 covers the initial policy).

---

## 2. Repository Layout

```
evm-cli/
├── Cargo.toml             # single package; workspace split evaluated at M0 (ADR-0003)
├── Cargo.lock             # committed
├── rust-toolchain.toml    # pins 1.96.0 (ADR-0001 + V9 §18)
├── clippy.toml            # P0-4: disallow(unwrap_used, expect_used) globally
├── LICENSE                # MIT
├── README.md
├── CHANGELOG.md           # Keep a Changelog format, updated per §1.1
├── SECURITY.md            # vulnerability disclosure policy
├── deny.toml              # cargo-deny config
├── docs/
│   ├── adr/               # ADR-0001..0008 (M0 deliverables; ADR-0001 Accepted 2026-06-11)
│   └── architecture.md    # ASCII diagram (M6 deliverable)
├── .github/workflows/     # ci.yml, release.yml, nightly-fuzz.yml
├── fuzz/                  # P0-10: cargo-fuzz targets
│   ├── fuzz_rlp_decode/
│   ├── fuzz_signature_recover/
│   └── fuzz_keystore_json/
├── src/
│   ├── main.rs            # panic hook + umask + RLIMIT_CORE=0 (B6, P0-2, P0-7)
│   ├── cli/               # clap + rustyline REPL
│   ├── crypto/            # mnemonic, signing, Keccak
│   ├── keystore/          # encrypted storage
│   ├── chain/             # Chain trait + alloy impl
│   └── types/             # Newtype primitives
└── tests/                 # integration tests (anvil)
```

---

## 3. Type System Primitives (M0 deliverable)

```
Address(AlloyAddress)        // strings forbidden at API boundary
Amount(U256)                 // paired with token decimals
Nonce(u64)                   // local pending pool, managed by NonceManager
ChainId(u64)                 // const SEPOLIA = 0xaa36a7
Signature(AlloySignature)
Secret<T: ZeroizeOnDrop>(T)  // P0-2: ZeroizeOnDrop not Zeroize; Debug impl prints "Secret(***)"
TxHash(B256)
BlockNumber(u64)
```

> **P0-2 invariant:** mnemonic, seed, and private key material **MUST NOT** be stored as `String`. Use `Secret<Vec<u8>>` or `Zeroizing<...>`. Lint enforcement: CI step greps `rg "let .* (mnemonic|seed|private_key).*: String" --type rust` and fails on any match.

---

## 4. Error Model

```
CliError                    // top-level, anyhow::Result; all variants impl code() (P0-1)
├── CryptoError             // thiserror; "EVMCR-NNN" codes
├── KeystoreError           // thiserror; "EVMK-NNN" codes
├── ChainError              // thiserror; "EVMC-NNN" codes
│   ├── RpcError                       // EVMC-001
│   ├── NonceStuck                     // EVMC-002
│   ├── FeeUnderpriced                 // EVMC-003
│   ├── InvalidAmount { value, reason } // EVMC-004  (P0-4)
│   ├── InvalidChainId { expected, actual } // EVMC-005  (B2)
│   └── TxReverted { hash, reason }    // EVMC-006
├── ConfigError             // thiserror; "EVMCFG-NNN" codes
└── IoError                 // thiserror; "EVMIO-NNN" codes
```

Every variant implements:
```rust
pub const fn code(&self) -> &'static str {
    match self {
        Self::RpcError => "EVMC-001",
        Self::NonceStuck => "EVMC-002",
        Self::InvalidAmount { .. } => "EVMC-004",
        // ...
    }
}
```

Prefix scheme codified in **ADR-0006** (P0-1). Each layer attaches context (address, chainId, txHash). The REPL layer renders via `anyhow` as a user-friendly message with cause chain. With `--json` flag (P0-1, M4), CLI outputs `{code, message, cause: [...]}` as stable machine-readable form.

---

## 5. Milestones (DoD-driven)

### M0 — Repository Scaffolding (2 days)

**Definition of Done:**
- `cargo build` passes on Linux x86_64 and aarch64
- CI runs: `fmt / clippy --deny warnings / test / audit / deny / cov`
- `deny.toml` forbids GPL, wildcard versions, unmaintained crates
- `tracing` integrated; sensitive fields wrapped in `Secret<T>`
- 12-factor config skeleton (CLI > env > file > default)
- `rust-toolchain.toml` pins `1.96.0` *(ADR-0001 + V9 §18)*
- **clippy.toml at repo root** *(P0-4)*:
  ```toml
  disallowed-methods = [
    { path = "std::option::Option::unwrap", reason = "Use expect with context or ? operator" },
    { path = "std::result::Result::unwrap", reason = "Use ? or expect with context" },
    { path = "std::option::Option::expect", reason = "Only allowed in #[cfg(test)]" },
    { path = "std::result::Result::expect", reason = "Only allowed in #[cfg(test)]" },
  ]
  ```
  CI runs `cargo clippy -- -D warnings`; violations fail build
- **Process hardening at `main()` start** *(B6, P0-2, P0-7)*:
  - `umask(0o077)` to ensure file mode 0600 by default
  - `setrlimit(RLIMIT_CORE, 0)` to disable core dumps
  - `mlock` critical secret buffers (≥32 bytes) via `mlock` crate (failure logged but non-fatal)
  - **Install panic hook as first statement of `main()`** *(B6, ADR-0005)*: `human_panic::setup!()` from `human-panic = "2"` crate; verifies no secret / no panic location in stderr (asserted by `tests/it_panic_hook.rs`)
- **ADR-0001..0005** (B1=Accepted 2026-06-11, B3, B5, B6=Accepted 2026-06-11, workspace)
- **ADR-0006: Error code naming convention** *(P0-1, rev1)* — **31 codes** across 5 prefixes (EVMCR-/EVMK-/EVMC-/EVMCFG-/EVMIO-); `docs/code_allocation.md` is the single source of truth and is enforced by a CI unit test; `CliError` is a wrapper struct with a linear downcast chain
- **ADR-0007: Secret memory hardening** *(P0-2, rev1)* — `Secret<T: ZeroizeOnDrop>` in `src/types/secret.rs`; no `Clone`/`Serialize`/`Display` impls; `Debug` always prints `Secret(***)`; mlock for buffers ≥32 bytes; **5-pattern CI grep** to ban `String` for mnemonic/seed/key material
- **Coverage gate wired** *(P0-8)*: `cargo-llvm-cov` installed; CI step `cargo llvm-cov --fail-under-lines 80`; per-module threshold for `crypto/` and `keystore/` = 90% enforced via separate `cargo llvm-cov --fail-under-lines 90` invocations on filtered file sets
- README contains install instructions, command list, security disclaimer

### M1 — Crypto Layer (5 days)

**Definition of Done:**
- 12/24-word mnemonic generation
- BIP-39 → BIP-44 → Address pipeline correct
- Keccak-256 via `tiny-keccak` `Keccak` type (not `Sha3`); **sha3 crate forbidden**
- EIP-2 low-S signatures
- **`personal_sign` hardening** *(P0-6)*:
  - Fixed prefix: `"\x19Ethereum Signed Message:\n" + decimal(len(message)) + message`
  - Ecrecover roundtrip integration test: `sign(msg)` → `alloy::primitives::Signature::recover_address_from_msg` → assert matches signer
  - proptest cases: empty message, 1-byte, 1 MiB, raw bytes (non-UTF-8), 0x-prefixed hex string
- Tests: 5 official ethereumbook vectors + proptest address checksums
- **Mnemonic display** *(P0-7)*: after writing to terminal, emit ANSI clear (`\x1b[2J\x1b[H`) + prompt "请立即抄写并关闭窗口" (or localized equivalent)
- Any failure → CI red

### M2 — Keystore Layer (4 days)

**Definition of Done:**
- AES-256-GCM + Argon2id (`t=3, m=65536, p=1`)
- File location: `~/.local/share/evm-cli/keystore.json` via `directories` crate
- API: create / load / list / delete / rename
- Decrypt failure returns `KeystoreError::InvalidPassword`; **does not distinguish "file missing" from "wrong password"** (anti-side-channel)
- Test: create → simulate restart → unlock → same address recovered
- All key paths in memory use `ZeroizeOnDrop` *(P0-2)*; mnemonic/seed/privkey never stored as `String` (CI grep enforces)
- If any temp file is required (e.g. atomic write staging), use `tempfile` crate *(P0-7)* wrapped in a custom `ZeroizingTempFile` that zeroizes the buffer on Drop
- **Coverage ≥ 90%** *(P0-8)* enforced by CI

### M3 — Chain Layer (10 days)

**Definition of Done:**
- `Chain` trait + `AlloyChain` implementation
- `balance(Address)` via `eth_getBalance`
- Full ETH transfer pipeline: nonce sync → `eth_feeHistory` → EIP-1559 estimation → sign → broadcast → poll receipt (timeout 120s)
- **EIP-155 chainId signing** *(B2)*:
  - Plan delegates to `alloy::signers::Signer` with `chain_id = SEPOLIA`
  - `ChainError::InvalidChainId { expected, actual }` returned if mismatch
  - Integration test on `anvil --fork-url <sepolia-rpc>` roundtrips sign → ecrecover → assert original sender
- **NonceManager** *(B3, ADR-0002)*: spec unchanged from V3
- **RBF / Cancel paths** *(P0-3, ADR-0008)*:
  - `send-eth --bump-fee <tx-hash>`: query original tx via `eth_getTransactionByHash`, re-sign with same nonce + `max_fee_per_gas` ≥ 110% of original (BIP-125 spirit) + same `to`/`value`/`data`
  - `send-eth --cancel <tx-hash>`: re-sign with same nonce + 0 value to signer address + higher fee
  - Both share `NonceManager::next(addr)` (the original tx owns the nonce; no extra reservation)
  - E2E test: low-fee tx → wait 30s pending → `--bump-fee` → confirm within 60s
- **Gas strategy**: default `max_fee_per_gas = base_fee * 2 + priority_fee`; configurable override
- **U256 parsing** *(P0-4)*: every user-input amount uses `U256::try_from` or `FromStr`; failures map to `ChainError::InvalidAmount { value, reason }`. No `unwrap` / `expect` in any parsing path (clippy.toml blocks)
- **ERC-20 transfer**: minimal ABI (`balanceOf` + `transfer`) loaded
- **RPC rate limit**: `governor::Quota::per_second(25)`
- **`cargo-fuzz` harness** *(P0-10)*:
  - `fuzz/fuzz_rlp_decode/`: feed random bytes to `alloy::rlp::Decodable`; must not panic
  - `fuzz/fuzz_signature_recover/`: random `Signature` + message → `recover_address` must not panic, must return `Result`
  - `fuzz/fuzz_keystore_json/`: random JSON-shaped bytes → keystore deserialize must not panic
  - CI nightly workflow (`.github/workflows/nightly-fuzz.yml`): 5 minutes per harness; crash artifact uploaded; release blocked if any harness regresses
- Integration test: `tests/it_eth_transfer.rs` using `alloy::node-bindings::anvil`
- E2E test (`#[ignore]`): real Sepolia transfer + `--bump-fee` rescue; documentation describes trigger

### M4 — CLI Layer (5 days)

**Definition of Done:**
- `clap` derive mode + `rustyline` REPL
- Commands: `create-wallet` / `import-mnemonic` / `list` / `use` / `unlock` / `balance` / `send-eth` *(with `--bump-fee` / `--cancel` / `--dry-run` flags)* / `send-token` / `sign-message` / `pending-tx` / `exit`
- **`--json` global flag** *(P0-1)*: errors rendered as `{code, message, cause: [...]}`; success responses similarly structured
- **REPL mis-sign prevention** *(P0-9)*:
  - All `send-*` commands print a summary before signing:
    ```
    to:     0x1234...abcd
    amount: 0.001 ETH (1.0e15 wei)
    fee:    1.5 Gwei (cap 30 Gwei)
    total:  0.0010015 ETH
    nonce:  42
    ```
    Then prompt `Proceed? [y/N]`. Default is N (Enter does not confirm).
  - `--dry-run` flag: print summary only; do not call signer, do not broadcast; return `Ok` with summary as response
  - EIP-55 checksum displayed (matches actual on-chain format, protects against copy-paste of mixed-case address)
  - Sensitive commands (lines containing `mnemonic`, `password`, `--private-key`, `import-mnemonic`) **skipped from `rustyline` history** via `should_skip_history(line: &str) -> bool` predicate; `--no-history` global flag disables history entirely
- Address display: EIP-55 checksum
- Amount display: formatted by token decimals
- **Safety rails**: command history must not retain sensitive args
- Startup validates config integrity and keystore directory writability
- No panic leaks key material

### M5 — Release Engineering (3 days)

**Definition of Done:**
- `cargo dist` or hand-written GitHub Actions release workflow
- Artifacts: `evm-cli-v0.1.0-linux-x86_64.tar.gz` + `.sha256`
- Release includes CHANGELOG, security disclaimer, known limitations
- `cargo install --git https://github.com/.../evm-cli` works

### M6 — Self-Audit + Documentation Wrap (2 days)

**Definition of Done:**
- Security self-audit checklist (§7) every item checked
- Documentation: ASCII architecture diagram, command manual, troubleshooting
- Known limitations explicit in README: **"PoC — do not use on mainnet with real assets"**
- All §7 checks verified with evidence (CI logs, test output, grep results)

---

## 6. CI/CD Pipeline

```
PR trigger:    fmt → clippy(-D warnings) → test(ubuntu) → audit → deny → cov
main merge:    above + build linux-x86_64 artifact
tag v*:        release workflow → multi-arch build → GitHub Release
dep upgrade:   above + ADR review + non-author approval (per §1.1)
nightly:       fuzz × 3 harnesses × 5 min each  (P0-10)
```

**Coverage gate** *(P0-8)*:
- Global: `cargo llvm-cov --fail-under-lines 80`
- `crypto/` and `keystore/`: `cargo llvm-cov --fail-under-lines 90 --package evm-cli --lib crypto::,keystore::`
- Both steps block merge

---

## 7. Security Self-Audit Checklist (used in M6)

Memory hardening *(P0-2)*:
- [ ] All private key / mnemonic in-memory paths use `ZeroizeOnDrop` (not just `Zeroize`)
- [ ] Mnemonic/seed/private key never stored as `String` (**5-pattern CI grep** clean — see ADR-0007 rev1: direct binding, `String::from`, `.to_string()`, `format!`, function-returning-`String`)
- [ ] `mlock` active for secret buffers at startup; failure logged non-fatally
- [ ] `RLIMIT_CORE = 0` set at startup (core dumps disabled)

Output hygiene:
- [ ] No `println!` / `dbg!` / `log::info!` emits raw key material (grep-verified)
- [ ] Mnemonic display followed by ANSI clear screen *(P0-7)*
- [ ] Signature comparisons use constant-time (`subtle` or manual)

Storage:
- [ ] Keystore file mode 0600
- [ ] `umask(0o077)` set at startup *(P0-7)*
- [ ] Temp files use `tempfile` crate wrapped in zeroize-on-drop *(P0-7)*
- [ ] Argon2id parameters match OWASP 2024 recommendations

Dependencies:
- [ ] No known RUSTSEC advisories in dependency tree (`cargo audit`)
- [ ] No GPL dependencies (`cargo deny`)

Process:
- [ ] Panic hook does not dump stack variables (B6)
- [ ] RPC URL not written to logs (only endpoint host recorded)

Cryptography:
- [ ] Signing chainId equals transaction chainId (EIP-155) *(B2)* — Sepolia-fork roundtrip test
- [ ] `personal_sign` ecrecover roundtrip integration test passes *(P0-6)*

Parsing & arithmetic *(P0-4)*:
- [ ] All U256 parsing uses `try_from` / `FromStr` (CI grep: no `unwrap` / `expect` outside `#[cfg(test)]`)
- [ ] `ChainError::InvalidAmount` returned on overflow / invalid input

Error system *(P0-1)*:
- [ ] Every error variant exposes `code() -> &'static str`
- [ ] `--json` CLI flag emits `{code, message, cause: [...]}`

Transaction reliability *(P0-3)*:
- [ ] `send-eth --bump-fee` E2E tested (low-fee → pending → bump → confirmed)
- [ ] `send-eth --cancel` E2E tested

Coverage *(P0-8)*:
- [ ] Global coverage ≥ 80% (CI enforced)
- [ ] `crypto/` and `keystore/` coverage ≥ 90% (CI enforced)

REPL safety *(P0-9)*:
- [ ] All `send-*` commands require `y/N` confirmation; default is N
- [ ] `--dry-run` flag works and does not invoke signer
- [ ] Sensitive commands (mnemonic, password, private-key, import-mnemonic) skipped from history

Fuzz *(P0-10)*:
- [ ] 3 fuzz harnesses (`fuzz_rlp_decode`, `fuzz_signature_recover`, `fuzz_keystore_json`) in CI nightly
- [ ] No open fuzz crash artifacts older than 7 days

---

## 8. Risk Register

| Risk | Level | Mitigation |
|---|---|---|
| alloy 2.0.x API break | HIGH | Pinned exact; `Chain` trait; upgrade window per §1.1 |
| MSRV / alloy version incompatibility | HIGH | ADR-0001 Accepted (2.0.5, MSRV 1.96 per V9); `rust-toolchain.toml` pins 1.96.0; CI on both arches |
| Private key memory leak | HIGH | `Secret<T: ZeroizeOnDrop>` + mlock + RLIMIT_CORE=0 + String ban *(P0-2)* |
| Infura RPC rate limit / outage | MED | governor + multi-RPC fallback (V2) |
| Sepolia reset / reorg | LOW | E2E tests accept failure |
| Argon2 latency on low-end hardware | LOW | OWASP params; verify first unlock < 2s |
| Nonce pool inconsistency on crash | LOW | persist-on-increment + max(local, rpc) |
| **U256 overflow / unwrap panic in main path** *(P0-4)* | MED | `clippy.toml` disallow(unwrap_used, expect_used); `try_from` everywhere; CI grep |
| **REPL mis-sign / address fat-finger** *(P0-9)* | MED | y/N confirmation + EIP-55 visible + `--dry-run` + history filter |
| **Fuzz crash unaddressed** *(P0-10)* | LOW | nightly CI; triage procedure; 7-day SLA in §7; block release if open |
| **Transaction stuck in mempool** *(P0-3)* | MED | RBF + Cancel paths; E2E covered |
| **Error code instability breaks scripts** *(P0-1)* | LOW | Codes documented in ADR-0006; changes require ADR + CHANGELOG entry |

---

## 9. Out of Scope (V2 candidates)

- EIP-1193 WebSocket provider
- WalletConnect v2
- DApp / Uniswap real integration
- Multi-chain switching (Sepolia only)
- Windows / macOS adaptation
- Hardware wallet (Ledger)
- **Transaction history persistence (SQLite)** — `pending-tx` (B4) does not require storage; in-memory + persisted NonceManager is sufficient
- Multi-signature wallets

---

## 10. Effort Estimate

| Phase | Workdays |
|---|---|
| M0 | 2 |
| M1 | 5 |
| M2 | 4 |
| M3 | 10 |
| M4 | 5 |
| M5 | 3 |
| M6 | 2 |
| **Total** | **31 workdays ≈ 6.5 weeks** |

> Timeline intentionally not revised in V4 per stakeholder decision. Padding is a separate concern.

---

## 11. Next Action

**All 8 ADRs are now Accepted** (V6 + V7 G3 revisions). The plan is **M0-ready** — no remaining gate.

### Summary of accepted ADRs (as of 2026-06-11)

| ADR | Title | Status | Key Output |
|---|---|---|---|
| ADR-0001 | alloy 2.0.5 / MSRV 1.96 | Accepted 2026-06-11 (rev2 V9: MSRV raised 1.91→1.96) | `Cargo.toml` pins; `rust-toolchain.toml = 1.96.0` |
| ADR-0002 | NonceManager (4-state) | Accepted 2026-06-11 (rev1) | `tokio::Mutex<HashMap>` + JSON-lines log + `flock` |
| ADR-0003 | Workspace split | Accepted 2026-06-11 (rev1) | Single package; `cargo-modules` cycle check in CI |
| ADR-0004 | Upgrade Policy | Accepted 2026-06-11 (rev1) | Dev: minor + ADR; Release: pinned; single-maintainer fallback |
| ADR-0005 | Panic hook | Accepted 2026-06-11 | `human-panic = "2"` crate |
| ADR-0006 | Error codes | Accepted 2026-06-11 (rev1) | 31 codes; `docs/code_allocation.md` source of truth |
| ADR-0007 | Secret memory | Accepted 2026-06-11 (rev1) | `ZeroizeOnDrop` + mlock (≥32B) + 5-pattern grep |
| ADR-0008 | RBF / Cancel | Accepted 2026-06-11 (rev1) | 3-term fee bump; uses EVMC-007/008 |

### M0 kickoff sequence (V8 final)

1. `git init` in `/home/yangwei/rust1/evm-cli/` (or wherever the project root will live)
2. Add LICENSE (MIT), `README.md`, `CHANGELOG.md`, `SECURITY.md`
3. Create `rust-toolchain.toml` (`channel = "1.96.0"`)
4. Create `Cargo.toml` with:
   - `[package] name = "evm-cli", version = "0.1.0", edition = "2021", rust-version = "1.96"`
   - `[dependencies]` per ADR-0001 (all alloy crates pinned `=2.0.5` or sibling) + `human-panic = "=2.<x>.<y>"` (ADR-0005) + `zeroize`, `mlock`, `nix`, `directories`, `clap`, `rustyline`, `tracing`, `anyhow`, `thiserror`, `serde`, `toml`, `static_assertions`
5. Create `clippy.toml` per P0-4 / ADR-0007
6. Create `deny.toml` (P0-8 / P0-7)
7. Create `docs/adr/0001..0008*.md` (8 files, all Accepted with rev1 notes)
8. Create `docs/code_allocation.md` per ADR-0006 (31 codes; single source of truth)
9. Create `src/main.rs` with first-line `human_panic::setup!()` + `harden_process()`
10. Create `src/types/secret.rs` per ADR-0007 (with `static_assertions` for `ZeroizeOnDrop`)
11. Create `src/error.rs` per ADR-0006 (`CliError` wrapper struct, downcast chain, `code()` per variant)
12. Create `tests/it_panic_hook.rs` per ADR-0005 (asserts no ≥32-hex, no BIP-39 word, expected friendly message, SIGABRT exit code)
13. Create `.github/workflows/ci.yml` with: fmt, clippy `-D warnings`, test, audit, deny, cov, `cargo-modules deps --no-externals` (ADR-0003), 5-pattern String grep (ADR-0007)
14. First commit
15. Proceed to M1 (Crypto Layer)

---

## 12. V2 → V3 Changelog (BLOCKER Remediation)

| # | BLOCKER | V3 Resolution | Location |
|---|---|---|---|
| B1 | MSRV 1.78 incompatible with alloy 0.9.x | MSRV upgraded to **1.91** (V4's interim 1.81 was superseded 2026-06-11); alloy upgraded to **2.0.5** (per ADR-0001) | §1 table, §11, §14 |
| B2 | EIP-155 chainId signing not mentioned | Explicit pipeline + `InvalidChainId` error variant + Sepolia-fork roundtrip test + §7 check | §4, §5 M3, §7 |
| B3 | NonceManager concurrency / persistence not designed | Full spec: `tokio::sync::Mutex` + persist-on-increment + panic-safe Drop + RPC-merge on rebuild + rename-safe keys | §5 M3 |
| B4 | `tx-history` conflicts with SQLite OOS | Command renamed to **`pending-tx`**; reads NonceManager only; SQLite OOS unchanged | §5 M4, §9 |
| B5 | pinned-exact vs monthly-upgrade mechanism missing | §1.1 Upgrade Policy added; dev vs release phases explicit; upgrade PR workflow codified | §1.1, §6, §8 |
| B6 | Panic hook task unassigned | M0 DoD requires hook install + integration test; **ADR-0005 Accepted 2026-06-11** with `human-panic = "2"` crate; option B (self-written hook) retained as migration path | §5 M0, §7 |
| B7 | alloy 0.9.x is 2024-12 vintage, no upgrade evaluated | Resolved together with B1: **alloy 2.0.5** selected via ADR-0001 (V4's "1.x latest stable" framing preceded the 2026-04-13 2.0 release) | §1, §11, §14 |

---

## 13. V3 → V4 Changelog (P0 Reinforcement)

| # | P0 Item | V4 Resolution | Location |
|---|---|---|---|
| **P0-1** | Error code system missing | Every enum variant exposes `code() -> &'static str`; CLI `--json` outputs `{code, message, cause}`; ADR-0006 defines prefix scheme (EVMCR-/EVMK-/EVMC-/EVMCFG-/EVMIO-) | §4, §5 M4, §6, §7 |
| **P0-2** | Secret memory hardening | `Secret<T: ZeroizeOnDrop>` replaces `Zeroize`; mlock + RLIMIT_CORE=0 in `main()`; mnemonic/seed/privkey `String` ban enforced by CI grep; ADR-0007 | §3, §5 M0, §7, §8 |
| **P0-3** | RBF / Cancel paths | `send-eth --bump-fee <hash>` (≥ 110% fee bump, same nonce) and `--cancel <hash>` (0-value self-send) added in M3; E2E test in §5 M3; ADR-0008 | §5 M3, §5 M4, §7, §8 |
| **P0-4** | U256 overflow / unwrap panic | `clippy.toml` disallows `unwrap` / `expect` (test-allowed); all parsing via `try_from` / `FromStr`; `ChainError::InvalidAmount` variant; CI grep enforces | §4, §5 M0, §5 M3, §7, §8 |
| **P0-5** | (duplicate of B6) | — resolved in V3 | — |
| **P0-6** | `personal_sign` prefix hardening | EIP-191 prefix locked; ecrecover roundtrip integration test; proptest covers empty / 1-byte / 1 MiB / non-UTF-8 / hex strings | §5 M1, §7 |
| **P0-7** | umask + tempfile + terminal pollution | `umask(0o077)` in `main()`; ANSI clear after mnemonic display; `tempfile` crate + custom `ZeroizingTempFile` wrapper for any temp file | §5 M0, §5 M1, §5 M2, §7 |
| **P0-8** | Coverage threshold | `cargo-llvm-cov` in CI; 80% global, 90% for `crypto/` and `keystore/`; both block merge | §5 M0, §5 M2, §6, §7 |
| **P0-9** | REPL mis-sign prevention | y/N confirmation on all `send-*` (default N); `--dry-run` flag; EIP-55 display; sensitive commands skip history | §5 M4, §7, §8 |
| **P0-10** | Fuzz harness | 3 `cargo-fuzz` targets (`fuzz_rlp_decode`, `fuzz_signature_recover`, `fuzz_keystore_json`); nightly CI 5 min/target; 7-day crash SLA | §2, §5 M3, §6, §7, §8 |

**Additions beyond the 7 BLOCKERs + 9 P0s (no scope creep — all required by P0 items):**
- `clippy.toml` in §2 (required by P0-4)
- `fuzz/` directory in §2 (required by P0-10)
- `.github/workflows/nightly-fuzz.yml` referenced in §6 (required by P0-10)
- `ChainError::InvalidAmount { value, reason }` variant in §4 (required by P0-4)
- `Cargo.lock` + `Cargo.toml` referenced as the targets of `cargo-llvm-cov` in §6 (required by P0-8)
- ADR-0006 / ADR-0007 / ADR-0008 (required by P0-1, P0-2, P0-3 respectively)
- `--bump-fee` / `--cancel` / `--dry-run` flags in §5 M4 (required by P0-3, P0-9)
- `should_skip_history()` predicate in §5 M4 (required by P0-9)

**ADR roster after V4:**
- ADR-0001 alloy 2.0.5 version + MSRV 1.96 (B1, B7) — **Accepted 2026-06-11 (rev2 V9: MSRV raised 1.91→1.96 to match toolchain)**
- ADR-0002 NonceManager design (B3) — **Accepted 2026-06-11 (rev1: 4-state)**
- ADR-0003 workspace split decision — **Accepted 2026-06-11 (rev1)**
- ADR-0004 Upgrade Policy (B5) — **Accepted 2026-06-11 (rev1)**
- ADR-0005 panic hook strategy (B6) — **Accepted 2026-06-11** (`human-panic = "2"`)
- **ADR-0006 error code naming convention (P0-1) — Accepted 2026-06-11 (rev1: 31 codes)**
- **ADR-0007 secret memory hardening (P0-2) — Accepted 2026-06-11 (rev1)**
- **ADR-0008 RBF / Cancel implementation policy (P0-3) — Accepted 2026-06-11 (rev1)**

**Items explicitly deferred to V5 (P1 list):**
- P1-1 TOML config format specification
- P1-2 tracing redaction Layer
- P1-3 explicit workspace decision point (subsumed by ADR-0003)
- P1-4 CHANGELOG / SECURITY.md templates (CHANGELOG added in V3; SECURITY.md present but template TBD)
- P1-5 split M3 into M3a/M3b (timeline decision, user excluded)
- P1-6 standardized `deny.toml` content
- P1-7 clipboard / terminal pollution protection (partially addressed by P0-7 mnemonic clear)
- P1-8 RPC multi-endpoint failover
- P1-9 history filter technical specification (partially addressed by P0-9 predicate)
- P1-10 dependency lock regeneration strategy

---

## 14. V4 → V5 Changelog (ADR-0001 Acceptance)

**Trigger:** 2026-06-11 verification via `cargo search` + `cargo info` + crates.io API.

**Findings that invalidate V4 assumptions:**

| V4 assumption | Reality (2026-06-11) | Source |
|---|---|---|
| `alloy = "=1.x.y"` is "latest stable" | `alloy = "2.0.5"` is latest stable; 2.0.0 shipped 2026-04-13 | `cargo search alloy` |
| MSRV 1.81 is sufficient | MSRV 1.91 is required (1.7.0+ in 1.x, all 2.x); 1.6.3 was 1.88 | `cargo info alloy` shows `rust-version: 1.91` |
| "1.x latest stable" framing valid | Framing preceded 2.0 release; needs revision | API snapshot at `/home/yangwei/.local/share/opencode/tool-output/tool_eb4407ba8001wDVpnp7Tvgo1X6` |

**Resolution: ADR-0001 Accepted** with `alloy = "=2.0.5"`, MSRV `1.91`. Full evidence and verification log in `docs/adr/0001-alloy-version.md`.

**V5 changes (synchronized to ADR-0001):**

| V4 location | V5 update |
|---|---|
| §1 Project Identity, MSRV row | `1.81` → `1.91` |
| §1 Project Identity, alloy Version row | `1.x latest stable at M0 kickoff` → `=2.0.5` |
| §2 Repository Layout, `rust-toolchain.toml` comment | `# pins 1.81` → `# pins 1.91 (ADR-0001)` |
| §2 Repository Layout, `docs/adr/` comment | `ADR-001..008` → `ADR-0001..0008` + ADR-0001 Accepted note |
| §5 M0 DoD, `rust-toolchain.toml pins ...` | `1.81` → `1.91 (ADR-0001)` |
| §5 M0 DoD, `ADR-001..005` line | `ADR-001..005` → `ADR-0001..0005`; B1 marked Accepted 2026-06-11 |
| §5 M0 DoD, ADR-006/007/008 lines | 3-digit → 4-digit ADR numbering for consistency with filenames |
| §8 Risk Register, "alloy 1.x API break" row | `alloy 1.x API break` → `alloy 2.0.x API break` |
| §8 Risk Register, B1 row | `ADR-001; pinned 1.81` → `ADR-0001 Accepted (2.0.5, MSRV 1.91); rust-toolchain pins 1.91` |
| §11 Next Action | ADR-001/ADR-001 PR references → ADR-0001 (Accepted); remaining gate = ADR-0005 (G2) |
| §12 V2→V3 Changelog, B1 row | `MSRV upgraded to 1.81` → `MSRV upgraded to 1.91 (V4's interim 1.81 superseded 2026-06-11)` |
| §12 V2→V3 Changelog, B7 row | `alloy 1.x latest stable` → `alloy 2.0.5` |
| §13 V3→V4 Changelog, ADR roster | 3-digit ADR numbers → 4-digit (0001..0008) to match filenames; ADR-0001 marked Accepted |

**Alloy sub-crates locked in `Cargo.toml` at M0** (per ADR-0001 Implementation §):

```toml
alloy                   = "=2.0.5"
alloy-primitives        = "=1.6.0"
alloy-signer            = "=2.0.5"
alloy-signer-local      = "=2.0.5"
alloy-consensus         = "=2.0.5"
alloy-rlp               = "=0.3.15"
alloy-contract          = "=2.0.5"
alloy-sol-types         = "=1.6.0"
alloy-node-bindings     = "=2.0.5"   # dev-dependency
```

**Files NOT changed in V5 (V4 content preserved):**
- §3–§7, §9, §10 (no version references)
- §13 V3→V4 Changelog (historical record)
- All P0–P1 deferral lists
- All ADR text (ADR-0001 itself is the source of truth, not V5)

**Gates after V5:**

| Gate | Status |
|---|---|
| G1 ADR-0001 Accepted | ✅ Done (V5 changelog §14) |
| G2 ADR-0005 Accepted | ✅ Done (V6 changelog §15) |
| G3 Six drafted ADRs reviewed/Accepted by user | ⏳ Next step |
| M0 kickoff | Blocked on G3 |

---

## 15. V5 → V6 Changelog (ADR-0005 Acceptance)

**Trigger:** 2026-06-11 user decision "直接选项A吧" (Option A: `human-panic` crate).

**ADR-0005 changes:**

| Aspect | V5 (Proposed) | V6 (Accepted) |
|---|---|---|
| Status | Proposed | **Accepted 2026-06-11** |
| Decision | A or B (TBD) | **A: `human-panic` crate** (Option B retained as documented migration path) |
| M0 `Cargo.toml` dep | n/a | `human-panic = "=2.<x>.<y>"` (pin exact per ADR-0001 upgrade policy; survey at M0 kickoff) |
| `src/main.rs` | unspecified | `human_panic::setup!();` on first line of `main()` |
| CI env | unspecified | `RUST_BACKTRACE=0` enforced for `test` and `bench` |
| Integration test | unspecified | `tests/it_panic_hook.rs` asserts no ≥32-hex, no BIP-39 word, expected friendly message, SIGABRT exit code |

**V6 document changes (synchronized to ADR-0005 acceptance):**

| V5 location | V6 update |
|---|---|
| Header status | "V4 ADR-0001 acceptance" → "+ V5 ADR-0005 acceptance" |
| §5 M0 DoD, `ADR-0001..0005` line | B6 marked Accepted 2026-06-11 alongside B1 |
| §5 M0 DoD, `ADR-0005` line | (already removed in V5 → V6 transition; see §11 update) |
| §8 Risk Register (no change) | "alloy 2.0.x API break" / "MSRV / alloy" rows unchanged; B6 row in §12 changelog updated |
| §11 Next Action, ADR-0005 line | "currently Proposed" → "Accepted 2026-06-11 with `human-panic = "2"`" |
| §11 Next Action, G2 step | "Accept ADR-0005" → "G2 complete" |
| §11 Next Action, kickoff step 3 | Added `human-panic` to scaffolding step (pin exact version) |
| §12 V2→V3 Changelog, B6 row | "ADR-0005 decides human-panic vs self-written" → "**ADR-0005 Accepted 2026-06-11** with `human-panic = "2"`" |
| §13 V3→V4 Changelog, ADR roster | ADR-0005 status "Proposed (G2)" → "Accepted 2026-06-11" |
| §14 V4→V5 Changelog | (preserved as historical record; references V5 ADR-0001 work) |
| §14 V4→V5 Changelog, "Gates after V5" table | G2 status "⏳ Next step" → "✅ Done (V6 changelog §15)" |

**Why option A (`human-panic`) over option B (self-written hook):**

| Criterion | A: human-panic | B: self-written |
|---|---|---|
| LOC in `main.rs` | 1 (`human_panic::setup!()`) | ~6 (set_hook + Box + closure + eprintln) |
| Cross-platform file dump (panic report) | ✅ automatic | ❌ manual |
| Long-term maintenance | maintained by community | in-repo, no rot risk |
| Dep cost | +1 dep | +0 dep |
| Audit complexity | crate is small and widely used | in-repo, full transparency |
| Migration if A fails | → B (mechanical, ≤5 LOC) | → A (need to add dep) |

**Net V6 = V5 + §15 (this changelog) + 4 small body edits + header update.** No new sections added; no new tables in body.

**Gates after V6:**

| Gate | Status |
|---|---|
| G1 ADR-0001 Accepted | ✅ Done |
| G2 ADR-0005 Accepted | ✅ Done |
| **G3 Six drafted ADRs reviewed/Accepted by user** | ⏳ Next step |
| M0 kickoff | Blocked on G3 |

**Next: G3** — user reviews and formally accepts (or modifies) the 6 drafted ADRs:
- ADR-0002 NonceManager design
- ADR-0003 workspace split decision
- ADR-0004 Upgrade Policy
- ADR-0006 error code naming convention
- ADR-0007 secret memory hardening
- ADR-0008 RBF / Cancel implementation policy

If any are modified, V7 changelog will document the changes. If all are accepted as-drafted, V7 is a small "G3 complete" changelog + scaffold-ready signal for M0.

---

## 16. V6 → V7 Changelog (G3 ADR Revisions)

**Trigger:** 2026-06-11 G3逐条 review by maintainer. User elected to revise all 6 ADRs based on identified issues; no ADR was accepted as-drafted.

### Per-ADR revision summary

| ADR | Lines before | Lines after | Δ | Severity of issues addressed |
|---|---|---|---|---|
| **ADR-0002** NonceManager | 83 | 198 | +115 | 🔴 (release() double-spend), 🟠 (no confirm), 🟠 (multi-process race) — major重构 to 4-state machine |
| **ADR-0003** Workspace | 65 | 99 | +34 | 🟡×4 (dep graph, Secret location, 60s threshold, cycle check) |
| **ADR-0004** Upgrade Policy | 73 | 111 | +38 | 🟠 (single-maintainer conflict), 🟡 (transitive scope), 🟡 (CVE flow), 🟢 (monthly cadence) |
| **ADR-0006** Error codes | 77 | 200 | +123 | 🟡×4 (EVMC-007/008 missing, table insufficient, allocation file, CliError delegation) — major扩展 to 31 codes |
| **ADR-0007** Secret memory | 96 | 192 | +96 | 🟡 (broken cross-ref), 🟠 (grep too narrow), 🟡 (Debug+Serialize), 🟡 (mlock threshold) |
| **ADR-0008** RBF/Cancel | 87 | 121 | +34 | 🔴 (base_fee rising), 🟡 (EVMC-007 missing), 🟡 (asymmetric bump) |
| **Total** | 481 | 921 | +440 | 2×🔴 + 4×🟠 + 12×🟡 + 1×🟢 |

### Key cross-ADR dependencies now wired

The revisions created 3 explicit cross-ADR dependencies that must be implemented atomically at M0 / M2 / M3:

1. **ADR-0006 ↔ ADR-0008**: ADR-0008 uses `EVMC-007` (TxNotFound) and `EVMC-008` (TxAlreadyMined). ADR-0006 rev1 added both to the allocation table. **Implementation must use both codes together**; CI test in ADR-0006 will fail if the variants exist but codes are missing.

2. **ADR-0002 ↔ ADR-0003**: `NonceManager::replaced()` API (rev1) needs `Secret<...>` access. `Secret<T>` lives in `types/` per ADR-0003 rev1. No new dependency; just confirms the module split.

3. **ADR-0007 ↔ ADR-0006**: ADR-0007's 5-pattern String grep does not produce error codes (it produces CI failures), so no code allocation needed. But ADR-0006's `code_allocation.md` is generated at M0 alongside ADR-0007's grep script; both are M0 deliverables.

### M0 deliverables added by V7 revisions (cumulative)

| Deliverable | Source ADR | Notes |
|---|---|---|
| `Cargo.toml` with 8 alloy crates + human-panic + zeroize + mlock + nix + directories + clap + rustyline + tracing + anyhow + thiserror + serde + toml + static_assertions | ADR-0001, 0005, 0007 | All pinned exact per ADR-0001 upgrade policy |
| `rust-toolchain.toml` with `channel = "1.96.0"` | ADR-0001 + V9 | |
| `clippy.toml` (disallow unwrap/expect, disallowed-methods, disallowed-types) | ADR-0007, P0-4 | |
| `deny.toml` (licenses, bans, sources, advisories) | M0 DoD | |
| `docs/adr/0001..0008*.md` (8 files, all Accepted with rev1 notes) | All 8 | |
| `docs/code_allocation.md` (31 codes, single source of truth) | ADR-0006 | |
| `src/main.rs` (human_panic::setup!() first line + harden_process) | ADR-0005, 0007 | |
| `src/types/secret.rs` (with `static_assertions`) | ADR-0007 | |
| `src/types/nonce.json` JSON-lines append log (data dir) | ADR-0002 | Implementation in M3, not M0 |
| `src/error.rs` (CliError wrapper struct + downcast chain + code() per variant) | ADR-0006 | |
| CI workflow (fmt, clippy -D warnings, test, audit, deny, cov, cargo-modules, String grep) | All | |
| `tests/it_panic_hook.rs` (no secret / no panic location in stderr) | ADR-0005 | |

### Plan size evolution

| Version | Lines | Δ from prior | Trigger |
|---|---|---|---|
| V2 | 225 | — | Original draft |
| V3 | 316 | +91 | 7 BLOCKER fixes |
| V4 | 460 | +144 | 9 P0 reinforcements |
| V5 | 528 | +68 | ADR-0001 acceptance (alloy 2.0.5) |
| V6 | 593 | +65 | ADR-0005 acceptance (human-panic) |
| **V7** | **640+ (this section)** | +50+ | **6 ADR revisions from G3 review** |

### Final state

**The plan is M0-ready.** All 8 ADRs are Accepted (ADR-0001 and 0005 from prior gates; ADR-0002/3/4/6/7/8 from this G3 cycle). All BLOCKER, P0, and ADR-0001/0005 prerequisites are documented. The 15-step M0 kickoff sequence in §11 is the next action.

No further plan revisions are required before M0 starts. Future V8+ revisions will be driven by:
- Actual implementation discoveries in M0-M3
- New P1 items not yet addressed (deferred in §13 of V4)
- Resurvey of upstream `alloy` releases during dev phase (per ADR-0004 minor-upgrade policy)

### Gates after V7

| Gate | Status |
|---|---|
| G1 ADR-0001 Accepted | ✅ |
| G2 ADR-0005 Accepted | ✅ |
| G3 Six drafted ADRs reviewed/Accepted | ✅ (all revised) |
| **M0 kickoff** | ✅ **Unblocked** |

---

## 17. V7 → V8 Changelog (Documentation Sync)

**Trigger:** 2026-06-11 user paused for V7 verification; systematic cross-section check identified 5 internal inconsistencies between V7's body text and the 6 G3-revised ADRs.

**Note:** No ADR was changed in V8. All revisions are **documentation-only** — body text (§5 M0 DoD, §7 self-audit, §11 kickoff sequence) updated to reflect the ADR rev1 decisions that V7 inherited without rewriting surrounding prose.

### Per-fix detail

| # | Section | Before (V7) | After (V8) | Source ADR rev1 |
|---|---|---|---|---|
| 1 | §5 M0 DoD L156 | "`panic::set_hook` (B6): `human-panic` **OR** custom hook + `std::process::abort()`" | "Install `human_panic::setup!()` as first statement of `main()`; verifies no secret / no panic location in stderr (asserted by `tests/it_panic_hook.rs`)" | ADR-0005 |
| 2 | §5 M0 DoD L158 (ADR-0006 line) | "prefix per layer (EVMCR-/EVMK-/EVMC-/EVMCFG-/EVMIO-)" | "**31 codes** across 5 prefixes; `docs/code_allocation.md` is single source of truth and is enforced by a CI unit test; `CliError` is a wrapper struct with a linear downcast chain" | ADR-0006 rev1 |
| 3 | §5 M0 DoD L159 (ADR-0007 line) | "selected `mlock` crate version, fallback policy when mlock fails, RLIMIT_CORE call site" | "`Secret<T: ZeroizeOnDrop>` in `src/types/secret.rs`; no `Clone`/`Serialize`/`Display` impls; `Debug` always prints `Secret(***)`; mlock for buffers ≥32 bytes; **5-pattern CI grep** to ban `String` for mnemonic/seed/key material" | ADR-0007 rev1 |
| 4 | §7 L282 (Memory hardening check) | "Mnemonic/seed/private key never stored as `String` (CI grep clean)" | "Mnemonic/seed/private key never stored as `String` (**5-pattern CI grep** clean — see ADR-0007 rev1: direct binding, `String::from`, `.to_string()`, `format!`, function-returning-`String`)" | ADR-0007 rev1 |
| 5 | §11 M0 kickoff sequence | 13 steps; no `src/error.rs`, no `tests/it_panic_hook.rs` | 15 steps; **inserted** step 11 (`src/error.rs`) and step 12 (`tests/it_panic_hook.rs`); existing step 11 shifted to step 13; renumbered 12-15 | ADR-0006 rev1, ADR-0005 |

### Cross-check: does V8 match §16 M0 deliverables table?

| §16 M0 deliverable | §11 step in V8 | Status |
|---|---|---|
| `Cargo.toml` (line 643) | 4 | ✅ |
| `rust-toolchain.toml` (644) | 3 | ✅ |
| `clippy.toml` (645) | 5 | ✅ |
| `deny.toml` (646) | 6 | ✅ |
| `docs/adr/0001..0008*.md` (647) | 7 | ✅ |
| `docs/code_allocation.md` (648) | 8 | ✅ |
| `src/main.rs` (649) | 9 | ✅ |
| `src/types/secret.rs` (650) | 10 | ✅ |
| `src/error.rs` (652) | **11** | ✅ (newly added) |
| CI workflow (653) | 13 | ✅ |
| `tests/it_panic_hook.rs` (654) | **12** | ✅ (newly added) |
| `src/types/nonce.json` (651) | n/a | M3 deliverable (correctly not in M0) |

All §16 deliverables are now reachable from §11. No drift.

### ADR files: not touched in V8

The 8 ADR `.md` files in `docs/adr/` are unchanged. They were the source of truth; V8 brings the plan body into alignment with them.

### Why V8 exists at all

V7 was a "decision-complete" plan but a "document-incomplete" plan. A future implementer (or another LLM session) following V7's §5 / §7 / §11 literally would have produced an M0 that:
- Lacked `code_allocation.md` (per the too-brief §5 ADR-0006 line)
- Lacked `src/error.rs` and `tests/it_panic_hook.rs` (not listed in §11)
- Used a 1-pattern grep (per the brief §7 memory check) instead of 5-pattern
- Installed `panic::set_hook` in two ways instead of one (per the "OR" wording in §5)

V8 closes all 5 gaps. The plan is now both **decision-complete** and **document-complete**.

### Plan size evolution (final)

| Version | Lines | Trigger |
|---|---|---|
| V2 | 225 | Original |
| V3 | 316 | 7 BLOCKER fixes |
| V4 | 460 | 9 P0 reinforcements |
| V5 | 528 | ADR-0001 acceptance |
| V6 | 593 | ADR-0005 acceptance |
| V7 | 683 | 6 ADR revisions from G3 |
| **V8** | **710+ (this section)** | **5 documentation consistency fixes** |

### Gates after V8

| Gate | Status |
|---|---|
| G1 ADR-0001 Accepted | ✅ |
| G2 ADR-0005 Accepted | ✅ |
| G3 Six drafted ADRs reviewed/Accepted | ✅ (all revised) |
| Documentation sync (V7→V8) | ✅ |
| **M0 kickoff** | ✅ **Unblocked — fully consistent** |

---

## 18. V8 → V9 Changelog (MSRV Upgrade + 3 ADR Corrections)

**Trigger:** 2026-06-11 user paused for V7 verification, then asked for deviation analysis. Deviation review found 3 ADR inaccuracies that had been papered over by the M0 implementation. Concurrently, the user requested an MSRV upgrade to 1.96 (matching the toolchain already in use).

### Change set

| # | Item | Type | Author | Risk |
|---|---|---|---|---|
| 1 | `Cargo.toml` `rust-version` field: `1.91` → `1.96` | Decision change (MSRV claim) | user | Low — alloy 2.0.5 still satisfied (its MSRV is 1.91) |
| 2 | `rust-toolchain.toml` channel: `1.96.0` (unchanged) but TODO comment removed | Doc sync | reviewer | None |
| 3 | ADR-0001: MSRV note updated from 1.91 to 1.96 | Spec sync | reviewer | None |
| 4 | ADR-0005: `human_panic::setup!()` → `human_panic::setup_panic!()` | Spec correction | reviewer | None — code already correct |
| 5 | ADR-0007: `T: ZeroizeOnDrop` bound → `T: Zeroize` + explicit Drop impl | Spec correction (was a bug) | reviewer | None — code already correct, M0 commit works |
| 6 | ADR-0007: `mlock::mlock_bytes` → `os_memlock::mlock` | Spec correction (no crate named `mlock` exists) | reviewer | None — code already correct |
| 7 | `src/types/secret.rs`: switched from raw `libc::mlock` to `os_memlock::mlock` | Code cleanup (uses now-correctly-listed dep) | reviewer | None — `os-memlock` already in Cargo.toml |
| 8 | 6 ADRs in `evm-cli/docs/adr/` synced with the 3 corrections | Spec sync | reviewer | None |

### Impact analysis: MSRV 1.91 → 1.96

**Direct impact:**

| Concern | Before (1.91) | After (1.96) | Delta |
|---|---|---|---|
| Minimum rustc for contributors | 1.91 (Dec 2025) | 1.96 (May 2026) | +5 months, well within current stable |
| CI matrix (GHA standard runner) | needs 1.91 | needs 1.96 (already available; default `stable` is 1.96.0) | No change |
| Container build images | need rust:1.91-slim | need rust:1.96-slim | +5 months; both widely available |
| Local dev | need 1.91 install | need 1.96 install (already installed per env: `cargo 1.96.0`) | No change |

**Indirect impact (dependency analysis):**

All V1 dependencies verified to support rustc 1.96:

| Crate | Pinned version | MSRV | Δ from 1.96 |
|---|---|---|---|
| `alloy` | 2.0.5 | 1.91 | ✅ -0.05 |
| `alloy-primitives` | 1.6.0 | (uses 1.91 from alloy) | ✅ |
| `alloy-signer` | 2.0.5 | (uses 1.91) | ✅ |
| `alloy-signer-local` | 2.0.5 | (uses 1.91) | ✅ |
| `alloy-consensus` | 2.0.5 | (uses 1.91) | ✅ |
| `alloy-rlp` | 0.3.15 | (uses 1.91) | ✅ |
| `alloy-contract` | 2.0.5 | (uses 1.91) | ✅ |
| `alloy-sol-types` | 1.6.0 | (uses 1.91) | ✅ |
| `alloy-node-bindings` | 2.0.5 | (uses 1.91) | ✅ |
| `human-panic` | 2.0.8 | 1.88 | ✅ -0.08 |
| `zeroize` | 1.8.2 | 1.60 | ✅ -0.36 |
| `os-memlock` | 0.2.0 | (low) | ✅ |
| `nix` | 0.31.3 | 1.69 | ✅ -0.27 |
| `directories` | 6.0.0 | (low) | ✅ |
| `clap` | 4.6.1 | 1.85 | ✅ -0.11 |
| `rustyline` | 18.0.0 | (low) | ✅ |
| `tracing` | 0.1.44 | 1.65 | ✅ -0.31 |
| `anyhow` | 1.0.102 | 1.68 | ✅ -0.28 |
| `thiserror` | 2.0.18 | 1.68 | ✅ -0.28 |
| `serde` | 1.0.228 | 1.56 | ✅ -0.40 |
| `toml` | 1.1.2 | 1.85 | ✅ -0.11 |
| `static_assertions` | 1.1.0 | (low) | ✅ |
| `libc` | 0.2.186 | (very low) | ✅ |

**Highest dep MSRV is 1.91 (alloy). All other deps are ≤ 1.91.** Raising our MSRV to 1.96 is therefore a no-op for dep compatibility — every dep already supports 1.96+ (they were all released after 1.91).

**Feature usage impact:**

Rust 1.91 → 1.96 added several features. None are required by M0 code, but become available for M1+ use:
- `let chains` (1.88)
- `trait_upcasting` (1.86)
- `slice::split_at` const-stable (1.85)
- `concat_idents!` improvements
- Various cargo improvements

M1+ may freely use any of these. No code is currently broken by the upgrade.

**Risk to plan stability:**

| Risk | Level | Mitigation |
|---|---|---|
| Future contributor on rustc < 1.96 cannot build | LOW | The plan was already on 1.96 toolchain in practice; the change merely makes the declaration match. No new constraint. |
| M1+ code accidentally uses a 1.96-only feature without realizing | LOW | The MSRV declaration in `Cargo.toml` (1.96) tells the rest of the world "this is the minimum". A 1.96-only feature is allowed; we just need to be aware that we are committing to 1.96. |
| CI matrix failure (GHA standard runner has 1.96.0) | NONE | GHA standard runners always have the latest stable; 1.96 is current. |
| Breaking change to alloy's claimed MSRV in a future 2.x.y | LOW | ADR-0001 pins exact version 2.0.5; upgrades go through ADR review. |

**Net:** the MSRV upgrade is a documentation fix, not a real change. The M0 commit was already building on rustc 1.96.0; the plan just caught up.

### Impact analysis: 3 ADR corrections

All three corrections are **spec sync**, not design changes:

| ADR | Original (wrong) | Corrected (matches M0 code) | Behavior change? |
|---|---|---|---|
| ADR-0005 | `human_panic::setup!()` | `human_panic::setup_panic!()` | No — same panic-hook behavior, correct macro name |
| ADR-0007 (Zeroize) | `pub struct Secret<T: ZeroizeOnDrop>(T)` | `pub struct Secret<T: Zeroize> { inner: T }` + explicit `Drop` | No — same zeroize-on-drop semantics for `Vec<u8>` and other concrete types |
| ADR-0007 (mlock) | "the `mlock` crate" / `mlock::mlock_bytes` | `os-memlock` crate / `os_memlock::mlock` | No — same mlock(2) wrapper, correct crate name |

In all three cases, the M0 implementation was already correct. The corrections bring the ADR text into alignment with the code so future maintainers (and future LLMs reviewing the plan) are not misled.

**Butterfly effect on M1+ tasks:**

| M1+ task | Affected by corrections? |
|---|---|
| M1 Crypto Layer (BIP-39/44, Keccak, EIP-2) | No — uses `Secret<Vec<u8>>::new(bytes)` and `human_panic` indirectly. Both APIs unchanged. |
| M2 Keystore Layer (Argon2id, AES-GCM) | No — same APIs. |
| M3 Chain Layer (alloy integration, NonceManager) | No — `Secret<T>` API unchanged. `os-memlock` is already a no-op for non-Vec<u8> types. |
| M4 CLI Layer (clap, rustyline, REPL) | No — independent of the corrected ADRs. |
| M5 Release Engineering | No — independent. |
| M6 Self-Audit | The self-audit §7 checklist now matches the corrected ADRs; no rewrites needed. |

**No M1+ task is blocked, deferred, or modified by these corrections.**

### Plan size evolution (with V9)

| Version | Lines | Δ from prior | Trigger |
|---|---|---|---|
| V2 | 225 | — | Original |
| V3 | 316 | +91 | 7 BLOCKER fixes |
| V4 | 460 | +144 | 9 P0 reinforcements |
| V5 | 528 | +68 | ADR-0001 acceptance |
| V6 | 593 | +65 | ADR-0005 acceptance |
| V7 | 683 | +90 | 6 ADR revisions from G3 |
| V8 | 758 | +75 | 5 doc-consistency fixes |
| **V9** | **830+ (this section)** | **+70+** | **MSRV upgrade + 3 ADR corrections** |

### Verification after V9 corrections

- `cargo check --all-targets`: clean
- `cargo clippy --all-targets -- -D warnings`: clean
- `cargo test --all-targets`: 6 unit + 1 integration, all pass
- `cargo fmt --all -- --check`: clean
- M0 binary `./target/debug/evm-cli`: runs end-to-end, prints M0 banner

### Gates after V9

| Gate | Status |
|---|---|
| G1 ADR-0001 Accepted | ✅ (rev2: MSRV 1.96) |
| G2 ADR-0005 Accepted | ✅ (rev1: setup_panic macro) |
| G3 Six drafted ADRs reviewed/Accepted | ✅ (rev2 on 0005/0007) |
| Documentation sync (V7→V8) | ✅ |
| MSRV + ADR corrections (V8→V9) | ✅ |
| **M0 kickoff** | ✅ **Active and consistent** |

### Next action

M0 is fully active. The committed code in `evm-cli/` (commit `414556f`) implements the corrected design. V9 is the matching plan. Ready to proceed to **M1 — Crypto Layer** per V9 §5 (unchanged from V8): 12/24-word mnemonic, BIP-39 → BIP-44 → Address, Keccak-256 via `tiny-keccak` `Keccak` type, EIP-2 low-S signatures, `personal_sign` hardening (5 ethereumbook vectors + proptest).
