// SPDX-License-Identifier: MIT
//
// Keccak-256 wrapper around the `tiny-keccak` crate.
//
// Per PLAN-V9 §5 M1 DoD:
//   "Keccak-256 via `tiny-keccak` `Keccak` type (not `Sha3`); `sha3` crate forbidden"
//
// Why not the `sha3` crate: it is dual-licensed MIT/Apache-2.0 and would
// be acceptable, but the V8 plan specifies `tiny-keccak` (CC0-1.0, single
// dependency, thinnest wrapper) and forbids `sha3` by name.
//
// Why `Keccak` and not `Sha3`: these are two different hash functions
// that share the same underlying permutation but use different padding
// rules. Ethereum uses the original Keccak (pre-NIST standardization),
// which is what `tiny_keccak::Keccak` provides. `tiny_keccak::Sha3`
// provides the NIST-standardized SHA3-256 and **must not** be used here.

use tiny_keccak::{Hasher, Keccak};

/// Compute Keccak-256 of the input bytes. Returns 32 bytes.
///
/// # Example
/// ```
/// use evm_cli::crypto::keccak::keccak256;
/// // The well-known Keccak-256 of empty input is:
/// //   c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
/// let h = keccak256(b"");
/// assert_eq!(hex::encode(h), "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
/// ```
pub fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(input);
    hasher.finalize(&mut output);
    output
}

/// Keccak-256 as a fixed-size heap-allocated `Vec<u8>` (32 bytes).
/// Convenience for callers that don't want a stack array.
#[allow(dead_code)] // used in tests and M2+ (keystore file hashing)
pub fn keccak256_vec(input: &[u8]) -> Vec<u8> {
    keccak256(input).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }

    /// The canonical Keccak-256 of the empty string.
    /// Cross-checked against multiple independent implementations.
    #[test]
    fn empty_input() {
        let h = keccak256(b"");
        assert_eq!(
            hex(&h),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    /// Keccak-256 of "abc".
    #[test]
    fn abc() {
        let h = keccak256(b"abc");
        assert_eq!(
            hex(&h),
            "4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
        );
    }

    /// Keccak-256 of arbitrary bytes (used as a sanity check). The
    /// expected value is the actual output of our `keccak256()` over
    /// "hello world" — verified against an independent implementation.
    #[test]
    fn arbitrary_bytes() {
        let h = keccak256(b"hello world");
        assert_eq!(
            hex(&h),
            "47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad"
        );
    }

    /// `Keccak` (Ethereum) and `Sha3` (NIST) differ for the same input.
    /// The NIST SHA3-256 of "abc" is:
    ///   3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46b9762450f3a248
    /// The Keccak-256 of "abc" is:
    ///   4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45
    /// (different — proves we are NOT using SHA3).
    #[test]
    fn not_sha3_nist() {
        let k = keccak256(b"abc");
        let nist_sha3 = "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46b9762450f3a248";
        assert_ne!(hex(&k), nist_sha3);
    }
}
