// SPDX-License-Identifier: MIT
//
// `Nonce` newtype — wraps `u64` (Ethereum account nonce).
//
// Per PLAN-V9 §3: "Nonce(u64) — local pending pool, managed by
// NonceManager". The newtype is the API boundary for nonces; the
// NonceManager (M3) is the only producer.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Nonce(pub u64);

impl Nonce {
    /// The genesis nonce (used for accounts that have never sent a tx).
    pub const ZERO: Nonce = Nonce(0);

    /// Increment by one, returning the new value. Saturates at `u64::MAX`.
    pub fn next(self) -> Self {
        Nonce(self.0.saturating_add(1))
    }
}

impl fmt::Display for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nonce({})", self.0)
    }
}

impl From<u64> for Nonce {
    #[inline]
    fn from(n: u64) -> Self {
        Self(n)
    }
}

impl From<Nonce> for u64 {
    #[inline]
    fn from(n: Nonce) -> Self {
        n.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_nonce() {
        assert_eq!(Nonce::ZERO.0, 0);
    }

    #[test]
    fn next_increments() {
        assert_eq!(Nonce(0).next(), Nonce(1));
        assert_eq!(Nonce(41).next(), Nonce(42));
    }

    #[test]
    fn next_saturates_at_max() {
        assert_eq!(Nonce(u64::MAX).next(), Nonce(u64::MAX));
    }
}
