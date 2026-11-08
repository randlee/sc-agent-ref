#!/usr/bin/env bash
# `cargo tree` dependency gates (docs/plans/phase-a-refine/plan.md, "`cargo
# tree` gates"). Consumer-owned: install.py never writes this path.
#
# Mode `dup` (duplicate versions) runs `cargo tree -e normal -d` and fails when a
# listed package is itself a duplicated package, i.e. appears at the start of a
# line (a top-level entry). `-d` also prints every ancestor of a duplicated
# third-party crate (two `syn` versions list sc-ai and its dependencies under
# them), so a pattern that is not anchored to the line start would always match.
#
# Each row names a crate, a mode, optional extra `cargo tree` arguments and
# packages. Mode `both` runs the row twice, with default features and with
# `--no-default-features`; mode `lib` runs `--no-default-features` only,
# because a binary crate's default `cli` feature legitimately links its
# allocator and argv parser. In those modes a row fails when any package
# pattern matches. Mode `present` is the positive control: it runs with
# default features and fails when a package pattern does NOT match, so an
# absence row cannot pass just because the pattern stopped matching. Patterns are word-bounded on the
# package name and version, so `sc-jev` does not match `sc-jev-check` and
# `mimalloc` does not match `libmimalloc-sys`:
#   grep -E '(^|[^[:alnum:]_-])<name> v[0-9]'
#
# Rows added by r-4: REQ-SAG-0134 (no mimalloc in either library tree, and
# mimalloc present in each binary crate's default tree). Rows added by r-3
# (QA-1, RBQA-F104 / RBQA-F009; parent-authorized): ureq only in sc-agent-http
# (depth-1 rows for sc-jev, sc-jev-backends, sc-jev-check; positive control on
# sc-agent-http), sc-agent-http's forbidden packages, and the graph rows
# below (no workspace `test-util` in any normal graph; sc-agent-http reaches
# sc-agent-types with feature `secret` only). Rows added by r-3b (REQ-SAG-0135,
# ADR-SAG-0026 host half): every remaining row of the plan table. clap in both
# libraries, ureq / pyo3 / tree-sitter / sc-observability absences, the
# sc-jev-host depth-1 row, and the duplicate-version rows for sc-ai and
# sc-jev-py (`-d`: a split sc-jev-check, sc-agent-types or sc-jev-host would
# give two incompatible `RunResponse` types). There is no workspace-tests
# dev-set row: sc-boundary counts dev edges (boundaries/README.md). These
# rows run on `cargo tree` (no test build), so a dev-only feature unified by
# `cargo test` cannot make them pass.
#
# Feature rows (r-3c, ADR-SAG-0015, RBQA-F009) inspect
# `cargo tree -e normal,features -i <package>` instead of package names: in
# mode `lib` a row fails when any listed feature of <package> is enabled in
# the crate's `--no-default-features` tree; in mode `present` it fails when a
# listed feature is NOT enabled with default features (positive control, so
# the absence row cannot pass on a dead pattern). Pattern:
#   grep -E '<package> feature "<feature>"'
# `-e normal,features` keeps a future dev-dependency from tripping the row
# (CPR-M208). Negative control (checked when the row landed): dropping
# `default-features = false` from sc-treesitter's sc-agent-types dependency
# makes the `lib` row fail on secret, obs and default. The
# sc-decision-schema row (r-7, D1.6) is the feature leak the boundary
# manifest cannot see: it reaches sc-agent-types core only.
#
# Only `cargo tree` stdout is matched; stderr (download and lock messages)
# goes to a temporary file and is printed only when cargo fails. Every call
# uses `--locked`, so Cargo.lock drift fails instead of being resolved.
#
# Usage: bash .github/scripts/cargo-tree-gates.sh
# Exit codes: 0 every row passes; 1 a row matched or `cargo tree` failed.
set -euo pipefail

cd "$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"

# crate | mode (both|lib|present) | extra cargo tree args | packages (space-separated)
rows=(
  "sc-ai|lib||mimalloc clap"
  "sc-treesitter|lib||mimalloc clap sc-observability"
  "sc-agent-types|both||sc-observability sc-observability-types ureq pyo3 tree-sitter"
  "sc-ai|both|--depth 1|sc-observability ureq"
  "sc-jev-py|both|--depth 1|sc-observability ureq"
  "sc-jev-host|both|--depth 1|ureq pyo3 tree-sitter clap mimalloc"
  "sc-ai|dup||sc-jev-check sc-agent-types sc-jev-host"
  "sc-jev-py|dup||sc-jev-check sc-agent-types sc-jev-host"
  "sc-ai|present||mimalloc"
  "sc-treesitter|present||mimalloc"
  "sc-jev|both|--depth 1|ureq sc-observability pyo3"
  "sc-jev-backends|both|--depth 1|ureq sc-observability pyo3"
  "sc-jev-check|both|--depth 1|ureq sc-observability pyo3 tree-sitter"
  "sc-agent-http|both||sc-observability pyo3 tree-sitter sc-agent-secrets sc-jev sc-jev-backends sc-jev-check"
  "sc-agent-http|present|--depth 1|ureq"
)

# Graph rows (r-3; `-e normal,features`, dev edges excluded, so a dev-only
# `test-util` does not count). Whole lines are matched with `grep -E`.
# Fields are separated by `;` because the regexes use `|`.
# label ; cargo tree args ; absent regex ; present regex (positive control)
graph_rows=(
  "test-util off in normal graphs;--workspace -e normal,features;feature \"test-util\";"
  "test-util control (dev edges);--workspace -e normal,dev,features;;^sc-agent-http feature \"test-util\""
  "sc-agent-http -> sc-agent-types secret only;-p sc-agent-http -e normal,features -i sc-agent-types;^sc-agent-types feature \"(obs|git|default|test-util)\";^sc-agent-types feature \"secret\""
)

# crate | mode (lib|present) | inverted package | features (space-separated)
feature_rows=(
  "sc-treesitter|lib|sc-agent-types|secret obs git default"
  "sc-decision-schema|lib|sc-agent-types|secret obs git default"
  "sc-treesitter|present|sc-agent-types|git"
)

failures=0
errfile="$(mktemp)"
trap 'rm -f "$errfile"' EXIT

run_row() {
  local crate="$1" features="$2" extra="$3" names="$4" want="${5:-absent}"
  local -a args=(tree --locked -p "$crate" -e normal --prefix none)
  [[ -n "$features" ]] && args+=("$features")
  if [[ -n "$extra" ]]; then
    local -a extra_args
    read -r -a extra_args <<<"$extra"
    args+=("${extra_args[@]}")
  fi
  local label="cargo ${args[*]}"
  local tree
  if ! tree="$(cargo "${args[@]}" 2>"$errfile")"; then
    echo "FAIL  $label: cargo tree failed"
    sed 's/^/    /' "$errfile"
    failures=$((failures + 1))
    return
  fi
  local name hits row_failed=0
  for name in $names; do
    hits="$(printf '%s\n' "$tree" | grep -E "(^|[^[:alnum:]_-])${name} v[0-9]" || true)"
    if [[ "$want" == absent && -n "$hits" ]]; then
      echo "FAIL  $label: must not contain $name"
      printf '%s\n' "$hits" | sort -u | sed 's/^/    /'
      row_failed=1
    elif [[ "$want" == present && -z "$hits" ]]; then
      echo "FAIL  $label: must contain $name (positive control)"
      row_failed=1
    fi
  done
  if [[ "$row_failed" -eq 0 ]]; then
    echo "ok    $label: $want: $names"
  else
    failures=$((failures + 1))
  fi
}

run_dup_row() {
  local crate="$1" flag="$2" names="$3"
  local -a args=(tree --locked -p "$crate" -e normal -d)
  [[ -n "$flag" ]] && args+=("$flag")
  local label="cargo ${args[*]}"
  local tree
  if ! tree="$(cargo "${args[@]}" 2>"$errfile")"; then
    echo "FAIL  $label: cargo tree failed"
    sed 's/^/    /' "$errfile"
    failures=$((failures + 1))
    return
  fi
  local name hits row_failed=0
  for name in $names; do
    hits="$(printf '%s\n' "$tree" | grep -E "^${name} v[0-9]" || true)"
    if [[ -n "$hits" ]]; then
      echo "FAIL  $label: $name appears in more than one version"
      printf '%s\n' "$hits" | sort -u | sed 's/^/    /'
      row_failed=1
    fi
  done
  if [[ "$row_failed" -eq 0 ]]; then
    echo "ok    $label: no duplicate: $names"
  else
    failures=$((failures + 1))
  fi
}

for row in "${rows[@]}"; do
  IFS='|' read -r crate mode extra names <<<"$row"
  case "$mode" in
    dup)
      run_dup_row "$crate" "" "$names"
      run_dup_row "$crate" "--no-default-features" "$names"
      ;;
    both)
      run_row "$crate" "" "$extra" "$names"
      run_row "$crate" "--no-default-features" "$extra" "$names"
      ;;
    lib)
      run_row "$crate" "--no-default-features" "$extra" "$names"
      ;;
    present)
      run_row "$crate" "" "$extra" "$names" present
      ;;
    *)
      echo "cargo-tree-gates: row '$row' has unknown mode '$mode'" >&2
      exit 1
      ;;
  esac
done

run_feature_row() {
  local crate="$1" mode="$2" package="$3" features="$4"
  local -a args=(tree --locked -p "$crate" -e normal,features -i "$package" --prefix none)
  local want=absent
  case "$mode" in
    lib) args+=(--no-default-features) ;;
    present) want=present ;;
    *)
      echo "cargo-tree-gates: feature row '$crate|$mode' has unknown mode" >&2
      exit 1
      ;;
  esac
  local label="cargo ${args[*]}"
  local tree
  if ! tree="$(cargo "${args[@]}" 2>"$errfile")"; then
    echo "FAIL  $label: cargo tree failed"
    sed 's/^/    /' "$errfile"
    failures=$((failures + 1))
    return
  fi
  local feature hits row_failed=0
  for feature in $features; do
    hits="$(printf '%s\n' "$tree" | grep -E "${package} feature \"${feature}\"" || true)"
    if [[ "$want" == absent && -n "$hits" ]]; then
      echo "FAIL  $label: must not enable $package feature $feature"
      row_failed=1
    elif [[ "$want" == present && -z "$hits" ]]; then
      echo "FAIL  $label: must enable $package feature $feature (positive control)"
      row_failed=1
    fi
  done
  if [[ "$row_failed" -eq 0 ]]; then
    echo "ok    $label: $want: $package features $features"
  else
    failures=$((failures + 1))
  fi
}

for row in "${feature_rows[@]}"; do
  IFS='|' read -r crate mode package features <<<"$row"
  run_feature_row "$crate" "$mode" "$package" "$features"
done

run_graph_row() {
  local label="$1" args_text="$2" absent="$3" present="$4"
  local -a args=(tree --locked --prefix none)
  local -a extra_args
  read -r -a extra_args <<<"$args_text"
  args+=("${extra_args[@]}")
  local tree
  if ! tree="$(cargo "${args[@]}" 2>"$errfile")"; then
    echo "FAIL  $label (cargo ${args[*]}): cargo tree failed"
    sed 's/^/    /' "$errfile"
    failures=$((failures + 1))
    return
  fi
  local hits rc row_failed=0
  if [[ -n "$absent" ]]; then
    rc=0
    hits="$(printf '%s\n' "$tree" | grep -E "$absent")" || rc=$?
    if [[ "$rc" -gt 1 ]]; then
      echo "FAIL  $label: bad regex /$absent/"
      row_failed=1
    elif [[ -n "$hits" ]]; then
      echo "FAIL  $label (cargo ${args[*]}): must not match /$absent/"
      printf '%s\n' "$hits" | sort -u | sed 's/^/    /'
      row_failed=1
    fi
  fi
  if [[ -n "$present" ]]; then
    rc=0
    printf '%s\n' "$tree" | grep -qE "$present" || rc=$?
    if [[ "$rc" -ne 0 ]]; then
      echo "FAIL  $label (cargo ${args[*]}): must match /$present/ (positive control, grep exit $rc)"
      row_failed=1
    fi
  fi
  if [[ "$row_failed" -eq 0 ]]; then
    echo "ok    $label (cargo ${args[*]})"
  else
    failures=$((failures + 1))
  fi
}

for row in "${graph_rows[@]}"; do
  IFS=';' read -r label args_text absent present <<<"$row"
  run_graph_row "$label" "$args_text" "$absent" "$present"
done

if [[ "$failures" -ne 0 ]]; then
  echo "cargo-tree-gates: $failures row run(s) failed" >&2
  exit 1
fi
echo "cargo-tree-gates: all rows passed"
