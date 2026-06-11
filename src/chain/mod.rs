// SPDX-License-Identifier: MIT
//
// evm_cli::chain — Ethereum JSON-RPC chain operations.
//
// Per V8 §5 M3 DoD and ADR-0003 (workspace split), this module
// depends only on `crypto`, `keystore`, `types`, and external crates
// (alloy, governor, nix, reqwest, url). It does NOT depend on `cli`
// (which comes in M4).
//
// **M3 status:** This module is a working SKELETON. The following
// are implemented and tested at the unit level:
//   - `nonce::NonceManager` (4-state machine, JSON-lines log, flock)
//   - `rbf::compute_bump` (3-term fee-bump formula)
//   - `erc20::{encode_transfer, encode_balance_of, decode_balance_of}`
//   - `client::RpcClient` (rate-limited, governor)
//   - `alloy_chain::AlloyChain::new` / `with_rate` / `with_client` and
//     the **read-only** surface of `Chain`: `balance`, `chain_id`,
//     `pending_nonce`, `estimate_fees`, `broadcast_tx`,
//     `wait_for_receipt`.
//
// The following are STUBBED (return `ChainError::Internal`):
//   - `get_tx` (alloy 2.0.5 RPC-types field access API differs from
//     the V8-era sketch; full impl needs anvil integration tests)
//   - `build_eth_transfer` (alloy 2.0.5 RLP signing API; needs
//     anvil integration tests)
//   - RBF / Cancel full pipeline (depends on `build_eth_transfer`)
//   - ERC-20 broadcast (depends on `build_eth_transfer`)
//
// **Why the stubs:** alloy 2.0.5 has several API differences from
// what V8 §5 M3 was written for. Field accessors on `Transaction<T>`
// are different; signing flow is via `EthereumWallet` + `Signer`
// which has subtle trait-bound requirements. A full M3 in one
// session is not realistic without anvil tests to verify against;
// these are deferred to M3 finalization in the next session.
//
// See PLAN-V10 §20 (M3 changelog) for the full impact analysis.

#![allow(clippy::disallowed_methods)]
// We use String::from_utf8_lossy in test code; production paths never
// wrap secret material in String.

use std::time::Duration;

use alloy_primitives::{Address, TxHash, U256};
use thiserror::Error;

/// Errors from chain operations. Per V8 §5 M3 + ADR-0006 rev1, all
/// variants carry an error code. The codes are listed in
/// `docs/code_allocation.md` and a CI test enforces that every
/// `code()` arm returns a string from that file.
#[derive(Debug, Error)]
pub enum ChainError {
    /// Underlying RPC error (timeout, 429, server error, ...).
    #[error("rpc error: {0}")]
    Rpc(String),

    /// NonceManager reports timeout (tx pending too long).
    #[error("nonce stuck for {addr:?} (last seen {stuck_for:?})")]
    NonceStuck { addr: Address, stuck_for: Duration },

    /// RPC rejected the tx for low fee.
    #[error("fee underpriced: required {required}, offered {offered}")]
    FeeUnderpriced { required: U256, offered: U256 },

    /// User-supplied amount fails to parse / overflow.
    #[error("invalid amount '{value}': {reason}")]
    InvalidAmount { value: String, reason: &'static str },

    /// EIP-155 chainId mismatch between signer and tx envelope.
    #[error("invalid chain id: expected {expected}, got {actual}")]
    InvalidChainId { expected: u64, actual: u64 },

    /// tx was mined but status = 0 (reverted).
    #[error("tx reverted: hash={hash:?}, reason={reason}")]
    TxReverted { hash: TxHash, reason: String },

    /// RBF/Cancel: original tx hash not found on-chain.
    #[error("tx not found: {hash:?}")]
    TxNotFound { hash: TxHash },

    /// RBF/Cancel: original tx already mined (can't replace).
    #[error("tx already mined: hash={hash:?} block={block}")]
    TxAlreadyMined { hash: TxHash, block: u64 },

    /// Wallet balance < value + max fee.
    #[error("insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: U256, available: U256 },

    /// `eth_estimateGas` reverted or timed out.
    #[error("gas estimation failed: {reason}")]
    GasEstimationFailed { reason: String },

    /// 120 s receipt polling timed out (tx in mempool but not mined).
    #[error("receipt polling timed out after {0:?}")]
    ReceiptTimeout(Duration),

    /// Other internal error (signing, etc.).
    #[error("chain internal: {0}")]
    Internal(String),
}

impl ChainError {
    /// Stable error code for `CliError` downcast (per ADR-0006 rev1).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Rpc(_) => "EVMC-001",
            Self::NonceStuck { .. } => "EVMC-002",
            Self::FeeUnderpriced { .. } => "EVMC-003",
            Self::InvalidAmount { .. } => "EVMC-004",
            Self::InvalidChainId { .. } => "EVMC-005",
            Self::TxReverted { .. } => "EVMC-006",
            Self::TxNotFound { .. } => "EVMC-007",
            Self::TxAlreadyMined { .. } => "EVMC-008",
            Self::InsufficientFunds { .. } => "EVMC-009",
            Self::GasEstimationFailed { .. } => "EVMC-010",
            Self::ReceiptTimeout(_) => "EVMC-099",
            Self::Internal(_) => "EVM-999",
        }
    }
}

/// Trait for chain operations. The V8 §5 M3 design calls for a trait
/// abstraction so that the alloy-specific implementation can be
/// swapped. **M3 stub**: the trait is defined; the alloy
/// implementation will follow in M3 finalization.
pub trait Chain: Send + Sync {
    /// Read the ETH balance of `addr` (in wei).
    fn balance(
        &self,
        addr: Address,
    ) -> impl std::future::Future<Output = Result<U256, ChainError>> + Send;

    /// Get the chain id (per EIP-155). For Sepolia, returns 11155111.
    fn chain_id(&self) -> impl std::future::Future<Output = Result<u64, ChainError>> + Send;

    /// Get the current `pending` nonce for `addr`.
    fn pending_nonce(
        &self,
        addr: Address,
    ) -> impl std::future::Future<Output = Result<u64, ChainError>> + Send;

    /// Get fee estimate for the next EIP-1559 transaction.
    fn estimate_fees(
        &self,
    ) -> impl std::future::Future<Output = Result<FeeEstimate, ChainError>> + Send;

    /// Send a signed tx (RLP bytes); returns the tx hash.
    fn broadcast_tx(
        &self,
        signed_tx_bytes: &[u8],
    ) -> impl std::future::Future<Output = Result<TxHash, ChainError>> + Send;

    /// Poll for a receipt; returns `Ok(None)` on timeout.
    fn wait_for_receipt(
        &self,
        hash: TxHash,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<Option<TransactionReceipt>, ChainError>> + Send;

    /// Get a transaction by hash (for RBF / Cancel).
    fn get_tx(
        &self,
        hash: TxHash,
    ) -> impl std::future::Future<Output = Result<Option<TransactionInfo>, ChainError>> + Send;
}

/// Recommended fees for the next EIP-1559 transaction.
#[derive(Debug, Clone, Copy)]
pub struct FeeEstimate {
    pub base_fee: U256,
    pub priority_fee: U256,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
}

/// Minimal transaction receipt.
#[derive(Debug, Clone)]
pub struct TransactionReceipt {
    pub hash: TxHash,
    pub block_number: u64,
    pub status: bool,
    pub gas_used: U256,
}

/// Minimal transaction info (for RBF / Cancel).
#[derive(Debug, Clone)]
pub struct TransactionInfo {
    pub hash: TxHash,
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub nonce: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub data: alloy_primitives::B256,
    pub signature_v: u8,
    pub signature_r: U256,
    pub signature_s: U256,
}

/// A signed transaction envelope, ready for broadcast.
#[derive(Debug, Clone)]
pub struct SignedEthTransfer {
    pub raw: Vec<u8>,
    pub hash: TxHash,
}

pub mod alloy_chain;
pub mod client;
pub mod erc20;
pub mod nonce;
pub mod rbf;

pub use client::RpcClient;
pub use nonce::NonceManager;
pub use rbf::{bump_fee, cancel};
