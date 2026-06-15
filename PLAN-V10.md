# evm-cli V2 — Implementation Plan v10

> Status: **DRAFT (V1 closed at commit `74f0aa6`; v0.2.1 released 2026-06-12)**
> Scope: V2 incremental — closes V1 gaps, expands UX, opens multi-chain / real-time / persistence / cross-platform.
> Author: V1 release-engineering lead (M5/M6 owner)
> Supersedes: PLAN-V9 §9 (Out of Scope / V2 candidates) for the in-scope items; V3+ items remain candidates.
> Predecessor: [PLAN.md](./PLAN.md) (V1 plan, V9 content).

---

## 1. V1 Recap

| Milestone | Status | Commit |
|---|---|---|
| M0 Scaffolding | ✅ | `414556f` |
| M1 Crypto Layer | ✅ | `28b60de` |
| M2 Keystore Layer | ✅ | `d442521` (with ADR-0009 deviation) |
| M3 Chain Layer | ✅ | `7e37bd7` |
| M4 CLI Layer | ✅ | `be5a489` + `fa3cb1b` (polish) |
| M5 Release Engineering | ✅ | `b75117f` (+ `2d7f7d1`, `d113404` post-release fixes) |
| M6 Self-Audit + Docs | ✅ | `163f9a8` (+ `74f0aa6` V1 cleanup) |

**v0.2.1 released:** https://github.com/glimjoe/evm-cli/releases/tag/v0.2.1
- 162 tests passing / 0 failed / 1 ignored (Sepolia bump E2E)
- 4 release artifacts (x86_64 + aarch64, .tar.gz + .sha256)
- `docs/audit/M6.md`: 27 PASS / 2 DEVIATION / 1 FAIL (P0-8 global coverage)

### V1 Known gaps (carried forward)

| Item | Source | Severity |
|---|---|---|
| **P0-8 global coverage** (61.46% < 80% target) | `cli/commands.rs` 0%, `chain/alloy_chain.rs` 57%, `chain/rbf.rs` 45% — all require anvil | MED |
| **Live REPL json/human toggle** (per M4 polish forward-look) | Stretch from M4 DoD | LOW |
| **Clipboard support** (P0-7 stretch) | Stretch from M4 DoD | LOW |
| **ERC-20 broadcast via calldata** (V1 builds value=0 envelope; calldata computed but not broadcast) | See ADR-0010 future | LOW |
| **glibc 2.38+ requirement** for prebuilt binaries | CI builds on Ubuntu 24.04 (`ubuntu-latest`); older LTS needs source build | LOW (documented in README) |
| **`actions/checkout@v4` Node 20 deprecation** | Fixed in `74f0aa6` (bumped to v6) | ✅ closed |

---

## 2. V2 Principles

Inherited from V1 (ADR-0003, ADR-0007, ADR-0008, the four P0 disciplines):

1. **No-magic single-package.** No `cargo dist`, no `xtask`, no workspace split. Direct shell scripts + hand-written YAML.
2. **TDD discipline.** RED → GREEN → REFACTOR on every milestone. Coverage gates stay at ≥ 80% global / ≥ 90% crypto+keystore.
3. **Security first.** Memory hardening (ADR-0007), zeroize-on-drop, no `String` for secret material. Every V2 surface audited.
4. **Backward compatibility.** v0.2.x → v0.3.x → v0.4.x with documented deprecations; no breaking CLI flag changes without an ADR.

### V2-specific adds

5. **Multi-chain by design.** v2.0.0 removes the hardcoded Sepolia `chain_id` (V1 §1 mandate, `src/cli/session.rs:97`); `--chain-id` becomes a first-class flag with sane defaults.
6. **Testability is a feature, not an after-thought.** M7 (P0-8 closure) extracts pure functions from `cli/commands.rs`; subsequent M8–M11 features must follow the same pattern (TDD-friendly module boundaries).
7. **Platform-aware, not platform-bound.** M12 introduces Windows + macOS, but Linux remains the primary. CI matrix grows; per-platform failure is non-blocking for the other platforms (warning, not error, until V3).

---

## 3. V2 Scope

### 3.1 In scope (M7–M12)

| # | Milestone | Effort | Version | DoD type |
|---|---|---|---|---|
| **M7** | P0-8 Gap Closure | 1–2 days | v0.3.0 (MINOR) | Refactor + coverage |
| **M8** | UX Polish | 1–2 days | v0.3.1 (PATCH) | Features |
| **M9** | Multi-Chain Foundation | 3–5 days | v0.4.0 (MINOR) | Feature + config |
| **M10** | EIP-1193 WebSocket provider | 2–3 days | v0.4.1 (PATCH) | Feature |
| **M11** | Transaction History (SQLite) | 2–3 days | v0.5.0 (MINOR) | New dep + feature |
| **M12** | Cross-Platform (Windows + macOS) | 5–7 days | v0.6.0 (MINOR) | CI matrix |
| | **V2 total** | **14–22 days ≈ 3–4 weeks** | | |

### 3.2 Deferred to V3+

| Candidate | Reason for deferral |
|---|---|
| Hardware wallet (Ledger) | Requires `ledger-transport` crate; per-device testing harness; out of Sepolia PoC scope |
| WalletConnect v2 | Requires `walletconnect-sdk` Rust binding (immature ecosystem); dApp UX is a different design exercise |
| DApp / Uniswap integration | Depends on WalletConnect (or WalletConnect-equivalent); contractual UX is a separate spec |
| Multi-signature wallets | Safe{Core} SDK integration; fundamentally a different key model |
| Transaction simulation (`eth_call` pre-broadcast) | V2 stretch; UX win but no new capability |

### 3.3 Explicitly out of V2 scope

- Layer-2 networks (Optimism, Arbitrum, Base, ...): each needs its own chain config + fee model
- MEV protection (Flashbots Protect): requires a separate RPC and an SDK
- Smart contract deployment: V1 is wallet-only; deployment is a separate product
- Browser extension: different distribution model

---

## 4. V2 Dependency Graph

```
M7 ─┐
    ├─→ M8 ─┐
            ├─→ M9 ─┬─→ M10
            │       └─→ M11
            └─→ M12 (orthogonal; can start any time after M7)
```

- **M7 is the critical-path start.** Closes V1's only known gap and establishes the pure-function pattern that all later milestones follow.
- **M8 and M12 can run in parallel** to M9 after M7.
- **M9 is the foundation** for M10 and M11 (both depend on chain-id being a first-class config).
- **M12 is orthogonal** — adds runners, doesn't touch code much. Can start as soon as M7 closes (no need to wait for M8/M9).

---

## 5. V2 Milestones

### M7 — P0-8 Gap Closure (v0.3.0, 1–2 days)

**Goal:** Close V1's only ❌ FAIL audit item. Global line coverage ≥ 75%.

**Definition of Done:**
- `src/cli/commands.rs` split into:
  - **Pure functions** (no I/O, no async, no global state): `format_send_summary`, `parse_amount_decimal`, `validate_address`, `build_send_args`, `derive_bump_fee_table`, …
  - **Orchestrators** (call pure functions + side effects): `cmd_send_eth`, `cmd_send_token`, `cmd_sign_message`, …
- ≥ 10 new unit tests for the pure functions (boundary, edge cases, error paths).
- `cargo llvm-cov --lib` reports global ≥ 75% (V1 was 61.46%).
- `cargo llvm-cov --fail-under-lines 90 --lib crypto::,keystore::` still passes.
- All 162 V1 tests still pass (no behavior change).

**TDD path:**
1. RED: write 10 tests against `format_send_summary` (varying fees, amounts, nonces).
2. GREEN: extract the formatter; move it to a `pure` module; rewire the call site.
3. REFACTOR: same pattern for 4–5 more extractable functions.

**Risk:** Low. Pure refactor; no API change.

**ADR:** None (follows existing `clippy.toml` and `tests/it_release_workflow.rs` patterns).

---

### M8 — UX Polish (v0.3.1, 1–2 days)

**Goal:** Three small UX wins from V1 forward-look.

**Definition of Done:**
- **Live REPL json/human toggle.** New command `:mode json|human` inside the REPL. State persists for the session. Default: human. Documented in [docs/commands.md](./docs/commands.md).
- **Clipboard support (P0-7 stretch).** New output mode `clipboard`: copies the resulting signature / signed-tx to the system clipboard. Platform-aware: `pbcopy` (macOS), `xclip`/`wl-copy` (Linux), `clip.exe` (Windows — paired with M12). Document the missing-tool behavior in [docs/troubleshooting.md](./docs/troubleshooting.md).
- **ERC-20 broadcast via calldata.** Remove the V1 workaround in `src/chain/erc20.rs` that builds a `value=0` envelope. Broadcast the actual calldata (`transfer(to, amount)`) as a real on-chain call. Covered by an anvil integration test that asserts the recipient's ERC-20 balance changes.

**TDD path:**
- `:mode` toggle: unit test on a `ReplMode` state machine.
- Clipboard: trait-abstracted `Clipboard` impl per platform; unit-test the trait with a mock.
- ERC-20 broadcast: extend `tests/it_eth_transfer.rs` with a token transfer assertion.

**Risk:** Medium. Clipboard needs platform detection; on minimal Linux installs (`xclip` absent) it must degrade gracefully (warning, not error). New `directories` lookups for the tool path.

**ADR:** ADR-0015 — Clipboard abstraction (per-platform trait).

---

### M9 — Multi-Chain Foundation (v0.4.0, 3–5 days)

**Goal:** Remove the hardcoded Sepolia `chain_id`; add mainnet as a first-class supported chain.

**Definition of Done:**
- `src/cli/session.rs:97` hardcoded Sepolia check **removed**. Chain config is now a struct (`ChainConfig { chain_id, name, rpc_url_default, explorer_url, native_symbol, ... }`) loaded from the same 12-factor chain (CLI > env > TOML > default).
- A `chains` registry: Sepolia (default), Mainnet, Holesky, Base Sepolia. (Mainnet is documented as a V2-PoC capability, **not** production-ready; the existing security warnings in README apply.)
- `ChainError::InvalidChainId` extended with a "supported chains" hint in the message.
- `cmd_create_wallet` and `cmd_sign_message` work on any chain (no Sepolia-specific code).
- New `ChainConfig` unit tests; chain-switching integration test via anvil fork of mainnet.
- [docs/commands.md](./docs/commands.md): new "Chain switching" section.
- [docs/architecture.md](./docs/architecture.md) ASCII diagram updated to show the `chains` registry as a new module.

**TDD path:**
1. RED: write tests for `ChainConfig::load` (CLI > env > TOML > default precedence).
2. GREEN: implement the config layer.
3. REFACTOR: extract a `chains` module; add the registry; rewire the call sites.

**Risk:** Medium. Multi-chain forces re-examination of any chain-specific assumption (fee defaults, EIP-1559 vs legacy, native symbol display). The registry is the right abstraction; per-chain quirks are caught by tests.

**ADR:** ADR-0011 — Multi-chain registry & defaults.

---

### M10 — EIP-1193 WebSocket provider (v0.4.1, 2–3 days)

**Goal:** Replace / augment the HTTP RPC client with a WebSocket transport for real-time features.

**Definition of Done:**
- `Chain` trait gains `subscribe_blocks(callback)` and `subscribe_pending_tx(callback)` methods.
- `RpcClient` is transport-agnostic: `--rpc-transport http|ws` selects at startup. WS URL inferred from `wss://` prefix; HTTP fallback on WS failure.
- New REPL command `pending-tx watch` polls the mempool in real time (not just the local NonceManager).
- Anvil integration test: spawn anvil with `--port 0` and a WS port; assert `subscribe_blocks` emits within 5s of a test transaction.
- Coverage: `chain/client.rs` ≥ 80% (currently 57% — V1's anvil-driven gap).

**TDD path:**
1. RED: tests for the transport selection (HTTP vs WS parsing).
2. GREEN: implement the WS client via alloy's `alloy-pubsub` (already in the dep tree).
3. REFACTOR: rewire `RpcClient` to hold a transport enum.

**Risk:** Medium. WS long-lived connection has new failure modes (disconnect, slow consumer). Need backpressure + reconnect. V1's HTTP-only path was simpler.

**ADR:** ADR-0012 — RPC transport selection & reconnect policy.

---

### M11 — Transaction History (SQLite) (v0.5.0, 2–3 days)

**Goal:** Persist confirmed transactions beyond the V1 in-memory `pending-tx` and NonceManager.

**Definition of Done:**
- New `rusqlite` (or `sqlx`) dependency (default-features = false, bundled).
- DB schema at `~/.local/share/evm-cli/history.sqlite`:
  - `tx(hash PRIMARY KEY, from_addr, to_addr, value_wei, token_addr NULL, nonce, block_number, block_timestamp, status, gas_used, gas_price_wei, created_at)`.
- New CLI command `history [--since YYYY-MM-DD] [--to 0x...] [--limit N] [--json]`.
- Existing `pending-tx` and NonceManager **unchanged** (do not regress V1).
- Auto-migration: schema version table; on startup, apply pending migrations.
- Integration test: insert + query round-trip; date and address filters; status filter.

**TDD path:**
1. RED: schema + CRUD unit tests (in-memory SQLite).
2. GREEN: implement the persistence layer.
3. REFACTOR: add the CLI command; ensure `--json` output is stable.

**Risk:** Medium. New dep; schema migration policy is a long-term commitment. Defer the multi-device sync story to V3.

**ADR:** ADR-0014 — Transaction history schema & migration policy.

---

### M12 — Cross-Platform (v0.6.0, 5–7 days)

**Goal:** Windows + macOS support; release artifacts for both.

**Definition of Done:**
- CI matrix adds `windows-latest` and `macos-latest` runners (Apple Silicon).
- Release artifacts:
  - `evm-cli-v0.6.0-windows-x86_64.zip` (`.zip`, not `.tar.gz`; Windows users expect `.zip`)
  - `evm-cli-v0.6.0-macos-aarch64.tar.gz` (Apple Silicon)
  - `evm-cli-v0.6.0-macos-x86_64.tar.gz` (Intel; best-effort)
  - Existing `evm-cli-v0.6.0-linux-x86_64.tar.gz` + `linux-aarch64.tar.gz`
- Platform differences handled:
  - File paths: `directories` crate (already in deps) — 0 LoC
  - `umask(0o077)`: Unix-only, gated `#[cfg(unix)]` (already `unsafe { libc::umask }` in V1; Windows gets a no-op)
  - Clipboard (paired with M8): per-platform impls
  - Tarball format: `.zip` for Windows, `.tar.gz` for *nix
- Per-platform failure is a **warning** in CI (not a gate) until V3, so a single broken matrix entry doesn't block releases. V1's Linux-only gate stays.
- Documentation: [README.md](./README.md) "Platform requirements" updated.

**TDD path:**
1. RED: platform-conditional unit tests (`#[cfg(windows)]` vs `#[cfg(unix)]`).
2. GREEN: gate Unix-only code; add per-platform tar/zip helpers.
3. REFACTOR: extract platform abstraction.

**Risk:** High. Cross-platform bugs are often runtime-only and need a real-machine smoke test. The "warning not gate" CI policy is a deliberate trade-off; V3 should tighten once the per-platform kinks are worked out.

**ADR:** ADR-0013 — Cross-platform CI strategy.

---

## 6. V2 Effort Estimate

| Milestone | Days | Cumulative | Version |
|---|---|---|---|
| M7 | 1–2 | 1–2 | v0.3.0 |
| M8 | 1–2 | 2–4 | v0.3.1 |
| M9 | 3–5 | 5–9 | v0.4.0 |
| M10 | 2–3 | 7–12 | v0.4.1 |
| M11 | 2–3 | 9–15 | v0.5.0 |
| M12 | 5–7 | 14–22 | v0.6.0 |
| **V2 total** | | **14–22 days ≈ 3–4 weeks** | |

> Roughly half of V1 (31 days). V2 is incremental: no ADR reshaping, no new module boundaries, no risk register rewrite. Each milestone is a focused, testable increment.

---

## 7. V2 Risk Register

| Risk | Level | Mitigation |
|---|---|---|
| M9 multi-chain forces re-examination of chain-specific assumptions | MED | TDD-first; per-chain integration tests on anvil forks; rollback to V1 if a major issue surfaces |
| M10 WS reconnect / backpressure is hard to get right | MED | V1's HTTP path stays as fallback; new feature is opt-in; defer "always-on WS" to V3 |
| M11 SQLite schema migration is a long-term commitment | LOW | V1's NonceManager pattern (single JSONL file) is the precedent; auto-migration with a version table is a small extension |
| M12 cross-platform CI matrix is a recurring time sink | MED | Per-platform failure is warning, not gate; V1 Linux release stays primary; macOS/Windows are "best-effort" until V3 |
| M7's pure-function extraction breaks some `cli/commands.rs` integration tests | LOW | All V1 tests must still pass before M7 closes; if they don't, the extraction is wrong, not the test |
| V2 expands scope into "more platforms, more features" without addressing underlying V1 tech debt | LOW | M7 is mandatory; M8/M10 are P1; M11 is P2; M12 is P2 |
| New dependencies (rusqlite, etc.) carry RUSTSEC risk | LOW | CI gate (`cargo audit`) is unchanged; P0-2 dep checklist still applies |

---

## 8. V2 ADR Candidates

| # | Title | Trigger | Status |
|---|---|---|---|
| **ADR-0011** | Multi-chain registry & defaults | M9 | DRAFT |
| **ADR-0012** | RPC transport selection & reconnect | M10 | DRAFT |
| **ADR-0013** | Cross-platform CI strategy | M12 | DRAFT |
| **ADR-0014** | Transaction history schema & migration | M11 | DRAFT |
| **ADR-0015** | Clipboard abstraction (per-platform) | M8 | DRAFT |
| ADR-0016+ | TBD as M7–M12 surface new design questions | — | — |

---

## 9. Next Action

**This document is the entry point.** Per the user's M5/M6 process, the first V2 milestone (M7) should follow TDD discipline:

1. Re-confirm M7 scope with the user (1 question: "Refactor + test extraction OK?")
2. Write TDD tests for the extracted pure functions (RED)
3. Extract and migrate (GREEN)
4. Refactor + commit + tag v0.3.0

**Defer M8–M12** until M7 ships. Each gets its own kickoff with the same branch-decision pattern used for M5.

---

## 10. V2 ↔ V1 ↔ V9 cross-references

- V1 plan: [PLAN.md](./PLAN.md) (V9 content)
- V1 audit: [docs/audit/M6.md](./docs/audit/M6.md)
- V1 commands: [docs/commands.md](./docs/commands.md)
- V1 troubleshooting: [docs/troubleshooting.md](./docs/troubleshooting.md)
- V1 architecture: [docs/architecture.md](./docs/architecture.md)
- V2 M5 release pipeline: [`.github/workflows/release.yml`](./.github/workflows/release.yml)
- V2 M5 release artifact layout: still `evm-cli-v<ver>-{linux-x86_64,linux-aarch64}.tar.gz` (M12 adds Windows `.zip` and macOS variants)
