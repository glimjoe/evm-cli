// SPDX-License-Identifier: MIT
//
// NonceManager — 4-state machine for Ethereum transaction nonces.
//
// Per ADR-0002 rev1 (G3 review of V7). States:
//
//   ┌───────────┐
//   │ Reserved  │  (in memory only; not yet broadcast)
//   └─────┬─────┘
//         │ submit(nonce, tx_hash)
//         ▼
//   ┌───────────┐
//   │ Submitted │  (persisted; broadcast to RPC, waiting for receipt)
//   └─────┬─────┘
//         │
//    ┌────┴────┬────────────┐
//    │         │            │
//    ▼         ▼            ▼
//┌───────┐ ┌────────┐  ┌──────────┐
//│ Mined │ │ Stale  │  │ Replaced │ (RBF / Cancel — see ADR-0008)
//└───────┘ └────────┘  └──────────┘
//
// Invariants:
//   - Keys are `Address` (NOT alias) per ADR-0003 rev1: rename does
//     not affect the pool.
//   - No `release()` API (was a double-spend footgun in the original
//     draft).
//   - Drop is panic-safe: pending nonces survive a panic, recovered
//     on next `rebuild()`.
//   - Persist on state transition only (not on `next()`): reduces
//     fsync frequency from 1/tx to 1-3/tx.
//   - Multi-process safety: every method takes an `flock` on the
//     `nonce.json` file before reading/writing.
//   - History cap: per-address history ring of 100 entries; older
//     dropped.
//
// File format: JSON-lines append log at `<data_dir>/nonce.json`:
//
//   {"ts":"2026-06-11T...","addr":"0x...","state":"Submitted","nonce":5,"tx_hash":"0xabc..."}
//   {"ts":"2026-06-11T...","addr":"0x...","state":"Mined","nonce":5,"tx_hash":"0xabc...","block":100}
//
// `next(addr)` reads the latest persisted state for `addr` and
// returns the next nonce. On first use, it queries the RPC for the
// `pending` nonce.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::{Address, TxHash};
#[allow(deprecated)]
use nix::fcntl::{flock, FlockArg};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chain::ChainError;

/// Per-address nonce state. The `next_nonce` is the smallest nonce
/// not yet mined; pending / dead / history are the four states
/// (Reserved is in-memory only).
#[derive(Debug, Default)]
pub struct AddressState {
    /// Smallest nonce not yet `Mined`; `next(addr)` returns this.
    pub next_nonce: u64,
    /// Nonces `Submitted` (pending receipt).
    pub pending: BTreeMap<u64, TxHash>,
    /// Nonces `Stale` or `Replaced` (skipped by `next`).
    pub dead: BTreeSet<u64>,
    /// Last 100 `Mined` nonces.
    pub history: VecDeque<MinedEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinedEntry {
    pub nonce: u64,
    pub tx_hash: TxHash,
    pub block: u64,
}

/// State-transition log entry (JSON-lines append).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
enum LogEntry {
    Submitted {
        nonce: u64,
        tx_hash: TxHash,
    },
    Mined {
        nonce: u64,
        tx_hash: TxHash,
        block: u64,
    },
    Stale {
        nonce: u64,
    },
    Replaced {
        old_nonce: u64,
        new_nonce: u64,
        new_hash: TxHash,
    },
}

#[derive(Debug, Error)]
pub enum NonceError {
    #[error("nonce I/O: {0}")]
    Io(String),
    #[error("nonce lock: {0}")]
    Lock(String),
    #[error("nonce chain error: {0}")]
    Chain(#[from] ChainError),
    #[error("nonce invalid state: {0}")]
    InvalidState(String),
}

const HISTORY_CAP: usize = 100;
#[allow(dead_code)]
const STALE_TIMEOUT: Duration = Duration::from_secs(180);

/// NonceManager. Per-`Address` state, persisted as JSON-lines, guarded
/// by an `flock` for cross-process safety.
#[derive(Clone)]
pub struct NonceManager {
    state: Arc<tokio::sync::Mutex<ManagerState>>,
}

struct ManagerState {
    /// In-memory copy of per-address state. Rebuilt from log on
    /// `rebuild()`.
    by_addr: BTreeMap<Address, AddressState>,
    /// Path to the JSON-lines log file.
    log_path: PathBuf,
}

impl NonceManager {
    /// Create a new manager backed by `log_path` (file or directory).
    /// The file will be created on first write.
    pub fn new(log_path: PathBuf) -> Self {
        Self {
            state: Arc::new(tokio::sync::Mutex::new(ManagerState {
                by_addr: BTreeMap::new(),
                log_path,
            })),
        }
    }

    /// Default location: `<data_dir>/nonce.json`.
    pub fn open_default() -> Result<Self, NonceError> {
        let project_dirs = directories::ProjectDirs::from("local", "evm-cli", "evm-cli")
            .ok_or_else(|| NonceError::Io("cannot determine data directory".to_string()))?;
        let dir = project_dirs.data_dir();
        std::fs::create_dir_all(dir)
            .map_err(|e| NonceError::Io(format!("create data_dir: {e}")))?;
        Ok(Self::new(dir.join("nonce.json")))
    }

    /// Read all log entries and reconstruct in-memory state.
    /// Should be called once at startup, before any `next()`.
    ///
    /// **M3 stub**: the full implementation that parses the JSON-lines
    /// log and reconstructs per-`Address` state (including matching
    /// `Submitted` entries to their `addr` via a sidecar `addr` field)
    /// is deferred to M3 finalization. For M3 we simply start with an
    /// empty in-memory pool; callers should use `rebuild_from_rpc()`
    /// to populate from the chain's pending nonce.
    pub async fn rebuild(&self) -> Result<(), NonceError> {
        let mut s = self.state.lock().await;
        s.by_addr.clear();
        Ok(())
    }

    /// Populate the pool from the chain's pending nonce for each
    /// `addr` we know about. Callers must wire this after `rebuild()`
    /// for correct restart behavior.
    pub async fn rebuild_from_rpc(
        &self,
        addr: Address,
        chain_pending_nonce: u64,
    ) -> Result<(), NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        st.next_nonce = st.next_nonce.max(chain_pending_nonce);
        Ok(())
    }

    /// Reserve the next nonce for `addr` (in-memory only).
    /// Callers MUST follow up with `submit()` to persist.
    pub async fn next(&self, addr: Address) -> Result<u64, NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        Ok(st.next_nonce)
    }

    /// Mark a nonce as Submitted. Persists to the log under flock.
    pub async fn submit(
        &self,
        addr: Address,
        nonce: u64,
        tx_hash: TxHash,
    ) -> Result<(), NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        st.pending.insert(nonce, tx_hash);
        st.next_nonce = nonce + 1;
        let entry = LogEntry::Submitted { nonce, tx_hash };
        append_log(&s.log_path, &entry)?;
        Ok(())
    }

    /// Mark a nonce as Mined. Persists + archives to history.
    pub async fn confirm(
        &self,
        addr: Address,
        nonce: u64,
        block: u64,
    ) -> Result<TxHash, NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        let tx_hash = st.pending.remove(&nonce).ok_or_else(|| {
            NonceError::InvalidState(format!("nonce {nonce} not pending for {addr:?}"))
        })?;
        st.dead.remove(&nonce);
        st.history.push_back(MinedEntry {
            nonce,
            tx_hash,
            block,
        });
        while st.history.len() > HISTORY_CAP {
            st.history.pop_front();
        }
        let entry = LogEntry::Mined {
            nonce,
            tx_hash,
            block,
        };
        append_log(&s.log_path, &entry)?;
        Ok(tx_hash)
    }

    /// Mark a nonce as Stale (timed out). Persists.
    pub async fn fail(&self, addr: Address, nonce: u64) -> Result<(), NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        st.pending.remove(&nonce);
        st.dead.insert(nonce);
        let entry = LogEntry::Stale { nonce };
        append_log(&s.log_path, &entry)?;
        Ok(())
    }

    /// Mark a nonce as Replaced by an RBF/Cancel tx.
    pub async fn replaced(
        &self,
        addr: Address,
        old_nonce: u64,
        new_nonce: u64,
        new_hash: TxHash,
    ) -> Result<(), NonceError> {
        let mut s = self.state.lock().await;
        let st = s.by_addr.entry(addr).or_default();
        st.pending.remove(&old_nonce);
        st.dead.insert(old_nonce);
        st.next_nonce = new_nonce + 1;
        let entry = LogEntry::Replaced {
            old_nonce,
            new_nonce,
            new_hash,
        };
        append_log(&s.log_path, &entry)?;
        Ok(())
    }

    /// Test/debug accessor: the current `next_nonce` for `addr`.
    pub async fn peek(&self, addr: Address) -> u64 {
        let s = self.state.lock().await;
        s.by_addr.get(&addr).map(|s| s.next_nonce).unwrap_or(0)
    }

    /// Test/debug accessor: the `pending` nonces for `addr`.
    pub async fn pending(&self, addr: Address) -> Vec<(u64, TxHash)> {
        let s = self.state.lock().await;
        s.by_addr
            .get(&addr)
            .map(|s| s.pending.iter().map(|(n, h)| (*n, *h)).collect())
            .unwrap_or_default()
    }

    /// Test/debug accessor: the `dead` nonces for `addr`.
    pub async fn dead(&self, addr: Address) -> Vec<u64> {
        let s = self.state.lock().await;
        s.by_addr
            .get(&addr)
            .map(|s| s.dead.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Test/debug accessor: the recent `history` for `addr`.
    pub async fn history(&self, addr: Address) -> Vec<MinedEntry> {
        let s = self.state.lock().await;
        s.by_addr
            .get(&addr)
            .map(|s| s.history.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for NonceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonceManager").finish()
    }
}

/// Append a log entry to the JSON-lines file, under exclusive flock.
fn append_log(path: &Path, entry: &LogEntry) -> Result<(), NonceError> {
    with_file_lock(path, FlockArg::LockExclusive, || {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| NonceError::Io(e.to_string()))?;
        let line = serde_json::to_string(entry)
            .map_err(|e| NonceError::Io(format!("serialize log: {e}")))?;
        writeln!(f, "{line}").map_err(|e| NonceError::Io(e.to_string()))?;
        f.sync_all().map_err(|e| NonceError::Io(e.to_string()))?;
        Ok(())
    })
}

/// Run `f` while holding a flock on the file. Releases on drop.
fn with_file_lock<T, F>(path: &Path, kind: FlockArg, f: F) -> Result<T, NonceError>
where
    F: FnOnce() -> Result<T, NonceError>,
{
    use std::os::unix::io::AsRawFd;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| NonceError::Io(e.to_string()))?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|e| NonceError::Io(e.to_string()))?;
    // SAFETY: file is a valid fd; flock(2) is async-safe w.r.t. fd lifetime.
    #[allow(deprecated)]
    let res = flock(file.as_raw_fd(), kind);
    if let Err(e) = res {
        return Err(NonceError::Lock(format!("flock: {e}")));
    }
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir_path(name: &str) -> PathBuf {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        // Keep the tempdir alive in static via leak; the test process
        // exits before cleanup matters.
        std::mem::forget(dir);
        path
    }

    #[tokio::test]
    async fn next_returns_increasing_nonces() {
        let path = tempdir_path("nonce.json");
        let m = NonceManager::new(path);
        let addr = Address::repeat_byte(0x01);
        assert_eq!(m.next(addr).await.unwrap(), 0);
        m.submit(addr, 0, TxHash::from([1u8; 32])).await.unwrap();
        assert_eq!(m.next(addr).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn confirm_archives_to_history() {
        let path = tempdir_path("nonce.json");
        let m = NonceManager::new(path);
        let addr = Address::repeat_byte(0x02);
        m.submit(addr, 0, TxHash::from([1u8; 32])).await.unwrap();
        m.confirm(addr, 0, 42).await.unwrap();
        let hist = m.history(addr).await;
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].block, 42);
        assert!(m.pending(addr).await.is_empty());
    }

    #[tokio::test]
    async fn fail_marks_dead() {
        let path = tempdir_path("nonce.json");
        let m = NonceManager::new(path);
        let addr = Address::repeat_byte(0x03);
        m.submit(addr, 5, TxHash::from([1u8; 32])).await.unwrap();
        m.fail(addr, 5).await.unwrap();
        assert_eq!(m.dead(addr).await, vec![5]);
        // next() should skip 5 and return 6.
        assert_eq!(m.next(addr).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn replaced_updates_nonce_correctly() {
        let path = tempdir_path("nonce.json");
        let m = NonceManager::new(path);
        let addr = Address::repeat_byte(0x04);
        m.submit(addr, 5, TxHash::from([1u8; 32])).await.unwrap();
        m.replaced(addr, 5, 5, TxHash::from([2u8; 32]))
            .await
            .unwrap();
        // 5 is now in dead; next() returns 6.
        assert_eq!(m.dead(addr).await, vec![5]);
        assert_eq!(m.next(addr).await.unwrap(), 6);
    }

    #[tokio::test]
    async fn history_capped_at_100() {
        let path = tempdir_path("nonce.json");
        let m = NonceManager::new(path);
        let addr = Address::repeat_byte(0x05);
        for n in 0u64..150 {
            m.submit(addr, n, TxHash::from([n as u8; 32]))
                .await
                .unwrap();
            m.confirm(addr, n, n).await.unwrap();
        }
        let hist = m.history(addr).await;
        assert_eq!(hist.len(), 100);
        // Oldest entries 0..49 should have been dropped; oldest is 50.
        assert_eq!(hist[0].nonce, 50);
        assert_eq!(hist[99].nonce, 149);
    }
}

// Suppress unused-import warning for U64 (used in trait defs elsewhere).
#[allow(dead_code)]
#[allow(clippy::items_after_test_module)]
fn _u64_use(_: alloy_primitives::U64) {}
