# ADR-0002: NonceManager Design

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision: four-state machine)
> Deciders: evm-cli maintainers
> Supersedes: V2 §5 M3 L121 ("local pending pool (HashMap<Address, Nonce>)" — design underspecified)

## Context and Problem Statement

V2 declared a `HashMap<Address, Nonce>` for nonce management but did not specify:

- Concurrency model (sync? async? lock granularity?)
- Persistence on crash
- Drop semantics during panic
- Conflict resolution when persisted state diverges from RPC
- Effect of keystore `rename` on pool keys

Without an explicit design, M3 implementation will accumulate ad-hoc decisions, each of which is a potential vulnerability (B3 BLOCKER).

## Decision Drivers

- **Crash safety**: a panic during tx broadcast must not lose the reserved nonce.
- **Concurrency**: REPL is async; multiple commands may be in flight (e.g. `send-eth` backgrounded while user runs `balance`).
- **Simplicity**: V1 is a single-user, single-process CLI; avoid actor frameworks.
- **Disk I/O cost**: every `next(addr)` triggers a write; must be cheap.
- **Rename safety**: user can rename a wallet alias; the nonce pool must not lose entries.

## Considered Options

- **A. `tokio::sync::Mutex<HashMap<Address, Nonce>>` + persist on every successful increment, with `release()` API** (initial choice; **superseded by A' below**)
- **A'. A + four-state state machine (no `release()`, replaced with `fail()` that marks `Stale`)** (chosen, see Decision Outcome)
- **B. `parking_lot::Mutex<HashMap<...>>` (sync) + persist on every increment**: no need to mark methods `async`, but mixes blocking I/O into the async runtime.
- **C. Actor / channel-based `NonceManager`**: separate task processes `Reserve(addr) -> Nonce` requests; over-engineered for V1.
- **D. SQLite-backed**: durable, transactional, but adds a heavy dep for one value per address. Deferred to V2.

## Decision Outcome

**Chosen option: A'** — A's structure (tokio Mutex + persist-on-state-change) plus a **four-state machine** for each nonce, **plus multi-process file locking**.

### Four-state machine

Each `(Address, Nonce)` tuple moves through exactly four states; transitions are append-only to a JSON log file (not in-place mutation):

```
   ┌───────────┐
   │ Reserved  │  (in memory only; not yet broadcast)
   └─────┬─────┘
         │ submit(nonce, tx_hash)
         ▼
   ┌───────────┐
   │ Submitted │  (persisted; broadcast to RPC, waiting for receipt)
   └─────┬─────┘
         │
    ┌────┴────┬────────────┐
    │         │            │
    ▼         ▼            ▼
┌───────┐ ┌────────┐  ┌──────────┐
│ Mined │ │ Stale  │  │ Replaced │ (RBF / Cancel — see ADR-0008)
└───────┘ └────────┘  └──────────┘
```

- **Reserved**: created by `next()`. Not persisted (lost on crash — next `next()` returns same nonce).
- **Submitted**: created by `submit(nonce, tx_hash)`. Persisted. Polled via `eth_getTransactionReceipt` until terminal.
- **Mined**: receipt status = 0x1. Archived to history.
- **Stale**: receipt not found after `EVMC_NONCE_STALE_TIMEOUT` (default 180s) **or** `eth_getTransactionReceipt` returns null after 5 consecutive polls. **Never re-used**; `next()` skips it.
- **Replaced**: superseded by an RBF or Cancel tx (see ADR-0008). Same as Stale from the pool's perspective; original tx_hash kept for forensics.

### API

```rust
pub struct NonceManager {
    state: tokio::sync::Mutex<HashMap<Address, AddressState>>,
    persist_path: PathBuf,
    file_lock: tokio::sync::Mutex<File>,  // flock on nonce.json
}

pub struct AddressState {
    /// Highest nonce known to be in use (Reserved or Submitted).
    /// next() returns `next_nonce`, then increments.
    next_nonce: u64,
    /// Nonces that are Submitted (waiting for receipt).
    pending: BTreeMap<Nonce, TxHash>,
    /// Nonces that are Stale or Replaced (skipped by next()).
    dead: BTreeSet<Nonce>,
    /// Last 100 Mined nonces (for tx-history display, not for pool logic).
    history: VecDeque<(Nonce, TxHash, BlockNumber)>,
}

impl NonceManager {
    /// Atomically reserve next nonce for `addr`. Persists nothing.
    pub async fn next(&self, addr: Address) -> Result<Nonce>;

    /// Mark a Reserved nonce as broadcast; tx_hash recorded.
    /// Persists: appends `(addr, Submitted, nonce, tx_hash)`.
    pub async fn submit(&self, addr: Address, nonce: Nonce, tx_hash: TxHash) -> Result<()>;

    /// Mark a Submitted nonce as confirmed on-chain.
    /// Persists: appends `(addr, Mined, nonce, tx_hash, block)`.
    pub async fn confirm(&self, addr: Address, nonce: Nonce, block: BlockNumber) -> Result<()>;

    /// Mark a Submitted nonce as not-mined after timeout.
    /// Persists: appends `(addr, Stale, nonce)`. Original tx_hash retained in history for forensics.
    pub async fn fail(&self, addr: Address, nonce: Nonce) -> Result<()>;

    /// Mark a Submitted nonce as superseded by RBF/Cancel.
    /// Persists: appends `(addr, Replaced, old_nonce, new_nonce, new_tx_hash)`.
    pub async fn replaced(&self, addr: Address, old: Nonce, new: Nonce, new_hash: TxHash) -> Result<()>;

    /// Rebuild from RPC on startup; conflict = max(local Submitted, rpc "pending").
    /// Acquired under file lock.
    pub async fn rebuild(&self, addr: Address) -> Result<()>;
}
```

### Key properties

- **Keys are `Address`, never alias**: `rename` operations from M2 do not affect this state.
- **No `release()` API exists**. A nonce that was once `Reserved` is either `Submitted` or implicitly dropped (e.g. process killed between `next()` and `submit()`). On restart, `next()` will return the same nonce if the previous `Reserved` was never `Submitted` — this is safe because nothing was broadcast.
- **Drop is panic-safe**: `Drop` does not mutate state. A panic between `next()` and `submit()` is recovered on next run via `rebuild()` (the unused nonce is reclaimed).
- **Persist on state transition only** (not on `next()`): `submit`, `confirm`, `fail`, `replaced` all persist. `next()` does not. Reduces fsync frequency from 1-per-tx to 1-3-per-tx.
- **Multi-process safety**: every method takes an `flock` on `nonce.json` before reading or writing. Concurrent processes see a consistent view; second writer blocks until first releases.
- **History cap**: `history` is a `VecDeque<(Nonce, TxHash, BlockNumber)>` capped at 100 entries per address. Older entries are dropped (we don't need a full audit log for V1).
- **Rebuild conflict**: `max(local Submitted nonces, eth_getTransactionCount(addr, "pending"))` wins. Local Stale / Mined / Replaced are kept (they're authoritative history).

### Why no `release()`?

The original `release()` was a foot-gun: a "released" nonce could be re-issued while the same nonce was still pending in the mempool from a parallel code path (or a parallel process). The four-state machine eliminates the concept entirely:

- A `Reserved` nonce is implicitly released on crash (next run picks it up again — no broadcast was made, so this is safe).
- A `Submitted` nonce is either confirmed (Mined), times out (Stale), or is replaced (Replaced). It is **never** made available for re-use.
- Only `Stale` and `Replaced` nonces are "skipped" by `next()` — they're already known not to be in the mempool, so skipping is correct.

### Worked example (normal happy path)

```
t=0:   next()    → 5   (Reserved)
t=1:   submit(5, 0xabc)  → Submitted{5, 0xabc}; persist
t=2:   next()    → 6   (Reserved)
t=3:   submit(6, 0xdef)  → Submitted{6, 0xdef}; persist
t=10:  confirm(5, block=100)  → Mined{5}; persist; history push
t=15:  confirm(6, block=100)  → Mined{6}; persist; history push
```

### Worked example (timeout)

```
t=0:   next()    → 5
t=1:   submit(5, 0xabc)
t=180: fail(5)   → Stale{5}; persist
t=181: next()    → 6   (skips 5)
```

### Consequences

* **Good**: `fail()` is the only "give up" path, and it never causes double-spend.
* **Good**: state machine makes the lifecycle explicit and testable; every transition is a unit-test seam.
* **Good**: persist-on-transition (not on `next()`) halves fsync frequency vs the initial draft.
* **Good**: history cap bounds memory regardless of wallet age.
* **Good**: `flock` makes the file safe across multiple evm-cli processes.
* **Bad**: state machine is more complex than the initial draft; M3 implementation is ~30% more code.
* **Bad**: `flock` on Linux only (NFS may not honor it; V1 is local fs only, so this is acceptable).

## Implementation

- PLAN-V4 §5 M3 DoD
- File location: `~/.local/share/evm-cli/nonce.json` (data dir from `directories` crate)
- Persist format: JSON-lines (one transition per line), append-only
  ```jsonl
  {"ts":"2026-06-11T12:00:00Z","addr":"0x...","state":"Submitted","nonce":5,"tx_hash":"0xabc..."}
  {"ts":"2026-06-11T12:00:30Z","addr":"0x...","state":"Mined","nonce":5,"tx_hash":"0xabc...","block":100}
  {"ts":"2026-06-11T12:03:00Z","addr":"0x...","state":"Stale","nonce":5}
  ```
- `fsync` after every transition (small file, infrequent writes)
- `nix::fcntl::flock` for cross-process locking
- Unit tests cover: each state transition, panic between `next` and `submit`, two processes racing on `rebuild`, RPC-vs-local conflict
- Integration test in `tests/it_nonce_manager.rs` using `anvil`
- `pending-tx` command (B4) reads from `pending: BTreeMap<Nonce, TxHash>` — no RPC scan

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 3 issues in the initial Accepted draft. All addressed by adopting a four-state state machine:

1. **`release()` API was a double-spend foot-gun**: the initial design allowed a nonce to be released back to the pool after broadcast failure, but a parallel submission of the same nonce (e.g. from another process or a retry loop) could have double-broadcast it. Now `release()` is gone; `fail()` marks `Stale` and the nonce is **never** re-issued.
2. **No `confirm()` API → unbounded growth**: the initial design had no way to mark a nonce as "spent on-chain"; the pool would grow forever. Now `confirm()` moves nonces to a 100-entry history ring; old entries are dropped.
3. **Multi-process race not addressed**: the initial design had no `flock`, so two simultaneous `evm-cli` processes could split-brain the pool. Now every method takes an `flock` on `nonce.json` before reading or writing.

The chosen option changed from A to A' (same storage substrate, different state model). Persist format changed from full-map rewrite to JSON-lines append log (better for forensic analysis and for the new state machine).

## References

- PLAN-V4 §5 M3 (B3 BLOCKER resolution)
- ADR-0008 (RBF / Cancel — uses `replaced()` API)
- BIP-125 (RBF fee bump policy)
- tokio::sync::Mutex docs
- `nix` crate (`flock` bindings): https://crates.io/crates/nix
- EIP-161 (account nonce semantics)
