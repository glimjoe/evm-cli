// SPDX-License-Identifier: MIT
//
// `evm_cli::cli` — the M4 CLI layer (PLAN-V9 §5 M4 DoD).
//
// Composes:
//   - `config` (12-factor layered config)
//   - `output` (Human / JSON formatters)
//   - `history` (should_skip_history predicate)
//   - `session` (runtime state: chain + keystore + active wallet)
//   - `commands` (11 command implementations)
//
// Entry point: `run()` parses `Cli` (clap derive), resolves a
// `Config` (CLI > env > file > default), builds a `Session`, and
// either runs a single command (one-shot) or enters the REPL.

#![cfg_attr(test, allow(clippy::disallowed_methods))]

use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cli::commands::CommandOutcome;
use crate::cli::config::{Config, PartialConfig};
use crate::cli::output::{HumanOutput, JsonOutput, OutputFormatter};
use crate::cli::session::Session;
use crate::error::CliError;

pub mod commands;
pub mod config;
pub mod history;
pub mod output;
pub mod pure;
pub mod session;

#[derive(Debug, Parser)]
#[command(
    name = "evm-cli",
    version,
    about = "Linux-only CLI wallet for Sepolia testnet",
    long_about = "evm-cli is a single-binary command-line EVM wallet targeting the Sepolia testnet. \
                  It supports BIP-39/BIP-44 HD wallets, EIP-1559 transactions, ERC-20 transfers, \
                  RBF/cancel, and standard CLI ergonomics (clap + rustyline REPL).\n\n\
                  Run with no subcommand to enter the interactive REPL.",
    after_help = "Run `evm-cli` with no subcommand to enter the interactive REPL.\n\
                  Environment variables: EVMCLI_RPC_URL, EVMCLI_KEYSTORE_DIR, EVMCLI_DATA_DIR, \
                  EVMCLI_JSON, EVMCLI_NO_HISTORY, EVMCLI_CHAIN_ID."
)]
pub struct Cli {
    /// Emit machine-readable JSON output (NDJSON: one object per line).
    /// Can also be set via EVMCLI_JSON (true|false|1|0|yes|no, parsed
    /// by `Config::load` — see `cli/config.rs`).
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable rustyline history (don't write/read the history file).
    /// Can also be set via EVMCLI_NO_HISTORY (parsed by `Config::load`).
    #[arg(long, global = true)]
    pub no_history: bool,

    /// HTTP(S) RPC endpoint. Default: public Sepolia endpoint.
    /// Can also be set via EVMCLI_RPC_URL.
    #[arg(long, global = true, env = "EVMCLI_RPC_URL")]
    pub rpc_url: Option<String>,

    /// Directory holding `<alias>` keystore files.
    /// Can also be set via EVMCLI_KEYSTORE_DIR.
    #[arg(long, global = true, env = "EVMCLI_KEYSTORE_DIR")]
    pub keystore_dir: Option<PathBuf>,

    /// Top-level data dir. Defaults to the parent of `keystore_dir`
    /// (or `~/.local/share/evm-cli`).
    /// Can also be set via EVMCLI_DATA_DIR.
    #[arg(long, global = true, env = "EVMCLI_DATA_DIR")]
    pub data_dir: Option<PathBuf>,

    /// The subcommand to run. If `None`, the REPL is entered.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Convert this CLI invocation into a `PartialConfig` for the
    /// 12-factor cascade.
    pub fn partial_config(&self) -> PartialConfig {
        PartialConfig {
            rpc_url: self.rpc_url.clone(),
            keystore_dir: self.keystore_dir.clone(),
            data_dir: self.data_dir.clone(),
            json: self.json,
            no_history: self.no_history,
            expected_chain_id: None,
            rpc_timeout_secs: None,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate a new 12-word mnemonic wallet.
    CreateWallet {
        /// Alias (filename) to save the wallet under.
        #[arg(value_name = "ALIAS")]
        alias: String,
    },
    /// Import an existing mnemonic.
    ImportMnemonic {
        /// Alias (filename) to save the wallet under.
        #[arg(value_name = "ALIAS")]
        alias: String,
        /// Mnemonic phrase (12 or 24 words).
        #[arg(value_name = "PHRASE")]
        phrase: String,
    },
    /// List all wallets in the keystore.
    List,
    /// Set the active wallet.
    Use {
        #[arg(value_name = "ALIAS")]
        alias: String,
    },
    /// Decrypt the active wallet (prompt for password).
    Unlock,
    /// Show the ETH balance of an address (defaults to the active wallet).
    Balance {
        /// Address to query. If omitted, queries the active wallet.
        #[arg(value_name = "ADDRESS", default_value = "")]
        address: String,
    },
    /// Send ETH (with optional --bump-fee, --cancel, --dry-run).
    SendEth {
        /// Destination address.
        #[arg(value_name = "TO", default_value = "")]
        to: String,
        /// Amount in ETH (e.g. "0.001"). Ignored with --bump-fee/--cancel.
        #[arg(value_name = "AMOUNT", default_value = "")]
        amount: String,
        /// RBF: replace an existing pending tx with bumped fees.
        #[arg(long, value_name = "TX_HASH")]
        bump_fee: Option<String>,
        /// Cancel: send a 0-value self-send with bumped fees.
        #[arg(long, value_name = "TX_HASH")]
        cancel: Option<String>,
        /// Print the summary only; do not sign or broadcast.
        #[arg(long)]
        dry_run: bool,
    },
    /// Send an ERC-20 token transfer.
    SendToken {
        /// Token contract address.
        #[arg(value_name = "TOKEN")]
        token: String,
        /// Destination address.
        #[arg(value_name = "TO")]
        to: String,
        /// Amount in token units (e.g. "1.5").
        #[arg(value_name = "AMOUNT")]
        amount: String,
        /// Token decimal places (e.g. 6 for USDC, 18 for DAI).
        #[arg(long, value_name = "DECIMALS")]
        decimals: u8,
        /// Print the summary only; do not sign or broadcast.
        #[arg(long)]
        dry_run: bool,
    },
    /// EIP-191 personal_sign the given message with the active wallet.
    SignMessage {
        #[arg(value_name = "MESSAGE")]
        message: String,
    },
    /// List locally-known pending transactions (from NonceManager).
    PendingTx,
    /// Exit the REPL.
    Exit,
}

/// Top-level entry: parse CLI, build session, dispatch.
pub async fn run() -> ExitCode {
    let cli = Cli::parse();
    // 12-factor config: CLI > env > file > default.
    let config = match Config::load(cli.partial_config()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config error: {e}");
            return ExitCode::from(2);
        }
    };

    // Build the output formatter using the resolved config (which
    // has the env-var-applied `json` flag, not just the CLI flag).
    let output: Box<dyn OutputFormatter> = if config.json {
        Box::new(JsonOutput::new())
    } else {
        Box::new(HumanOutput::new())
    };

    // PoC warning (always printed to stderr, even in JSON mode —
    // this is a safety rail, not a result).
    if !config.json {
        eprintln!(
            "evm-cli v{} — Sepolia testnet PoC; do NOT use on mainnet with real assets.",
            env!("CARGO_PKG_VERSION")
        );
        eprintln!();
    }

    // Capture for the error path (we move `output` into Session::build
    // below, so we re-derive the json setting from config).
    let output_is_json = config.json;

    // Build the session.
    let mut session = match Session::build(config, output).await {
        Ok(s) => s,
        Err(e) => {
            // Session::build needs a formatter to write errors. We
            // can re-create one here using the same json setting.
            let mut out: Box<dyn OutputFormatter> = if output_is_json {
                Box::new(JsonOutput::new())
            } else {
                Box::new(HumanOutput::new())
            };
            out.error(&e);
            out.flush();
            return ExitCode::from(1);
        }
    };

    // Dispatch.
    match cli.command {
        Some(cmd) => match dispatch_one_shot(&mut session, cmd).await {
            Ok(()) => {
                session.output.flush();
                ExitCode::SUCCESS
            }
            Err(e) => {
                session.output.error(&e);
                session.output.flush();
                ExitCode::from(1)
            }
        },
        None => match run_repl(&mut session).await {
            Ok(()) => {
                session.output.flush();
                ExitCode::SUCCESS
            }
            Err(e) => {
                session.output.error(&e);
                session.output.flush();
                ExitCode::from(1)
            }
        },
    }
}

/// Dispatch a single one-shot command. Returns `Ok(())` on success
/// (including `cmd_exit`, which is just a Continue for one-shot).
pub async fn dispatch_one_shot(session: &mut Session, cmd: Command) -> Result<(), CliError> {
    let outcome = match cmd {
        Command::CreateWallet { alias } => commands::cmd_create_wallet(session, &alias).await?,
        Command::ImportMnemonic { alias, phrase } => {
            commands::cmd_import_mnemonic(session, &alias, &phrase).await?
        }
        Command::List => commands::cmd_list(session).await?,
        Command::Use { alias } => commands::cmd_use(session, &alias).await?,
        Command::Unlock => commands::cmd_unlock(session).await?,
        Command::Balance { address } => {
            let addr_opt: Option<&str> = if address.is_empty() {
                None
            } else {
                Some(&address)
            };
            commands::cmd_balance(session, addr_opt).await?
        }
        Command::SendEth {
            to,
            amount,
            bump_fee,
            cancel,
            dry_run,
        } => {
            commands::cmd_send_eth(
                session,
                &to,
                &amount,
                bump_fee.as_deref(),
                cancel.as_deref(),
                dry_run,
            )
            .await?
        }
        Command::SendToken {
            token,
            to,
            amount,
            decimals,
            dry_run,
        } => commands::cmd_send_token(session, &token, &to, &amount, decimals, dry_run).await?,
        Command::SignMessage { message } => commands::cmd_sign_message(session, &message).await?,
        Command::PendingTx => commands::cmd_pending_tx(session).await?,
        Command::Exit => commands::cmd_exit(),
    };
    if matches!(outcome, CommandOutcome::Exit) {
        // One-shot `exit` is a no-op (it only makes sense in REPL).
    }
    Ok(())
}

/// Run the interactive REPL. Returns when the user types `exit`,
/// EOF, or Ctrl-D.
pub async fn run_repl(session: &mut Session) -> Result<(), CliError> {
    use rustyline::config::{Builder as RlBuilder, ColorMode};
    use rustyline::error::ReadlineError;
    use rustyline::Editor;

    // Build the rustyline editor. We wrap the result in `Option`
    // to gracefully handle non-TTY environments (tests, CI logs).
    // rustyline 18.0.0 changed `Editor` to take `<H: Helper, I: History>`
    // generics; for our use we leave the default `()` helper and a
    // basic file-based history implementation.
    let mut rl: Option<Editor<(), rustyline::history::FileHistory>> = if !session.config.no_history
    {
        let cfg = RlBuilder::new()
            .auto_add_history(false)
            .color_mode(ColorMode::Enabled)
            .build();
        match Editor::with_config(cfg) {
            Ok(mut e) => {
                // Try to load the history. Failure is non-fatal.
                let _ = e.load_history(&history_path());
                Some(e)
            }
            Err(_) => None,
        }
    } else {
        None
    };

    session
        .output
        .info("Welcome to evm-cli. Type 'help' for a list of commands, 'exit' to quit.");
    // Show a help line in human mode.
    if !session.config.json {
        eprintln!("\nCommands:");
        eprintln!("  create-wallet <alias>");
        eprintln!("  import-mnemonic <alias> <phrase>");
        eprintln!("  list");
        eprintln!("  use <alias>");
        eprintln!("  unlock");
        eprintln!("  balance [address]");
        eprintln!("  send-eth <to> <amount> [--bump-fee <hash>] [--cancel <hash>] [--dry-run]");
        eprintln!("  send-token <token> <to> <amount> --decimals <n> [--dry-run]");
        eprintln!("  sign-message <message>");
        eprintln!("  pending-tx");
        eprintln!("  help");
        eprintln!("  exit\n");
    }

    loop {
        let prompt = match &session.active_alias {
            Some(a) => format!("evm-cli [{a}]> "),
            None => "evm-cli> ".to_string(),
        };

        // Read one line. From rustyline in TTY mode, or from stdin
        // when rustyline couldn't initialize.
        let line = match rl.as_mut() {
            Some(editor) => match editor.readline(&prompt) {
                Ok(l) => l,
                Err(ReadlineError::Interrupted) => {
                    // Ctrl-C: print a fresh prompt, continue.
                    eprintln!("(Ctrl-C; type 'exit' to quit)");
                    continue;
                }
                Err(ReadlineError::Eof) => break,
                Err(e) => {
                    return Err(CliError::from(crate::chain::ChainError::Internal(format!(
                        "readline: {e}"
                    ))));
                }
            },
            None => {
                // Non-TTY (e.g. tests piping stdin). Read a line via
                // plain stdin, print the prompt to stdout.
                print!("{prompt}");
                let _ = io::stdout().flush();
                let mut s = String::new();
                match io::stdin().read_line(&mut s) {
                    Ok(0) => break, // EOF
                    Ok(_) => s,
                    Err(e) => {
                        return Err(CliError::from(crate::chain::ChainError::Internal(format!(
                            "stdin read: {e}"
                        ))));
                    }
                }
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "help" {
            // Quick help: re-print the command list.
            session.output.info(
                "Commands: create-wallet <alias>, import-mnemonic <alias> <phrase>, list, \
                 use <alias>, unlock, balance [address], \
                 send-eth <to> <amount> [--bump-fee <hash>] [--cancel <hash>] [--dry-run], \
                 send-token <token> <to> <amount> --decimals <n> [--dry-run], \
                 sign-message <message>, pending-tx, help, exit",
            );
            continue;
        }
        if trimmed == "exit" {
            break;
        }

        // History filter: skip the line if it contains a sensitive
        // token (per PLAN-V9 §5 M4 DoD).
        let skip = history::should_skip_history(trimmed);
        if !skip {
            if let Some(editor) = rl.as_mut() {
                editor.add_history_entry(trimmed).ok();
            }
        }

        // Re-parse the trimmed line as a Cli invocation. We prepend
        // "evm-cli" so clap's argument parser sees the program name.
        let args = std::iter::once("evm-cli").chain(trimmed.split_whitespace());
        let parsed = Cli::try_parse_from(args);
        let inner_cmd = match parsed {
            Ok(c) => c.command,
            Err(e) => {
                // clap renders a friendly help/usage message.
                e.print().ok();
                continue;
            }
        };
        let cmd = match inner_cmd {
            Some(c) => c,
            None => {
                session
                    .output
                    .info("Type a command, or 'help' for the list.");
                continue;
            }
        };
        match dispatch_one_shot(session, cmd).await {
            Ok(()) => {}
            Err(e) => session.output.error(&e),
        }
        session.output.flush();
    }

    // Persist history on exit.
    if let Some(editor) = rl.as_mut() {
        let _ = editor.save_history(&history_path());
    }
    Ok(())
}

/// Path to the rustyline history file. Lives in the data dir so it
/// follows the same writability check as the keystore.
fn history_path() -> PathBuf {
    use crate::cli::config::default_data_dir;
    default_data_dir().join(".evm_cli_history")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_minimal() {
        // No subcommand: `Cli::try_parse_from(["evm-cli"]).unwrap()`
        // should succeed with `command = None`.
        let cli = Cli::try_parse_from(["evm-cli"]).unwrap();
        assert!(cli.command.is_none());
        assert!(!cli.json);
        assert!(!cli.no_history);
    }

    #[test]
    fn cli_parses_list() {
        let cli = Cli::try_parse_from(["evm-cli", "list"]).unwrap();
        assert!(matches!(cli.command, Some(Command::List)));
    }

    #[test]
    fn cli_parses_send_eth_with_flags() {
        let cli =
            Cli::try_parse_from(["evm-cli", "send-eth", "0x1234", "0.001", "--dry-run"]).unwrap();
        match cli.command {
            Some(Command::SendEth {
                to,
                amount,
                dry_run,
                ..
            }) => {
                assert_eq!(to, "0x1234");
                assert_eq!(amount, "0.001");
                assert!(dry_run);
            }
            _ => panic!("expected SendEth"),
        }
    }

    #[test]
    fn cli_parses_send_token() {
        let cli = Cli::try_parse_from([
            "evm-cli",
            "send-token",
            "0xtoken",
            "0xdest",
            "100",
            "--decimals",
            "6",
        ])
        .unwrap();
        match cli.command {
            Some(Command::SendToken {
                token,
                to,
                amount,
                decimals,
                ..
            }) => {
                assert_eq!(token, "0xtoken");
                assert_eq!(to, "0xdest");
                assert_eq!(amount, "100");
                assert_eq!(decimals, 6);
            }
            _ => panic!("expected SendToken"),
        }
    }

    #[test]
    fn cli_parses_json_flag() {
        let cli = Cli::try_parse_from(["evm-cli", "--json", "list"]).unwrap();
        assert!(cli.json);
        assert!(matches!(cli.command, Some(Command::List)));
    }
}
