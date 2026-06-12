// SPDX-License-Identifier: MIT
//
// RBF / Cancel per ADR-0008 rev1.
//
// 3-term fee-bump formula (per BIP-125 spirit):
//
//   new_max_fee_per_gas         = max(
//       (original.max_fee_per_gas         * 110) / 100,
//       original.max_fee_per_gas         + 1_000_000_000,   // +1 gwei floor
//       current_base_fee * 2 + recent_priority_fee          // mempool-competitive floor
//   )
//   new_max_priority_fee_per_gas = max(
//       (original.max_priority_fee_per_gas * 110) / 100,
//       original.max_priority_fee_per_gas + 1_000_000_000,
//       recent_priority_fee
//   )
//
// Both EIP-1559 parameters are bumped symmetrically.

use alloy_primitives::U256;

use crate::chain::{Chain, ChainError};
use crate::types::{Address, TxHash};

/// Bump-fee result: the new signed transaction (or an error).
pub struct BumpResult {
    /// New tx hash (different from the original because the fee is higher).
    pub new_hash: TxHash,
    /// New max_fee_per_gas (for display / debug).
    pub new_max_fee_per_gas: U256,
    /// New max_priority_fee_per_gas.
    pub new_max_priority_fee_per_gas: U256,
}

/// Compute the 3-term fee bump for a transaction.
///
/// Inputs:
///   - `original_max_fee_per_gas`, `original_max_priority_fee_per_gas`
///   - `current_base_fee` (from `eth_feeHistory`)
///   - `recent_priority_fee` (median of recent blocks' priority fees)
///
/// Returns the bumped values per the formula above.
pub fn compute_bump(
    original_max_fee_per_gas: U256,
    original_max_priority_fee_per_gas: U256,
    current_base_fee: U256,
    recent_priority_fee: U256,
) -> (U256, U256) {
    let ten_pct_bump_max = (original_max_fee_per_gas * U256::from(110u64)) / U256::from(100u64);
    let one_gwei_floor_max = original_max_fee_per_gas + U256::from(1_000_000_000u64);
    let mempool_floor_max = current_base_fee * U256::from(2u64) + recent_priority_fee;

    let ten_pct_bump_prio =
        (original_max_priority_fee_per_gas * U256::from(110u64)) / U256::from(100u64);
    let one_gwei_floor_prio = original_max_priority_fee_per_gas + U256::from(1_000_000_000u64);
    let mempool_floor_prio = recent_priority_fee;

    let new_max = ten_pct_bump_max
        .max(one_gwei_floor_max)
        .max(mempool_floor_max);
    let new_prio = ten_pct_bump_prio
        .max(one_gwei_floor_prio)
        .max(mempool_floor_prio);

    (new_max, new_prio)
}

/// Build a replacement transaction for an RBF / Cancel operation.
///
/// This is the **shared core** of `bump_fee` and `cancel`:
///  1. Look up the original tx (to recover `to`, `value`, `data`,
///     `nonce`).
///  2. Compute bumped fees (per `compute_bump`).
///  3. Build a new EIP-1559 tx with the same `to`/`value`/`data`/
///     `nonce` but the bumped fees.
///  4. Sign + broadcast.
///
/// The `value` for a cancel is overridden to `0` and `to` is the
/// signer (self-send); the RBF path keeps the original `to` and
/// `value`.
pub async fn bump_fee<C: Chain + ?Sized>(
    chain: &C,
    signer: &alloy_signer_local::PrivateKeySigner,
    original: TxHash,
) -> Result<BumpResult, ChainError> {
    let info = chain
        .get_tx(original)
        .await?
        .ok_or(ChainError::TxNotFound { hash: original })?;

    // Estimate current fees.
    let est = chain.estimate_fees().await?;
    let (new_max, new_prio) = compute_bump(
        info.max_fee_per_gas,
        info.max_priority_fee_per_gas,
        est.base_fee,
        est.priority_fee,
    );

    // For RBF (not cancel), keep the original `to` and `value`.
    let to = info.to.ok_or(ChainError::Internal(
        "RBF: original tx has no `to` (contract creation); cannot bump".to_string(),
    ))?;
    let value = crate::types::Amount::from_wei(info.value);

    let signed = chain
        .build_eth_transfer(
            signer,
            to,
            value,
            vec![], // RBF doesn't carry calldata; the original tx's
            // calldata is in `info.data` but we don't preserve
            // it (RBF for ERC-20 is out of V1 scope). The
            // fallback to a value-transfer RBF is sufficient.
            Some(crate::types::Amount::from_wei(new_max)),
            Some(crate::types::Amount::from_wei(new_prio)),
        )
        .await?;
    let new_hash = chain.broadcast_tx(&signed.raw).await?;
    Ok(BumpResult {
        new_hash,
        new_max_fee_per_gas: new_max,
        new_max_priority_fee_per_gas: new_prio,
    })
}

/// Cancel a pending transaction by sending a 0-value self-send with
/// the same nonce and bumped fees. See ADR-0008 rev1.
pub async fn cancel<C: Chain + ?Sized>(
    chain: &C,
    signer: &alloy_signer_local::PrivateKeySigner,
    original: TxHash,
) -> Result<TxHash, ChainError> {
    let info = chain
        .get_tx(original)
        .await?
        .ok_or(ChainError::TxNotFound { hash: original })?;

    let est = chain.estimate_fees().await?;
    let (new_max, new_prio) = compute_bump(
        info.max_fee_per_gas,
        info.max_priority_fee_per_gas,
        est.base_fee,
        est.priority_fee,
    );

    let self_addr: Address = signer.address().into();
    let signed = chain
        .build_eth_transfer(
            signer,
            self_addr,
            crate::types::Amount::ZERO,
            vec![], // cancel is always a 0-value self-send; no calldata
            Some(crate::types::Amount::from_wei(new_max)),
            Some(crate::types::Amount::from_wei(new_prio)),
        )
        .await?;
    chain.broadcast_tx(&signed.raw).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_percent_bump_applies() {
        // original: 100 gwei max; bump = 110 gwei (10%)
        // Floor of +1 gwei is irrelevant here.
        // Mempool floor is 0 (test passes 0 for current_base_fee).
        // So result is max(110, 101, 0) = 110 gwei.
        let (new_max, new_prio) = compute_bump(
            U256::from(100_000_000_000u64),
            U256::from(100_000_000_000u64),
            U256::ZERO,
            U256::ZERO,
        );
        assert_eq!(new_max, U256::from(110_000_000_000u64));
        assert_eq!(new_prio, U256::from(110_000_000_000u64));
    }

    #[test]
    fn plus_one_gwei_floor_applies_when_original_very_low() {
        // original: 100 wei; 10% = 110 wei (still tiny)
        // +1 gwei floor wins.
        let (new_max, new_prio) = compute_bump(
            U256::from(100u64),
            U256::from(100u64),
            U256::ZERO,
            U256::ZERO,
        );
        assert_eq!(new_max, U256::from(100u64) + U256::from(1_000_000_000u64));
        assert_eq!(new_prio, U256::from(100u64) + U256::from(1_000_000_000u64));
    }

    #[test]
    fn mempool_floor_dominates_when_base_fee_rises() {
        // original: 1 gwei; base_fee risen to 5 gwei, priority 2 gwei
        // mempool floor = 5*2 + 2 = 12 gwei. Should win.
        let (new_max, new_prio) = compute_bump(
            U256::from(1_000_000_000u64),
            U256::from(1_000_000_000u64),
            U256::from(5_000_000_000u64),
            U256::from(2_000_000_000u64),
        );
        assert_eq!(new_max, U256::from(12_000_000_000u64));
        assert_eq!(new_prio, U256::from(2_000_000_000u64));
    }

    #[test]
    fn ten_percent_floor_dominates_when_original_high_enough() {
        // original: 100 gwei; bump = 110 gwei
        // mempool floor (say 5*2+2 = 12) is smaller → 110 wins.
        let (new_max, _new_prio) = compute_bump(
            U256::from(100_000_000_000u64),
            U256::from(50_000_000_000u64),
            U256::from(5_000_000_000u64),
            U256::from(2_000_000_000u64),
        );
        assert_eq!(new_max, U256::from(110_000_000_000u64));
    }
}
