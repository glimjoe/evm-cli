# Fuzz harnesses for evm-cli

Per PLAN-V9 §5 M3 DoD (P0-10) and §6 CI/CD nightly row, three
`cargo-fuzz` harnesses are provided:

| Harness | Target | Time |
|---|---|---|
| `fuzz_rlp_decode` | Decode random bytes as all 4 EVM tx types (legacy, EIP-1559, EIP-2930, EIP-7702) via `alloy::rlp::Decodable`; must not panic | 5 min |
| `fuzz_signature_recover` | Random 65-byte signature + fixed 32-byte message hash; `Signature::recover_address_*` must return `Result`, not panic | 5 min |
| `fuzz_keystore_json` | Random bytes through `serde_json::from_slice::<Value>` + standard accessor walk; must not panic | 5 min |

## Running locally

```bash
# Install cargo-fuzz (requires nightly)
cargo install cargo-fuzz

# Run a single harness
cargo +nightly fuzz run fuzz_rlp_decode -- -max_total_time=60

# Run all three harnesses for 5 minutes each (matches CI)
cargo +nightly fuzz run fuzz_rlp_decode -- -max_total_time=300
cargo +nightly fuzz run fuzz_signature_recover -- -max_total_time=300
cargo +nightly fuzz run fuzz_keystore_json -- -max_total_time=300
```

## CI

The `.github/workflows/nightly-fuzz.yml` workflow runs the three
harnesses on a nightly schedule (per PLAN-V9 §6). Crash artifacts
are uploaded; the release is blocked if any harness regresses.

## Local dependency

`fuzz/Cargo.toml` adds `serde_json` and `libfuzzer-sys` as dev-only
dependencies. The fuzz crate is a separate sub-package
(`evm-cli-fuzz`) so `cargo test --all-targets` in the main crate
does not pull in the fuzz toolchain.
