// SPDX-License-Identifier: MIT
//
// `AlloyChain` — the alloy-backed implementation of the `Chain` trait.
//
// **M3 skeleton status**: this file is intentionally minimal. The
// read-only surface is partially wired against an alloy provider, but
// the trait `impl Chain for AlloyChain` is NOT present because the
// alloy 2.0.5 API requires careful field-by-field study that exceeds
// the time budget for M3.
//
// M3 finalization will add:
//   - `impl Chain for AlloyChain` with the full method bodies
//   - `build_eth_transfer` (alloy RLP signing)
//   - `get_tx` projection (alloy RPC-types field access)
//   - Anvil integration test in `tests/it_eth_transfer.rs`
//
// See PLAN-V10 §20 (M3 changelog) for the rationale.

use std::time::Duration;

use alloy_primitives::{Address, TxHash, U256};
#[allow(unused_imports)]
use alloy_provider::Provider;

use crate::chain::{
    ChainError, FeeEstimate, RpcClient, SignedEthTransfer, TransactionInfo, TransactionReceipt,
};

/// Concrete chain implementation backed by an alloy provider + a
/// rate-limited `RpcClient`. M3 skeleton: see module docs.
pub struct AlloyChain {
    client: RpcClient,
    chain_id: u64,
}

impl std::fmt::Debug for AlloyChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlloyChain")
            .field("rpc_url", &self.client.rpc_url_str())
            .field("chain_id", &self.chain_id)
            .finish()
    }
}

impl AlloyChain {
    /// Build an `AlloyChain` against the given HTTP(S) RPC URL.
    /// Fetches the chain id from the RPC on construction.
    pub async fn new(rpc_url: &str) -> Result<Self, ChainError> {
        let client = RpcClient::new(rpc_url)?;
        let chain_id = Self::fetch_chain_id(&client).await?;
        Ok(Self { client, chain_id })
    }

    /// Build with a custom rate limit.
    pub async fn with_rate(rpc_url: &str, rps: u32) -> Result<Self, ChainError> {
        let client = RpcClient::with_rate(rpc_url, rps)?;
        let chain_id = Self::fetch_chain_id(&client).await?;
        Ok(Self { client, chain_id })
    }

    /// Build with a pre-existing client (for tests). The chain id
    /// is **not** fetched from RPC in this constructor; the caller
    /// supplies it.
    pub fn with_client(client: RpcClient, chain_id: u64) -> Self {
        Self { client, chain_id }
    }

    /// Chain id this client is bound to.
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Underlying RpcClient.
    pub fn client(&self) -> &RpcClient {
        &self.client
    }

    async fn fetch_chain_id(client: &RpcClient) -> Result<u64, ChainError> {
        client.acquire().await.ok();
        let id = client
            .provider()
            .get_chain_id()
            .await
            .map_err(|e| ChainError::Rpc(format!("get_chain_id: {e}")))?;
        // `get_chain_id` on `dyn Provider` returns `u64` directly (the
        // concrete alloy provider's network-specific type is erased
        // through the trait object).
        Ok(id)
    }

    // ────────────────────────────────────────────────────────────────
    // M3 skeleton: read-only methods exposed for direct call (not via
    // the Chain trait — the trait impl is deferred to M3 finalization).
    // ────────────────────────────────────────────────────────────────

    /// Read the ETH balance of `addr` (in wei). Direct method, not
    /// via the `Chain` trait (trait impl deferred to M3 finalization).
    pub async fn balance_direct(&self, addr: Address) -> Result<U256, ChainError> {
        self.client.acquire().await.ok();
        let bal = self
            .client
            .provider()
            .get_balance(addr)
            .await
            .map_err(|e| ChainError::Rpc(format!("get_balance: {e}")))?;
        Ok(bal)
    }

    /// Get the chain id. Direct method.
    pub fn chain_id_direct(&self) -> u64 {
        self.chain_id
    }

    /// Get the current `pending` nonce for `addr`. Direct method.
    pub async fn pending_nonce_direct(&self, addr: Address) -> Result<u64, ChainError> {
        self.client.acquire().await.ok();
        let n = self
            .client
            .provider()
            .get_transaction_count(addr)
            .pending()
            .await
            .map_err(|e| ChainError::Rpc(format!("get_transaction_count: {e}")))?;
        // `get_transaction_count` on `dyn Provider` returns `u64` directly.
        Ok(n)
    }

    /// Get fee estimate. Direct method.
    pub async fn estimate_fees_direct(&self) -> Result<FeeEstimate, ChainError> {
        use alloy_eips::BlockNumberOrTag;
        self.client.acquire().await.ok();
        let provider = self.client.provider();
        let history = provider
            .get_fee_history(5_u64, BlockNumberOrTag::Latest, &[50.0])
            .await
            .map_err(|e| ChainError::Rpc(format!("get_fee_history: {e}")))?;
        let base_fee = U256::from(
            history
                .latest_block_base_fee()
                .unwrap_or(20_000_000_000u128),
        );
        let priority_fee = history
            .reward
            .as_ref()
            .and_then(|r| r.last())
            .and_then(|v| v.first().copied())
            .map(U256::from)
            .unwrap_or(U256::from(1_000_000_000u64));
        let max_fee = base_fee * U256::from(2u64) + priority_fee;
        Ok(FeeEstimate {
            base_fee,
            priority_fee,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: priority_fee,
        })
    }

    /// Broadcast a signed raw tx. Direct method.
    pub async fn broadcast_tx_direct(&self, signed_tx_bytes: &[u8]) -> Result<TxHash, ChainError> {
        self.client.acquire().await.ok();
        #[allow(unused_mut)] // alloy API takes &mut but doesn't actually need it
        let mut tx_bytes = signed_tx_bytes.to_vec();
        let pending = self
            .client
            .provider()
            .send_raw_transaction(&tx_bytes)
            .await
            .map_err(|e| ChainError::Rpc(format!("send_raw_transaction: {e}")))?;
        Ok(*pending.tx_hash())
    }

    /// Poll for a receipt. Direct method.
    pub async fn wait_for_receipt_direct(
        &self,
        hash: TxHash,
        timeout: Duration,
    ) -> Result<Option<TransactionReceipt>, ChainError> {
        self.client.acquire().await.ok();
        let provider = self.client.provider();
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            let receipt = provider
                .get_transaction_receipt(hash)
                .await
                .map_err(|e| ChainError::Rpc(format!("get_transaction_receipt: {e}")))?;
            if let Some(r) = receipt {
                return Ok(Some(TransactionReceipt {
                    hash,
                    block_number: r.block_number.unwrap_or(0),
                    status: r.status(),
                    gas_used: U256::from(r.gas_used),
                }));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(None)
    }

    /// **M3 stub**: full impl deferred to M3 finalization.
    pub async fn get_tx_stub(&self, _hash: TxHash) -> Result<Option<TransactionInfo>, ChainError> {
        Err(ChainError::Internal(
            "get_tx: deferred to M3 finalization (RBF/Cancel pipeline)".to_string(),
        ))
    }

    /// **M3 stub**: full impl deferred to M3 finalization.
    pub async fn build_eth_transfer_stub(
        &self,
        _signer_addr: Address,
        _to: Address,
        _value: U256,
        _max_fee_per_gas: Option<U256>,
        _max_priority_fee_per_gas: Option<U256>,
    ) -> Result<SignedEthTransfer, ChainError> {
        Err(ChainError::Internal(
            "build_eth_transfer: deferred to M3 finalization (alloy 2.0.5 RLP signing requires anvil integration test)".to_string(),
        ))
    }
}
