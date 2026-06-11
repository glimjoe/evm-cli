# ADR-0005: Panic Hook Strategy

> Status: **Accepted**
> Date: 2026-06-11 (updated from Proposed 2026-06-10)
> Deciders: evm-cli maintainers
> Supersedes: V2 §7 L175 (self-audit requirement with no assigned M)

## Context and Problem Statement

V2's §7 self-audit required "panic hook does not dump stack variables" but no milestone was responsible for installing such a hook. B6 BLOCKER mandates the work in M0.

The threat model: a panic while `Secret<Vec<u8>>` is in scope on the stack could leak the secret via Rust's default panic message (which prints the panic location, but not local variables — yet Cargo-built binaries may also dump backtraces via `RUST_BACKTRACE=1`).

## Decision Drivers

- **Don't leak secret material**: no panic output should contain a private key, mnemonic, or seed.
- **Don't aid attackers**: panic location line numbers help an attacker localize bugs.
- **User experience**: a panic should produce a friendly "internal error, report to <repo>" message, not a stack trace.
- **Minimal new dependencies**: prefer stdlib when possible.

## Considered Options

- **A. `human-panic` crate** (chosen)
- **B. Self-written hook via `std::panic::set_hook` + `std::process::abort()`** (acceptable fallback)
- **C. Default Rust panic behavior** (rejected — violates §7 self-audit)

## Decision Outcome

**Chosen option: A (`human-panic` crate)**, finalized 2026-06-11.

### Rationale

`human-panic` is purpose-built for this exact need: it catches the panic, generates a self-contained report file, prints a friendly message, and aborts — without ever printing the panic location, message, payload, or backtrace to the terminal. It satisfies all four §7 self-audit criteria for the panic-hook check.

### M0 implementation

1. Add `human-panic = "2"` to `[dependencies]` in `Cargo.toml` (latest 2.x; pin exact per ADR-0001 upgrade policy).
2. In `src/main.rs`, **first line of `main()`** (before any `Secret` allocation, before any `tracing` init):
   ```rust
   fn main() {
       human_panic::setup!();
       // ... rest of main
   }
   ```
3. CI sets `RUST_BACKTRACE=0` (env var) for both `test` and `bench` jobs.
4. `[[bin]].strip = true` in `Cargo.toml` to reduce release-binary string disclosure of source paths.

### Verification (M0 DoD sub-item)

An integration test (`tests/it_panic_hook.rs`) that:

- Spawns the binary as a subprocess via `assert_cmd` or `std::process::Command`
- Causes a panic via a hidden `--trigger-panic-for-test` CLI flag (gated by `#[cfg(test)]` or env var)
- Captures stderr
- Asserts:
  - No hex string of length ≥ 32 (private key length) appears
  - No BIP-39 word appears
  - The expected friendly message appears
  - Exit code matches `SIGABRT` behavior (Unix) or `0xC0000409` (Windows; not in V1 scope)

If `human-panic` ever fails to satisfy these assertions, the M0 PR must switch to option B (self-written hook) and document the migration in a new ADR revision.

### Migration path to Option B (if needed)

If `human-panic` becomes unmaintained or introduces a vulnerability:

```rust
use std::panic;

fn main() {
    panic::set_hook(Box::new(|_| {
        eprintln!("internal error; please report to https://github.com/<org>/evm-cli/issues");
    }));
    // ... rest of main
}
// In Cargo.toml: no human-panic dep needed.
```

The migration is mechanical (≤ 5 LOC change + 1 dep removal). This ADR's "Option B as acceptable alternative" framing remains valid.

### Consequences

* **Good**: simple, satisfies §7 self-audit, ~5 lines of code (or one `human-panic::setup!()` call).
* **Good**: works in `dev` and `release` profiles identically.
* **Bad**: `human-panic` adds one dependency. If the maintainers prefer zero new deps, option B is the fallback.
* **Bad**: friendly error message makes diagnosis harder for the maintainer when a user reports a bug. Mitigation: include a request for the user to re-run with `EVMC_DEBUG=1` if they want a backtrace (this env var is checked in `main()` and explicitly opts in to default behavior).

## Implementation

- PLAN-V4 §5 M0 DoD (B6 mandate)
- `src/main.rs` first line
- `[[bin]].strip = true` in `Cargo.toml` to reduce release-binary string disclosure of source paths
- Integration test in `tests/it_panic_hook.rs`

## References

- PLAN-V4 §5 M0 DoD
- PLAN-V4 §7 self-audit checklist (panic hook check)
- `human-panic` crate: https://crates.io/crates/human-panic
- Rust `std::panic`: https://doc.rust-lang.org/std/panic/fn.set_hook.html
