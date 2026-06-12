# ADR-0009: Standard Ethereum Keystore Format (M2 deviation from PLAN-V9 §5)

> Status: **Accepted**
> Date: 2026-06-11
> Deciders: evm-cli maintainers
> Supersedes: PLAN-V9 §5 M2 DoD (Argon2id + AES-256-GCM) and the
> earlier M2 commit `d442521` (which adopted the deviation but did not
> file a standalone ADR; that commit message referenced a non-existent
> "V10 §19")
> Related: M2-REVIEW.md §C "Deviations from spec — judgment call"

## Context and Problem Statement

V9 §5 M2 DoD specified an in-house keystore format:

- **KDF**: Argon2id (t=3, m=65536, p=1)
- **Cipher**: AES-256-GCM
- **MAC**: implicit (GCM tag)
- **File path**: `~/.local/share/evm-cli/keystore.json` (single file)

The M2 implementation (commit `d442521`) ships with a **standard
Ethereum keystore format** instead:

- **KDF**: scrypt (N=2¹³ = 8192, r=8, p=1) — `eth-keystore 0.5.0` default
- **Cipher**: AES-128-CTR
- **MAC**: Keccak-256 (last 16 bytes of ciphertext)
- **File path**: `<data_dir>/keystore/<alias>` (one file per alias, no extension)

The deviation was flagged by the M2 review (M2-REVIEW.md §C, §F item 1)
as a **legitimate design refinement on layout (multi-file) but scope
creep on crypto (scrypt instead of Argon2id)**. The layout change is
self-justifying (atomic rename, simpler CRUD); the crypto change needs
its own ADR with explicit risk acknowledgement. This document is that
ADR.

## Decision Drivers

- **Interop**: standard format produced by `geth`, `ethers.js`,
  `MyEtherWallet`, and `alloy_signer_local::PrivateKeySigner::encrypt_keystore`.
  Whether this is a real benefit for V1 (a single-binary CLI wallet
  whose only consumer is its own user) is **debatable** — V1 §9
  ("Out of Scope") does not require it. The M2 review's take: "**no**,
  for V1 it is not a real benefit." We accept the deviation anyway
  because the cost is small and the door is left open.
- **Code volume**: the custom Argon2id + AES-256-GCM path would require
  a hand-rolled encoder/decoder (~500 LoC, ~10 unit tests) for a
  feature that the upstream `eth-keystore` crate already provides and
  audits. V1's P0 list is already large; eliminating ~500 LoC and 10
  tests from M2 is a real win.
- **Standard tooling**: scrypt at N=2¹³ is what `geth` uses on a fresh
  install. Users moving from `geth` to `evm-cli` would otherwise have
  to re-encrypt their wallets.
- **Cost of scrypt at N=8192**: ~100 ms per decrypt on a modern x86_64
  core, ~5 MB peak working memory. CPU is the limit, not memory.
- **OWASP 2024 baseline**: Argon2id m=19 MiB t=2 p=1, or scrypt
  N=2¹⁷ r=8 p=1. Our N=2¹³ is **16× weaker** than the OWASP-recommended
  scrypt cost. This is the central risk of this ADR.

## Considered Options

- **A. Revert to Argon2id + AES-256-GCM, hand-rolled codec** (V9 §5
  original spec). Add `argon2 = "=0.5.x"` + `aes-gcm = "=0.11.0-rc.4"`
  to `Cargo.toml`. ~500 LoC of crypto code, 10+ unit tests, 12 existing
  keystore format tests would need to be regenerated with new test
  vectors. Delays M2 by ~2 days of focused work.
- **B. Use `eth-keystore` (scrypt + AES-128-CTR + Keccak-256 MAC)**
  (chosen). ~50 LoC of glue, all crypto delegated to the audited
  upstream crate. Standard tooling interop as a side benefit.
- **C. Defer the decision to M6 self-audit**. Equivalent to "ship
  whatever is in M2 today" + a follow-up. Rejected because the M2
  review must close before M4.

## Decision Outcome

**Chosen option: B**, with the following **known limitations** that
are accepted for V1:

### Known V1 limitations

1. **KDF cost is 16× below OWASP 2024 baseline** (scrypt N=8192 vs.
   N=2¹⁷). Mitigation in V1: wallet files are protected by a
   user-chosen password with no rate-limit on attempts. A determined
   attacker with the wallet file (the threat model assumes the file
   is exfiltrated) and modern hardware can attempt ~10 passwords/sec
   against scrypt N=8192, vs. <1/sec against Argon2id m=65536. V1
   accepts this. **V2 should bump to N=2¹⁷ or migrate to Argon2id.**

2. **Cipher is AES-128-CTR, not AES-256-GCM**. The MAC is a separate
   Keccak-256 over the ciphertext (encrypt-then-MAC), not the AEAD
   GCM tag. This is the format `geth` uses and it has been the
   Ethereum standard since 2016. The MAC is verified on every decrypt
   (see `eth-keystore 0.5.0` `decrypt_key`); a wrong password returns
   `MacMismatch` and we collapse that to `EVMK-001 InvalidPassword`
   (anti-side-channel per PLAN-V9 §5).

3. **Multi-file layout instead of single `keystore.json`**. Justified
   by the M2 review's §C.3 — single JSON would force rewrite-on-every-
   mutation, much harder to make atomic. Multi-file enables `rename`
   as a simple `fs::rename` (atomic within the same filesystem).

### Mnemonic string leak via `MnemonicBuilder::phrase()`

The `alloy_signer_local::MnemonicBuilder` stores its `phrase: P where
P: Into<String>` argument as a plain `Option<String>` internally
(`alloy-signer-local-2.0.5/src/mnemonic.rs:28`). This `String` is
**not** zeroized on drop. The 5-pattern grep in
`scripts/secret_string_grep.sh` does not catch this because the
variable is named `phrase`, not `mnemonic/seed/private_key` (per
ADR-0007 rev1 "Implementation Note on false positives").

**Mitigation applied in M2 (commit post-review):** the cloned phrase
is wrapped in `zeroize::Zeroizing<String>` before being passed to
`.phrase()`. The wrapper is dropped immediately after `.build()`
returns, so our local copy is zeroized. The alloy-internal `String`
remains a small leak window (~one `MnemonicBuilder::build()` call,
microseconds). **V2 should patch `alloy-signer-local` to use
`Zeroizing<String>` internally** (or fork it).

### File mode 0600

`main()` sets `umask(0o077)` before any file allocation. `eth-keystore`'s
`File::create(dir.join(name))` does not set an explicit mode, so files
are created with the umask-derived mode = 0600. Verified by reading
`eth-keystore 0.5.0` source. **Note:** if `main()` is bypassed (e.g.
in tests), umask is not set. The test fixture `temp_store()` uses
`tempfile::tempdir()` which sets the dir to 0700, so test files inherit
0700. **Acceptable for V1.**

### `PermissionDenied` collapses into `Io(String)`

PLAN-V9 §5 had a separate `EVMK-008 PermissionDenied { path: PathBuf }` code.
This has been collapsed into `EVMK-011 Io(String)` because the path
is already in the `String` payload, and the variant is rarely hit
(file mode 0600 + umask 0o077 makes it nearly unreachable on Linux
when run via `main()`). See `docs/code_allocation.md` revision history
for the demotion.

## Consequences

* **Good**: ~500 LoC of crypto code deferred to the audited `eth-keystore` crate.
* **Good**: standard tooling interop (a user can take an `evm-cli` wallet file and
  decrypt it with `geth` or `ethers.js`, and vice versa).
* **Good**: multi-file layout enables atomic `rename` and per-alias
  umask protection.
* **Bad**: scrypt N=8192 is below OWASP 2024. V2 must revisit. README
  should mention this.
* **Bad**: residual mnemonic leak in `alloy-signer-local`. Patched locally
  with `Zeroizing<String>`; V2 should patch upstream.
* **Bad**: `PermissionDenied` is not a distinct code. Operator-facing
  diagnostics will read "keystore I/O error: permission denied (os error 13)"
  instead of a typed EVMK-008. Acceptable; V2 may re-introduce.

## Implementation

- `src/keystore/mod.rs` (KDF/cipher delegated to `eth-keystore`)
- `docs/code_allocation.md` (EVMK-001..012 realigned)
- `src/keystore/mod.rs` `create()` / `import()` (Zeroizing<String> wrap)
- `CHANGELOG.md` "Unreleased" → "M2 review fixes" entry on next release
- README: add a "Security limitations" subsection that flags scrypt
  N=8192 as a V1 known-weakness vs. OWASP 2024

## References

- PLAN-V9 §5 M2 DoD (Argon2id + AES-256-GCM, original spec)
- The earlier M2 commit `d442521` (deviation reference, but no ADR
  until this document; that commit referenced a non-existent
  "V10 §19")
- M2-REVIEW.md §C "Deviations from spec — judgment call"
- M2-REVIEW.md §F item 1 (mnemonic String leak)
- `eth-keystore 0.5.0` source: `~/.cargo/registry/.../eth-keystore-0.5.0/src/lib.rs`
- `alloy-signer-local 2.0.5` source: `~/.cargo/registry/.../alloy-signer-local-2.0.5/src/mnemonic.rs`
- EIP-1081 (Ethereum keystore format): https://eips.ethereum.org/EIPS/eip-1081
- EIP-2335 (BLS keystore, related but not the format used here): https://eips.ethereum.org/EIPS/eip-2335
- OWASP Password Storage Cheat Sheet (2024): https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html
- ADR-0007 (Secret memory: Zeroizing<String> rationale)
- ADR-0006 (Error codes: EVMK-001..012 realignment)
