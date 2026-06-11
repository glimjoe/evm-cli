#!/usr/bin/env bash
#
# ADR-0007 rev1: 5-pattern grep that bans `String` for sensitive
# material. Exits non-zero on any match. Intended to be run by CI
# (see .github/workflows/ci.yml) but also runnable locally:
#
#   bash scripts/secret_string_grep.sh

set -euo pipefail

SENSITIVE='mnemonic|seed|private[_-]?key|priv[_-]?key|secret'

# Pattern 1: direct binding to String
#   let foo: String = ...
#   let mut bar: String = ...
if rg --quiet --type rust "(let|let mut) .* ($SENSITIVE).*: String"; then
  echo "ERROR (pattern 1): secret bound to String"
  rg --type rust "(let|let mut) .* ($SENSITIVE).*: String"
  exit 1
fi

# Pattern 2: String::from on a sensitive source
if rg --quiet --type rust "String::from\(.*($SENSITIVE)"; then
  echo "ERROR (pattern 2): String::from on secret"
  rg --type rust "String::from\(.*($SENSITIVE)"
  exit 1
fi

# Pattern 3: .to_string() on a sensitive source (both directions)
if rg --quiet --type rust "\.to_string\(\).*($SENSITIVE)|($SENSITIVE).*\.to_string\(\)"; then
  echo "ERROR (pattern 3): .to_string() on secret"
  rg --type rust "\.to_string\(\).*($SENSITIVE)|($SENSITIVE).*\.to_string\(\)"
  exit 1
fi

# Pattern 4: format! with a sensitive argument
if rg --quiet --type rust 'format!\([^)]*\b('"$SENSITIVE"')\b'; then
  echo "ERROR (pattern 4): format! on secret"
  rg --type rust 'format!\([^)]*\b('"$SENSITIVE"')\b'
  exit 1
fi

# Pattern 5: function returning String with sensitive in signature
if rg --quiet --type rust "fn .* ($SENSITIVE).* -> String"; then
  echo "ERROR (pattern 5): function returning String of secret"
  rg --type rust "fn .* ($SENSITIVE).* -> String"
  exit 1
fi

echo "All 5 String-on-secret patterns passed (zero matches)."
