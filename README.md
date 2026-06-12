# evm-cli

> **PoC — do not use on mainnet with real assets.**
> Linux-only CLI wallet for Sepolia testnet.

[![CI](https://github.com/glimjoe/evm-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/glimjoe/evm-cli/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)
[![MSRV 1.96](https://img.shields.io/badge/MSRV-1.96-orange.svg)](https://blog.rust-lang.org/)
[![M0–M3](https://img.shields.io/badge/milestone-M0–M3-blueviolet.svg)](./CHANGELOG.md)

`evm-cli` is a single-binary command-line EVM wallet targeting the Sepolia testnet. It supports BIP-39/BIP-44 HD wallets, EIP-1559 transactions, ERC-20 transfers, RBF/cancel, and standard CLI ergonomics (clap + rustyline REPL).

## Install

```bash
# From git
cargo install --git https://github.com/glimjoe/evm-cli
```

(Not yet published to crates.io; V1.x series is the Sepolia PoC line.)

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

### Known V1 security limitations

Per [ADR-0009](./docs/adr/0009-eth-keystore-deviation.md):

- **KDF cost**: scrypt at N=2¹³ (8192) is **16× below** the OWASP 2024
  baseline of N=2¹⁷. A determined attacker with the keystore file and
  modern hardware can attempt ~10 passwords/sec against the V1
  parameters, vs. <1/sec against Argon2id m=65536. V2 should bump to
  N=2¹⁷ or migrate to Argon2id.
- **Cipher**: AES-128-CTR + Keccak-256 MAC (encrypt-then-MAC), not
  AES-256-GCM AEAD. This is the standard Ethereum keystore format
  (EIP-1081) used by `geth` and is verified on every decrypt.

Both limitations are accepted for V1 and recorded in ADR-0009 with
the rationale and V2 follow-up.

## Documentation

- [PLAN-V9.md](../PLAN-V9.md) — implementation plan
- [docs/adr/](./docs/adr/) — Architecture Decision Records
- [docs/code_allocation.md](./docs/code_allocation.md) — error code registry

## License

MIT — see [LICENSE](./LICENSE).
