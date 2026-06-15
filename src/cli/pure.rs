// SPDX-License-Identifier: MIT
//
// `evm_cli::cli::pure` — pure functions extracted from `commands.rs`.
//
// Per PLAN-V10 §5 M7 (P0-8 Gap Closure): this module holds pure
// functions (no I/O, no async, no global state). Each function is a
// candidate for unit tests without anvil, keystore, or REPL setup.
//
// Companion to `commands.rs` which orchestrates these functions
// alongside side effects (broadcast, prompt, output).
//
// # What lives here
//
// - `format_gwei(wei)` — wei -> "N.NNNN" (4 fractional digits).
// - `format_eth(amount)` — wei -> "N.NNNNNN ETH" (6 fractional digits).
// - `format_send_summary(...)` — assemble the entire send-summary
//   block as a single `String` (header + body + footer). The impure
//   `commands::print_send_summary` calls this and writes the result
//   via the session's `OutputFormatter`.
//
// # What does NOT live here
//
// Anything that touches:
// - the filesystem (keystore load/save, history append)
// - the RPC chain (broadcast, fee estimation, receipt polling)
// - the REPL session (output formatter, prompt, confirm)
// - the alloy signer / wallet state
//
// Those remain in `commands.rs`.

use alloy_primitives::U256;

use crate::chain::FeeEstimate;
use crate::types::{Address, Amount, Nonce, TxHash};

/// Display-only constant: the maximum fee-per-gas cap that the
/// summary prints alongside the estimated fee. The actual chain
/// (Sepolia) has no hard cap; this is the Infura free-tier ceiling
/// used by the M4 polish summary (see docs/troubleshooting.md).
pub const FEE_CAP_GWEI_DISPLAY: &str = "30";

/// Format a wei value (U256) as a Gwei string with 4 fractional
/// digits, e.g. `1.5000` (the caller adds the " Gwei" suffix).
///
/// Algorithm:
///   whole = wei / 1e9
///   frac  = (wei % 1e9) / 1e5   (i.e. 0.0001-Gwei precision; 1e5 wei = 0.0001 Gwei)
///
/// Examples:
/// - 0                -> "0.0000"
/// - 1.5e9            -> "1.5000"
/// - 30e9             -> "30.0000"
/// - 1 wei            -> "0.0000"
/// - 999_999_999 wei  -> "9.9999"
pub fn format_gwei(wei: U256) -> String {
    use std::fmt::Write;
    // 1 Gwei = 1e9 wei
    let one_gwei = U256::from(1_000_000_000u64);
    let whole = wei / one_gwei;
    let frac_wei = wei % one_gwei;
    // frac_wei is at most 1e9 - 1. To get 4 fractional digits in
    // Gwei (precision 0.0001 Gwei), we divide frac_wei by 1e5
    // (because 1e5 wei = 0.0001 Gwei). The result is in [0, 10000).
    let one_ten_thousandth = U256::from(100_000u64); // 1e5 wei = 0.0001 Gwei
    let frac_units = frac_wei / one_ten_thousandth;
    let mut s = String::new();
    let _ = write!(s, "{whole}.");
    let mut frac_str = frac_units.to_string();
    while frac_str.len() < 4 {
        frac_str.insert(0, '0');
    }
    s.push_str(&frac_str);
    s
}

/// Format an `Amount` (wei) as a human-readable ETH string with 6
/// fractional digits, e.g. `1.000000 ETH`.
///
/// Algorithm:
///   whole = wei / 1e18
///   frac  = (wei % 1e18) / 1e12   (i.e. micro-ETH precision)
///
/// Examples:
/// - 0           -> "0.000000 ETH"
/// - 1 wei       -> "0.000000 ETH"
/// - 1e18 wei    -> "1.000000 ETH"
/// - 0.001 ETH   -> "0.001000 ETH"
/// - 1.5 ETH     -> "1.500000 ETH"
pub fn format_eth(amount: Amount) -> String {
    use std::fmt::Write;
    let wei = amount.as_wei();
    // wei / 10^18. We do long division with 6 fractional digits.
    let one_eth: u128 = 1_000_000_000_000_000_000;
    // Convert to U512 (avoids overflow on 1e18 * 1e6 = 1e24).
    let wei_u512: alloy_primitives::U512 = alloy_primitives::U512::from(*wei);
    let one_eth_u512 = alloy_primitives::U512::from(one_eth);
    let one_micro = alloy_primitives::U512::from(1_000_000_000_000u64); // 1e12

    let whole = wei_u512 / one_eth_u512;
    let frac_wei = wei_u512 % one_eth_u512;
    // frac_wei is at most 1e18 - 1. Multiply by 1e6 / 1e18 to get
    // micro-ETH. We use integer division; remainder is discarded
    // (caller is OK with 6-digit precision).
    let micros = frac_wei / one_micro;

    let mut s = String::new();
    let _ = write!(s, "{whole}.");
    // Pad micros to 6 digits.
    let mut frac_str = micros.to_string();
    while frac_str.len() < 6 {
        frac_str.insert(0, '0');
    }
    s.push_str(&frac_str);
    s.push_str(" ETH");
    s
}

/// Build the human-readable send-summary block as a single `String`.
/// Pure function: no I/O, no side effects.
///
/// Layout:
/// ```text
/// --- {label} ---
/// to:     {to}
/// amount: {eth_str} ({wei_str} wei)
/// fee:    {max_fee_gwei_str} Gwei (cap 30 Gwei)
/// total:  {total_str}
/// nonce:  {nonce}
/// hash:   {tx_hash}
/// --- end summary ---
/// ```
///
/// `gas` is the gas limit the builder will use. For ETH transfers
/// this is 21_000; for ERC-20 token transfers via the V1 `value=0`
/// envelope it is 0 (so `total == amount` and `fee` shows 0 Gwei).
///
/// `fee_estimate.max_fee_per_gas` is used for both the displayed fee
/// line and the worst-case total calculation.
pub fn format_send_summary(
    to: Address,
    amount: Amount,
    fee_estimate: &FeeEstimate,
    nonce: Nonce,
    tx_hash: TxHash,
    gas: u64,
    label: &str,
) -> String {
    let wei = amount.as_wei();
    let eth_str = format_eth(amount);
    let wei_str = wei.to_string();

    // Format max_fee_per_gas in Gwei (1 Gwei = 1e9 wei). For Sepolia
    // this is the only display unit; for token sends the gas is 0,
    // so fee=0 Gwei in the summary (real fee paid is in the token
    // amount, not in ETH).
    let max_fee_gwei_str = format_gwei(fee_estimate.max_fee_per_gas);

    // Worst-case total: amount + (gas * max_fee). For token sends,
    // gas=0 means total==amount. For ETH sends, gas=21000.
    let total_wei: U256 = *wei + U256::from(gas) * fee_estimate.max_fee_per_gas;
    let total_amount = Amount::from_wei(total_wei);
    let total_str = format_eth(total_amount);

    let mut s = String::new();
    use std::fmt::Write;
    let _ = writeln!(s, "--- {label} ---");
    let _ = writeln!(s, "to:     {to}");
    let _ = writeln!(s, "amount: {eth_str} ({wei_str} wei)");
    let _ = writeln!(
        s,
        "fee:    {max_fee_gwei_str} Gwei (cap {FEE_CAP_GWEI_DISPLAY} Gwei)"
    );
    let _ = writeln!(s, "total:  {total_str}");
    let _ = writeln!(s, "nonce:  {nonce}");
    let _ = writeln!(s, "hash:   {tx_hash}");
    s.push_str("--- end summary ---");
    s
}

// =====================================================================
// M7 cycle 2: more pure extractions from `commands.rs`.
// =====================================================================

/// Parse a 32-byte transaction hash from a hex string. Accepts both
/// `0x`-prefixed and bare hex. Returns the raw 32 bytes on success.
///
/// # Errors
///
/// Returns an error string when:
/// - the input is not valid hex
/// - the byte length (after `0x` strip) is not exactly 32
///
/// The original V1 behavior in `commands::parse_tx_hash` returned a
/// `CliError::from(ChainError::Internal(...))`. This pure function
/// returns a `String` (the error message) instead — the orchestrator
/// is responsible for wrapping it into the appropriate error type.
/// Keeping this layer pure is the whole point of the extraction.
pub fn parse_tx_hash(s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| format!("invalid tx hash: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("tx hash must be 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Parse a yes/no answer from a user prompt. Returns `true` only for
/// `"y"` or `"yes"` (case-insensitive, whitespace-trimmed). All other
/// inputs — including empty string, `"n"`, `"no"`, `"maybe"`, and
/// arbitrary gibberish — return `false`. Default is **N** (matches
/// V1's `confirm` behavior).
///
/// Pure: no I/O. The orchestrator (`commands::confirm`) handles the
/// prompt + read; this function only decides the answer.
pub fn is_yes_answer(s: &str) -> bool {
    let answer = s.trim().to_lowercase();
    answer == "y" || answer == "yes"
}

#[cfg(test)]
mod tests {
    //! M7 TDD — RED phase: these tests exercise the public API of
    //! `pure` and MUST fail at this commit (every body is
    //! `unimplemented!`). They turn GREEN after the next commit.
    //!
    //! Coverage targets (per PLAN-V10 §5 M7 DoD):
    //! - ≥ 10 tests against `format_send_summary`
    //! - boundary, edge case, error path on every public function
    //! - 100% coverage of `format_gwei` and `format_eth` (pure, easy)

    use super::*;
    use alloy_primitives::{B256, U256};

    // -----------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------

    /// Build a `FeeEstimate` with the given `max_fee_per_gas` and
    /// sensible defaults for the other fields. The other fields are
    /// not displayed by `format_send_summary` so defaults are fine.
    fn fee(max_fee_gwei_whole: u64) -> FeeEstimate {
        let one_gwei = U256::from(1_000_000_000u64);
        let max_fee = U256::from(max_fee_gwei_whole) * one_gwei;
        FeeEstimate {
            base_fee: one_gwei,
            priority_fee: U256::from(1u64) * one_gwei,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: U256::from(1u64) * one_gwei,
        }
    }

    fn addr_recipient() -> Address {
        // Vitalik's well-known address (EIP-55 mixed-case).
        Address::parse("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045").expect("parse vitalik")
    }

    fn tx_hash_demo() -> TxHash {
        TxHash::from_b256(B256::repeat_byte(0xab))
    }

    // =================================================================
    // format_gwei
    // =================================================================

    #[test]
    fn format_gwei_zero() {
        assert_eq!(format_gwei(U256::ZERO), "0.0000");
    }

    #[test]
    fn format_gwei_one_wei_is_zero_gwei() {
        // Sub-Gwei precision rounds to 0.0000 at 4 fractional digits.
        assert_eq!(format_gwei(U256::from(1u64)), "0.0000");
    }

    #[test]
    fn format_gwei_exactly_one_gwei() {
        assert_eq!(format_gwei(U256::from(1_000_000_000u64)), "1.0000");
    }

    #[test]
    fn format_gwei_with_fractional() {
        // 1.5 Gwei = 1_500_000_000 wei.
        assert_eq!(format_gwei(U256::from(1_500_000_000u64)), "1.5000");
    }

    #[test]
    fn format_gwei_thirty_gwei_no_overflow() {
        // The display cap (Infura free-tier).
        assert_eq!(format_gwei(U256::from(30_000_000_000u64)), "30.0000");
    }

    #[test]
    fn format_gwei_thousand_gwei_large_value() {
        assert_eq!(format_gwei(U256::from(1_000_000_000_000u64)), "1000.0000");
    }

    // =================================================================
    // format_eth
    // =================================================================

    #[test]
    fn format_eth_zero() {
        assert_eq!(format_eth(Amount::ZERO), "0.000000 ETH");
    }

    #[test]
    fn format_eth_one_wei_is_zero_eth() {
        // Sub-micro-ETH precision rounds to 0.000000.
        assert_eq!(
            format_eth(Amount::from_wei(U256::from(1u64))),
            "0.000000 ETH"
        );
    }

    #[test]
    fn format_eth_one_eth() {
        assert_eq!(
            format_eth(Amount::from_wei(U256::from(1_000_000_000_000_000_000u128))),
            "1.000000 ETH"
        );
    }

    #[test]
    fn format_eth_milli_eth() {
        // 0.001 ETH = 1e15 wei.
        assert_eq!(
            format_eth(Amount::from_wei(U256::from(1_000_000_000_000_000u64))),
            "0.001000 ETH"
        );
    }

    #[test]
    fn format_eth_fractional_eth() {
        // 1.5 ETH = 1.5e18 wei.
        assert_eq!(
            format_eth(Amount::from_wei(U256::from(1_500_000_000_000_000_000u128))),
            "1.500000 ETH"
        );
    }

    #[test]
    fn format_eth_thousand_eth_large_value() {
        // 1000 ETH = 1e21 wei.
        assert_eq!(
            format_eth(Amount::from_wei(U256::from(
                1_000_000_000_000_000_000_000u128
            ))),
            "1000.000000 ETH"
        );
    }

    // =================================================================
    // format_send_summary
    // =================================================================

    #[test]
    fn summary_eth_transfer_one_eth() {
        // Happy path: 1 ETH at 1 Gwei, gas 21_000, nonce 0.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000_000u128)),
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        // All required fields are present.
        assert!(
            summary.contains("--- ETH transfer ---"),
            "label in header: {summary}"
        );
        assert!(
            summary.contains("--- end summary ---"),
            "footer present: {summary}"
        );
        assert!(
            summary.contains("amount: 1.000000 ETH"),
            "amount line: {summary}"
        );
        assert!(
            summary.contains("fee:    1.0000 Gwei"),
            "fee line: {summary}"
        );
        assert!(summary.contains("(cap 30 Gwei)"), "fee cap: {summary}");
        assert!(summary.contains("nonce:  0"), "nonce line: {summary}");
        assert!(summary.contains("hash:   "), "hash line: {summary}");
        assert!(summary.contains("to:     "), "to line: {summary}");
    }

    #[test]
    fn summary_eth_transfer_small_amount() {
        // 0.001 ETH at 1 Gwei.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000u64)),
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("amount: 0.001000 ETH"));
        assert!(summary.contains("fee:    1.0000 Gwei"));
    }

    #[test]
    fn summary_eth_transfer_zero_amount() {
        // Zero amount is allowed (e.g. contract call with no value).
        let summary = format_send_summary(
            addr_recipient(),
            Amount::ZERO,
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("amount: 0.000000 ETH"));
        // Total is amount + gas*fee = 0 + 21000*1e9 wei = 21000 Gwei
        // = 0.000021 ETH.
        assert!(summary.contains("total:  0.000021 ETH"));
    }

    #[test]
    fn summary_token_transfer_gas_zero() {
        // V1 ERC-20 envelope: gas=0. The `fee` line shows the
        // *rate* (max_fee_per_gas), NOT the cost. The cost shows
        // up in `total` (gas=0 means total==amount). The rate
        // line is unchanged from an ETH send with the same fee
        // estimate — it's a property of the estimate, not the call.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000_000u128)),
            &fee(30),
            Nonce(7),
            tx_hash_demo(),
            0,
            "Token transfer",
        );
        assert!(summary.contains("--- Token transfer ---"));
        // The RATE (max_fee_per_gas) is displayed, not the cost.
        assert!(
            summary.contains("fee:    30.0000 Gwei"),
            "rate displayed: {summary}"
        );
        // total must equal amount since gas=0.
        assert!(
            summary.contains("total:  1.000000 ETH"),
            "total==amount: {summary}"
        );
        assert!(summary.contains("nonce:  7"));
    }

    #[test]
    fn summary_high_fee_above_cap_still_displayed() {
        // The summary shows the *estimated* fee even if it exceeds
        // the Infura free-tier cap; the cap is informational only.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000_000u128)),
            &fee(50),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("fee:    50.0000 Gwei"));
        // Total = 1 ETH + 21_000 * 50 Gwei
        //       = 1e18 + 21_000 * 50e9 wei
        //       = 1e18 + 1.05e15 wei
        //       = 1.00105 ETH
        assert!(
            summary.contains("total:  1.001050 ETH"),
            "expected total 1.001050 ETH, got: {summary}"
        );
    }

    #[test]
    fn summary_low_fee_one_gwei() {
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000_000u128)),
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("fee:    1.0000 Gwei"));
        // Total = 1 ETH + 21_000 * 1 Gwei = 1 ETH + 0.000021 ETH.
        assert!(summary.contains("total:  1.000021 ETH"));
    }

    #[test]
    fn summary_nonce_zero() {
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1u64)),
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("nonce:  0"));
    }

    #[test]
    fn summary_large_nonce() {
        // Mid-range nonce; format is decimal.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1u64)),
            &fee(1),
            Nonce(123_456_789),
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains("nonce:  123456789"));
    }

    #[test]
    fn summary_max_nonce() {
        // u64::MAX boundary.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1u64)),
            &fee(1),
            Nonce(u64::MAX),
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(summary.contains(&format!("nonce:  {}", u64::MAX)));
    }

    #[test]
    fn summary_total_includes_amount_and_gas_cost() {
        // Mathematical sanity check: total == amount + gas * max_fee.
        // 1 ETH, 21_000 gas, 2 Gwei.
        // gas_cost = 21_000 * 2e9 = 42_000e9 wei = 0.000042 ETH.
        // total    = 1.000042 ETH.
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1_000_000_000_000_000_000u128)),
            &fee(2),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        assert!(
            summary.contains("total:  1.000042 ETH"),
            "expected 1.000042 ETH, got: {summary}"
        );
    }

    #[test]
    fn summary_custom_label_appears_in_header() {
        let summary = format_send_summary(
            addr_recipient(),
            Amount::ZERO,
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            0,
            "ETH transfer (dry-run)",
        );
        assert!(summary.contains("--- ETH transfer (dry-run) ---"));
        assert!(summary.contains("amount: 0.000000 ETH"));
        // The rate (max_fee_per_gas = 1 Gwei) is displayed even
        // when gas=0 — the fee line is about the estimate, not the
        // cost. The cost shows in `total` (here == amount because
        // gas=0).
        assert!(summary.contains("fee:    1.0000 Gwei"));
        assert!(summary.contains("total:  0.000000 ETH"));
    }

    #[test]
    fn summary_recipient_address_appears() {
        let recipient = addr_recipient();
        let summary = format_send_summary(
            recipient,
            Amount::from_wei(U256::from(1u64)),
            &fee(1),
            Nonce::ZERO,
            tx_hash_demo(),
            21_000,
            "ETH transfer",
        );
        // The recipient's EIP-55 display form appears after `to:     `.
        let expected_to_line = format!("to:     {}", recipient);
        assert!(
            summary.contains(&expected_to_line),
            "expected `{expected_to_line}` in:\n{summary}"
        );
    }

    #[test]
    fn summary_tx_hash_appears() {
        let hash = tx_hash_demo();
        let summary = format_send_summary(
            addr_recipient(),
            Amount::from_wei(U256::from(1u64)),
            &fee(1),
            Nonce::ZERO,
            hash,
            21_000,
            "ETH transfer",
        );
        let expected_hash_line = format!("hash:   {hash}");
        assert!(
            summary.contains(&expected_hash_line),
            "expected `{expected_hash_line}` in:\n{summary}"
        );
    }

    // =================================================================
    // parse_tx_hash
    // =================================================================

    /// The canonical EIP-155 tx-hash test vector: a real-looking
    /// 32-byte hash. Used as the happy-path input below.
    fn sample_hash_hex() -> &'static str {
        "0xabababababababababababababababababababababababababababababababab"
    }

    fn sample_hash_bytes() -> [u8; 32] {
        [0xab; 32]
    }

    #[test]
    fn parse_tx_hash_with_0x_prefix() {
        let bytes = parse_tx_hash(sample_hash_hex()).expect("valid hash");
        assert_eq!(bytes, sample_hash_bytes());
    }

    #[test]
    fn parse_tx_hash_without_0x_prefix() {
        let stripped = &sample_hash_hex()[2..];
        let bytes = parse_tx_hash(stripped).expect("valid hash without prefix");
        assert_eq!(bytes, sample_hash_bytes());
    }

    #[test]
    fn parse_tx_hash_mixed_case_hex() {
        // Mixed-case hex must be accepted (it's valid hex).
        let mixed = "0xAbAbABabababababababababababababababababababababababababababABab";
        let bytes = parse_tx_hash(mixed).expect("mixed case valid");
        assert_eq!(bytes, sample_hash_bytes());
    }

    #[test]
    fn parse_tx_hash_uppercase_hex() {
        let upper = "0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
        let bytes = parse_tx_hash(upper).expect("uppercase valid");
        assert_eq!(bytes, [0xff; 32]);
    }

    #[test]
    fn parse_tx_hash_zero_hash() {
        let bytes =
            parse_tx_hash("0x0000000000000000000000000000000000000000000000000000000000000000")
                .expect("zero hash valid");
        assert_eq!(bytes, [0u8; 32]);
    }

    #[test]
    fn parse_tx_hash_rejects_garbage() {
        // Non-hex characters.
        let err = parse_tx_hash("not-hex-at-all").expect_err("garbage must fail");
        // We don't pin the exact message (it's a UX choice), but
        // it must mention the word "invalid" or "hex" so the user
        // sees what went wrong.
        assert!(
            err.to_lowercase().contains("invalid") || err.to_lowercase().contains("hex"),
            "error should mention `invalid` or `hex`: {err}"
        );
    }

    #[test]
    fn parse_tx_hash_rejects_short() {
        // Only 4 bytes.
        let err = parse_tx_hash("0xdeadbeef").expect_err("short must fail");
        assert!(
            err.contains("32") || err.to_lowercase().contains("length"),
            "error should mention length 32: {err}"
        );
    }

    #[test]
    fn parse_tx_hash_rejects_long() {
        // 33 bytes.
        let too_long = "0xababababababababababababababababababababababababababababababababab";
        let err = parse_tx_hash(too_long).expect_err("long must fail");
        assert!(
            err.contains("32") || err.to_lowercase().contains("length"),
            "error should mention length 32: {err}"
        );
    }

    #[test]
    fn parse_tx_hash_rejects_empty_string() {
        let err = parse_tx_hash("").expect_err("empty must fail");
        // Don't pin the message; just ensure we get *some* error.
        let _ = err;
    }

    // =================================================================
    // is_yes_answer
    // =================================================================

    #[test]
    fn is_yes_lowercase_y() {
        assert!(is_yes_answer("y"));
    }

    #[test]
    fn is_yes_uppercase_y() {
        assert!(is_yes_answer("Y"));
    }

    #[test]
    fn is_yes_lowercase_yes() {
        assert!(is_yes_answer("yes"));
    }

    #[test]
    fn is_yes_uppercase_yes() {
        assert!(is_yes_answer("YES"));
    }

    #[test]
    fn is_yes_mixed_case_yes() {
        assert!(is_yes_answer("YeS"));
    }

    #[test]
    fn is_yes_with_surrounding_whitespace() {
        // The orchestrator trims, but the pure function should be
        // robust to accidental whitespace inside the input.
        assert!(is_yes_answer("  y  "));
        assert!(is_yes_answer("\tyes\n"));
    }

    #[test]
    fn is_no_lowercase_n() {
        assert!(!is_yes_answer("n"));
    }

    #[test]
    fn is_no_explicit_no() {
        assert!(!is_yes_answer("no"));
    }

    #[test]
    fn is_no_empty_string() {
        // Default is N — empty input means "no".
        assert!(!is_yes_answer(""));
    }

    #[test]
    fn is_no_garbage() {
        // Anything that isn't y/yes (case-insensitive) is treated as no.
        assert!(!is_yes_answer("maybe"));
        assert!(!is_yes_answer("sure"));
        assert!(!is_yes_answer("yikes")); // starts with y but != "y" or "yes"
    }

    #[test]
    fn is_no_yes_with_trailing_chars() {
        // "yesterday" starts with "y" but isn't exactly "y" or "yes".
        assert!(!is_yes_answer("yesterday"));
    }
}
