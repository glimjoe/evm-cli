// SPDX-License-Identifier: MIT
//
// evm_cli::chain — Ethereum JSON-RPC chain operations.
//
// Per PLAN-V9 §5 M3 DoD and ADR-0003 (workspace split), this module
// depends only on `crypto`, `keystore`, `types`, and external crates
// (alloy, governor, nix, reqwest, url). It does NOT depend on `cli`
// (which comes in M4).
//
// See PLAN-V9 §5 M3 DoD for the full specification.
//
// **M3 status (post-audit fix):** This module is now complete. The
// following are implemented and unit-tested:
//   - `nonce::NonceManager` (4-state machine, JSON-lines log, flock)
//   - `rbf::compute_bump` (3-term fee-bump formula) +
//     `bump_fee` / `cancel` (full pipeline, alloy 2.0.5 EIP-1559)
//   - `erc20::{encode_transfer, encode_balance_of, decode_balance_of}`
//   - `client::RpcClient` (rate-limited, governor)
//   - `alloy_chain::AlloyChain` — full `impl Chain for AlloyChain`,
//     including `build_eth_transfer`, `get_tx`, `broadcast_tx`,
//     `wait_for_receipt`.
//
// Tests: unit tests in every submodule + anvil integration test
// `tests/it_eth_transfer.rs` (M3 DoD §5 L215) +
// `tests/e2e_sepolia_bump.rs` (`#[ignore]`, M3 DoD §5 L216).

#![cfg_attr(test, allow(clippy::disallowed_methods))]
// Test-only allow: tests legitimately use `.expect()` / `.unwrap()` on
// fixed inputs. Production paths must not trip
// `clippy::disallowed_methods` (P0-4). Same narrow form as
// `src/keystore/mod.rs:33` and `src/crypto/mod.rs:15` (M3 audit C12).

use std::time::Duration;

use alloy_primitives::{B256, U256};
use thiserror::Error;

use crate::types::{Address, Amount, BlockNumber, ChainId, Nonce, TxHash};

/// Errors from chain operations. Per PLAN-V9 §5 M3 + ADR-0006 rev1, all
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
    TxAlreadyMined { hash: TxHash, block: BlockNumber },

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

/// P0-1 / ADR-0006 rev1: `CodeSource` impl lives next to the real
/// `ChainError` enum (not in a placeholder `pub mod chain { ... }` in
/// `src/error.rs`). The M3 audit (issue B1) found that a placeholder
/// `ChainError` with only `RpcError { kind: RpcErrorKind }` was wired
/// into `CliError::code()`'s downcast chain, while the real enum
/// (above) was used by all M3 code paths — so every real `ChainError`
/// fell through to `EVM-999`. With this impl, the downcast finds the
/// real enum and returns the correct EVMC-NNN code.
impl crate::error::CodeSource for ChainError {
    fn code(&self) -> &'static str {
        // Same mapping as the inherent `ChainError::code` method.
        ChainError::code(self)
    }
}

/// Trait for chain operations. The PLAN-V9 §5 M3 design calls for a
/// trait abstraction so that the alloy-specific implementation can be
/// swapped. Per PLAN-V9 §3, the API boundary uses the project's
/// newtypes (`Address`, `Amount`, `BlockNumber`, `ChainId`, `Nonce`,
/// `Signature`, `TxHash`); alloy types are converted to/from at the
/// implementation boundary.
pub trait Chain: Send + Sync {
    /// Chain id this client is bound to.
    fn chain_id(&self) -> ChainId;

    /// Read the ETH balance of `addr` (in wei).
    fn balance(
        &self,
        addr: Address,
    ) -> impl std::future::Future<Output = Result<Amount, ChainError>> + Send;

    /// Get the current `pending` nonce for `addr`.
    fn pending_nonce(
        &self,
        addr: Address,
    ) -> impl std::future::Future<Output = Result<Nonce, ChainError>> + Send;

    /// Get fee estimate for the next EIP-1559 transaction.
    fn estimate_fees(
        &self,
    ) -> impl std::future::Future<Output = Result<FeeEstimate, ChainError>> + Send;

    /// Build + sign an EIP-1559 transaction. Returns the raw
    /// RLP-encoded signed bytes (ready for `broadcast_tx`) and the
    /// resulting transaction hash.
    ///
    /// For plain ETH transfers, pass `data: vec![]` and a non-zero
    /// `value`. For ERC-20 (or any contract) calls, pass
    /// `value: Amount::ZERO` and the calldata in `data`.
    fn build_eth_transfer(
        &self,
        signer: &alloy_signer_local::PrivateKeySigner,
        to: Address,
        value: Amount,
        data: Vec<u8>,
        max_fee_per_gas: Option<Amount>,
        max_priority_fee_per_gas: Option<Amount>,
    ) -> impl std::future::Future<Output = Result<SignedEthTransfer, ChainError>> + Send;

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
    pub block_number: BlockNumber,
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
    pub nonce: Nonce,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub data: B256,
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

// Re-export the alloy type so existing callers (e.g. `sign.rs`,
// `keystore/mod.rs`) that previously used `alloy_primitives::Address`
// can keep using it without re-importing.
#[allow(unused_imports)]
pub(crate) use alloy_primitives::Address as _AlloyAddress;
