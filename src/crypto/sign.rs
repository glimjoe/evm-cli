// SPDX-License-Identifier: MIT
//
// Signing: EIP-2 low-S check, EIP-191 personal_sign, ecrecover roundtrip.
//
// PLAN-V9 §5 M1 DoD:
//   - EIP-2 low-S signatures
//   - `personal_sign` with `\x19Ethereum Signed Message:\n` prefix
//   - Ecrecover roundtrip integration test
//   - proptest: empty / 1-byte / 1 MiB / non-UTF-8 / hex string

use alloy_primitives::{Address, Signature, B256, U256};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use thiserror::Error;

use crate::crypto::keccak::keccak256;

/// secp256k1 curve order divided by 2 — the EIP-2 "low S" threshold.
/// Any valid signature has S in the lower half of the curve order; the
/// upper half is "high S" and is malleable (a different valid signature
/// of the same message+key). EIP-2 requires all signers to use the
/// low-S form so that a message has exactly one valid signature.
const SECP256K1_HALF_ORDER: [u8; 32] = [
    0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D, 0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B, 0x20, 0xA0,
];

/// Errors from signing operations.
#[derive(Debug, Error)]
pub enum SignError {
    #[error("signature is high-S (EIP-2 violation)")]
    HighS,
    #[error("alloy signer error: {0}")]
    Alloy(String),
    #[error("ecrecover failed: {0}")]
    Recover(String),
}

/// Check whether a signature is in EIP-2 low-S form.
///
/// Returns `true` if `s <= SECP256K1_HALF_ORDER`, `false` if it is in
/// the upper half. Note: signatures with s == 0 are not strictly
/// valid (degenerate), so this function also returns `false` for s=0.
pub fn is_low_s(sig: &Signature) -> bool {
    let s: U256 = sig.s();
    if s.is_zero() {
        return false;
    }
    let half = U256::from_be_bytes(SECP256K1_HALF_ORDER);
    s <= half
}

/// Sign a message with EIP-191 `personal_sign` semantics.
///
/// The signing is delegated to `alloy::signers::Signer::sign_message`,
/// which produces the EIP-191 prefixed hash internally:
///   `keccak256(b"\x19Ethereum Signed Message:\n" + len(msg) + msg)`
///
/// The returned `Signature` is guaranteed to be in EIP-2 low-S form
/// (alloy's k256 signer enforces this; we verify in tests).
pub async fn personal_sign(
    signer: &PrivateKeySigner,
    message: &[u8],
) -> Result<Signature, SignError> {
    let sig = signer
        .sign_message(message)
        .await
        .map_err(|e| SignError::Alloy(e.to_string()))?;
    if !is_low_s(&sig) {
        return Err(SignError::HighS);
    }
    Ok(sig)
}

/// Sign a 32-byte prehashed digest (no EIP-191 prefix). Caller is
/// responsible for the hash. Used internally and in tests.
#[allow(dead_code)] // exposed for M2+ keystore; currently used only in tests
pub async fn sign_hash(signer: &PrivateKeySigner, hash: &B256) -> Result<Signature, SignError> {
    let sig = signer
        .sign_hash(hash)
        .await
        .map_err(|e| SignError::Alloy(e.to_string()))?;
    if !is_low_s(&sig) {
        return Err(SignError::HighS);
    }
    Ok(sig)
}

/// Recover the signing address from a signature over a message.
///
/// This is the inverse of `personal_sign` — given the same message and
/// signature, returns the address that produced the signature. Used
/// by the ecrecover roundtrip test.
pub fn ecrecover(sig: &Signature, message: &[u8]) -> Result<Address, SignError> {
    sig.recover_address_from_msg(message)
        .map_err(|e| SignError::Recover(e.to_string()))
}

/// Raw Keccak-256 of `message` (no prefix). Exposed for tests and for
/// callers that need to verify hashes without EIP-191.
#[allow(dead_code)]
pub fn hash_message_raw(message: &[u8]) -> B256 {
    B256::from(keccak256(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;

    /// EIP-2 low-S check: random signatures from alloy should always be
    /// low-S (k256 enforces this).
    #[test]
    fn random_signatures_are_low_s() {
        let signer = PrivateKeySigner::random();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for _ in 0..16 {
            let sig = rt.block_on(personal_sign(&signer, b"test")).expect("sign");
            assert!(is_low_s(&sig), "signature not low-S: {sig:?}");
        }
    }

    /// Ecrecover roundtrip: sign a message, then recover the address
    /// and assert it matches the signer's address.
    #[tokio::test]
    async fn ecrecover_roundtrip_known_message() {
        let signer = PrivateKeySigner::random();
        let msg = b"hello evm-cli";
        let sig = personal_sign(&signer, msg).await.expect("sign");
        let recovered = ecrecover(&sig, msg).expect("recover");
        assert_eq!(recovered, signer.address());
    }

    /// Ecrecover with wrong message returns a different (or invalid) address.
    #[tokio::test]
    async fn ecrecover_wrong_message_does_not_match() {
        let signer = PrivateKeySigner::random();
        let sig = personal_sign(&signer, b"original").await.expect("sign");
        let recovered = ecrecover(&sig, b"tampered").expect("recover (may be any addr)");
        assert_ne!(recovered, signer.address());
    }

    /// personal_sign uses the EIP-191 prefix internally. A raw keccak
    /// of the message bytes is NOT the same as what gets signed.
    /// This test ensures we don't accidentally bypass the prefix.
    #[tokio::test]
    async fn personal_sign_uses_eip191_prefix() {
        let signer = PrivateKeySigner::random();
        let msg = b"x";
        let sig = personal_sign(&signer, msg).await.expect("sign");
        // The hash that was signed was:
        //   keccak256(b"\x19Ethereum Signed Message:\n1x")
        // = keccak256(b"\x19Ethereum Signed Message:\n" + len(msg) + msg)
        let expected_hash = {
            let mut buf = Vec::new();
            buf.extend_from_slice(b"\x19Ethereum Signed Message:\n");
            buf.extend_from_slice(b"1"); // len("x") = 1
            buf.extend_from_slice(msg);
            B256::from(keccak256(&buf))
        };
        // Recover from this prehash directly (bypassing EIP-191):
        let recovered = sig
            .recover_address_from_prehash(&expected_hash)
            .expect("recover");
        assert_eq!(recovered, signer.address());
    }

    // ────────────────────────────────────────────────────────────────
    // proptest: personal_sign across message variants
    // ────────────────────────────────────────────────────────────────
    proptest! {
        /// Empty message.
        #[test]
        fn personal_sign_empty(sig_seed in 0u32..1000) {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let signer = PrivateKeySigner::random_with(&mut rand::rngs::StdRng::seed_from_u64(sig_seed as u64));
            let sig = rt.block_on(personal_sign(&signer, b"")).expect("sign");
            let recovered = ecrecover(&sig, b"").expect("recover");
            prop_assert_eq!(recovered, signer.address());
        }

        /// 1-byte message (any byte 0..255).
        #[test]
        fn personal_sign_1byte(byte in 0u8..=255) {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let signer = PrivateKeySigner::random();
            let msg = [byte];
            let sig = rt.block_on(personal_sign(&signer, &msg)).expect("sign");
            let recovered = ecrecover(&sig, &msg).expect("recover");
            prop_assert_eq!(recovered, signer.address());
        }

        /// Non-UTF-8 bytes (raw byte string, any 8-byte sequence).
        #[test]
        fn personal_sign_non_utf8(bytes in proptest::collection::vec(any::<u8>(), 8..64)) {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let signer = PrivateKeySigner::random();
            let sig = rt.block_on(personal_sign(&signer, &bytes)).expect("sign");
            let recovered = ecrecover(&sig, &bytes).expect("recover");
            prop_assert_eq!(recovered, signer.address());
        }

        /// 0x-prefixed hex string (looks like an Ethereum address or hash).
        #[test]
        fn personal_sign_0x_hex(hexstr in "[0-9a-fA-F]{8,128}") {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let signer = PrivateKeySigner::random();
            let msg = format!("0x{hexstr}");
            let sig = rt.block_on(personal_sign(&signer, msg.as_bytes())).expect("sign");
            let recovered = ecrecover(&sig, msg.as_bytes()).expect("recover");
            prop_assert_eq!(recovered, signer.address());
        }

        /// 1 KiB message (stress: large but not 1 MiB to keep CI fast).
        /// Full 1 MiB variant is exercised manually, not in default proptest.
        #[test]
        fn personal_sign_1kib(bytes in proptest::collection::vec(any::<u8>(), 1024..1025)) {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let signer = PrivateKeySigner::random();
            let sig = rt.block_on(personal_sign(&signer, &bytes)).expect("sign");
            let recovered = ecrecover(&sig, &bytes).expect("recover");
            prop_assert_eq!(recovered, signer.address());
        }
    }

    /// Manual 1 MiB test (kept out of proptest to avoid CI slowness).
    #[tokio::test]
    async fn personal_sign_1mib() {
        let signer = PrivateKeySigner::random();
        let msg = vec![0xABu8; 1024 * 1024];
        let sig = personal_sign(&signer, &msg).await.expect("sign");
        let recovered = ecrecover(&sig, &msg).expect("recover");
        assert_eq!(recovered, signer.address());
    }
}
