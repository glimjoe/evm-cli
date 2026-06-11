# evm-cli

> **PoC — do not use on mainnet with real assets.**
> Linux-only CLI wallet for Sepolia testnet.

`evm-cli` is a single-binary command-line EVM wallet targeting the Sepolia testnet. It supports BIP-39/BIP-44 HD wallets, EIP-1559 transactions, ERC-20 transfers, RBF/cancel, and standard CLI ergonomics (clap + rustyline REPL).

## Install

```bash
# From crates.io (when published)
cargo install evm-cli

# From git
cargo install --git https://github.com/<org>/evm-cli
```

## Commands

| Command | Description |
|---|---|
| `create-wallet` | Generate a new 12/24-word mnemonic |
| `import-mnemonic` | Import an existing mnemonic |
| `list` | List wallets in keystore |
| `use` | Switch active wallet |
| `unlock` | Decrypt keystore (provide password) |
| `balance` | Show ETH balance for active address |
| `send-eth` | Send ETH (with `--bump-fee` / `--cancel` / `--dry-run`) |
| `send-token` | Send ERC-20 |
| `sign-message` | EIP-191 `personal_sign` |
| `pending-tx` | List locally-known pending transactions |
| `exit` | Quit REPL |

## Security disclaimer

This is a **proof-of-concept** intended for the Sepolia testnet. Do not use it on mainnet with assets of real value. The codebase has not been audited by a third party. See [SECURITY.md](./SECURITY.md) for the vulnerability disclosure process.

## Documentation

- [PLAN-V8.md](./PLAN-V8.md) — implementation plan
- [docs/adr/](./docs/adr/) — Architecture Decision Records
- [docs/code_allocation.md](./docs/code_allocation.md) — error code registry

## License

MIT — see [LICENSE](./LICENSE).
