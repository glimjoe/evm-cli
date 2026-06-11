# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: (development) |
| < 0.1   | :x:                |

## Reporting a Vulnerability

**Please do not file public issues for security vulnerabilities.**

Send a private report to: `security@evm-cli.local` (placeholder; replace before public release).

Include:
- Description of the vulnerability
- Reproduction steps or proof-of-concept
- Affected versions
- Your assessment of impact

We will acknowledge receipt within 72 hours and provide an initial assessment within 7 days. Critical issues may receive out-of-band patches.

## Threat Model

`evm-cli` is a PoC CLI wallet for the Sepolia testnet. The threat model assumes:
- The host machine is **not** compromised (no malware pre-installed).
- The user keeps their keystore file with mode 0600.
- The user does not paste their mnemonic into untrusted terminals.

The threat model **does not** cover:
- Hardware keyloggers
- Cold-boot attacks
- Compromised RPC endpoints (use multiple, with caution)
- Supply-chain attacks on Rust toolchain or transitive crates (mitigated by `cargo audit` and pinned exact versions per ADR-0001)

## Out-of-Scope for V1

- Hardware wallet integration (Ledger, Trezor)
- Multi-signature wallets
- Encrypted-at-rest backups beyond local keystore
- EIP-1193 browser-style provider
- Network transport security beyond the host's TLS stack
