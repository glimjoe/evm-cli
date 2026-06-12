// SPDX-License-Identifier: MIT
//
// `Signature` newtype — wraps `alloy_primitives::Signature`.
//
// Per PLAN-V9 §3: "Signature(AlloySignature)". The newtype is a
// thin wrapper; the underlying `Signature` already enforces
// EIP-2 low-S via the k256 signer. We add a `Debug` that prints
// the `(r, s, v)` tuple in hex so the struct can be `{:?}`-printed
// in logs without accidentally leaking the signer's secret key.

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests use `.expect()` on fixed inputs (e.g.
// `signer.sign_message(...).expect("sign")` on a known test key).
// Production paths must not trip `clippy::disallowed_methods` (P0-4).
// Same narrow form as `src/keystore/mod.rs:33`.

use std::fmt;

use alloy_primitives::Signature as AlloySignature;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature(pub AlloySignature);

impl Signature {
    /// Wrap an alloy signature.
    #[inline]
    pub const fn from_alloy(s: AlloySignature) -> Self {
        Self(s)
    }

    /// Borrow the inner `alloy::Signature`.
    #[inline]
    pub const fn as_alloy(&self) -> &AlloySignature {
        &self.0
    }

    /// Consume and return the inner alloy signature.
    #[inline]
    pub const fn into_alloy(self) -> AlloySignature {
        self.0
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // alloy's `Signature` Display is `0x{r}{s}{v}`. We delegate.
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Print as (r, s, v) hex triple — enough to identify the
        // signature without leaking secret material.
        write!(
            f,
            "Signature(r={}, s={}, v={})",
            self.0.r(),
            self.0.s(),
            self.0.v()
        )
    }
}

impl From<AlloySignature> for Signature {
    #[inline]
    fn from(s: AlloySignature) -> Self {
        Self(s)
    }
}

impl From<Signature> for AlloySignature {
    #[inline]
    fn from(s: Signature) -> Self {
        s.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::Signer;
    use alloy_signer_local::PrivateKeySigner;

    #[tokio::test]
    async fn roundtrip_through_alloy() {
        let signer = PrivateKeySigner::random();
        let alloy_sig = signer.sign_message(b"x").await.expect("sign");
        let s: Signature = alloy_sig.into();
        let back: AlloySignature = s.into();
        assert_eq!(back, alloy_sig);
    }
}
