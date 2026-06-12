// SPDX-License-Identifier: MIT
//
// `BlockNumber` newtype — wraps `u64` (Ethereum block height).
//
// Per PLAN-V9 §3: "BlockNumber(u64)". The newtype prevents
// accidentally passing a nonce, chain id, or timestamp where a
// block number is expected.

use std::fmt;

/// An Ethereum block number (height in the canonical chain).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockNumber(pub u64);

impl BlockNumber {
    /// The genesis block (height 0).
    pub const GENESIS: BlockNumber = BlockNumber(0);

    /// Underlying `u64`.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for BlockNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for BlockNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockNumber({})", self.0)
    }
}

impl From<u64> for BlockNumber {
    #[inline]
    fn from(n: u64) -> Self {
        Self(n)
    }
}

impl From<BlockNumber> for u64 {
    #[inline]
    fn from(b: BlockNumber) -> Self {
        b.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_is_zero() {
        assert_eq!(BlockNumber::GENESIS.as_u64(), 0);
    }

    #[test]
    fn from_u64() {
        let b: BlockNumber = 12345u64.into();
        assert_eq!(b.as_u64(), 12345);
    }
}
