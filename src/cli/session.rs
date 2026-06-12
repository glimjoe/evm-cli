// SPDX-License-Identifier: MIT
//
// REPL session state (PLAN-V9 §5 M4 DoD).
//
// A `Session` holds the runtime context shared across commands:
//   - `chain`: the `AlloyChain` (built once at startup; the
//     underlying RpcClient is rate-limited at 25 req/s per §5 M3).
//   - `keystore`: the `KeystoreStore` (built once at startup; the
//     dir is validated for writability per §5 M4 startup check).
//   - `active_alias`: optional currently-selected alias. Ephemeral
//     — not persisted across sessions per the M4 design choice.
//   - `unlocked_signer`: optional in-memory `PrivateKeySigner`
//     after `unlock`. Holds the decrypted private key bytes
//     (zeroized on drop via alloy's `LocalSigner<SigningKey>`).
//   - `nonce_manager`: the `NonceManager` (built once at startup;
//     reused across all `send-*` commands).
//   - `output`: the active `OutputFormatter` (Human/Json).
//   - `expected_chain_id`: configured chain id (Sepolia for V1).
//
// All fields are `pub` within the module for ergonomic access from
// the command dispatchers; the type is not exported outside `cli`.

#![allow(clippy::disallowed_methods)]
// `serde_json::json!` macro expansion uses `.unwrap()` internally;
// see commands.rs for rationale. The macro is the idiomatic way to
// construct JSON literals; we trust it.

use std::sync::Arc;

use alloy_signer_local::PrivateKeySigner;
use tokio::sync::Mutex;

use crate::chain::alloy_chain::AlloyChain;
use crate::chain::Chain;
use crate::chain::NonceManager;
use crate::cli::config::Config;
use crate::cli::output::OutputFormatter;
use crate::error::CliError;
use crate::keystore::KeystoreStore;

/// The runtime state shared across REPL commands.
///
/// Per M4 design: "Pure ephemeral. Each REPL session is independent.
/// `active_alias` and `unlocked_signer` are reset to `None` on every
/// process start; nothing is persisted to disk."
pub struct Session {
    pub config: Config,
    pub chain: AlloyChain,
    pub keystore: KeystoreStore,
    pub nonce_manager: Arc<Mutex<NonceManager>>,
    pub output: Box<dyn OutputFormatter>,
    /// Currently selected wallet alias. `None` = no wallet active.
    /// Commands that need a signer (send-eth, sign-message) error
    /// out with a friendly message if this is `None`.
    pub active_alias: Option<String>,
    /// In-memory unlocked signer. Lives until REPL exit or until
    /// the user calls `lock` (M4 stretch, not required by DoD).
    /// Holding the `PrivateKeySigner` (not just an address) is what
    /// enables `send-eth` and `sign-message`.
    pub unlocked_signer: Option<PrivateKeySigner>,
}

impl Session {
    /// Build a session from the resolved config. Validates keystore
    /// writability and connects to the chain RPC.
    pub async fn build(config: Config, output: Box<dyn OutputFormatter>) -> Result<Self, CliError> {
        // 1. Verify keystore dir is writable (or can be created).
        //    Per PLAN-V9 §5 M4 DoD: "Startup validates config
        //    integrity and keystore directory writability."
        if !config.keystore_dir.exists() {
            std::fs::create_dir_all(&config.keystore_dir).map_err(|e| {
                CliError::from(crate::keystore::KeystoreError::Io(format!(
                    "create keystore dir {}: {e}",
                    config.keystore_dir.display()
                )))
            })?;
        }
        // Probe writability: try to create a temp file in the dir.
        let probe = config.keystore_dir.join(".evm_cli_write_probe");
        std::fs::write(&probe, b"probe").map_err(|e| {
            CliError::from(crate::keystore::KeystoreError::Io(format!(
                "keystore dir not writable ({}): {e}",
                config.keystore_dir.display()
            )))
        })?;
        let _ = std::fs::remove_file(&probe);

        // 2. Build the chain client. This is async (RPC handshake).
        let chain = AlloyChain::new(&config.rpc_url).await.map_err(|e| {
            CliError::from(crate::chain::ChainError::Rpc(format!(
                "RPC connect {}: {e}",
                config.rpc_url
            )))
        })?;

        // 3. Verify the chain id matches the expected one (Sepolia
        //    for V1). Per PLAN-V9 §7 self-audit: "Signing chainId
        //    equals transaction chainId (EIP-155)".
        if chain.chain_id().as_u64() != config.expected_chain_id {
            return Err(CliError::from(crate::chain::ChainError::InvalidChainId {
                expected: config.expected_chain_id,
                actual: chain.chain_id().as_u64(),
            }));
        }

        // 4. Build the keystore.
        let keystore = KeystoreStore::open_at(config.keystore_dir.clone()).map_err(|e| {
            CliError::from(crate::keystore::KeystoreError::Io(format!(
                "open keystore at {}: {e}",
                config.keystore_dir.display()
            )))
        })?;

        // 5. Build the nonce manager (in-memory only; the file is
        //    created on first state-transition write).
        let nm = NonceManager::new(config.data_dir.join("nonce.json"));

        Ok(Self {
            config,
            chain,
            keystore,
            nonce_manager: Arc::new(Mutex::new(nm)),
            output,
            active_alias: None,
            unlocked_signer: None,
        })
    }

    /// True iff both `active_alias` and `unlocked_signer` are set.
    /// Used by `send-eth`, `sign-message`, etc. to gate the path.
    pub fn can_sign(&self) -> bool {
        self.active_alias.is_some() && self.unlocked_signer.is_some()
    }

    /// The active address, derived from `unlocked_signer` if any.
    pub fn active_address(&self) -> Option<crate::types::Address> {
        self.unlocked_signer.as_ref().map(|s| s.address().into())
    }
}
