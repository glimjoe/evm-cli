# ADR-0007: Secret Memory Hardening

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision)
> Deciders: evm-cli maintainers
> Supersedes: V2 §3 L55 (`Secret<T: Zeroize>(T)`), V3 §3 (same as V2)

## Context and Problem Statement

V2/V3 used `Secret<T: Zeroize>`. `Zeroize` is a **manual** trait — the developer must call `.zeroize()` on every code path. On panic, stack unwinding does NOT call `zeroize()`. V3 also lacked `mlock` (swap-to-disk protection) and did not disable core dumps.

P0-2 mandates comprehensive hardening.

## Decision Drivers

- **Panic safety**: secrets must be zeroized even on unwinding, not only on normal drop.
- **Swap safety**: secrets must not be written to swap (which is unencrypted disk).
- **Core dump safety**: secrets must not appear in `core` files.
- **No String for secrets**: `String`'s allocator does not zeroize on drop; the heap buffer can be read after `String` is dropped.
- **Simplicity**: the developer should not need to remember to call `zeroize()`.

## Considered Options

- **A. `ZeroizeOnDrop` + `mlock` + `RLIMIT_CORE=0` + `String` ban** (chosen)
- **B. `ZeroizeOnDrop` only**: panic safety without swap or core protection.
- **C. `Zeroize` + lint rules**: relies on discipline, easy to bypass.

## Decision Outcome

**Chosen option: A**, with these concrete mandates:

### 1. Type system

```rust
// Lives in src/types/secret.rs (per ADR-0003 rev1: Secret<T> is a primitive)
use zeroize::{Zeroize, ZeroizeOnDrop};

pub struct Secret<T: ZeroizeOnDrop>(T);

impl<T: ZeroizeOnDrop> Secret<T> {
    pub fn new(value: T) -> Self { Self(value) }
    /// Explicit access for serialization / signing. Caller is responsible
    /// for not cloning or logging the returned reference.
    pub fn expose_secret(&self) -> &T { &self.0 }
}

// Drop is auto-generated: drops T, which is ZeroizeOnDrop, which zeroizes.
static_assertions::assert_impl_all!(Secret<Vec<u8>>: ZeroizeOnDrop);

// Debug redacts content; never prints inner.
impl<T: ZeroizeOnDrop> std::fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Secret(***)")
    }
}

// Secret does NOT implement Serialize. Callers must explicitly
// unwrap via expose_secret() and serialize the inner T themselves.
// This prevents accidental serialization of secrets (e.g. to logs,
// network responses, error context).
//
// Secret does NOT implement Clone. Cloning would silently duplicate
// the secret into a new allocation, defeating the point of zeroization.
//
// Secret does NOT implement Display. Printing must go through Debug.
```

Mnemonic, seed, and private key material **MUST** be wrapped in `Secret<Vec<u8>>` or `Zeroizing<...>`. They **MUST NOT** be stored as `String` (a `String` is just a `Vec<u8>` with UTF-8 invariant, and its heap buffer is not zeroized on drop).

**When to use `Secret<T>` vs `Zeroizing<T>`:**
- `Secret<T>`: when the value is a **named, named-lifetime** sensitive asset (private key, mnemonic, seed). Lives in `types::Secret` and re-exported across modules.
- `Zeroizing<T>`: when the value is an **intermediate computation** that may carry secret-derived bytes temporarily (e.g. `keccak256(mnemonic_bytes)` while building an address). The borrow is short-lived; `Zeroizing` is sufficient and lighter-weight.

### 2. Process hardening (at `main()` start, before any `Secret` allocation)

```rust
fn harden_process() -> Result<(), ProcessError> {
    // 1. Disable core dumps
    let rlim = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    unsafe { libc::setrlimit(libc::RLIMIT_CORE, &rlim) };

    // 2. Restrict file mode for any file we create
    unsafe { libc::umask(0o077) };

    // 3. Best-effort: mlock critical buffers (failure logged non-fatally)
    //    See "mlock policy" below for what gets mlocked and when.
    Ok(())
}
```

### 3. mlock policy

**Threshold:** any secret-bearing buffer of **≥ 32 bytes** (private key size) is mlocked at allocation time. Smaller buffers (e.g. a 20-byte address, a 4-byte chain id) are not mlocked — the overhead is not worth it for material that is not by itself a credential.

**Implementation:** `Secret<Vec<u8>>::new(value)` calls `mlock::mlock_bytes(&value)` immediately after the `Vec` is built. Failure is logged at `WARN` and the `Secret` is still returned; the mlock is best-effort, not mandatory. Documented in troubleshooting for sandboxed environments where mlock may be denied.

**When mlock is invoked:**
- Creating a `Secret<Vec<u8>>` from a parsed mnemonic
- Loading a `Secret<Vec<u8>>` from the keystore (the encrypted payload, which contains the seed)
- Receiving a `Secret<Vec<u8>>` from BIP-39 / BIP-44 derivation

**When mlock is NOT invoked:**
- The encrypted keystore on disk is already protected by file mode 0600; mlock is not needed for it after the in-memory decryption copy is mlocked.
- Intermediate hashes (e.g. `keccak256(mnemonic)`) are not mlocked — they are 32 bytes but the `Zeroizing` wrapper covers them at short lifetime.

### 4. CI enforcement (strengthened)

**`clippy.toml` at repo root** (per P0-4 in PLAN-V4 §5 M0):
- `disallow(unwrap_used, expect_used)` globally — catches forgotten error handling in secret paths
- `disallowed-methods` block listing `String::from_utf8`, `String::from_utf8_lossy` (these can leak bytes into a String)
- `disallowed-types` block listing `String`, `Box<str>` in functions/structs whose name contains `mnemonic`/`seed`/`private_key` (best-effort name-based heuristic)

**`String` ban for secrets — multi-pattern grep** (CI step):

```bash
set -e
SENSITIVE='mnemonic|seed|private[_-]?key|priv[_-]?key|secret'

# Pattern 1: direct binding to String
if rg --quiet "(let|let mut) .* ($SENSITIVE).*: String" --type rust; then
  echo "ERROR: secret bound to String" && exit 1
fi

# Pattern 2: String::from on a sensitive source
if rg --quiet "String::from\(.*($SENSITIVE)" --type rust; then
  echo "ERROR: String::from on secret" && exit 1
fi

# Pattern 3: to_string on a sensitive source
if rg --quiet "\.to_string\(\).*($SENSITIVE)|($SENSITIVE).*\.to_string\(\)" --type rust; then
  echo "ERROR: .to_string() on secret" && exit 1
fi

# Pattern 4: format! with sensitive arg
if rg --quiet 'format!.*\b($SENSITIVE)\b' --type rust; then
  echo "ERROR: format! on secret" && exit 1
fi

# Pattern 5: function return type String with sensitive in signature
if rg --quiet "fn .* ($SENSITIVE).* -> String" --type rust; then
  echo "ERROR: function returning String of secret" && exit 1
fi

echo "All String-on-secret greps passed."
```

All 5 patterns must pass with zero matches. Any failure blocks CI.

**Note on false positives:** the greps are name-based. A `String` named `user_facing_mnemonic_for_logging` would not match. The greps catch honest mistakes; they do not catch malicious code. Code review remains the primary defense.

### Consequences

* **Good**: panic, swap, and core-dump paths are all covered.
* **Good**: `String` ban is enforced mechanically (CI grep), not by developer memory.
* **Good**: `ZeroizeOnDrop` is a well-known pattern in the RustCrypto ecosystem.
* **Bad**: `mlock` may fail in containers / CI runners. Logged non-fatally; documented in troubleshooting.
* **Bad**: every new secret-bearing variable must be a `Secret<...>`. Enforced by code review + grep.

## Implementation

- PLAN-V4 §3 (Type System — `Secret<T>` in `types/`)
- PLAN-V4 §5 M0 DoD (process hardening sub-items; `clippy.toml` per P0-4)
- PLAN-V4 §5 M2 DoD (`ZeroizeOnDrop` requirement)
- PLAN-V4 §7 (self-audit: 4 new memory-hardening checks)
- PLAN-V4 §8 (private key memory leak risk → HIGH, mitigated)
- `static_assertions` crate used at compile time to assert `Secret<Vec<u8>>: ZeroizeOnDrop`

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 4 issues in the initial Accepted draft. All addressed:

1. **Broken cross-reference**: line 66 said "see ADR-0008-adjacent clippy rules"; ADR-0008 is RBF/Cancel, not clippy. The clippy config is defined by P0-4 in PLAN-V4 §5 M0. The cross-reference now points to the correct source, and `clippy.toml` rules are spelled out in the body.
2. **`String` ban grep was too narrow**: the initial single grep `rg "let .* (mnemonic|seed|private_key).*: String"` missed `String::from(...)`, `.to_string()`, `format!`, and function-returning-`String` paths. Now 5 patterns cover all common leak paths, with a `set -e` shell script that's the canonical CI step.
3. **Missing `Debug` and `Serialize` requirements**: V4 §3 claimed `Secret<T>` has `Debug impl prints 'Secret(***)'` but the ADR did not specify it. The ADR now shows the explicit `impl Debug` that always prints `Secret(***)`, and adds the requirement that `Secret<T>` does **not** implement `Serialize`, `Clone`, or `Display` — all of these would defeat the wrapper. The accessor `.expose_secret(&self) -> &T` is the only path to the inner value, making the leak point explicit at the call site.
4. **`mlock` policy was vague**: "of significant size" was undefined. Now explicit threshold: **≥ 32 bytes** (private key size) gets mlocked. Smaller buffers (addresses, chain ids) are not. The ADR also distinguishes `Secret<T>` (named, long-lived) from `Zeroizing<T>` (intermediate, short-lived) and when mlock applies to each.

No change to the core "ZeroizeOnDrop + mlock + RLIMIT_CORE=0 + String ban" decision. All revisions are concrete specifications of how to implement it.

## References

- PLAN-V4 §3
- PLAN-V4 §5 M0, M2
- PLAN-V4 §7
- PLAN-V4 §8
- ADR-0003 (workspace: `Secret<T>` lives in `types/`)
- `zeroize` crate: https://crates.io/crates/zeroize
- `mlock` crate: https://crates.io/crates/mlock
- `static_assertions` crate: https://crates.io/crates/static_assertions
- clippy `disallowed_types` / `disallowed_methods`: https://rust-lang.github.io/rust-clippy/master/index.html#disallowed_types
- OWASP Password Storage Cheat Sheet (Argon2id reference)
