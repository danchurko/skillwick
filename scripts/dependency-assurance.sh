#!/bin/sh
set -eu

# Keep these pins together with the commands below. The lockfile remains the
# only dependency input; cargo-outdated is informational and never selects or
# writes an upgrade.
cargo_deny_version=0.20.2
cargo_outdated_version=0.19.0

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
manifest="$root/Cargo.toml"
lockfile="$root/Cargo.lock"
policy="$root/deny.toml"

die() {
  echo "dependency assurance: $*" >&2
  exit 1
}

[ -f "$manifest" ] || die "missing Cargo.toml"
[ -f "$lockfile" ] || die "missing Cargo.lock; assurance requires a locked graph"
[ -f "$policy" ] || die "missing deny.toml"

deny_bin=${SKILLWICK_CARGO_DENY_BIN:-}
if [ -z "$deny_bin" ]; then
  deny_bin=$(command -v cargo-deny 2>/dev/null || true)
fi
[ -n "$deny_bin" ] || die "cargo-deny $cargo_deny_version is required; install with: cargo install --locked --version $cargo_deny_version cargo-deny"

outdated_bin=${SKILLWICK_CARGO_OUTDATED_BIN:-}
if [ -z "$outdated_bin" ]; then
  outdated_bin=$(command -v cargo-outdated 2>/dev/null || true)
fi
[ -n "$outdated_bin" ] || die "cargo-outdated $cargo_outdated_version is required; install with: cargo install --locked --version $cargo_outdated_version cargo-outdated"

deny_tool_version=$("$deny_bin" --version 2>&1 || true)
printf '%s\n' "$deny_tool_version" | grep -Fqx "cargo-deny $cargo_deny_version" || die "expected cargo-deny $cargo_deny_version, got: $deny_tool_version"

outdated_tool_version=$("$outdated_bin" outdated --version 2>&1 || true)
printf '%s\n' "$outdated_tool_version" | grep -Fqx "cargo-outdated-outdated $cargo_outdated_version" || die "expected cargo-outdated $cargo_outdated_version, got: $outdated_tool_version"

lock_before=$(shasum -a 256 "$lockfile" | awk '{print $1}')
export CARGO_TERM_COLOR=never

echo "required dependency checks (cargo-deny $cargo_deny_version)"
echo "  advisories: known vulnerabilities in the locked graph"
echo "  licenses: explicit allowlist in deny.toml"
echo "  bans/sources: wildcard and registry policy"
(
  cd "$root"
  "$deny_bin" --manifest-path "$manifest" --config "$policy" --workspace --format human --color never --locked check advisories bans licenses sources
)

echo "informational dependency staleness (cargo-outdated $cargo_outdated_version; no upgrades are applied)"
outdated_output=$(mktemp)
trap 'rm -f "$outdated_output"' EXIT HUP INT TERM
if (
  cd "$root"
  "$outdated_bin" outdated --manifest-path "$manifest" --workspace --format list --color never --exit-code 0
) >"$outdated_output" 2>&1; then
  cat "$outdated_output"
else
  status=$?
  echo "informational dependency staleness check failed to produce a report (exit $status):" >&2
  cat "$outdated_output" >&2
  exit "$status"
fi

lock_after=$(shasum -a 256 "$lockfile" | awk '{print $1}')
[ "$lock_before" = "$lock_after" ] || die "cargo-outdated changed Cargo.lock; refusing to continue"

echo "dependency assurance passed; Cargo.lock unchanged"
