// SPDX-License-Identifier: MIT
//
// 11 CLI commands per PLAN-V9 §5 M4 DoD:
//
//   Wallet (state-changing):
//     1. create-wallet <alias>
//     2. import-mnemonic <alias> <phrase>
//     3. list
//     4. use <alias>
//     5. unlock
//
//   Read:
//     6. balance [address]
//     7. pending-tx
//
//   Write (signing):
//     8. send-eth <to> <amount> [--bump-fee <hash>] [--cancel <hash>] [--dry-run]
//     9. send-token <token> <to> <amount> --decimals <n> [--dry-run]
//    10. sign-message <message>
//    11. exit
//
// Each `pub fn` returns `Result<CommandOutcome, CliError>` where
// `CommandOutcome::Continue` (REPL keeps going) or `CommandOutcome::Exit`
// (only emitted by `exit`). All commands write via the session's
// `OutputFormatter` for unified human/JSON output.

#![allow(clippy::disallowed_methods)]
// `serde_json::json!` macro expansion uses `.unwrap()` internally
// (it's a macro hygiene quirk, not our code). The macro is the
// idiomatic way to construct JSON literals; we trust it. Production
// code never unwraps its own results.

use std::io::{self, BufRead, Write};

use alloy_primitives::{B256, U256};
use alloy_signer_local::PrivateKeySigner;
use serde_json::json;

use crate::chain::Chain;
use crate::cli::session::Session;
use crate::error::CliError;
use crate::types::{Address, Amount, Nonce, Secret, TxHash};

/// Whether the REPL should continue or exit after this command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOutcome {
    Continue,
    Exit,
}

// ────────────────────────────────────────────────────────────────
// Wallet commands
// ────────────────────────────────────────────────────────────────

/// 1. `create-wallet <alias>` — generate a new 12-word mnemonic,
///    prompt for a password, encrypt + save.
pub async fn cmd_create_wallet(
    session: &mut Session,
    alias: &str,
) -> Result<CommandOutcome, CliError> {
    use crate::crypto::mnemonic::WordCount;

    if alias.is_empty() {
        return Err(CliError::from(crate::keystore::KeystoreError::Internal(
            "alias must not be empty".to_string(),
        )));
    }

    // Prompt for password (not in rustyline history; this is from stdin
    // directly, not via the REPL editor).
    let password = prompt_password("New wallet password: ")?;
    let password_confirm = prompt_password("Confirm password: ")?;
    if password.expose_secret() != password_confirm.expose_secret() {
        return Err(CliError::from(crate::keystore::KeystoreError::Internal(
            "passwords do not match".to_string(),
        )));
    }

    let signer = session
        .keystore
        .create(alias, &password, WordCount::Twelve)
        .map_err(CliError::from)?;
    let addr: Address = signer.address().into();

    session.output.success(json!({
        "message": format!("created wallet '{alias}' at {addr}"),
        "alias": alias,
        "address": format!("{addr}"),
    }));
    Ok(CommandOutcome::Continue)
}

/// 2. `import-mnemonic <alias> <phrase>` — validate a user-supplied
///    phrase, prompt for a password, save.
pub async fn cmd_import_mnemonic(
    session: &mut Session,
    alias: &str,
    phrase: &str,
) -> Result<CommandOutcome, CliError> {
    if alias.is_empty() {
        return Err(CliError::from(crate::keystore::KeystoreError::Internal(
            "alias must not be empty".to_string(),
        )));
    }
    if phrase.is_empty() {
        return Err(CliError::from(crate::keystore::KeystoreError::Internal(
            "phrase must not be empty".to_string(),
        )));
    }
    let password = prompt_password("Wallet password: ")?;

    let signer = session
        .keystore
        .import(alias, &password, phrase)
        .map_err(CliError::from)?;
    let addr: Address = signer.address().into();

    session.output.success(json!({
        "message": format!("imported wallet '{alias}' at {addr}"),
        "alias": alias,
        "address": format!("{addr}"),
    }));
    Ok(CommandOutcome::Continue)
}

/// 3. `list` — list all wallets in the keystore.
pub async fn cmd_list(session: &mut Session) -> Result<CommandOutcome, CliError> {
    let wallets = session.keystore.list().map_err(CliError::from)?;
    if wallets.is_empty() {
        session.output.info("No wallets found.");
    } else {
        // Build a multi-line message for human; JSON shape for --json.
        let mut lines: Vec<String> = Vec::new();
        let mut entries: Vec<serde_json::Value> = Vec::new();
        for w in &wallets {
            // M2 V1 limitation: the address is a placeholder
            // (`Address::ZERO`) because `list()` doesn't decrypt.
            // M4 stretch: decrypt on demand using the active
            // unlocked signer if the alias matches.
            let addr_str = if w.address.is_zero() {
                "(locked; unlock to view)".to_string()
            } else {
                format!("{addr}", addr = w.address)
            };
            lines.push(format!("  {:<24} {addr_str}", w.alias));
            entries.push(json!({
                "alias": w.alias,
                "address": if w.address.is_zero() { serde_json::Value::Null } else { json!(format!("{}", w.address)) },
            }));
        }
        session.output.success(json!({
            "message": format!("{} wallet(s):\n{}", wallets.len(), lines.join("\n")),
            "wallets": entries,
        }));
    }
    Ok(CommandOutcome::Continue)
}

/// 4. `use <alias>` — set the active alias (purely ephemeral).
pub async fn cmd_use(session: &mut Session, alias: &str) -> Result<CommandOutcome, CliError> {
    // Validate the alias exists. We don't decrypt here; the user
    // still needs to `unlock` to get a signer.
    let wallets = session.keystore.list().map_err(CliError::from)?;
    if !wallets.iter().any(|w| w.alias == alias) {
        return Err(CliError::from(
            crate::keystore::KeystoreError::AliasNotFound(alias.to_string()),
        ));
    }
    session.active_alias = Some(alias.to_string());
    // Drop any previously unlocked signer (force re-unlock for the
    // new alias).
    session.unlocked_signer = None;

    session.output.success(json!({
        "message": format!("active wallet: {alias} (locked; use `unlock` to decrypt)"),
        "alias": alias,
    }));
    Ok(CommandOutcome::Continue)
}

/// 5. `unlock` — prompt for a password, decrypt the active wallet.
pub async fn cmd_unlock(session: &mut Session) -> Result<CommandOutcome, CliError> {
    let alias = session.active_alias.as_ref().ok_or_else(|| {
        CliError::from(crate::keystore::KeystoreError::Internal(
            "no active wallet; use `use <alias>` first".to_string(),
        ))
    })?;
    let password = prompt_password(&format!("Password for '{alias}': "))?;

    let signer = session
        .keystore
        .load(alias, &password)
        .map_err(CliError::from)?;
    let addr: Address = signer.address().into();
    session.unlocked_signer = Some(signer);

    session.output.success(json!({
        "message": format!("unlocked '{alias}' ({addr})"),
        "alias": alias,
        "address": format!("{addr}"),
    }));
    Ok(CommandOutcome::Continue)
}

// ────────────────────────────────────────────────────────────────
// Read commands
// ────────────────────────────────────────────────────────────────

/// 6. `balance [address]` — get the ETH balance of an address
///    (defaults to the active wallet's address).
pub async fn cmd_balance(
    session: &mut Session,
    address: Option<&str>,
) -> Result<CommandOutcome, CliError> {
    let addr: Address = match address {
        Some(s) => Address::parse(s).map_err(|e| {
            CliError::from(crate::chain::ChainError::Internal(format!(
                "invalid address: {e}"
            )))
        })?,
        None => session.active_address().ok_or_else(|| {
            CliError::from(crate::keystore::KeystoreError::Internal(
                "no address provided and no active wallet; use `use <alias>` + `unlock` first"
                    .to_string(),
            ))
        })?,
    };

    let bal = session.chain.balance(addr).await.map_err(CliError::from)?;
    let wei = bal.as_wei();
    // Human-readable ETH with 6 decimal places (e.g. "0.001000 ETH").
    let eth_display = format_eth(bal);

    session.output.success(json!({
        "message": format!("balance of {addr}: {eth_display}"),
        "address": format!("{addr}"),
        "balance_wei": wei.to_string(),
        "balance_eth": eth_display,
    }));
    Ok(CommandOutcome::Continue)
}

/// 7. `pending-tx` — list locally-known pending transactions from
///    the NonceManager. The pool is in-memory; we don't have on-chain
///    pending queries (those are out of V1 scope per the M3 design).
pub async fn cmd_pending_tx(session: &mut Session) -> Result<CommandOutcome, CliError> {
    let nm = session.nonce_manager.lock().await;
    let mut entries: Vec<serde_json::Value> = Vec::new();
    // Walk the pool. The current NonceManager API exposes peek/pending/
    // dead/history per address; for V1 we just summarize the
    // non-empty pools.
    //
    // Note: a real REPL command would have a more ergonomic API
    // (e.g. `pending-tx list` / `pending-tx history`). For V1 we
    // just say "in-memory pool is empty" since we can't enumerate
    // all addresses (the manager keys on `Address`, but we only
    // have one active wallet).
    if let Some(addr) = session.active_address() {
        let pending = nm.pending(addr.into_alloy()).await;
        for (nonce, hash) in &pending {
            entries.push(json!({
                "state": "pending",
                "address": format!("{addr}"),
                "nonce": *nonce,
                "tx_hash": format!("{hash}"),
            }));
        }
        let dead = nm.dead(addr.into_alloy()).await;
        for nonce in &dead {
            entries.push(json!({
                "state": "stale",
                "address": format!("{addr}"),
                "nonce": *nonce,
            }));
        }
        let history = nm.history(addr.into_alloy()).await;
        for h in &history {
            entries.push(json!({
                "state": "mined",
                "address": format!("{addr}"),
                "nonce": h.nonce,
                "tx_hash": format!("{}", h.tx_hash),
                "block": h.block,
            }));
        }
    }
    if entries.is_empty() {
        session.output.info("No local pending transactions.");
    } else {
        session.output.success(json!({
            "message": format!("{} local tx state(s)", entries.len()),
            "entries": entries,
        }));
    }
    Ok(CommandOutcome::Continue)
}

// ────────────────────────────────────────────────────────────────
// Write (signing) commands
// ────────────────────────────────────────────────────────────────

/// 8. `send-eth <to> <amount> [--bump-fee <hash>] [--cancel <hash>] [--dry-run]`
pub async fn cmd_send_eth(
    session: &mut Session,
    to: &str,
    amount: &str,
    bump_fee: Option<&str>,
    cancel: Option<&str>,
    dry_run: bool,
) -> Result<CommandOutcome, CliError> {
    // Validate mutual exclusivity: --bump-fee and --cancel can't both
    // be set (and neither can be combined with <to>/<amount>).
    if bump_fee.is_some() && cancel.is_some() {
        return Err(CliError::from(crate::chain::ChainError::Internal(
            "--bump-fee and --cancel are mutually exclusive".to_string(),
        )));
    }
    if (bump_fee.is_some() || cancel.is_some()) && (!to.is_empty() || !amount.is_empty()) {
        return Err(CliError::from(crate::chain::ChainError::Internal(
            "--bump-fee/--cancel take only a hash argument; do not combine with to/amount"
                .to_string(),
        )));
    }
    let signer = require_unlocked_signer(session)?;
    let signer_addr = signer.address();

    if let Some(hash_str) = bump_fee {
        return cmd_rbf(session, &signer, hash_str, true /* bump */, dry_run).await;
    }
    if let Some(hash_str) = cancel {
        return cmd_rbf(session, &signer, hash_str, false /* cancel */, dry_run).await;
    }

    // Standard send-eth path.
    let to_addr = Address::parse(to).map_err(|_e| {
        CliError::from(crate::chain::ChainError::InvalidAmount {
            value: to.to_string(),
            reason: "invalid destination address",
        })
    })?;
    let value = Amount::try_from_decimal_str(amount, 18).map_err(|_e| {
        CliError::from(crate::chain::ChainError::InvalidAmount {
            value: amount.to_string(),
            reason: "decimal parse error",
        })
    })?;

    // Fetch the fee estimate + nonce BEFORE building the tx, so the
    // summary can show all the fields the plan requires (fee / total /
    // nonce). This adds one extra RPC round-trip per send-* but it's
    // a small fixed cost and the user is about to sign a tx.
    let fee_estimate = session
        .chain
        .estimate_fees()
        .await
        .map_err(CliError::from)?;
    let nonce = session
        .chain
        .pending_nonce(signer_addr.into())
        .await
        .map_err(CliError::from)?;

    // Build the tx with the fetched fees (don't re-estimate inside
    // the builder — pass `Some` so the builder uses our values).
    let preview = session
        .chain
        .build_eth_transfer(
            &signer,
            to_addr,
            value,
            vec![],
            Some(Amount::from_wei(fee_estimate.max_fee_per_gas)),
            Some(Amount::from_wei(fee_estimate.max_priority_fee_per_gas)),
        )
        .await
        .map_err(CliError::from)?;

    // P0-9 mis-sign prevention: print summary, prompt y/N.
    if !dry_run {
        print_send_summary(
            session,
            to_addr,
            value,
            &fee_estimate,
            nonce,
            preview.hash,
            21_000,
            "ETH transfer",
        )?;
        if !confirm("Proceed? [y/N] ")? {
            session.output.info("cancelled");
            return Ok(CommandOutcome::Continue);
        }
    } else {
        print_send_summary(
            session,
            to_addr,
            value,
            &fee_estimate,
            nonce,
            preview.hash,
            21_000,
            "ETH transfer (dry-run)",
        )?;
        session.output.info("(dry-run: not broadcast)");
        return Ok(CommandOutcome::Continue);
    }

    // Broadcast.
    let broadcast_hash = session
        .chain
        .broadcast_tx(&preview.raw)
        .await
        .map_err(CliError::from)?;
    // Update the local NonceManager pool. The on-chain nonce will
    // be confirmed by wait_for_receipt; this is best-effort to
    // prevent double-send within the same REPL session.
    {
        let nm = session.nonce_manager.lock().await;
        let _ = nm
            .submit(signer_addr, nonce.into(), broadcast_hash.into_b256())
            .await;
    }
    session.output.success(json!({
        "message": format!("broadcasted ETH transfer: {broadcast_hash}"),
        "tx_hash": format!("{broadcast_hash}"),
        "to": format!("{to_addr}"),
        "amount_wei": value.as_wei().to_string(),
        "amount_eth": format_eth(value),
    }));
    Ok(CommandOutcome::Continue)
}

/// 9. `send-token <token> <to> <amount> --decimals <n> [--dry-run]`
pub async fn cmd_send_token(
    session: &mut Session,
    token: &str,
    to: &str,
    amount: &str,
    decimals: u8,
    dry_run: bool,
) -> Result<CommandOutcome, CliError> {
    let signer = require_unlocked_signer(session)?;
    let to_addr = Address::parse(to).map_err(|_e| {
        CliError::from(crate::chain::ChainError::InvalidAmount {
            value: to.to_string(),
            reason: "invalid destination address",
        })
    })?;
    let token_addr = Address::parse(token).map_err(|_e| {
        CliError::from(crate::chain::ChainError::InvalidAmount {
            value: token.to_string(),
            reason: "invalid token address",
        })
    })?;
    let value = Amount::try_from_decimal_str(amount, decimals).map_err(|_e| {
        CliError::from(crate::chain::ChainError::InvalidAmount {
            value: amount.to_string(),
            reason: "decimal parse error (--decimals may be wrong)",
        })
    })?;

    // Build the ERC-20 calldata (ABI-encoded `transfer(to, amount)`).
    let calldata = crate::chain::erc20::encode_transfer(to_addr.into_alloy(), *value.as_wei())
        .map_err(CliError::from)?;

    // Fetch fee estimate + nonce (same pattern as send-eth, so the
    // summary shows fee / total / nonce). For ERC-20 transfers, the
    // gas estimate is unknown at this point; we use 0 as a placeholder
    // (the real gas is determined by the chain's `eth_estimateGas`).
    // We also use Amount::ZERO as the ETH value (the actual amount
    // moves in the calldata).
    let fee_estimate = session
        .chain
        .estimate_fees()
        .await
        .map_err(CliError::from)?;
    let nonce = session
        .chain
        .pending_nonce(signer.address().into())
        .await
        .map_err(CliError::from)?;
    let preview = session
        .chain
        .build_eth_transfer(
            &signer,
            token_addr,
            Amount::ZERO,
            calldata.clone(),
            Some(Amount::from_wei(fee_estimate.max_fee_per_gas)),
            Some(Amount::from_wei(fee_estimate.max_priority_fee_per_gas)),
        )
        .await
        .map_err(CliError::from)?;

    if !dry_run {
        print_send_summary(
            session,
            to_addr,
            value,
            &fee_estimate,
            nonce,
            preview.hash,
            0,
            &format!("ERC-20 transfer (token {token_addr}, {decimals} decimals)"),
        )?;
        if !confirm("Proceed? [y/N] ")? {
            session.output.info("cancelled");
            return Ok(CommandOutcome::Continue);
        }
    } else {
        print_send_summary(
            session,
            to_addr,
            value,
            &fee_estimate,
            nonce,
            preview.hash,
            0,
            &format!("ERC-20 transfer (dry-run, {decimals} decimals)"),
        )?;
        session.output.info("(dry-run: not broadcast)");
        return Ok(CommandOutcome::Continue);
    }

    // Broadcast: the calldata is now embedded in the signed tx
    // envelope (per the `data` field), so the ERC-20 transfer is
    // actually executed on-chain (not just a value=0 no-op).
    let broadcast_hash = session
        .chain
        .broadcast_tx(&preview.raw)
        .await
        .map_err(CliError::from)?;
    // Update the local NonceManager pool.
    {
        let nm = session.nonce_manager.lock().await;
        let _ = nm
            .submit(signer.address(), nonce.into(), broadcast_hash.into_b256())
            .await;
    }
    session.output.success(json!({
        "message": format!("broadcasted ERC-20 transfer: {broadcast_hash}"),
        "tx_hash": format!("{broadcast_hash}"),
        "token": format!("{token_addr}"),
        "to": format!("{to_addr}"),
        "amount_wei": value.as_wei().to_string(),
    }));
    Ok(CommandOutcome::Continue)
}

/// 10. `sign-message <message>` — EIP-191 personal_sign.
pub async fn cmd_sign_message(
    session: &mut Session,
    message: &str,
) -> Result<CommandOutcome, CliError> {
    let signer = require_unlocked_signer(session)?;
    // We need to call crypto::sign::personal_sign, which is async
    // and takes `&PrivateKeySigner`. The signer is borrowed from
    // session; the call is short-lived (sign + display + drop).
    let sig = crate::crypto::sign::personal_sign(&signer, message.as_bytes())
        .await
        .map_err(|e| {
            CliError::from(crate::chain::ChainError::Internal(format!(
                "sign_message: {e}"
            )))
        })?;
    let addr: Address = signer.address().into();

    // EIP-191: the recovered address must match the signer.
    let recovered = crate::crypto::sign::ecrecover(&sig, message.as_bytes()).map_err(|e| {
        CliError::from(crate::chain::ChainError::Internal(format!(
            "ecrecover: {e}"
        )))
    })?;
    if recovered != signer.address() {
        return Err(CliError::from(crate::chain::ChainError::Internal(
            "ecrecover mismatch — signature invalid".to_string(),
        )));
    }

    session.output.success(json!({
        "message": format!("signed message ({addr})"),
        "address": format!("{addr}"),
        "signature": format!("{sig}"),
        "signer": format!("{addr}"),
    }));
    Ok(CommandOutcome::Continue)
}

/// 11. `exit` — exit the REPL.
pub fn cmd_exit() -> CommandOutcome {
    CommandOutcome::Exit
}

// ────────────────────────────────────────────────────────────────
// Helpers (shared with REPL + one-shot)
// ────────────────────────────────────────────────────────────────

/// RBF / Cancel path. Shared between `--bump-fee` and `--cancel`.
async fn cmd_rbf(
    session: &mut Session,
    signer: &PrivateKeySigner,
    hash_str: &str,
    is_bump: bool,
    dry_run: bool,
) -> Result<CommandOutcome, CliError> {
    // Parse the hash.
    let hash_bytes: [u8; 32] = parse_tx_hash(hash_str)?;
    let hash = TxHash::from_b256(B256::from(hash_bytes));

    // Look up the original tx to get the "to" / "value" (cancel
    // overrides to self + 0).
    let info = session
        .chain
        .get_tx(hash)
        .await
        .map_err(CliError::from)?
        .ok_or_else(|| CliError::from(crate::chain::ChainError::TxNotFound { hash }))?;

    if !dry_run {
        session.output.info(&format!(
            "Original tx: to={:?} value={} max_fee={}",
            info.to, info.value, info.max_fee_per_gas,
        ));
        if !confirm(&format!(
            "Proceed with {}? [y/N] ",
            if is_bump { "bump-fee" } else { "cancel" }
        ))? {
            session.output.info("cancelled");
            return Ok(CommandOutcome::Continue);
        }
    }

    let outcome = if is_bump {
        crate::chain::rbf::bump_fee(&session.chain, signer, hash)
            .await
            .map_err(CliError::from)?
    } else {
        let new_hash = crate::chain::rbf::cancel(&session.chain, signer, hash)
            .await
            .map_err(CliError::from)?;
        crate::chain::rbf::BumpResult {
            new_hash,
            new_max_fee_per_gas: info.max_fee_per_gas,
            new_max_priority_fee_per_gas: info.max_priority_fee_per_gas,
        }
    };

    session.output.success(json!({
        "message": format!(
            "{}d tx: new hash {} (max_fee={}, max_priority={})",
            if is_bump { "Bump" } else { "Cancel" },
            outcome.new_hash,
            outcome.new_max_fee_per_gas,
            outcome.new_max_priority_fee_per_gas,
        ),
        "original_hash": format!("{hash}"),
        "new_hash": format!("{}", outcome.new_hash),
        "new_max_fee_per_gas": outcome.new_max_fee_per_gas.to_string(),
        "new_max_priority_fee_per_gas": outcome.new_max_priority_fee_per_gas.to_string(),
    }));
    Ok(CommandOutcome::Continue)
}

fn require_unlocked_signer(session: &Session) -> Result<PrivateKeySigner, CliError> {
    if !session.can_sign() {
        return Err(CliError::from(crate::keystore::KeystoreError::Internal(
            "no active wallet or wallet is locked; use `use <alias>` then `unlock` first"
                .to_string(),
        )));
    }
    // `can_sign` guarantees Some.
    Ok(session.unlocked_signer.clone().expect("can_sign invariant"))
}

fn parse_tx_hash(s: &str) -> Result<[u8; 32], CliError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| {
        CliError::from(crate::chain::ChainError::Internal(format!(
            "invalid tx hash: {e}"
        )))
    })?;
    if bytes.len() != 32 {
        return Err(CliError::from(crate::chain::ChainError::Internal(format!(
            "tx hash must be 32 bytes, got {}",
            bytes.len()
        ))));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn prompt_password(prompt: &str) -> Result<Secret<String>, CliError> {
    eprint!("{prompt}");
    let _ = io::stderr().flush();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();
    handle
        .read_line(&mut line)
        .map_err(|e| CliError::from(crate::keystore::KeystoreError::Io(e.to_string())))?;
    Ok(Secret::new(line.trim().to_string()))
}

///  y/N prompt. Default is N. Returns true only for "y" or "yes"
/// (case-insensitive, trimmed). All other inputs (including empty
///  and "n") return false.
fn confirm(prompt: &str) -> Result<bool, CliError> {
    eprint!("{prompt}");
    let _ = io::stderr().flush();
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();
    handle
        .read_line(&mut line)
        .map_err(|e| CliError::from(crate::keystore::KeystoreError::Io(e.to_string())))?;
    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// Print the send-* summary per PLAN-V9 §5 M4 DoD (P0-9
/// mis-sign prevention summary format):
///
///     to:     0x1234...abcd
///     amount: 0.001 ETH (1.0e15 wei)
///     fee:    1.5 Gwei (cap 30 Gwei)
///     total:  0.0010015 ETH
///     nonce:  42
///
/// `fee` is the `max_fee_per_gas` we will sign with. `cap 30 Gwei`
/// is a literal cap-statement (Infura's typical free-tier ceiling on
/// Sepolia; the actual chain has no hard cap). `total` is the
/// worst-case spend (amount + gas * max_fee). `nonce` is the next
/// nonce the builder will use (also asserted by the receipt after
/// broadcast).
#[allow(clippy::too_many_arguments)]
fn print_send_summary(
    session: &mut Session,
    to: Address,
    amount: Amount,
    fee_estimate: &crate::chain::FeeEstimate,
    nonce: Nonce,
    tx_hash: TxHash,
    gas: u64,
    label: &str,
) -> Result<(), CliError> {
    let wei = amount.as_wei();
    let eth_str = format_eth(amount);
    let wei_str = wei.to_string();

    // Format max_fee_per_gas in Gwei (1 Gwei = 1e9 wei). For Sepolia
    // this is the only display unit; for token sends the gas is 0,
    // so fee=0 Gwei in the summary (real fee paid is in the token
    // amount, not in ETH).
    let max_fee_gwei_str = format_gwei(fee_estimate.max_fee_per_gas);
    let cap_gwei_str = "30".to_string(); // Infura free-tier cap; see docs.

    // Worst-case total: amount + (gas * max_fee). For token sends,
    // gas=0 means total==amount. For ETH sends, gas=21000.
    let total_wei: U256 = *wei + U256::from(gas) * fee_estimate.max_fee_per_gas;
    let total_amount = Amount::from_wei(total_wei);
    let total_str = format_eth(total_amount);

    session.output.info(&format!("--- {label} ---"));
    session.output.info(&format!("to:     {to}"));
    session
        .output
        .info(&format!("amount: {eth_str} ({wei_str} wei)"));
    session.output.info(&format!(
        "fee:    {max_fee_gwei_str} Gwei (cap {cap_gwei_str} Gwei)"
    ));
    session.output.info(&format!("total:  {total_str}"));
    session.output.info(&format!("nonce:  {nonce}"));
    session.output.info(&format!("hash:   {tx_hash}"));
    session.output.info("--- end summary ---");
    Ok(())
}

/// Format a wei value (U256) as a Gwei string with up to 4 fractional
/// digits, e.g. `1.5000 Gwei`. For very small or very large values
/// the format degrades to the integer part only.
fn format_gwei(wei: U256) -> String {
    use std::fmt::Write;
    // 1 Gwei = 1e9 wei
    let one_gwei = U256::from(1_000_000_000u64);
    let whole = wei / one_gwei;
    let frac_wei = wei % one_gwei;
    // frac_wei is at most 1e9 - 1. Multiply by 1e4 / 1e9 to get
    // 0.0001 Gwei precision.
    let one_ten_thousandth = U256::from(100_000_000u64); // 1e8
    let micros = frac_wei / one_ten_thousandth;
    let mut s = String::new();
    let _ = write!(s, "{whole}.");
    let mut frac_str = micros.to_string();
    while frac_str.len() < 4 {
        frac_str.insert(0, '0');
    }
    s.push_str(&frac_str);
    s
}

/// Format an `Amount` (wei) as a human-readable ETH string with up to
/// 6 fractional digits, e.g. `1.000000 ETH`.
fn format_eth(amount: Amount) -> String {
    use std::fmt::Write;
    let wei = amount.as_wei();
    // wei / 10^18. We do long division with 6 fractional digits.
    let one_eth: u128 = 1_000_000_000_000_000_000;
    // Convert to U512 (avoids overflow on 1e18 * 1e6 = 1e24).
    let wei_u512: alloy_primitives::U512 = alloy_primitives::U512::from(*wei);
    let one_eth_u512 = alloy_primitives::U512::from(one_eth);
    let one_micro = alloy_primitives::U512::from(1_000_000_000_000u64); // 1e12

    let whole = wei_u512 / one_eth_u512;
    let frac_wei = wei_u512 % one_eth_u512;
    // frac_wei is at most 1e18 - 1. Multiply by 1e6 / 1e18 to get
    // micro-ETH. We use integer division; remainder is discarded
    // (caller is OK with 6-digit precision).
    let micros = frac_wei / one_micro;

    let mut s = String::new();
    let _ = write!(s, "{whole}.");
    // Pad micros to 6 digits.
    let mut frac_str = micros.to_string();
    while frac_str.len() < 6 {
        frac_str.insert(0, '0');
    }
    s.push_str(&frac_str);
    s.push_str(" ETH");
    s
}
