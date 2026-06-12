// SPDX-License-Identifier: MIT
//
// `Amount` newtype — wraps `alloy_primitives::U256` (wei amount).
//
// Per PLAN-V9 §3: "Amount(U256) — paired with token decimals". The
// `Amount` is a raw integer in the smallest unit (wei for ETH, or
// the token's smallest unit for ERC-20). The `decimals` is held by
// the *caller* (e.g. CLI flag, token registry entry) and used for
// display formatting only.
//
// Construction is done via `try_from_decimal_str` (which is the only
// string entry point) or via direct `Amount(U256)` for already-parsed
// values. `FromStr` is intentionally NOT implemented to avoid
// silent integer parsing — callers must go through `try_from_*`.

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests use `.expect()` on fixed inputs.

use std::fmt;

use alloy_primitives::U256;

/// A token amount in the smallest unit (wei for ETH). Wraps `U256`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Amount(pub U256);

impl Amount {
    /// Zero amount.
    pub const ZERO: Amount = Amount(U256::ZERO);

    /// 1 wei.
    pub const ONE_WEI: Amount = Amount(U256::ONE);

    /// Wrap a `U256` value directly. No validation.
    #[inline]
    pub const fn from_wei(wei: U256) -> Self {
        Self(wei)
    }

    /// Borrow the inner `U256`.
    #[inline]
    pub const fn as_wei(&self) -> &U256 {
        &self.0
    }

    /// Consume the newtype and return the inner `U256`.
    #[inline]
    pub const fn into_wei(self) -> U256 {
        self.0
    }

    /// Parse a decimal string (e.g. `"0.001"`, `"1000"`) into wei.
    /// Returns `InvalidAmount` (chains `ChainError` via `.code()` →
    /// `EVMC-004`) on overflow or malformed input.
    ///
    /// `decimals` is the token's decimal places (18 for ETH). The
    /// string may have at most `decimals` fractional digits.
    pub fn try_from_decimal_str(s: &str, decimals: u8) -> Result<Self, AmountParseError> {
        let s = s.trim();
        let (int_part, frac_part) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if frac_part.len() > decimals as usize {
            return Err(AmountParseError(format!(
                "fractional part has {} digits; token has only {} decimals",
                frac_part.len(),
                decimals
            )));
        }
        let int_value = if int_part.is_empty() {
            U256::ZERO
        } else {
            U256::from_str_radix(int_part, 10)
                .map_err(|e| AmountParseError(format!("integer part: {e}")))?
        };
        // Pad the fractional part on the right with zeros to exactly
        // `decimals` digits; this preserves the value (e.g. "001"
        // padded to 18 digits = "001000000000000000" = 1e15, NOT 1e0).
        let frac_value = if frac_part.is_empty() {
            U256::ZERO
        } else {
            let padded = format!("{:0<width$}", frac_part, width = decimals as usize);
            U256::from_str_radix(&padded, 10)
                .map_err(|e| AmountParseError(format!("fractional part: {e}")))?
        };
        let multiplier = U256::from(10u64).pow(U256::from(decimals));
        let total = int_value
            .checked_mul(multiplier)
            .and_then(|v| v.checked_add(frac_value))
            .ok_or_else(|| AmountParseError("amount overflows U256".to_string()))?;
        Ok(Self(total))
    }
}

impl fmt::Display for Amount {
    /// Display as raw wei (lowercase hex, no formatting). For human-
    /// friendly display, the CLI layer (M4) converts to decimal
    /// using the token's `decimals`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} wei", self.0)
    }
}

impl fmt::Debug for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Amount({})", self.0)
    }
}

impl From<U256> for Amount {
    #[inline]
    fn from(v: U256) -> Self {
        Self(v)
    }
}

impl From<Amount> for U256 {
    #[inline]
    fn from(a: Amount) -> Self {
        a.0
    }
}

/// Error returned by `Amount::try_from_decimal_str`.
#[derive(Debug, thiserror::Error)]
#[error("invalid amount: {0}")]
pub struct AmountParseError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_amount() {
        assert_eq!(Amount::ZERO.0, U256::ZERO);
    }

    #[test]
    fn parse_eth_decimal() {
        // 0.001 ETH at 18 decimals = 1e15 wei
        let a = Amount::try_from_decimal_str("0.001", 18).expect("parse 0.001 ETH");
        assert_eq!(a.0, U256::from(1_000_000_000_000_000u64));
    }

    #[test]
    fn parse_eth_integer() {
        // 1 ETH at 18 decimals = 1e18 wei
        let a = Amount::try_from_decimal_str("1", 18).expect("parse 1 ETH");
        assert_eq!(a.0, U256::from(1_000_000_000_000_000_000u128));
    }

    #[test]
    fn parse_too_many_decimals() {
        // 7 fractional digits < 18 (allowed).
        let s = "0.0000001";
        assert!(Amount::try_from_decimal_str(s, 18).is_ok());
        // 18 fractional digits (allowed — exactly at the limit).
        let s = "0.000000000000000001";
        assert!(Amount::try_from_decimal_str(s, 18).is_ok());
        // 19 fractional digits > 18 (rejected).
        let s = "0.0000000000000000001";
        assert!(Amount::try_from_decimal_str(s, 18).is_err());
    }

    #[test]
    fn parse_garbage() {
        assert!(Amount::try_from_decimal_str("not a number", 18).is_err());
    }

    #[test]
    fn parse_overflow() {
        // A 80-digit number is too large for U256 (max ~78 digits).
        let huge = "9".repeat(80);
        assert!(Amount::try_from_decimal_str(&huge, 0).is_err());
    }

    #[test]
    fn from_u256_and_back() {
        let v = U256::from(42u64);
        let a: Amount = v.into();
        assert_eq!(a.0, v);
        let back: U256 = a.into();
        assert_eq!(back, v);
    }
}
