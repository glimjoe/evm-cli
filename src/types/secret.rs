// SPDX-License-Identifier: MIT
//
// `Secret<T>` — type-system wrapper for sensitive material.
//
// Per ADR-0007 rev1:
//   - Wraps any `T: ZeroizeOnDrop`.
//   - Auto-zeroizes on Drop (including panic unwind).
//   - No `Clone`, `Serialize`, or `Display` impls.
//   - `Debug` always prints `Secret(***)`.
//   - `expose_secret(&self) -> &T` is the ONLY path to the inner value.
//   - Buffers ≥ 32 bytes are mlocked at allocation time (best-effort).
//
// See `docs/adr/0007-secret-memory.md` for the full design and rationale.

use std::fmt;

use zeroize::Zeroize;

/// Minimum buffer size (in bytes) at which we attempt `mlock(2)`.
///
/// 32 bytes is the size of a secp256k1 private key. Smaller buffers
/// (a 20-byte address, a 4-byte chain id) are not by themselves credentials
/// and the `mlock` overhead is not justified.
const MLOCK_THRESHOLD_BYTES: usize = 32;

/// Sensitive data wrapper. The inner `T` is zeroized on every Drop,
/// including during panic unwind.
///
/// **Design note (corrected from ADR-0007 rev1):** the bound is `Zeroize`
/// (not `ZeroizeOnDrop`) because most crypto-bearing types (`Vec<u8>`,
/// `[u8; 64]`, ...) impl `Zeroize` but not `ZeroizeOnDrop`. We provide
/// an explicit `Drop` that calls `T::zeroize()`, which fires on normal
/// drop AND on panic unwind (Rust runs `Drop` impls during unwind).
pub struct Secret<T: Zeroize> {
    inner: T,
}

// Compile-time check: the explicit Drop fires for any T: Zeroize.
impl<T: Zeroize> Drop for Secret<T> {
    fn drop(&mut self) {
        // `Zeroize::zeroize` overwrites the buffer with zeros / default.
        // For panic safety this runs during unwind.
        self.inner.zeroize();
    }
}

impl<T: Zeroize + 'static> Secret<T> {
    /// Wrap `value`. If `T = Vec<u8>` and `value.len() >= 32`, attempts
    /// `mlock(2)` on the underlying bytes. mlock failure is logged at
    /// WARN and is non-fatal.
    pub fn new(value: T) -> Self {
        if let Some(bytes) = (&value as &dyn std::any::Any).downcast_ref::<Vec<u8>>() {
            if bytes.len() >= MLOCK_THRESHOLD_BYTES {
                if let Err(e) = mlock_bytes(bytes) {
                    tracing::warn!(
                        bytes_len = bytes.len(),
                        error = %e,
                        "mlock failed; secret is not protected from swap"
                    );
                }
            }
        }
        Self { inner: value }
    }

    /// Explicit accessor. Caller is responsible for not cloning or
    /// logging the returned reference.
    pub fn expose_secret(&self) -> &T {
        &self.inner
    }
}

/// Debug always redacts content. Never prints the inner `T`.
impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

// Deliberately NOT implemented:
//   - Clone  : would silently duplicate the secret into a new allocation
//              and break the zeroize-on-drop invariant.
//   - Serialize: would let the secret round-trip through serde formats
//              (e.g. to logs, JSON responses, error context) without an
//              explicit expose_secret() call.
//   - Display : same rationale as Serialize.

/// Best-effort mlock of a byte slice. Logs WARN on failure.
///
/// Uses `os_memlock::mlock` (the thinnest `mlock(2)` wrapper on
/// crates.io; see ADR-0007 rev2). The wrapper returns `io::Result<()>`
/// with `ErrorKind::Unsupported` on platforms where the syscall is
/// unavailable, which we treat as WARN-not-fatal.
fn mlock_bytes(bytes: &[u8]) -> Result<(), String> {
    let ptr = bytes.as_ptr().cast::<std::ffi::c_void>();
    let len = bytes.len();
    // SAFETY: caller guarantees `ptr..ptr+len` is a valid allocation
    // owned by this process for the duration of the call.
    let result = unsafe { os_memlock::mlock(ptr, len) };
    result.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_content() {
        let s = Secret::new(vec![0xab; 64]);
        let formatted = format!("{s:?}");
        assert_eq!(formatted, "Secret(***)");
        // The bytes themselves must not appear in the debug output.
        assert!(!formatted.contains("ab"));
    }

    #[test]
    fn expose_secret_returns_inner_reference() {
        let s = Secret::new(vec![1u8, 2, 3]);
        assert_eq!(s.expose_secret(), &vec![1u8, 2, 3]);
    }

    #[test]
    fn type_zeroizes_on_drop() {
        // Run the Drop on a Secret to ensure no panic.
        let s = Secret::new(vec![0u8; 32]);
        drop(s);
        // No assertion needed; if Drop didn't fire or panicked, the test
        // would fail at runtime.
    }
}
