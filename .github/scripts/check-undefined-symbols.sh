#!/usr/bin/env bash
# macOS link check. Consumer-owned: install.py never writes this path.
#
# `.cargo/config.toml` sets `-undefined dynamic_lookup` for both darwin
# targets, because maturin 1.9.4 (the version the kit's release workflows
# install, inside the allowed `>=1.8,<2`) does not pass it for
# `sc-observability-py`'s cdylib when it builds the wheel. The flag applies to
# every darwin build, so a symbol that is missing from a binary would link and
# then crash at run time. This check closes that gap: a release binary must
# have no symbol that the linker left to be looked up at run time.
#
# `nm -um` lists undefined symbols. A normal one names the library it comes
# from, `(from libSystem)`; one left by `dynamic_lookup` is marked
# `(dynamically looked up)`. Any such line fails the check.
#
# Usage:
#   bash .github/scripts/check-undefined-symbols.sh [BINARY...]
#       default: target/release/sc-ai target/release/sc-treesitter
#   bash .github/scripts/check-undefined-symbols.sh --self-test
#       proves the check passes a clean binary and fails one with a missing
#       extern
#
# Exit codes: 0 clean (or not macOS: nothing to check); 1 a looked-up symbol,
# a missing binary, or a failed self-test; 2 usage error. CI runs it on
# macOS (job `darwin-link`).
set -euo pipefail

if [ "$(uname -s)" != "Darwin" ]; then
  echo "check-undefined-symbols: not macOS; nothing to check"
  exit 0
fi

check_binary() {
  local binary="$1" found
  if [ ! -f "$binary" ]; then
    echo "check-undefined-symbols: $binary does not exist" >&2
    return 1
  fi
  found="$(nm -um "$binary" | grep -F "dynamically looked up" || true)"
  if [ -n "$found" ]; then
    echo "check-undefined-symbols: $binary has symbols left to run-time lookup:" >&2
    echo "$found" >&2
    return 1
  fi
  echo "check-undefined-symbols: $binary ok"
}

self_test() {
  local dir flags=(-C link-arg=-undefined -C link-arg=dynamic_lookup)
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  cat >"$dir/clean.rs" <<'RS'
fn main() { println!("clean"); }
RS
  cat >"$dir/missing.rs" <<'RS'
unsafe extern "C" { fn sc_agent_symbol_that_does_not_exist(); }
fn main() { unsafe { sc_agent_symbol_that_does_not_exist() } }
RS
  rustc --edition 2024 "${flags[@]}" -o "$dir/clean" "$dir/clean.rs"
  rustc --edition 2024 "${flags[@]}" -o "$dir/missing" "$dir/missing.rs"
  if ! check_binary "$dir/clean" >/dev/null; then
    echo "self-test failed: a clean binary was rejected" >&2
    return 1
  fi
  if check_binary "$dir/missing" >/dev/null 2>&1; then
    echo "self-test failed: a binary with a missing extern passed" >&2
    return 1
  fi
  echo "check-undefined-symbols: self-test ok (clean passes, missing extern fails)"
}

case "${1:-}" in
  --self-test) self_test ;;
  -h | --help) sed -n '2,25p' "${BASH_SOURCE[0]}" ;;
  --*) echo "usage: $0 [--self-test | BINARY...]" >&2; exit 2 ;;
  *)
    if [ "$#" -eq 0 ]; then
      set -- target/release/sc-ai target/release/sc-treesitter
    fi
    status=0
    for binary in "$@"; do
      check_binary "$binary" || status=1
    done
    exit "$status"
    ;;
esac
