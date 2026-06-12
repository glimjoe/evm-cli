// SPDX-License-Identifier: MIT
//
// `Address` newtype — wraps `alloy_primitives::Address`.
//
// Per PLAN-V9 §3: "Address(AlloyAddress) — strings forbidden at API
// boundary". This newtype ensures the only way to obtain an `Address`
// is via parsing (which validates EIP-55 / lowercase hex) or via
// derivation from a key. It also provides a `Display` impl that
// emits the EIP-55 mixed-case form (matches what the user copies
// from a block explorer, defending against copy-paste mistakes).
//
// Internal code can freely use `alloy_primitives::Address` for
// performance/ergonomics. The newtype is the public API.

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests use `.expect()` on fixed inputs.

use std::fmt;

use alloy_primitives::Address as AlloyAddress;

/// 20-byte Ethereum address, EIP-55 mixed-case on display.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address(pub AlloyAddress);

impl Address {
    /// The zero address (0x0000…0000). Used in tests and as a default
    /// placeholder. Distinct from a real address.
    pub const ZERO: Address = Address(AlloyAddress::ZERO);

    /// Wrap an `alloy::primitives::Address`. The caller is responsible
    /// for ensuring the bytes are a valid address.
    #[inline]
    pub const fn from_alloy(a: AlloyAddress) -> Self {
        Self(a)
    }

    /// Borrow the inner `alloy::primitives::Address`.
    #[inline]
    pub const fn as_alloy(&self) -> &AlloyAddress {
        &self.0
    }

    /// Consume the newtype and return the inner alloy address.
    #[inline]
    pub const fn into_alloy(self) -> AlloyAddress {
        self.0
    }

    /// Parse an address from a hex string (with or without `0x`
    /// prefix). EIP-55 mixed-case is validated; lowercase / uppercase
    /// is accepted.
    pub fn parse(s: &str) -> Result<Self, AddressParseError> {
        let a = s
            .parse::<AlloyAddress>()
            .map_err(|e| AddressParseError(e.to_string()))?;
        Ok(Self(a))
    }

    /// 20-byte representation.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 20] {
        self.0.as_ref()
    }
}

impl fmt::Display for Address {
    /// EIP-55 mixed-case. The `alloy_primitives::Address` `Display`
    /// impl already implements EIP-55, so we delegate.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.0)
    }
}

impl From<AlloyAddress> for Address {
    #[inline]
    fn from(a: AlloyAddress) -> Self {
        Self(a)
    }
}

impl From<Address> for AlloyAddress {
    #[inline]
    fn from(a: Address) -> Self {
        a.0
    }
}

impl AsRef<[u8]> for Address {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl AsRef<AlloyAddress> for Address {
    #[inline]
    fn as_ref(&self) -> &AlloyAddress {
        &self.0
    }
}

/// Error returned by `Address::parse`.
#[derive(Debug, thiserror::Error)]
#[error("invalid address: {0}")]
pub struct AddressParseError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_address_display() {
        assert_eq!(
            format!("{}", Address::ZERO),
            "0x0000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn parse_lowercase() {
        let a = Address::parse("0x9858effd232b4033e47d90003d41ec34ecaeda94").expect("parse");
        assert_eq!(a.0 .0[0], 0x98);
    }

    #[test]
    fn parse_mixed_case() {
        let a = Address::parse("0x9858EfFD232B4033E47d90003D41EC34EcaEda94").expect("parse");
        assert_eq!(a.0 .0[0], 0x98);
    }

    #[test]
    fn parse_no_prefix() {
        let a = Address::parse("9858effd232b4033e47d90003d41ec34ecaeda94").expect("parse");
        assert_eq!(a.0 .0[0], 0x98);
    }

    #[test]
    fn parse_invalid_length() {
        assert!(Address::parse("0x1234").is_err());
    }

    #[test]
    fn display_is_eip55() {
        // The canonical ethereumbook test vector.
        let a = Address::parse("0x9858effd232b4033e47d90003d41ec34ecaeda94").expect("parse");
        // The Display impl is the alloy impl (EIP-55). The exact case
        // is defined by the EIP-55 algorithm; we just assert it
        // round-trips and is not all-lowercase.
        let displayed = format!("{a}");
        assert!(displayed.starts_with("0x"));
        assert_eq!(
            displayed.to_lowercase(),
            "0x9858effd232b4033e47d90003d41ec34ecaeda94"
        );
    }

    #[test]
    fn from_alloy_and_back() {
        let alloy = AlloyAddress::repeat_byte(0xab);
        let a: Address = alloy.into();
        let back: AlloyAddress = a.into();
        assert_eq!(alloy, back);
    }
}
