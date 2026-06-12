// SPDX-License-Identifier: MIT
//
// E2E test: Sepolia `--bump-fee` rescue (M3 DoD).
//
// Per PLAN-V9 §5 M3 DoD: "E2E test (`#[ignore]`): real Sepolia
// transfer + `--bump-fee` rescue; documentation describes trigger".
//
// **Status: `#[ignore]`d.** To run:
//
// ```bash
// EVMCLI_E2E_KEY=<hex private key w/o 0x> \
//   cargo test --test e2e_sepolia_bump -- --ignored --nocapture
// ```
//
// Required env:
//   - `EVMCLI_E2E_KEY`: 32-byte hex private key for the sender
//     (the account must be funded on Sepolia).
//   - `EVMCLI_E2E_RPC`: Sepolia HTTP(S) RPC URL. Defaults to the
//     public Sepolia endpoint `https://rpc.sepolia.org` if unset.
//
// What it does:
//   1. Build an `AlloyChain` against the Sepolia RPC.
//   2. Send an ETH transfer with a low max_fee (so it stays pending).
//   3. Wait 30 seconds (the tx should still be pending).
//   4. Build a replacement tx with a bumped max_fee (≥110% per
//      `rbf::compute_bump`).
//   5. Broadcast the replacement.
//   6. Wait up to 60 seconds for the receipt; assert `status = true`.
//
// **Do not run this test in CI automatically.** It uses real ETH
// (testnet ETH) and external RPC. The `#[ignore]` ensures it stays
// out of the default test run; the explicit invocation above
// requires the operator to supply credentials.

#![allow(unused_crate_dependencies)]
#![allow(clippy::disallowed_methods)] // E2E test: `.expect()` on real RPC
#![allow(clippy::expect_used, clippy::unwrap_used)] // same

use std::env;
use std::time::Duration;

use alloy_primitives::B256;
use alloy_signer_local::PrivateKeySigner;
use evm_cli::chain::rbf;
use evm_cli::chain::Chain;
use evm_cli::types::{Address, Amount};

fn env_key() -> Option<B256> {
    let raw = env::var("EVMCLI_E2E_KEY").ok()?;
    let s: &str = raw.strip_prefix("0x").unwrap_or(&raw);
    let bytes = hex::decode(s).expect("EVMCLI_E2E_KEY must be valid hex");
    if bytes.len() != 32 {
        panic!("EVMCLI_E2E_KEY must be 32 bytes (64 hex chars)");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(B256::from(arr))
}

fn env_rpc() -> String {
    env::var("EVMCLI_E2E_RPC").unwrap_or_else(|_| "https://rpc.sepolia.org".to_string())
}

#[tokio::test]
#[ignore = "requires EVMCLI_E2E_KEY and EVMCLI_E2E_RPC; see file header"]
async fn sepolia_bump_fee_rescue() {
    let key = env_key().expect("EVMCLI_E2E_KEY env var must be set");
    let rpc = env_rpc();
    eprintln!("E2E: Sepolia RPC = {rpc}");

    let chain = evm_cli::chain::alloy_chain::AlloyChain::new(&rpc)
        .await
        .expect("build chain");
    eprintln!("E2E: chain_id = {:?}", chain.chain_id());

    // Build the signer.
    let signer = PrivateKeySigner::from_slice(&key.0).expect("signer from env key");
    let sender: Address = signer.address().into();
    eprintln!("E2E: sender = {sender}");

    // 1. Send a low-fee tx to ourselves (1 wei). Will likely stay
    //    pending because the fee is below the mempool floor.
    let value = Amount::from_wei(alloy_primitives::U256::from(1u64));
    let signed1 = chain
        .build_eth_transfer(&signer, sender, value, None, None)
        .await
        .expect("build initial low-fee tx");
    eprintln!("E2E: initial tx hash = {}", signed1.hash);
    let hash1 = chain.broadcast_tx(&signed1.raw).await.expect("broadcast 1");
    eprintln!("E2E: broadcast1 done, hash = {hash1}");

    // 2. Wait 30s for the tx to settle into the mempool.
    eprintln!("E2E: sleeping 30s to let the tx sit in the mempool…");
    tokio::time::sleep(Duration::from_secs(30)).await;

    // 3. Bump the fee using the RBF pipeline.
    let bump = rbf::bump_fee(&chain, &signer, hash1)
        .await
        .expect("bump_fee");
    eprintln!(
        "E2E: bumped tx hash = {}, new max_fee = {}, new prio = {}",
        bump.new_hash, bump.new_max_fee_per_gas, bump.new_max_priority_fee_per_gas
    );
    assert_ne!(bump.new_hash, hash1, "bumped tx must have a different hash");

    // 4. Wait for the receipt (60s timeout).
    let receipt = chain
        .wait_for_receipt(bump.new_hash, Duration::from_secs(60))
        .await
        .expect("wait_for_receipt");
    let receipt = receipt.expect("bumped tx should be mined within 60s");
    eprintln!(
        "E2E: bumped tx mined: block={} status={} gas={}",
        receipt.block_number, receipt.status, receipt.gas_used
    );
    assert!(receipt.status, "bumped tx should have succeeded");
}
