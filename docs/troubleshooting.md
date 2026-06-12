# Troubleshooting

> Per PLAN-V9 §5 M6 DoD. Common error codes (from
> [docs/code_allocation.md](./code_allocation.md)) with diagnosis and
> resolution steps. For the full error-code registry, see
> [docs/code_allocation.md](./code_allocation.md). For the
> architecture, see [docs/architecture.md](./architecture.md).

## Quick reference: error-code lookup

Each code below maps to a stable variant. Deprecated codes are
**never** removed (only aliased to new codes per ADR-0006 rev1).

| Code | Category | When you'll see it | First thing to try |
|---|---|---|---|
| `EVMCR-001` | Crypto | `import-mnemonic` with a typo'd word | Re-check the BIP-39 wordlist; the last word carries the checksum |
| `EVMCR-002` | Crypto | `import-mnemonic` with malformed path | Path must be `m/44'/60'/0'/0/<i>` |
| `EVMCR-003` | Crypto | Internal ecrecover roundtrip fail | File an issue — this indicates a signer/EIP-2 bug |
| `EVMCR-004` | Crypto | KDF computation error | Rare; check `keystore-dir` permissions |
| `EVMK-001` | Keystore | `unlock` with wrong password | **Note:** same code for missing file (anti-side-channel) |
| `EVMK-002` | Keystore | JSON parse fail or non-keystore content | The file is corrupted; recover from backup or recreate |
| `EVMK-009` | Keystore | `use` / `delete` on a non-existent alias | Run `list` to see existing aliases |
| `EVMK-010` | Keystore | `create-wallet` / `import-mnemonic` with used alias | Pick a new alias or `delete` the old one |
| `EVMK-011` | Keystore | Filesystem I/O error (permission / disk full) | Check `keystore-dir` permissions and disk space |
| `EVMK-012` | Keystore | Other internal error (alloy / eth-keystore / BIP-39) | Run with `RUST_LOG=evm_cli=debug`; file an issue with the log |
| `EVMC-001` | Chain | RPC unreachable / 429 / server error | Check `--rpc-url`; wait and retry; check the rate limit (25 r/s) |
| `EVMC-002` | Chain | NonceManager timeout (tx pending > 120s) | Check Etherscan; use `send-eth --bump-fee` to accelerate or `--cancel` |
| `EVMC-003` | Chain | `eth_sendRawTransaction` rejected: fee too low | Use `send-eth --bump-fee` to retry with a higher fee |
| `EVMC-004` | Chain | AMOUNT unparseable / negative / overflow | AMOUNT must be a positive decimal string (e.g. `0.001`) |
| `EVMC-005` | Chain | config chain_id ≠ tx chain_id (EIP-155) | Verify `--chain-id 11155111` (Sepolia) |
| `EVMC-006` | Chain | tx mined but status = 0 (reverted) | Check the contract logic / recipient; inspect on Etherscan |
| `EVMC-007` | Chain | `--bump-fee`/`--cancel` hash not in mempool | The original tx may have already dropped or been mined |
| `EVMC-008` | Chain | `--bump-fee`/`--cancel` already mined | The tx is in a block; no replacement possible |
| `EVMC-009` | Chain | balance < value + max_fee | Fund the wallet or reduce the amount |
| `EVMC-010` | Chain | `eth_estimateGas` reverted / timed out | The contract would revert; check the receiver / calldata |
| `EVMC-099` | Chain | 120s receipt polling timed out | The tx may still confirm; check Etherscan by hash |
| `EVMCFG-001` | Config | Required config field missing | Set the env var or add the TOML key (see [docs/architecture.md](./architecture.md)) |
| `EVMCFG-002` | Config | Parse / range failure | Check the value format; the error message names the field |
| `EVMCFG-003` | Config | Config file path missing | Verify the path with `ls`; default is `~/.config/evm-cli/config.toml` |
| `EVMCFG-004` | Config | Config file not readable | `chmod 600` the file; ensure you're the owner |
| `EVMIO-001` | I/O | Filesystem permission denied | Check ownership / mode on the affected path |
| `EVMIO-002` | I/O | Disk full | Free space; the `data-dir` writes a NonceManager JSONL log |
| `EVMIO-003` | I/O | Parent dir missing | Create the parent dir (e.g. `mkdir -p ~/.config/evm-cli`) |
| `EVMIO-004` | I/O | Generic I/O failure | Re-run; if persistent, capture the message and file an issue |
| `EVM-999` | Catch-all | Downcast failure (should never trigger) | File an issue with the cause chain |

## Common scenarios

### "I forgot my password"

**No recovery path.** The keystore uses scrypt N=2¹³ (see ADR-0009);
brute-forcing is feasible at ~10 passwords/sec on consumer hardware
for short passphrases. Long passphrases are safe.

**Prevention:** record the mnemonic in a secure offline location
(paper, metal seed plate). The mnemonic is the source of truth; the
keystore is a convenience wrapper.

### "My tx is stuck in pending"

Two options:

1. **Speed it up** (RBF):
   ```bash
   $ evm-cli pending-tx           # find the hash
   42  0xabcd…ef  age: 5m
   $ evm-cli send-eth 0xfeed… 0.001 --bump-fee 0xabcd…ef
   ```
   This re-signs with a higher `max_fee_per_gas` and the same nonce,
   so the new tx replaces the old one in the mempool.

2. **Cancel it** (0-value self-send):
   ```bash
   $ evm-cli send-eth 0xself 0 --cancel 0xabcd…ef
   ```
   Sends 0 ETH to your own address with a bumped fee. The new tx
   takes the same nonce slot; the old one is dropped.

If both fail (`EVMC-007` / `EVMC-008`), wait for the original to
confirm or time out, then send fresh.

### "I sent ETH to the wrong address"

**No recovery path.** Ethereum transactions are irreversible. The
recipient controls the funds; you would need their cooperation to
return them. This is a feature of the protocol, not a bug.

**Prevention:** always run `send-eth` once with `--dry-run` to see
the summary (P0-9 mis-sign prevention). The CLI also displays the
recipient in EIP-55 mixed-case for human inspection.

### "RPC keeps timing out"

1. **Check the URL** — `evm-cli --rpc-url https://rpc.sepolia.org balance`
2. **Check the rate limit** — the project caps at 25 req/s
   (Infura free tier ceiling). If you hit it, you'll see HTTP 429.
3. **Try a different provider** — public Sepolia endpoints include
   `https://ethereum-sepolia-rpc.publicnode.com` and
   `https://1rpc.io/sepolia`.

If you have an Infura/Alchemy key, set it in the URL:
```bash
$ evm-cli --rpc-url https://sepolia.infura.io/v3/<YOUR_KEY> balance
```

### "Coverage gate fails in CI"

The P0-8 global coverage gate is currently at **61.46% lines** (per
CHANGELOG 0.2.0). This is **accepted for V1**; the anvil-driven
integration tests (`tests/it_eth_transfer.rs`, 3 tests) provide
behavioral coverage of `cli/commands.rs` (0% lines), `chain/alloy_chain.rs`
(57% lines), and `chain/rbf.rs` (45% lines) but don't count toward
`cargo llvm-cov` line coverage (different processes).

**Workarounds:**
- For local development: ignore the global gate, rely on the
  per-module gate (crypto/ ≥ 97%, keystore/ = 90.56%, both ≥ 90%).
- For CI: deferred to V2; the M6 audit documents this as a known
  V1 limitation. See [CHANGELOG 0.2.0](../CHANGELOG.md#020--2026-06-12).

### "I want to use a different chain"

Out of scope for V1 (PLAN-V9 §9 explicitly excludes mainnet). The
`--chain-id` flag is plumbed through but the startup validation
enforces `chain_id == 11155111` (Sepolia).

V2 candidates include mainnet and additional testnets.

### "I get `EVMK-001` (InvalidPassword) on a brand new install"

This is the **anti-side-channel** behavior from ADR-0009: the same
code is returned for both "wrong password" and "file missing" so an
attacker can't probe for valid aliases by error type.

**Diagnosis:** run `evm-cli list` — if your alias isn't there, the
file is missing. If it IS there, the password is wrong.

### "My terminal shows `^[[3J` or other escape codes"

That's the ANSI clear-screen sequence from [P0-7](./adr/0007-secret-memory.md):
after displaying a mnemonic, the CLI sends an escape to clear the
scrollback. Modern terminals (kitty, alacritty, iTerm2, gnome-terminal)
honor it. If yours doesn't, the mnemonic will be visible in scrollback
for one page-up.

**Mitigation:** use `--no-history` (also disables shell history) and
consider `script(1)`-equivalent screen recorders that capture the
raw stream.

## How to file a useful issue

1. Run the failing command with `RUST_LOG=evm_cli=debug` and capture
   the full output (stderr).
2. With `--json`, the failure is a structured `{ok: false, code, message, cause}` body.
3. The `code` field is stable; quote it in the issue.
4. Include the `evm-cli --version` output (currently 0.2.1).
