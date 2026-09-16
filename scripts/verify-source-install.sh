#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
developer_home=${HOME:?HOME is required}

tmp_parent=${TMPDIR:-/tmp}
[ -d "$tmp_parent" ] || { echo "temporary directory does not exist: $tmp_parent" >&2; exit 1; }
temporary=$(mktemp -d "${tmp_parent%/}/skillwick-source-install.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
prefix="$temporary/prefix"
cli_roots_file="$temporary/cli-roots.nul"
cli_required_file="$temporary/cli-required.nul"
: >"$cli_roots_file"
: >"$cli_required_file"

config_path=${SKILLWICK_CONFIG:-}
source_cwd=${SKILLWICK_SOURCE_CWD:-$developer_home}
binary_arg=
skip_build=false

usage() {
  echo "usage: $0 [--binary PATH] [--config PATH] [--cwd PATH] [--root PATH] [--require NAME] [--skip-build]" >&2
  exit 2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --binary)
      [ "$#" -ge 2 ] || usage
      binary_arg=$2
      shift 2
      ;;
    --config)
      [ "$#" -ge 2 ] || usage
      config_path=$2
      shift 2
      ;;
    --cwd)
      [ "$#" -ge 2 ] || usage
      source_cwd=$2
      shift 2
      ;;
    --root|--source-root)
      [ "$#" -ge 2 ] || usage
      # NUL-delimited storage preserves colons, newlines, and leading dashes.
      printf '%s\0' "$2" >>"$cli_roots_file"
      shift 2
      ;;
    --require|--required-skill)
      [ "$#" -ge 2 ] || usage
      # NUL-delimited storage preserves required names containing spaces.
      printf '%s\0' "$2" >>"$cli_required_file"
      shift 2
      ;;
    --skip-build)
      skip_build=true
      shift
      ;;
    -h|--help)
      usage
      ;;
    *)
      usage
      ;;
  esac
done

if [ -z "$config_path" ]; then
  config_path="${XDG_CONFIG_HOME:-$developer_home/.config}/skillwick/config.toml"
fi

binary="$binary_arg"
if [ -z "$binary" ]; then
  [ "$skip_build" = false ] || { echo "--skip-build requires --binary" >&2; exit 2; }
  cargo_target_dir=${SKILLWICK_CARGO_TARGET_DIR:-$temporary/target}
  CARGO_TARGET_DIR="$cargo_target_dir" cargo install --locked --path "$root" --root "$prefix"
  binary="$prefix/bin/skillwick"
else
  case "$binary" in
    /*) ;;
    *) binary="$root/$binary" ;;
  esac
fi
[ -x "$binary" ] || { echo "source verification requires an executable: $binary" >&2; exit 2; }
version=$("$binary" --version | awk '{print $2}')

# Run deterministic fixtures with the installed executable. These cover the
# source contract before the live corpus pass exercises the maintained roots.
sh "$root/tests/cli_acceptance.sh" "$binary"
PYTHONDONTWRITEBYTECODE=1 python3 "$root/tests/invocation_contract.py" "$binary"
PYTHONDONTWRITEBYTECODE=1 python3 "$root/tests/setup_contract.py" "$binary"
PYTHONDONTWRITEBYTECODE=1 python3 "$root/tests/discovery_contract.py" "$binary"
sh "$root/tests/trust_boundary.sh" "$binary"
sh "$root/tests/filesystem_integration.sh" "$binary"
sh "$root/tests/distribution_smoke.sh" "$binary"
sh "$root/tests/docs_check.sh" "$binary"

[ -d "$source_cwd" ] || { echo "source workspace does not exist: $source_cwd" >&2; exit 1; }
codex_home=${CODEX_HOME:-$developer_home/.codex}
claude_home=${CLAUDE_CONFIG_DIR:-$developer_home/.claude}
local_config="$temporary/local-config.toml"
automatic_config="$temporary/automatic-config.toml"
corpus_roots="$temporary/corpus-roots.nul"
corpus_files="$temporary/corpus-files.nul"
auto_marker="$temporary/automatic-source"
required_file="$temporary/required-skills.nul"

# Parse the real configuration strictly, preserve all paths as structured data,
# and render a temporary explicit view for the local pass. The candidate never
# receives the developer-owned configuration as its writable config directory.
PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-local-corpus.py" prepare \
  --config="$config_path" \
  --home="$developer_home" \
  --codex-home="$codex_home" \
  --claude-home="$claude_home" \
  --cli-roots-file="$cli_roots_file" \
  --cli-required-file="$cli_required_file" \
  --env-roots-value="${SKILLWICK_SOURCE_ROOTS:-}" \
  --env-required-value="${SKILLWICK_REQUIRED_SKILLS:-}" \
  --allow-test-harness-legacy \
  --explicit-config="$local_config" \
  --automatic-config="$automatic_config" \
  --roots-file="$corpus_roots" \
  --files-file="$corpus_files" \
  --auto-marker="$auto_marker" \
  --required-file="$required_file"

hash_inputs() {
  PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-local-corpus.py" hash \
    --roots-file="$corpus_roots" --files-file="$corpus_files"
}

local_run() {
  env HOME="$developer_home" CODEX_HOME="$codex_home" \
    CLAUDE_CONFIG_DIR="$claude_home" \
    XDG_CONFIG_HOME="$temporary/explicit-config" \
    XDG_CACHE_HOME="$temporary/explicit-cache" \
    XDG_STATE_HOME="$temporary/explicit-state" TMPDIR="$temporary/explicit-tmp" \
    "$binary" --cwd "$source_cwd" --config "$local_config" "$@"
}

automatic_run() {
  env HOME="$developer_home" CODEX_HOME="$codex_home" \
    CLAUDE_CONFIG_DIR="$claude_home" \
    XDG_CONFIG_HOME="$temporary/automatic-config" \
    XDG_CACHE_HOME="$temporary/automatic-cache" \
    XDG_STATE_HOME="$temporary/automatic-state" TMPDIR="$temporary/automatic-tmp" \
    "$binary" --cwd "$source_cwd" --config "$automatic_config" "$@"
}

check_required_doctor() {
  runner=$1
  [ -s "$required_file" ] || return 0
  # Required names can contain spaces. Newlines are not valid skill names, so
  # converting the NUL records to lines keeps each name as one shell argument.
  tr '\0' '\n' <"$required_file" | while IFS= read -r required; do
    [ -n "$required" ] || continue
    if ! "$runner" doctor --strict --require "$required" >"$temporary/required-doctor"; then
      cat "$temporary/required-doctor" >&2
      return 1
    fi
  done
}

mkdir -p \
  "$temporary/explicit-config" "$temporary/explicit-cache" "$temporary/explicit-state" \
  "$temporary/explicit-tmp" "$temporary/automatic-config" "$temporary/automatic-cache" \
  "$temporary/automatic-state" "$temporary/automatic-tmp"

before_inputs=$(hash_inputs)
if [ -s "$auto_marker" ]; then
  automatic_inventory_file="$temporary/automatic-inventory.json"
  automatic_doctor_file="$temporary/automatic-doctor.json"
  automatic_run --json list >"$automatic_inventory_file"
  automatic_run --json doctor --strict >"$automatic_doctor_file"
  PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-local-corpus.py" check \
    --cache="$temporary/automatic-cache/skillwick/index-v4.sqlite" \
    --inventory-file="$automatic_inventory_file" --doctor-file="$automatic_doctor_file"
  check_required_doctor automatic_run >/dev/null
  echo "automatic corpus: installed binary verified against provider-resolved sources"
else
  echo "automatic corpus: skipped (no configured or supported provider sources)"
fi

local_inventory_file="$temporary/local-inventory.json"
local_doctor_file="$temporary/local-doctor.json"
local_run --json list >"$local_inventory_file"
local_run --json doctor --strict >"$local_doctor_file"
PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-local-corpus.py" check \
  --cache="$temporary/explicit-cache/skillwick/index-v4.sqlite" \
  --inventory-file="$local_inventory_file" --doctor-file="$local_doctor_file"
# Required names describe the complete provider inventory when available.
# Explicit roots alone may intentionally contain only a subset or use native names.
[ -s "$auto_marker" ] || check_required_doctor local_run >/dev/null
after_inputs=$(hash_inputs)
[ "$before_inputs" = "$after_inputs" ] || {
  echo "local source or agent configuration changed during verification" >&2
  exit 1
}
echo "local corpus: installed binary verified against read-only sources"

echo "source install verification passed: skillwick $version"
