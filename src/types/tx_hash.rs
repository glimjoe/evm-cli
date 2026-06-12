// SPDX-License-Identifier: MIT
//
// `TxHash` newtype — wraps `alloy_primitives::B256` (32-byte hash).
//
// Per PLAN-V9 §3: "TxHash(B256)". The newtype prevents accidentally
// passing a block hash, storage root, or other 32-byte value where a
// transaction hash is expected.

use std::fmt;

use alloy_primitives::B256;

/// 32-byte transaction hash (EIP-155 transaction receipt identifier).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxHash(pub B256);

impl TxHash {
    /// The zero hash (used as a "not yet broadcast" placeholder).
    pub const ZERO: TxHash = TxHash(B256::ZERO);

    /// Wrap a `B256` value directly. No validation.
    #[inline]
    pub const fn from_b256(b: B256) -> Self {
        Self(b)
    }

    /// Borrow the inner `B256`.
    #[inline]
    pub const fn as_b256(&self) -> &B256 {
        &self.0
    }

    /// Consume and return the inner `B256`.
    #[inline]
    pub const fn into_b256(self) -> B256 {
        self.0
    }
}

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // alloy's `B256` Display is `0x{...}` 66-char hex.
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TxHash({})", self.0)
    }
}

impl From<B256> for TxHash {
    #[inline]
    fn from(b: B256) -> Self {
        Self(b)
    }
}

impl From<TxHash> for B256 {
    #[inline]
    fn from(t: TxHash) -> Self {
        t.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_hash_display() {
        assert_eq!(
            format!("{}", TxHash::ZERO),
            "0x0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn from_b256_and_back() {
        let b = B256::repeat_byte(0xab);
        let t: TxHash = b.into();
        assert_eq!(t.0, b);
        let back: B256 = t.into();
        assert_eq!(back, b);
    }
}
