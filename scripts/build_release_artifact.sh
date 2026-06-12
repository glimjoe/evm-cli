#!/usr/bin/env bash
#
# build_release_artifact.sh — local release-build helper (M5)
#
# Mirrors the steps in `.github/workflows/release.yml` so a contributor
# can dry-run a release locally before pushing a tag. Does NOT push or
# create a GitHub Release; that is the workflow's job.
#
# Usage:
#   bash scripts/build_release_artifact.sh 0.2.0
#   bash scripts/build_release_artifact.sh 0.2.0 x86_64-unknown-linux-gnu
#
# Output:
#   dist/evm-cli-v<ver>-<arch>.tar.gz
#   dist/evm-cli-v<ver>-<arch>.sha256
#
# Exit codes:
#   0   success
#   1   bad arguments
#   2   prerequisites missing
#   3   build failed
#   4   tar / sha256 failed
#
# Per PLAN-V9 §5 M5 DoD, the artifact naming convention is
# `evm-cli-v<ver>-<platform>.tar.gz` where `<platform>` is the
# normalized tag (e.g. `linux-x86_64`, `linux-aarch64`).

set -euo pipefail

# ── args ────────────────────────────────────────────────────────────
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <version> [<target>]" >&2
  echo "  <version>  semver X.Y.Z (e.g. 0.2.0)" >&2
  echo "  <target>   rust target triple (default: \$(rustc -vV | sed -n 's|host: ||p'))" >&2
  exit 1
fi
VERSION="$1"
TARGET="${2:-$(rustc -vV | sed -n 's|host: ||p')}"

# ── preflight ───────────────────────────────────────────────────────
if ! command -v cargo >/dev/null 2>&1; then
  echo "ERROR: cargo not on PATH" >&2
  exit 2
fi
if ! command -v sha256sum >/dev/null 2>&1; then
  echo "ERROR: sha256sum not on PATH (coreutils)" >&2
  exit 2
fi
if ! command -v tar >/dev/null 2>&1; then
  echo "ERROR: tar not on PATH" >&2
  exit 2
fi

# Validate version looks like X.Y.Z (same shape as the workflow's
# `softprops/action-gh-release` tag → version stripping).
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "ERROR: version '$VERSION' is not in X.Y.Z form" >&2
  exit 1
fi

# Map target triple → release-archive platform tag. The mapping mirrors
# `evm_cli::release::platform_tag` (single source of truth: keep this
# table in sync with the Rust function).
case "$TARGET" in
  x86_64-unknown-linux-gnu)    PLATFORM="linux-x86_64"   ;;
  aarch64-unknown-linux-*)     PLATFORM="linux-aarch64"  ;;
  *)
    echo "ERROR: target '$TARGET' not in the release matrix (only linux-x86_64 / linux-aarch64 supported)" >&2
    exit 1
    ;;
esac

# ── build ───────────────────────────────────────────────────────────
echo "==> Building evm-cli v$VERSION for $TARGET (artifact platform: $PLATFORM)"
mkdir -p dist

if ! cargo build --release --target "$TARGET" --locked; then
  echo "ERROR: cargo build failed" >&2
  exit 3
fi

BIN="target/$TARGET/release/evm-cli"
if [ ! -x "$BIN" ]; then
  echo "ERROR: built binary not found at $BIN" >&2
  exit 3
fi

# Strip debug symbols to shrink the tarball.
strip "$BIN" 2>/dev/null || true

# ── tar + sha256 ────────────────────────────────────────────────────
TARBALL="dist/evm-cli-v$VERSION-$PLATFORM.tar.gz"
SIDECAR="dist/evm-cli-v$VERSION-$PLATFORM.sha256"

tar -C "target/$TARGET/release" -czf "$TARBALL" evm-cli
(
  cd dist
  sha256sum "evm-cli-v$VERSION-$PLATFORM.tar.gz" > "evm-cli-v$VERSION-$PLATFORM.sha256"
)

echo ""
echo "==> Artifacts:"
ls -l "$TARBALL" "$SIDECAR"
echo ""
echo "==> SHA256:"
cat "$SIDECAR"
echo ""
echo "Next steps (push to a real release):"
echo "  git tag v$VERSION"
echo "  git push origin v$VERSION"
echo "  # .github/workflows/release.yml takes over from here"
