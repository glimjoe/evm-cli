// SPDX-License-Identifier: MIT
//
// `ChainId` newtype — wraps `u64` (EIP-155 chain id).
//
// Per PLAN-V9 §3: "ChainId(u64) — const SEPOLIA = 0xaa36a7". The
// Sepolia chain id is the ONLY chain V1 supports. The newtype
// ensures we never accidentally cross-chain a transaction.

use std::fmt;

/// EIP-155 chain id.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChainId(pub u64);

impl ChainId {
    /// The Sepolia testnet chain id (per PLAN-V9 §1, EIP-155).
    /// Hex `0xaa36a7` = decimal `11155111`.
    pub const SEPOLIA: ChainId = ChainId(0xaa36a7);

    /// Underlying `u64`.
    #[inline]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId(0x{:x})", self.0)
    }
}

impl From<u64> for ChainId {
    #[inline]
    fn from(n: u64) -> Self {
        Self(n)
    }
}

impl From<ChainId> for u64 {
    #[inline]
    fn from(c: ChainId) -> Self {
        c.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sepolia_value() {
        // 0xaa36a7 = 11155111
        assert_eq!(ChainId::SEPOLIA.as_u64(), 11_155_111);
        assert_eq!(ChainId::SEPOLIA.as_u64(), 0xaa36a7);
    }

    #[test]
    fn display() {
        assert_eq!(format!("{}", ChainId::SEPOLIA), "11155111");
    }
}
