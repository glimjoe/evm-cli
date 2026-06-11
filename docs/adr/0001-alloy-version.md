# ADR-0001: alloy Version Selection

> Status: **Accepted**
> Date: 2026-06-11 (updated from Proposed 2026-06-10)
> Deciders: evm-cli maintainers
> Supersedes: V2 §1 L19 (`alloy = "=0.9.x"`); V3/V4 §1 (interim "1.x latest stable at M0 kickoff")

## Context and Problem Statement

V2 plan locked `alloy = "=0.9.x"` with MSRV 1.78. V3/V4 upgraded to "1.x latest stable" with MSRV 1.81. Both interim choices are invalidated by the state of crates.io as of 2026-06-11:

- **B1**: alloy's actual MSRV at the time of evaluation is **1.91** (raised to 1.91 starting at 1.7.0; 1.6.3 was 1.88). V2's "1.78" and V4's "1.81" both underestimate the true requirement.
- **B7**: alloy 2.0.0 was released **2026-04-13**; 2.0.5 is the current stable as of 2026-06. The "1.x latest stable" framing in V3/V4 reflects state of the world before 2.0 shipped.

We must commit to a specific version and MSRV, documented with evidence.

## Decision Drivers

- **MSRV compatibility**: declared MSRV must satisfy `alloy`'s actual MSRV (1.91 in 1.7+/2.0+).
- **Stability**: prefer a line with at least one patch release (i.e. not 2.0.0 itself).
- **Ecosystem alignment**: tutorial code, examples, and SO answers should match the chosen line.
- **Security posture**: pick a line receiving active CVE backports (2.x is the active line).
- **Reproducibility**: the chosen version must be pinned exactly in release builds per §1.1 (ADR-0004).
- **API surface availability**: M1–M3 require `Signer`, `LocalSigner`, `Signature::recover_address_from_msg`, `alloy::node_bindings::anvil`, `sol!` macro. All verified present in 2.0.5.

## Considered Options (re-evaluated 2026-06-11)

- **A. `alloy = "=2.0.5"`** (chosen)
- **B. `alloy = "=1.8.3"`** (last 1.x, released 2026-03-27): same MSRV (1.91), but on a maintenance branch; 17 days after 1.8.3 the 2.0 line shipped.
- **C. `alloy = "=1.6.3"`** (last 1.88 MSRV): keeps MSRV at 1.88, but loses ~2 months of upstream fixes and is well behind 2.0.
- **D. Switch to `ethers-rs`**: massive scope change; rejected (V1 is alloy-bound per B1/B7 intent).

## Decision Outcome

**Chosen option: A** — `alloy = "=2.0.5"`, MSRV **1.91**.

### Verification log (2026-06-11)

```
$ cargo search alloy | head -3
alloy = "2.0.5"
alloy-erc20-full = "1.0.0"
alloy-transport-balancer = "0.1.2"

$ cargo info alloy | head -3
alloy
version: 2.0.5
rust-version: 1.91

# Sub-crates needed for V1 (all 2.0.5 except primitives/sol-types/rlp, all MSRV 1.91):
alloy-primitives = 1.6.0    (Address, B256, U256, Signature, FixedBytes)
alloy-signer = 2.0.5        (Signer trait, EIP-155 logic)
alloy-signer-local = 2.0.5  (PrivateKeySigner, mnemonic import)
alloy-consensus = 2.0.5     (TxEnvelope, TxEip1559, recoverable signatures)
alloy-rlp = 0.3.15          (Decodable trait — fuzz target)
alloy-contract = 2.0.5      (Instance<>, call_builder)
alloy-sol-types = 1.6.0     (sol! macro, ERC-20 ABI)
alloy-node-bindings = 2.0.5 (AnvilInstance for integration tests)
```

Local `cargo --version` is 1.96.0 (2026-05-25), which satisfies MSRV 1.91.

### Consequences

* **Good**: truly latest stable; full bug-fix surface of 2.0.x line; CVE backports active.
* **Good**: API surface for M1–M3 verified present (Signer, LocalSigner, anvil, sol!, rlp::Decodable all exist in 2.0.5).
* **Good**: ecosystem migration to 2.0 is recent (2026-04-13), so tutorial/example coverage is building up but not yet saturated — the V1 project is on a forward-looking line.
* **Bad**: MSRV rises from V2's 1.78 → 1.91. This affects any consumer of evm-cli as a library (not relevant for V1 since `no published lib`).
* **Bad**: 2.0.5 is a young release (~3 weeks old as of 2026-06-11). Minor edge-case bugs may still surface. Mitigation: pinned exact, Upgrade Policy (ADR-0004) provides a path to migrate.
* **Bad**: this ADR is the first re-baseline. Future minor bumps (2.0.x → 2.0.x' or 2.0.x → 2.1.0) require a new ADR + PR review per §1.1.

## Implementation

- `Cargo.toml`:
  ```toml
  [dependencies]
  alloy = "=2.0.5"
  alloy-primitives = "=1.6.0"
  alloy-signer = "=2.0.5"
  alloy-signer-local = "=2.0.5"
  alloy-consensus = "=2.0.5"
  alloy-rlp = "=0.3.15"
  alloy-contract = "=2.0.5"
  alloy-sol-types = "=1.6.0"
  alloy-node-bindings = "=2.0.5"  # dev-dependency, gated to #[cfg(test)]
  ```
- `rust-toolchain.toml`:
  ```toml
  [toolchain]
  channel = "1.91"
  ```
- CI matrix: `cargo build` + `cargo test` on x86_64 and aarch64 Linux
- Re-verify `cargo metadata --format-version=1` lists MSRV = 1.91 in the lockfile

## V4 Sync Required (post-ADR-0001 accept)

The following V4 sections reference stale assumptions and must be updated to reflect `alloy = 2.0.5` and MSRV `1.91`:

| V4 location | Stale text | Updated text |
|---|---|---|
| §1 Project Identity, MSRV row | `1.81` | `1.91` |
| §1 Project Identity, alloy row | `1.x latest stable` | `2.0.5` |
| §5 M0 DoD, `rust-toolchain.toml` | `pins 1.81` | `pins 1.91` |
| §8 Risk Register, B1 row | "ADR-001; pinned 1.81" | "ADR-0001 accepted at 1.91 with 2.0.5" |
| §11 Next Action | "ADR-001 TBD" | "ADR-0001 accepted; gating on ADR-0005" |
| §12 V2→V3 Changelog, B1 row | "MSRV upgraded to 1.81" | "MSRV upgraded to 1.91 (intermediate V4 '1.81' superseded by 2026-06-11 verification)" |
| §13 V3→V4 Changelog | (no entry — V4 preceded this verification) | Add "ADR-0001 re-evaluated 2026-06-11: 2.0.5 + MSRV 1.91" |

This sync can be batched with the ADR-0005 acceptance (G2) into a single V4 → V5 update.

## References

- PLAN-V4 §1, §5 M0, §8, §11, §12
- ADR-0004 (Upgrade Policy — governs future minor bumps)
- crates.io API snapshot: `/home/yangwei/.local/share/opencode/tool-output/tool_eb4407ba8001wDVpnp7Tvgo1X6`
- alloy 2.0.5 docs: https://docs.rs/alloy/2.0.5
- MADR: https://adr.github.io/madr/
