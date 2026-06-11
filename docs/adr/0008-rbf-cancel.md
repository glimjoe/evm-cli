# ADR-0008: RBF / Cancel Implementation Policy

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision)
> Deciders: evm-cli maintainers
> Supersedes: V2 §9 L200 (Out of Scope: RBF/cancel was implicit)

## Context and Problem Statement

V2's Out of Scope section did not explicitly list RBF / Cancel, but the M3 pipeline terminated at "poll receipt (timeout 120s)" with no recovery path. A V1 PoC that lets a user get stuck with an unconfirmed tx and no rescue is a usability failure.

P0-3 mandates RBF and Cancel paths in V1.

## Decision Drivers

- **User experience**: a user who sets too-low fees must have a recovery path.
- **Simplicity**: V1 is a single-user CLI; elaborate mempool policies are out of scope.
- **EIP-1559 alignment**: V1 sends type-2 (EIP-1559) transactions; RBF policy should match.
- **BIP-125 compatibility**: the Bitcoin RBF policy (`>= 110% fee bump`) has become a de facto Ethereum mempool expectation.

## Considered Options

- **A. Type-2 (EIP-1559) RBF only, with Cancel as 0-value self-send** (chosen)
- **B. Legacy (type-0) replace transaction**: V1 doesn't send type-0; out of scope.
- **C. Both type-0 and type-2 RBF**: doubles the test surface, doubles the policy docs.
- **D. No RBF; user must wait and hope**: violates P0-3.

## Decision Outcome

**Chosen option: A**, with these concrete rules:

### Bump-fee (`send-eth --bump-fee <tx-hash>`)

1. Query the original tx via `eth_getTransactionByHash`. If not found → error `EVMC-007` `TxNotFound`. If found but already mined → error `EVMC-008` `TxAlreadyMined`. (Codes pending ADR-0006 sync.)
2. Extract `nonce`, `to`, `value`, `data`, `from`, `max_fee_per_gas`, `max_priority_fee_per_gas` from the original envelope.
3. Query the **current mempool context**:
   - `eth_feeHistory(block_count=5, newest="latest", reward_percentiles=[50])` to get `current_base_fee` and recent `priority_fee`
   - `eth_gasPrice` as a fallback baseline
4. Compute replacement fees (both EIP-1559 parameters):
   ```
   new_max_fee_per_gas         = max(
       (original.max_fee_per_gas         * 110) / 100,
       original.max_fee_per_gas         + 1_000_000_000,   // +1 gwei floor
       current_base_fee * 2 + recent_priority_fee          // mempool-competitive floor
   )
   new_max_priority_fee_per_gas = max(
       (original.max_priority_fee_per_gas * 110) / 100,
       original.max_priority_fee_per_gas + 1_000_000_000,   // +1 gwei floor
       recent_priority_fee                                  // mempool-competitive floor
   )
   ```
   The `* 110 / 100` is the BIP-125 minimum bump; the other two terms are absolute floors that prevent underbidding when the original was set for a calm mempool but base_fee has since risen.
5. Re-sign with:
   - Same `nonce`
   - Same `to` / `value` / `data` / `from`
   - New fees as computed above
6. The replacement does **not** reserve a new nonce in `NonceManager` — the original tx already owns that nonce. Call `NonceManager::replaced(addr, old_nonce, new_nonce, new_hash)` to record the supersession.
7. Broadcast; poll receipt (same 120 s timeout as initial send).
8. E2E test: low-fee tx → wait 30 s pending → `--bump-fee` → confirmed within 60 s.

### Cancel (`send-eth --cancel <tx-hash>`)

1. Query the original tx (same as bump-fee step 1: `TxNotFound` / `TxAlreadyMined` errors).
2. Re-sign with:
   - Same `nonce`
   - `to = <signer address>` (send to self)
   - `value = 0`
   - `data = 0x`
   - New fees = same three-term `max(...)` formula as bump-fee (BOTH `max_fee_per_gas` and `max_priority_fee_per_gas`)
3. The replacement is a no-op on value transfer but evicts the original from the mempool.
4. Broadcast; confirm.
5. Call `NonceManager::replaced(addr, old_nonce, new_nonce, new_hash)` to record the supersession.

**Note on cost:** the user pays gas for both the original broadcast and the cancel. The cancel itself transfers 0 ETH, so the original `value` is not at risk (it never moved from the wallet). However, the user has now paid 2× the gas for an effectively no-op outcome. This is intentional — it is the cost of "undo".

### Cross-cutting rules

- **`--bump-fee` and `--cancel` accept a tx-hash, not a nonce**: this avoids the user having to track which nonce belongs to which tx.
- **Both flags require REPL mis-sign prevention** (P0-9): y/N confirmation showing the original vs replacement fee, `--dry-run` works.
- **No automatic bumping**: V1 does not run a background "watcher" that auto-bumps. Bumping is always an explicit user action.

### Consequences

* **Good**: clear rescue path for stuck txs; documented fee bump policy.
* **Good**: same code path for bump-fee and cancel (only `to`/`value` differ).
* **Good**: no new NonceManager reservations needed — the original tx "owns" the nonce.
* **Bad**: the 10% bump rule is hard-coded; V2 may want it configurable. Mitigation: a config key (e.g. `rbf.bump_ratio`) is a small extension; deferred.
* **Bad**: if the original tx is already mined when the user runs `--bump-fee`, we get a `TxNotPending` error. UX mitigation: error message is friendly and suggests checking `eth_getTransactionReceipt`.

## Implementation

- PLAN-V4 §5 M3 DoD (RBF / Cancel sub-items)
- PLAN-V4 §5 M4 DoD (`send-eth` flags)
- PLAN-V4 §7 (self-audit: E2E test for both paths)
- PLAN-V4 §8 (stuck-tx risk → MED, mitigated)
- Unit tests in `src/chain/rbf.rs` covering the fee-bump math (both EIP-1559 parameters, all three terms of the `max(...)` formula)
- E2E test in `tests/e2e_sepolia_bump.rs` (marked `#[ignore]`)
- **Cross-ADR dependency**: this ADR introduces 2 new error codes (`EVMC-007` `TxNotFound`, `EVMC-008` `TxAlreadyMined`) that must be added to **ADR-0006** during its G3 review.

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 3 issues in the initial Accepted draft. All addressed:

1. **Fee bump formula ignored rising `base_fee`**: the initial formula `max(original * 1.10, original + 1 wei)` compares only to the original tx, not the current mempool. If `base_fee` rose 30% between original broadcast and `--bump-fee`, the bumped tx could still underbid the mempool. Now the formula has **three terms**: (a) BIP-125 10% bump over original, (b) absolute +1 gwei floor, (c) `current_base_fee * 2 + recent_priority_fee` as mempool-competitive floor. Both EIP-1559 parameters (`max_fee_per_gas` and `max_priority_fee_per_gas`) are bumped.
2. **Cross-ADR error code inconsistency**: the initial draft referenced `EVMC-007` `TxNotPending`, which was not in ADR-0006's enumerated list (which stops at `EVMC-006`). Now split into two semantically distinct codes: `EVMC-007` `TxNotFound` (hash unknown) and `EVMC-008` `TxAlreadyMined` (already in a block). Both will be added to ADR-0006 during G3 review of that ADR.
3. **Bump rule asymmetric across EIP-1559 parameters**: the initial formula was shown for one parameter only. Now both are explicitly bumped with identical rules, and the unit test requirement is updated to cover both.

No change to the core Type-2-only decision or the `tx-hash`-not-`nonce` UX choice.

## References

- PLAN-V4 §5 M3, M4
- PLAN-V4 §7, §8
- BIP-125: https://github.com/bitcoin/bips/blob/master/bip-0125.mediawiki
- EIP-1559: https://eips.ethereum.org/EIPS/eip-1559
- EIP-155 (chainId in signature, B2)
- ADR-0002 (NonceManager: `replaced()` API used here; same nonce rules)
- ADR-0006 (error codes: `EVMC-007` and `EVMC-008` to be added in lockstep)
- ADR-0007 (memory hardening: signed tx in flight is a `Secret`)
