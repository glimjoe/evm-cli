# Command Manual

> Per PLAN-V9 §5 M6 DoD. Comprehensive reference for all 11 CLI commands
> (`src/cli/mod.rs::Command`). Each section documents the command's
> purpose, arguments, examples, and known failure modes. For the high-
> level flow and architecture, see [docs/architecture.md](./architecture.md).
> For error-code lookup, see [docs/troubleshooting.md](./troubleshooting.md).

## Global flags

All commands (CLI and REPL) honor:

| Flag | Env var | Default | Effect |
|---|---|---|---|
| `--json` | `EVMCLI_JSON=true` | `false` | Emit NDJSON output (`{"ok": true, "data": …}` / `{"ok": false, "code": …, "message": …, "cause": […]}`) instead of human-readable text |
| `--no-history` | `EVMCLI_NO_HISTORY=true` | `false` | Disable rustyline history file (defense-in-depth; sensitive commands are also auto-skipped, see [P0-9](./adr/0009-eth-keystore-deviation.md)) |
| `--rpc-url <URL>` | `EVMCLI_RPC_URL` | `https://rpc.sepolia.org` | Sepolia JSON-RPC endpoint |
| `--keystore-dir <DIR>` | `EVMCLI_KEYSTORE_DIR` | `~/.config/evm-cli/keystore` | Keystore directory |
| `--data-dir <DIR>` | `EVMCLI_DATA_DIR` | `~/.local/share/evm-cli` | NonceManager + state dir |
| `--chain-id <N>` | `EVMCLI_CHAIN_ID` | `11155111` (Sepolia) | Network ID this client is bound to (PLAN-V9 §1 mandates Sepolia) |
| `--rpc-timeout <SECS>` | `EVMCLI_RPC_TIMEOUT` | `30` | Per-request timeout |

12-factor precedence: **CLI flag > env var > TOML config > default**.
Optional TOML at `~/.config/evm-cli/config.toml`.

## `create-wallet`

Generate a new 12-word BIP-39 mnemonic and encrypt it to a keystore file.

**Args:** `ALIAS` (filename; e.g. `alice` → `~/.config/evm-cli/keystore/alice`).

```bash
$ evm-cli create-wallet alice
generated mnemonic (12 words):
abandon … about
wallet saved to /home/yangwei/.config/evm-cli/keystore/alice
address: 0x9858EfFD232B4033E47d90003D41EC34EcaEda94
```

**Side effects:** creates the keystore file (mode 0600, see [P0-7]). The
plaintext mnemonic is shown ONCE on stdout and then cleared with an
ANSI escape sequence (see [P0-7](./adr/0007-secret-memory.md)).

**Failure modes:**
- `EVMK-010` (`AliasExists`) — alias already in use.
- `EVMIO-001` (`PermissionDenied`) — `keystore-dir` not writable.

## `import-mnemonic`

Import an existing BIP-39 mnemonic (12 or 24 words) and save to keystore.

**Args:** `ALIAS`, `PHRASE` (12 or 24 space-separated words).

```bash
$ evm-cli import-mnemonic alice "abandon abandon … about"
imported → /home/yangwei/.config/evm-cli/keystore/alice
address: 0x9858EfFD232B4033E47d90003D41EC34EcaEda94
```

**Side effects:** same as `create-wallet`. The phrase argument value is
**not** logged or added to shell history (auto-skipped by
[`should_skip_history`](./../src/cli/history.rs)).

**Failure modes:**
- `EVMCR-001` (`InvalidMnemonic`) — checksum/length/wordlist fail.
- `EVMK-010` (`AliasExists`) — alias already in use.
- `EVMK-012` (`Internal`) — imported words empty (string returned as `"imported words must not be empty"`).

## `list`

List all wallets in the keystore directory.

```bash
$ evm-cli list
[active] * alice    0x9858EfFD…d94
         bob      0x1234…cdef
```

The `*` marks the active wallet (set with `use`).

**Failure modes:** none expected; the command is read-only.

## `use`

Set the active wallet (subsequent commands operate on this one).

**Args:** `ALIAS`.

```bash
$ evm-cli use bob
active: bob (0x1234…cdef)
```

**Failure modes:**
- `EVMK-009` (`AliasNotFound`) — alias not in keystore.

## `unlock`

Decrypt the active wallet's keystore and cache the signer in memory.
Required before `send-eth` / `send-token` / `sign-message`.

```bash
$ evm-cli unlock
Password: ********
unlocked alice (60s timeout)
```

The decrypted signer is held in a `Secret<PrivateKeySigner>` with
explicit `Drop` zeroization (see [ADR-0007](./adr/0007-secret-memory.md)).
The 60-second idle timeout re-zeros automatically.

**Failure modes:**
- `EVMK-001` (`InvalidPassword`) — wrong password **or** file missing
  (anti-side-channel, returns the same code for both).

## `balance`

Show the ETH balance of an address (defaults to active wallet).

**Args:** `ADDRESS` (optional; default = active wallet).

```bash
$ evm-cli balance
0x9858EfFD…d94: 0.5 ETH (500000000000000000 wei)

$ evm-cli balance 0x1234…cdef
0x1234…cdef: 0 ETH
```

**Failure modes:**
- `EVMC-001` (`Rpc`) — RPC unreachable / timeout / 429.
- `EVMCFG-002` (`InvalidValue`) — `ADDRESS` not a valid hex.

## `send-eth`

Send ETH (with optional RBF bump or cancel).

**Args:**
- `TO` — destination address (EIP-55 mixed-case or all-lowercase).
- `AMOUNT` — ETH amount (e.g. `0.001`). Ignored with `--bump-fee` / `--cancel`.
- `--bump-fee <TX_HASH>` — RBF: replace an existing pending tx with bumped fees.
- `--cancel <TX_HASH>` — cancel: send 0-value self-send with bumped fees.
- `--dry-run` — print the summary only; **do not** sign or broadcast (P0-9).

```bash
$ evm-cli send-eth 0xfeed… 0.001
to:     0xfeed…
amount: 0.001 ETH (1.0e15 wei)
fee:    1.5 Gwei (cap 30 Gwei)
total:  0.0010015 ETH
nonce:  42
Proceed? [y/N] y
tx sent: 0xabcd…ef
```

**Side effects:** requires `unlock` first; writes to `NonceManager`
JSONL log; increments pending-tx set.

**Failure modes:**
- `EVMK-001` (`InvalidPassword`) — wallet not unlocked.
- `EVMC-004` (`InvalidAmount`) — AMOUNT unparseable / negative.
- `EVMC-005` (`InvalidChainId`) — config chain_id ≠ Sepolia.
- `EVMC-007` (`TxNotFound`) — `--bump-fee`/`--cancel` hash not in mempool.
- `EVMC-008` (`TxAlreadyMined`) — `--bump-fee`/`--cancel` already mined.
- `EVMC-009` (`InsufficientFunds`) — balance < value + max fee.
- `EVMC-099` (`ReceiptTimeout`) — 120s receipt poll timed out.

## `send-token`

Send an ERC-20 token transfer.

**Args:**
- `TOKEN` — token contract address.
- `TO` — destination.
- `AMOUNT` — token amount in human units (e.g. `1.5`).
- `--decimals <N>` — token decimals (e.g. `6` for USDC, `18` for DAI).
- `--dry-run` — print the summary only.

```bash
$ evm-cli send-token 0xUSDC… 0xfeed… 100 --decimals 6
token:  0xUSDC…  (USDC, decimals 6)
to:     0xfeed…
amount: 100 USDC (100000000 raw)
fee:    1.5 Gwei
total:  0.00015 ETH
Proceed? [y/N] y
tx sent: 0xabcd…ef
```

**Failure modes:** Same as `send-eth` plus:
- `EVMC-004` (`InvalidAmount`) — bad `AMOUNT` / `DECIMALS`.

## `sign-message`

EIP-191 `personal_sign` the given message with the active wallet.
Returns the signature as a 0x-prefixed hex string.

**Args:** `MESSAGE` (free-form string).

```bash
$ evm-cli sign-message "Hello, world!"
0xsignature…
address: 0x9858EfFD…d94
```

**Side effects:** the message is shown on stdout (verbatim) and the
signature is returned. The signer is **not** zeroized (it remains
unlocked for the 60s timeout).

**Failure modes:**
- `EVMK-001` (`InvalidPassword`) — wallet not unlocked.
- `EVMCR-003` (`InvalidSignature`) — internal signing inconsistency.

## `pending-tx`

List locally-known pending transactions (those the NonceManager has
emitted but not yet observed mined).

```bash
$ evm-cli pending-tx
42  0xabcd…ef  to: 0xfeed…  amount: 0.001 ETH  age: 12s
43  0xbeef…01  to: 0x1234…  amount: 0 ETH     age:  3s  (cancel)
```

**Failure modes:** none expected; in-memory query.

## `exit`

Quit the REPL. In one-shot CLI mode (`evm-cli <command>`), this is a
no-op equivalent to process exit.

```bash
$ evm-cli exit
bye
```

**Failure modes:** none.

## Exit codes (binary, not REPL)

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Generic error (see `--json` output for `code` field) |
| `2` | Usage error (clap parse failure) |

When `--json` is set, the exit code is **always 1** for any error and
the structured `{ok: false, code, message, cause}` body identifies the
specific failure. See [docs/code_allocation.md](./code_allocation.md)
for the full code registry.
