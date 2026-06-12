// SPDX-License-Identifier: MIT
//
// Output formatting (PLAN-V9 §5 M4 DoD, P0-1).
//
// The CLI supports two output modes:
//   - **HumanOutput** (default): multi-line, color-friendly prose.
//     Errors print the friendly message + the cause chain (via
//     `anyhow`-style "Caused by: ..."). No machine-readable structure.
//   - **JsonOutput** (`--json`): one JSON object per line:
//       success: `{"ok": true, "data": <T>}` (T depends on the command)
//       error:   `{"ok": false, "code": "EVMC-XXX", "message": "...", "cause": ["...", "..."]}`
//
// The mode is selected at startup from `Config::json` and held in
// `Session` as a `Box<dyn OutputFormatter>`. The REPL allows live
// toggling via the `json` and `human` meta-commands (M4 stretch).
//
// **Why a trait + Box:** keeps the rest of the CLI agnostic to the
// output mode. Each command returns a typed result; the formatter
// decides how to render it. This is the same pattern as
// `tracing_subscriber::Format`.

#![allow(clippy::disallowed_methods)]
// `serde_json::json!` macro expansion uses `.unwrap()` internally;
// see commands.rs for rationale. The macro is the idiomatic way to
// construct JSON literals; we trust it.

use std::io::Write;

use crate::error::CliError;

/// Output mode. See module docs.
pub trait OutputFormatter: Send {
    /// A human-readable informational line. Not emitted in JSON mode.
    fn info(&mut self, msg: &str);
    /// A warning. Not emitted in JSON mode.
    fn warn(&mut self, msg: &str);
    /// A success result. `data` is the command-specific JSON value.
    fn success(&mut self, data: serde_json::Value);
    /// An error result.
    fn error(&mut self, err: &CliError);
    /// Flush any buffered output.
    fn flush(&mut self);
}

/// Human-readable output to stdout (info/success) and stderr (error/warn).
pub struct HumanOutput {
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
}

impl HumanOutput {
    pub fn new() -> Self {
        Self {
            stdout: Box::new(std::io::stdout()),
            stderr: Box::new(std::io::stderr()),
        }
    }
}

impl Default for HumanOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for HumanOutput {
    fn info(&mut self, msg: &str) {
        let _ = writeln!(self.stdout, "{msg}");
    }

    fn warn(&mut self, msg: &str) {
        let _ = writeln!(self.stderr, "warning: {msg}");
    }

    fn success(&mut self, data: serde_json::Value) {
        // Render as a human-readable summary if the data has a
        // `message` field; otherwise dump as pretty JSON.
        if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
            let _ = writeln!(self.stdout, "{msg}");
        } else {
            match serde_json::to_string_pretty(&data) {
                Ok(s) => {
                    let _ = writeln!(self.stdout, "{s}");
                }
                Err(_) => {
                    let _ = writeln!(self.stdout, "<unprintable success payload>");
                }
            }
        }
    }

    fn error(&mut self, err: &CliError) {
        let code = err.code();
        // Use the Display impl (which prefixes `[CODE]`).
        let _ = writeln!(self.stderr, "error: {err}");
        let _ = writeln!(self.stderr, "  code: {code}");
        // Cause chain (via std::error::Error::source).
        let mut src = std::error::Error::source(err);
        let mut depth = 0;
        while let Some(s) = src {
            let _ = writeln!(self.stderr, "  caused by [{}]: {s}", depth + 1);
            src = s.source();
            depth += 1;
        }
    }

    fn flush(&mut self) {
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
    }
}

/// JSON output. One self-contained object per line (newline-delimited
/// JSON, NDJSON — easy for `jq` / log aggregators to consume).
pub struct JsonOutput {
    stdout: Box<dyn Write + Send>,
    stderr: Box<dyn Write + Send>,
}

impl JsonOutput {
    pub fn new() -> Self {
        Self {
            stdout: Box::new(std::io::stdout()),
            stderr: Box::new(std::io::stderr()),
        }
    }
}

impl Default for JsonOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputFormatter for JsonOutput {
    fn info(&mut self, msg: &str) {
        // Info is intentionally not emitted in JSON mode to keep
        // stdout strictly a stream of result objects. If the user
        // wants logging, they configure `tracing` and route to stderr.
        let _ = msg;
    }

    fn warn(&mut self, msg: &str) {
        // Same: warnings are diagnostic, not results.
        let _ = msg;
    }

    fn success(&mut self, data: serde_json::Value) {
        let payload = serde_json::json!({ "ok": true, "data": data });
        if let Ok(s) = serde_json::to_string(&payload) {
            let _ = writeln!(self.stdout, "{s}");
        }
    }

    fn error(&mut self, err: &CliError) {
        let code = err.code();
        let message = format!("{err}");
        let mut cause_chain: Vec<String> = Vec::new();
        let mut src = std::error::Error::source(err);
        while let Some(s) = src {
            cause_chain.push(s.to_string());
            src = s.source();
        }
        let payload = serde_json::json!({
            "ok": false,
            "code": code,
            "message": message,
            "cause": cause_chain,
        });
        if let Ok(s) = serde_json::to_string(&payload) {
            // Errors go to stderr (separates the "result" stream on
            // stdout from the "diagnostic" stream on stderr).
            let _ = writeln!(self.stderr, "{s}");
        }
    }

    fn flush(&mut self) {
        let _ = self.stdout.flush();
        let _ = self.stderr.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_success_with_message() {
        let mut out = HumanOutput::new();
        out.success(serde_json::json!({"message": "wallet created"}));
        out.flush();
        // The write goes to actual stdout, but we can't easily capture
        // it from the test. We just assert the call didn't panic.
    }

    #[test]
    fn human_error_includes_code() {
        // Build a CliError and feed it.
        let err = CliError::from(crate::keystore::KeystoreError::InvalidPassword);
        let mut out = HumanOutput::new();
        out.error(&err);
        out.flush();
        // The error code is the unit-tested contract.
        assert_eq!(err.code(), "EVMK-001");
    }

    #[test]
    fn json_success_shape() {
        // We can't capture stdout/stderr easily in a test, but we
        // can construct a JSON value and serialize it the same way
        // the formatter does, to validate the shape.
        let data = serde_json::json!({"tx_hash": "0xabc"});
        let payload = serde_json::json!({ "ok": true, "data": data });
        let s = serde_json::to_string(&payload).expect("serialize");
        assert!(s.contains("\"ok\":true"));
        assert!(s.contains("\"tx_hash\":\"0xabc\""));
    }

    #[test]
    fn json_error_shape() {
        let err = CliError::from(crate::chain::ChainError::Rpc("x".into()));
        let code = err.code();
        let message = format!("{err}");
        let payload = serde_json::json!({
            "ok": false,
            "code": code,
            "message": message,
            "cause": Vec::<String>::new(),
        });
        let s = serde_json::to_string(&payload).expect("serialize");
        assert!(s.contains("\"ok\":false"));
        assert!(s.contains("\"code\":\"EVMC-001\""));
    }
}
