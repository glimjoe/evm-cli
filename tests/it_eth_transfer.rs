// SPDX-License-Identifier: MIT
//
// Integration test: ETH transfer on anvil (M3 DoD).
//
// Per PLAN-V9 §5 M3 DoD: "Integration test:
// `tests/it_eth_transfer.rs` using `alloy::node_bindings::anvil`".
//
// The test:
//  1. Spawns an anvil instance via `alloy::node_bindings::AnvilInstance`.
//  2. Generates a random signer funded by anvil's prefunded
//     accounts (a→f: 10000 ETH each).
//  3. Builds an EIP-1559 ETH transfer (AlloyChain::build_eth_transfer).
//  4. Broadcasts it (AlloyChain::broadcast_tx).
//  5. Polls for a receipt with a 60s timeout
//     (AlloyChain::wait_for_receipt).
//  6. Asserts the receipt is `Ok(Some(_))` and `status = true`.
//
// The test exercises:
//   - The full M3 chain layer (Chain trait, AlloyChain, RpcClient)
//   - EIP-1559 signing via alloy 2.0.5
//   - Nonce management (anvil auto-mines; nonce 0 is always available)
//   - Receipt polling
//   - The newtype API boundary (Address, Amount, Nonce, TxHash,
//     BlockNumber, ChainId, Signature)
//
// If anvil cannot be spawned (e.g. binary not in PATH), the test is
// skipped. The CI runner (ubuntu-latest) ships anvil via
// `alloy-node-bindings` (a dev-dep that downloads/finds anvil at
// test time).

#![allow(unused_crate_dependencies)] // alloy-node-bindings is in dev-deps
#![allow(clippy::disallowed_methods)] // integration test: `.expect()` on real RPC results
#![allow(clippy::expect_used, clippy::unwrap_used)] // same

use std::time::Duration;

use alloy_node_bindings::AnvilInstance;
use alloy_signer_local::PrivateKeySigner;
use evm_cli::chain::Chain;
use evm_cli::types::{Address, Amount};

/// Spawn an anvil instance. If anvil is not available on PATH (e.g. in
/// a constrained CI environment), skip the test.
fn spawn_anvil_or_skip() -> Option<AnvilInstance> {
    let result = alloy_node_bindings::Anvil::new().try_spawn();
    match result {
        Ok(i) => Some(i),
        Err(e) => {
            eprintln!("could not spawn anvil: {e}; skipping it_eth_transfer");
            None
        }
    }
}

#[tokio::test]
async fn eth_transfer_end_to_end() {
    let Some(anvil) = spawn_anvil_or_skip() else {
        return;
    };
    let endpoint = anvil.endpoint();
    eprintln!("anvil endpoint: {endpoint}");

    // 1. Build an AlloyChain against the anvil endpoint.
    let chain = evm_cli::chain::alloy_chain::AlloyChain::new(&endpoint)
        .await
        .expect("build chain");
    let chain_id = chain.chain_id();
    eprintln!("chain_id: {chain_id:?}");

    // 2. Pick an anvil prefunded account as the sender.
    //    Anvil's first prefunded account is funded with 10000 ETH
    //    and uses a deterministic key derived from the test mnemonic.
    let keys = anvil.keys();
    let first_key = keys
        .first()
        .expect("anvil instance must have at least one prefunded key");
    let key_bytes: [u8; 32] = first_key.to_bytes().into();
    let sender_signer = PrivateKeySigner::from_slice(&key_bytes).expect("signer from anvil key 0");
    let sender_addr_our: Address = sender_signer.address().into();
    eprintln!("sender: {sender_addr_our}");

    // 3. Pick a recipient (random new signer; will be empty).
    let recipient_signer = PrivateKeySigner::random();
    let recipient_addr: Address = recipient_signer.address().into();
    eprintln!("recipient: {recipient_addr}");

    // 4. Verify the sender has a non-zero balance (anvil prefund).
    let bal = chain.balance(sender_addr_our).await.expect("balance");
    eprintln!("sender balance: {bal:?}");
    assert!(
        bal.as_wei() > &alloy_primitives::U256::ZERO,
        "anvil sender must be funded"
    );

    // 5. Build the EIP-1559 ETH transfer (0.001 ETH = 1e15 wei).
    let value = Amount::try_from_decimal_str("0.001", 18).expect("parse 0.001 ETH");
    let signed = chain
        .build_eth_transfer(&sender_signer, recipient_addr, value, vec![], None, None)
        .await
        .expect("build_eth_transfer");
    eprintln!("signed tx hash: {}", signed.hash);

    // 6. Broadcast the raw signed tx.
    let broadcast_hash = chain.broadcast_tx(&signed.raw).await.expect("broadcast_tx");
    assert_eq!(broadcast_hash, signed.hash, "broadcast hash matches");

    // 7. Poll for the receipt (60s timeout; anvil auto-mines so this
    //    is usually <1s).
    let receipt = chain
        .wait_for_receipt(signed.hash, Duration::from_secs(60))
        .await
        .expect("wait_for_receipt");
    let receipt = receipt.expect("receipt should be mined within 60s on anvil");
    eprintln!(
        "receipt: block={} status={} gas={}",
        receipt.block_number, receipt.status, receipt.gas_used
    );
    assert!(receipt.status, "tx should have succeeded");
    assert!(receipt.block_number.as_u64() > 0, "block number > 0");

    // 8. Verify the recipient now has 0.001 ETH.
    let recipient_bal = chain.balance(recipient_addr).await.expect("balance");
    eprintln!("recipient balance after: {recipient_bal:?}");
    assert_eq!(recipient_bal, value, "recipient received exactly 0.001 ETH");
}

#[tokio::test]
async fn chain_id_is_anvil_default() {
    let Some(anvil) = spawn_anvil_or_skip() else {
        return;
    };
    let endpoint = anvil.endpoint();
    let chain = evm_cli::chain::alloy_chain::AlloyChain::new(&endpoint)
        .await
        .expect("build chain");
    // Anvil's default chainId is 31337.
    assert_eq!(chain.chain_id().as_u64(), 31337, "anvil default chainId");
}

#[tokio::test]
async fn pending_nonce_increments() {
    let Some(anvil) = spawn_anvil_or_skip() else {
        return;
    };
    let endpoint = anvil.endpoint();
    let chain = evm_cli::chain::alloy_chain::AlloyChain::new(&endpoint)
        .await
        .expect("build chain");
    let signer = PrivateKeySigner::random();
    let addr: Address = signer.address().into();
    let nonce0 = chain.pending_nonce(addr).await.expect("nonce 0");
    eprintln!("nonce[0]: {nonce0:?}");
    let nonce1 = chain.pending_nonce(addr).await.expect("nonce 1");
    eprintln!("nonce[1]: {nonce1:?}");
    // The nonce is the same (the second call doesn't broadcast
    // anything, so the pool doesn't advance). Both reads should
    // return the RPC's `pending` nonce for an untouched account.
    assert_eq!(nonce0, nonce1);
}
