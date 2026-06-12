// SPDX-License-Identifier: MIT
//
// 12-factor configuration for the CLI (PLAN-V9 §5 M0 DoD + §5 M4 DoD).
//
// Layered loading (highest priority first):
//   1. CLI flags (passed in by `Cli::parse`)
//   2. Environment variables (`EVMCLI_*`)
//   3. Optional TOML config file at `~/.config/evm-cli/config.toml`
//   4. Hard-coded defaults
//
// Each `From<...>` impl on the source layers (`From<CliFlags>`, `From<EnvVars>`)
// produces a partial `Config` that is then `merge()`d. Lower-priority layers
// fill in only the fields the higher-priority layer left as `None`.

use std::path::PathBuf;

use serde::Deserialize;

/// Resolved runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP(S) RPC endpoint. Default: public Sepolia endpoint.
    pub rpc_url: String,
    /// Directory holding `<alias>` keystore files.
    pub keystore_dir: PathBuf,
    /// Top-level data dir (keystore + nonce.json). Default parent of `keystore_dir`.
    pub data_dir: PathBuf,
    /// Emit machine-readable JSON output instead of human prose.
    pub json: bool,
    /// Disable rustyline history (don't write/read `~/.evm_cli_history`).
    pub no_history: bool,
    /// Echo commands to the history file as they are entered.
    /// Inverse of `no_history`; the user-facing flag is `no_history`.
    pub write_history: bool,
    /// Network ID this client is bound to. PLAN-V9 §1 mandates Sepolia
    /// (chainId `0xaa36a7`). A different value triggers a startup error
    /// (per §7 self-audit "Signing chainId equals transaction chainId").
    pub expected_chain_id: u64,
    /// Connection timeout for the initial `eth_chainId` probe.
    pub rpc_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        let data_dir = default_data_dir();
        Self {
            rpc_url: "https://rpc.sepolia.org".to_string(),
            keystore_dir: data_dir.join("keystore"),
            data_dir: data_dir.clone(),
            json: false,
            no_history: false,
            write_history: true,
            expected_chain_id: 0xaa36a7, // Sepolia
            rpc_timeout_secs: 10,
        }
    }
}

impl Config {
    /// Layer-1 (lowest) source: hard-coded defaults.
    pub fn from_defaults() -> Self {
        Self::default()
    }

    /// Layer-2 source: optional TOML config file. Missing file is OK
    /// (returns the same as `from_defaults`).
    pub fn from_file(path: &std::path::Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::from_defaults());
        }
        let raw = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Read(path.to_path_buf(), e.to_string()))?;
        let parsed: TomlConfig = toml::from_str(&raw)
            .map_err(|e| ConfigError::Parse(path.to_path_buf(), e.to_string()))?;
        Ok(Self::from_defaults().merge_partial(parsed.into()))
    }

    /// Layer-3 source: environment variables. Missing vars fall through
    /// to lower-priority layers.
    pub fn from_env() -> PartialConfig {
        PartialConfig {
            rpc_url: std::env::var("EVMCLI_RPC_URL").ok(),
            keystore_dir: std::env::var_os("EVMCLI_KEYSTORE_DIR")
                .map(|s| PathBuf::from(s.to_string_lossy().to_string())),
            data_dir: std::env::var_os("EVMCLI_DATA_DIR")
                .map(|s| PathBuf::from(s.to_string_lossy().to_string())),
            json: parse_env_bool("EVMCLI_JSON"),
            no_history: parse_env_bool("EVMCLI_NO_HISTORY"),
            expected_chain_id: std::env::var("EVMCLI_CHAIN_ID")
                .ok()
                .and_then(|s| s.parse().ok()),
            rpc_timeout_secs: std::env::var("EVMCLI_RPC_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok()),
        }
    }

    /// Layer-4 (highest) source: CLI flags as already-parsed by clap.
    /// Pass `None` for any flag the user did not provide.
    pub fn from_cli(cli: &PartialConfig) -> PartialConfig {
        cli.clone()
    }

    /// Merge: `self` is the base (lower priority); `higher` (higher
    /// priority) overrides any field that is `Some(_)`. This is the
    /// 12-factor `CLI > env > file > default` cascade.
    pub fn merge_partial(mut self, higher: PartialConfig) -> Self {
        if let Some(v) = higher.rpc_url {
            self.rpc_url = v;
        }
        if let Some(p) = higher.keystore_dir {
            self.keystore_dir = p.clone();
            // If data_dir wasn't set separately, keep it as the parent.
            if higher.data_dir.is_none() {
                self.data_dir = p.parent().map(|p| p.to_path_buf()).unwrap_or(self.data_dir);
            }
        }
        if let Some(p) = higher.data_dir {
            self.data_dir = p;
        }
        if higher.json {
            self.json = true;
        }
        if higher.no_history {
            self.no_history = true;
            self.write_history = false;
        }
        if let Some(c) = higher.expected_chain_id {
            self.expected_chain_id = c;
        }
        if let Some(t) = higher.rpc_timeout_secs {
            self.rpc_timeout_secs = t;
        }
        self
    }

    /// Full 12-factor load. Reads the default config file path
    /// (`~/.config/evm-cli/config.toml`), then env, then any CLI
    /// partial already parsed.
    pub fn load(cli: PartialConfig) -> Result<Self, ConfigError> {
        let path = default_config_file();
        let from_file = Self::from_file(&path)?;
        let from_env = Self::from_env();
        let resolved = from_file.merge_partial(from_env).merge_partial(cli);
        Ok(resolved)
    }
}

/// Partial config: `None` means "not set at this layer; fall through".
/// Used for both the env layer and the CLI layer (clap parses every
/// flag as `Option<_>` and we collect them into this struct).
#[derive(Debug, Clone, Default)]
pub struct PartialConfig {
    pub rpc_url: Option<String>,
    pub keystore_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub json: bool,
    pub no_history: bool,
    pub expected_chain_id: Option<u64>,
    pub rpc_timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TomlConfig {
    rpc_url: Option<String>,
    keystore_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    json: Option<bool>,
    no_history: Option<bool>,
    expected_chain_id: Option<u64>,
    rpc_timeout_secs: Option<u64>,
}

impl From<TomlConfig> for PartialConfig {
    fn from(t: TomlConfig) -> Self {
        Self {
            rpc_url: t.rpc_url,
            keystore_dir: t.keystore_dir,
            data_dir: t.data_dir,
            json: t.json.unwrap_or(false),
            no_history: t.no_history.unwrap_or(false),
            expected_chain_id: t.expected_chain_id,
            rpc_timeout_secs: t.rpc_timeout_secs,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file read error at {0}: {1}")]
    Read(PathBuf, String),
    #[error("config file parse error at {0}: {1}")]
    Parse(PathBuf, String),
}

/// Default data dir: `$XDG_DATA_HOME/evm-cli` (or `~/.local/share/evm-cli`).
pub fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("local", "evm-cli", "evm-cli")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            // Fallback if `directories` can't determine the home dir.
            dirs_fallback_home()
                .join(".local")
                .join("share")
                .join("evm-cli")
        })
}

/// Default config file path: `$XDG_CONFIG_HOME/evm-cli/config.toml`.
pub fn default_config_file() -> PathBuf {
    directories::ProjectDirs::from("local", "evm-cli", "evm-cli")
        .map(|d| d.config_dir().join("config.toml"))
        .unwrap_or_else(|| {
            dirs_fallback_home()
                .join(".config")
                .join("evm-cli")
                .join("config.toml")
        })
}

fn dirs_fallback_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(|s| PathBuf::from(s.to_string_lossy().to_string()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn parse_env_bool(var: &str) -> bool {
    matches!(
        std::env::var(var).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_sepolia() {
        let c = Config::from_defaults();
        assert_eq!(c.expected_chain_id, 0xaa36a7);
        assert_eq!(c.rpc_url, "https://rpc.sepolia.org");
        assert!(!c.json);
        assert!(!c.no_history);
        assert!(c.write_history);
    }

    #[test]
    fn merge_higher_overrides_lower() {
        let lower = Config::from_defaults();
        let higher = PartialConfig {
            rpc_url: Some("https://my-rpc.example".to_string()),
            json: true,
            no_history: true,
            ..Default::default()
        };
        let merged = lower.merge_partial(higher);
        assert_eq!(merged.rpc_url, "https://my-rpc.example");
        assert!(merged.json);
        assert!(merged.no_history);
        assert!(
            !merged.write_history,
            "no_history should clear write_history"
        );
    }

    #[test]
    fn merge_no_history_clears_write_history() {
        let lower = Config {
            write_history: true,
            ..Config::from_defaults()
        };
        let higher = PartialConfig {
            no_history: true,
            ..Default::default()
        };
        assert!(!lower.clone().merge_partial(higher).write_history);
    }

    #[test]
    fn merge_partial_none_does_not_override() {
        let lower = Config::from_defaults();
        let higher = PartialConfig::default(); // everything None / false
        let merged = lower.clone().merge_partial(higher);
        // Defaults should survive when the higher layer is empty.
        assert_eq!(merged.rpc_url, lower.rpc_url);
        assert_eq!(merged.expected_chain_id, lower.expected_chain_id);
    }

    #[test]
    fn from_file_missing_returns_defaults() {
        let path = PathBuf::from("/nonexistent/evm-cli/config.toml");
        let c = Config::from_file(&path).expect("missing file is OK");
        assert_eq!(c.expected_chain_id, 0xaa36a7);
    }

    #[test]
    fn from_file_parses_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
                rpc_url = "https://example-rpc.test"
                json = true
                expected_chain_id = 11155111
            "#,
        )
        .expect("write");
        let c = Config::from_file(&path).expect("parse");
        assert_eq!(c.rpc_url, "https://example-rpc.test");
        assert!(c.json);
        assert_eq!(c.expected_chain_id, 11155111);
        // Untouched defaults survive.
        assert!(!c.no_history);
    }

    #[test]
    fn from_file_bad_toml_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml = = =").expect("write");
        let result = Config::from_file(&path);
        assert!(matches!(result, Err(ConfigError::Parse(_, _))));
    }
}
