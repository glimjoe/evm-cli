# ADR-0004: Upgrade Policy

> Status: **Accepted** (revised 2026-06-11 — see Revisions §)
> Date: 2026-06-10 (initial); 2026-06-11 (revision)
> Deciders: evm-cli maintainers
> Supersedes: V2 §1 L19 (pinned exact) and §8 L184 (monthly upgrade) — these were mutually inconsistent

## Context and Problem Statement

V2 said two contradictory things:

- §1 L19: "Pinned exact versions (e.g. `alloy = "=0.9.x"`)"
- §8 L184: "monthly upgrade window"

These cannot both be true for the published release artifact. The plan needed a two-phase policy: one for development, one for release. (B5 BLOCKER.)

## Decision Drivers

- **Reproducibility**: release artifacts must be bit-for-bit reproducible.
- **Security**: critical CVEs in deps must be patchable without waiting for a major release.
- **Development velocity**: blocking all dep upgrades for the whole V1 cycle would accumulate technical debt.
- **Review burden**: dep upgrades should be deliberate, not accidental.

## Considered Options

- **A. Pinned exact, no exceptions**: safest reproducibility, but a CVE in a critical dep blocks until a new `evm-cli` release.
- **B. Floating minor in dev, pinned in release** (chosen)
- **C. Floating everything in dev, pinned in release**: blanket `cargo update` allowed; risk of accidental major-version drift.

## Decision Outcome

**Chosen option: B**, codified in PLAN-V4 §1.1:

**Development period (pre-0.1.0):**

- Each **direct** dep is pinned exact in `Cargo.toml` (e.g. `alloy = "=2.0.5"`).
- The policy applies to the **entire reachable dep graph** in `Cargo.lock`, not only direct deps. A `cargo update -p <crate>` may pull in compatible transitive updates; those are part of the same PR and require the same ADR + CI + CHANGELOG workflow.
- Minor-version bumps are permitted via PR.
- Each upgrade PR **must**:
  1. Reference an ADR (e.g. ADR-0001 for `alloy` minor bumps; one ADR can cover a multi-crate upgrade)
  2. Pass the full CI matrix (`fmt / clippy / test / audit / deny / cov / fuzz`)
  3. Add a `CHANGELOG.md` entry under "Unreleased" listing all changed crates (direct + transitive)
  4. Be reviewed and merged by a maintainer other than the author — **except** under the single-maintainer fallback below
- `cargo update -p <crate>` is the canonical command for targeted upgrades.
- Blanket `cargo update` (no `-p`) is **forbidden** in PRs.
- **Monthly review hint**: at the start of each month the lead maintainer scans `cargo outdated` / `cargo audit` output and opens upgrade PRs for anything actionable. This is a reminder, not a hard deadline — high-priority CVEs can be PR'd any time.

**Single-maintainer fallback (when no second reviewer exists):**

- The non-author review requirement (rule 4 above) is **waived**.
- A self-merge is permitted only if **all** of the following hold:
  1. The PR has been open for **≥ 24 hours** (cooling-off period; can be shortened for live CVE patches at the maintainer's discretion)
  2. CI is fully green
  3. The PR description explicitly states `self-merge: single-maintainer fallback per ADR-0004`
  4. The merged commit is **tagged in `CHANGELOG.md`** with a `[self-merge]` marker for auditability
- The bus-factor risk in §8 remains MED; this fallback does not eliminate it, only documents the operational workaround.
- If a second maintainer joins the project, the fallback is automatically disabled and the standard non-author review resumes.

**Release period (0.1.0+):**

- `Cargo.toml` in the published crate pins every dep exact.
- Any dep change triggers a new minor release of `evm-cli`.
- **CVE backport flow** (patch-level fixes to a frozen release line):
  1. Upstream releases a patch (e.g. `alloy 2.0.5` → `2.0.6` for a CVE).
  2. A `release/X.Y` branch is cut at the time of `X.Y.0` release; the CVE fix is applied here.
  3. Bump the affected line in `Cargo.toml` (e.g. `alloy = "=2.0.6"`) on the `release/X.Y` branch.
  4. Run the full CI matrix.
  5. Cherry-pick the `CHANGELOG.md` entry from `release/X.Y` to `main` so the next minor also reflects the fix.
  6. Tag a new patch release of `evm-cli` (e.g. `X.Y.1`).
  7. The release PR follows the same non-author review (or single-maintainer fallback) rules as dev period.
- A "release advisory" is published with: affected dep, CVE id, severity, evm-cli versions patched, mitigation for unpatched versions.

**Meta-rule**: changes to this policy itself require a new ADR (this one is the initial).

### Consequences

* **Good**: development can absorb necessary upgrades; release artifacts are reproducible.
* **Good**: every dep upgrade produces a CHANGELOG entry, giving downstream users visibility.
* **Bad**: review burden — every dep bump needs a second maintainer. For a single-maintainer project this is a real cost. Mitigation: documented in §8 (bus factor risk).
* **Bad**: "ADR for every dep bump" is overhead for trivial patches. Mitigation: ADR can be a one-liner linking to the upstream changelog.

## Implementation

- PLAN-V4 §1.1 (Upgrade Policy)
- PLAN-V4 §6 (CI/CD Pipeline — "dep upgrade" workflow row)
- `CHANGELOG.md` follows Keep a Changelog format; self-merge entries tagged `[self-merge]`
- `cargo-deny` configured to fail on unmaintained / wildcard crates
- `cargo outdated` / `cargo audit` run monthly by lead maintainer as review hint

## Revisions

### 2026-06-11 (revision 1)

G3 review by maintainer identified 4 issues in the initial Accepted draft. All addressed:

1. **Non-author review vs single-maintainer conflict**: was "every PR must be reviewed by another maintainer" with no fallback. Now explicit single-maintainer fallback (24h cooling-off + CI green + `[self-merge]` CHANGELOG tag + auto-disable if 2nd maintainer joins).
2. **Transitive dep scope**: policy mentioned only direct deps; now explicitly covers the entire reachable dep graph in `Cargo.lock`, with one ADR + one CHANGELOG entry covering all changed crates.
3. **CVE backport flow**: was one-line "backport to release/X.Y branch"; now a 7-step flow (upstream patch → branch cut → Cargo.toml bump → CI → CHANGELOG cherry-pick → tag → release advisory).
4. **Monthly cadence**: was missing entirely. Now a soft "monthly review hint" — lead maintainer scans `cargo outdated` / `cargo audit` at start of each month, opens PRs for actionable items. CVEs can be PR'd any time.

No change to the core dev/release phase split. All revisions are operational details and pre-emptive clarifications.

## References

- PLAN-V4 §1.1
- PLAN-V4 §6
- PLAN-V4 §8 (bus factor risk)
- Keep a Changelog: https://keepachangelog.com/
- `cargo-deny`: https://crates.io/crates/cargo-deny
- `cargo-outdated`: https://crates.io/crates/cargo-outdated
- `cargo-audit`: https://crates.io/crates/cargo-audit
