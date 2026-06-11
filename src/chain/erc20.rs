// SPDX-License-Identifier: MIT
//
// ERC-20 transfer via sol! macro.
//
// Per V8 §5 M3 DoD: "ERC-20 transfer: minimal ABI (`balanceOf` +
// `transfer`) loaded; `sol!` macro or manual encoding".
//
// We use `alloy::sol!` to generate typed Rust bindings for the
// minimal ERC-20 ABI. This gives us compile-time type checking
// on the call arguments and returns.

use alloy_primitives::{Address, U256};
use alloy_sol_types::{sol, SolCall};

use crate::chain::ChainError;

// Minimal ERC-20 ABI. `sol!` expands these into typed Rust structs
// and function selectors at compile time.
sol! {
    /// The ERC-20 token interface (minimal subset for V1).
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

/// Encode an ERC-20 `transfer(to, amount)` call.
///
/// Returns the calldata bytes that the caller can wrap in an EIP-1559
/// transaction and broadcast.
pub fn encode_transfer(to: Address, amount: U256) -> Result<Vec<u8>, ChainError> {
    let call = IERC20::transferCall { to, amount };
    Ok(call.abi_encode())
}

/// Encode an ERC-20 `balanceOf(account)` call.
pub fn encode_balance_of(account: Address) -> Result<Vec<u8>, ChainError> {
    let call = IERC20::balanceOfCall { account };
    Ok(call.abi_encode())
}

/// Decode the return value of `balanceOf`.
pub fn decode_balance_of(data: &[u8]) -> Result<U256, ChainError> {
    let ret = IERC20::balanceOfCall::abi_decode_returns(data)
        .map_err(|e| ChainError::Internal(format!("decode balanceOf: {e}")))?;
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_transfer_produces_known_selector() {
        let to = Address::repeat_byte(0xab);
        let amount = U256::from(1000u64);
        let data = encode_transfer(to, amount).expect("encode");
        // The first 4 bytes of ERC-20 transfer calldata are the
        // keccak256("transfer(address,uint256)") first 4 bytes:
        //   0xa9059cbb
        assert_eq!(&data[0..4], &[0xa9, 0x05, 0x9c, 0xbb]);
        // Followed by the ABI-encoded (to, amount).
        assert_eq!(data.len(), 4 + 32 + 32);
    }

    #[test]
    fn encode_balance_of_produces_known_selector() {
        let account = Address::repeat_byte(0xcd);
        let data = encode_balance_of(account).expect("encode");
        // keccak256("balanceOf(address)") first 4 bytes:
        //   0x70a08231
        assert_eq!(&data[0..4], &[0x70, 0xa0, 0x82, 0x31]);
    }
}
