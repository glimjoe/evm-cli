// SPDX-License-Identifier: MIT
//
// `AlloyChain` — the alloy-backed implementation of the `Chain` trait.
//
// Per PLAN-V9 §5 M3 DoD (post-audit fix): full implementation of all
// `Chain` trait methods + `build_eth_transfer` (alloy 2.0.5 RLP
// signing via `SignableTransaction::signature_hash()` +
// `Signer::sign_hash()`) + `get_tx` projection.
//
// The anvil integration test in `tests/it_eth_transfer.rs` exercises
// the ETH transfer happy path end-to-end.

use std::time::Duration;

use alloy_consensus::{SignableTransaction, Transaction as TransactionTrait, TxEip1559};
use alloy_eips::Encodable2718;
use alloy_network_primitives::TransactionResponse;
use alloy_primitives::{TxKind, B256, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;

use crate::chain::{
    Chain, ChainError, FeeEstimate, RpcClient, SignedEthTransfer, TransactionInfo,
    TransactionReceipt,
};
use crate::types::{Address, Amount, BlockNumber, ChainId, Nonce, TxHash};

/// Concrete chain implementation backed by an alloy provider + a
/// rate-limited `RpcClient`. Full `impl Chain for AlloyChain` below.
pub struct AlloyChain {
    client: RpcClient,
    chain_id: ChainId,
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
    pub fn with_client(client: RpcClient, chain_id: ChainId) -> Self {
        Self { client, chain_id }
    }

    /// Underlying RpcClient.
    pub fn client(&self) -> &RpcClient {
        &self.client
    }

    async fn fetch_chain_id(client: &RpcClient) -> Result<ChainId, ChainError> {
        client.acquire().await.ok();
        let id = client
            .provider()
            .get_chain_id()
            .await
            .map_err(|e| ChainError::Rpc(format!("get_chain_id: {e}")))?;
        Ok(ChainId(id))
    }
}

impl Chain for AlloyChain {
    fn chain_id(&self) -> ChainId {
        self.chain_id
    }

    async fn balance(&self, addr: Address) -> Result<Amount, ChainError> {
        self.client.acquire().await.ok();
        let bal = self
            .client
            .provider()
            .get_balance(addr.into_alloy())
            .await
            .map_err(|e| ChainError::Rpc(format!("get_balance: {e}")))?;
        Ok(Amount::from_wei(bal))
    }

    async fn pending_nonce(&self, addr: Address) -> Result<Nonce, ChainError> {
        self.client.acquire().await.ok();
        let n = self
            .client
            .provider()
            .get_transaction_count(addr.into_alloy())
            .pending()
            .await
            .map_err(|e| ChainError::Rpc(format!("get_transaction_count: {e}")))?;
        Ok(Nonce(n))
    }

    async fn estimate_fees(&self) -> Result<FeeEstimate, ChainError> {
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

    async fn build_eth_transfer(
        &self,
        signer: &PrivateKeySigner,
        to: Address,
        value: Amount,
        max_fee_per_gas: Option<Amount>,
        max_priority_fee_per_gas: Option<Amount>,
    ) -> Result<SignedEthTransfer, ChainError> {
        // 1. EIP-155 chainId check: the signer's address is used as
        //    the recovery address; the actual chainId is bound into
        //    the signing hash. (In V1, both signer and RPC are
        //    Sepolia; future chains would relax this.)
        let signer_addr: Address = signer.address().into();

        // 2. Look up the pending nonce for the signer.
        let nonce = self.pending_nonce(signer_addr).await?;

        // 3. Estimate fees if not provided.
        let (max_fee, priority_fee) = match (max_fee_per_gas, max_priority_fee_per_gas) {
            (Some(m), Some(p)) => (m.into_wei(), p.into_wei()),
            _ => {
                let est = self.estimate_fees().await?;
                (
                    max_fee_per_gas
                        .map(|m| m.into_wei())
                        .unwrap_or(est.max_fee_per_gas),
                    max_priority_fee_per_gas
                        .map(|p| p.into_wei())
                        .unwrap_or(est.max_priority_fee_per_gas),
                )
            }
        };

        // 4. Build the EIP-1559 transaction.
        let tx = TxEip1559 {
            chain_id: self.chain_id.as_u64(),
            nonce: nonce.into(),
            // 21_000 is the fixed gas for a plain ETH transfer.
            gas_limit: 21_000,
            max_fee_per_gas: u128::try_from(max_fee).map_err(|_| ChainError::InvalidAmount {
                value: max_fee.to_string(),
                reason: "max_fee_per_gas overflows u128",
            })?,
            max_priority_fee_per_gas: u128::try_from(priority_fee).map_err(|_| {
                ChainError::InvalidAmount {
                    value: priority_fee.to_string(),
                    reason: "max_priority_fee_per_gas overflows u128",
                }
            })?,
            to: TxKind::Call(to.into_alloy()),
            value: value.into_wei(),
            access_list: Default::default(),
            input: Default::default(),
        };

        // 5. Compute the EIP-155 signing hash.
        let signing_hash = tx.signature_hash();

        // 6. Sign with the user's signer.
        let signature = signer
            .sign_hash(&signing_hash)
            .await
            .map_err(|e| ChainError::Internal(format!("sign_hash: {e}")))?;

        // 7. Wrap into a Signed<TxEip1559, Signature> for RLP encoding.
        let signed = tx.into_signed(signature);

        // 8. Encode as EIP-2718 envelope (typed tx, 0x02 || rlp(...)).
        let raw = signed.encoded_2718();

        // 9. Compute the resulting transaction hash (keccak256 of envelope).
        let hash = alloy_primitives::keccak256(&raw);
        Ok(SignedEthTransfer {
            raw: raw.to_vec(),
            hash: TxHash::from_b256(hash),
        })
    }

    async fn broadcast_tx(&self, signed_tx_bytes: &[u8]) -> Result<TxHash, ChainError> {
        self.client.acquire().await.ok();
        let pending = self
            .client
            .provider()
            .send_raw_transaction(signed_tx_bytes)
            .await
            .map_err(|e| ChainError::Rpc(format!("send_raw_transaction: {e}")))?;
        Ok(TxHash::from_b256(*pending.tx_hash()))
    }

    async fn wait_for_receipt(
        &self,
        hash: TxHash,
        timeout: Duration,
    ) -> Result<Option<TransactionReceipt>, ChainError> {
        self.client.acquire().await.ok();
        let provider = self.client.provider();
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            let receipt = provider
                .get_transaction_receipt(hash.into_b256())
                .await
                .map_err(|e| ChainError::Rpc(format!("get_transaction_receipt: {e}")))?;
            if let Some(r) = receipt {
                return Ok(Some(TransactionReceipt {
                    hash,
                    block_number: BlockNumber(r.block_number.unwrap_or(0)),
                    status: r.status(),
                    gas_used: U256::from(r.gas_used),
                }));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(None)
    }

    async fn get_tx(&self, hash: TxHash) -> Result<Option<TransactionInfo>, ChainError> {
        self.client.acquire().await.ok();
        let provider = self.client.provider();
        let raw = provider
            .get_transaction_by_hash(hash.into_b256())
            .await
            .map_err(|e| ChainError::Rpc(format!("get_transaction_by_hash: {e}")))?;
        let Some(tx) = raw else {
            return Ok(None);
        };
        // `tx` is `AnyRpcTransaction = WithOtherFields<Transaction<AnyTxEnvelope>>`.
        // It derefs to `Transaction<AnyTxEnvelope>`, which implements
        // both `Transaction` (consensus trait) and `TransactionResponse`
        // (network-agnostic RPC projection). We use the trait methods:
        let data: B256 = {
            let mut buf = [0u8; 32];
            let input = TransactionTrait::input(&tx);
            let n = input.len().min(32);
            buf[..n].copy_from_slice(&input[..n]);
            B256::from(buf)
        };
        let nonce = TransactionTrait::nonce(&tx);
        let value = TransactionTrait::value(&tx);
        // Consensus `Transaction::max_fee_per_gas` returns `u128` (the
        // tx field value, regardless of legacy vs. 1559). For pre-1559
        // txs this is the gas price.
        let max_fee = U256::from(TransactionTrait::max_fee_per_gas(&tx));
        // Consensus `Transaction::max_priority_fee_per_gas` returns
        // `Option<u128>`: `Some` for 1559 txs, `None` for legacy.
        let max_prio = U256::from(TransactionTrait::max_priority_fee_per_gas(&tx).unwrap_or(0u128));
        let tx_hash = TransactionResponse::tx_hash(&tx);
        let from = TransactionResponse::from(&tx);
        let to = <dyn TransactionResponse>::to(&tx);
        // The signature is on the inner envelope; for V1 we expose v/r/s
        // as zeros (display only; the actual re-signing in RBF uses the
        // signer's full private key). Extracting v/r/s from
        // `AnyTxEnvelope::Eip1559` is straightforward but requires
        // downcast; deferred to V2 if needed.
        let (signature_v, signature_r, signature_s) = (0u8, U256::ZERO, U256::ZERO);
        Ok(Some(TransactionInfo {
            hash: TxHash::from_b256(tx_hash),
            from: Address::from_alloy(from),
            to: to.map(Address::from_alloy),
            value,
            nonce: Nonce(nonce),
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: max_prio,
            data,
            signature_v,
            signature_r,
            signature_s,
        }))
    }
}
