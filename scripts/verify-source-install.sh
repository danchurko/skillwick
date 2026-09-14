#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
codex=${SKILLWICK_RELEASE_CODEX:-$(command -v codex)}
developer_home=${HOME:?HOME is required}
codex_home=${CODEX_HOME:-$developer_home/.codex}
temporary=$(mktemp -d /private/tmp/skillwick-source-install.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
prefix="$temporary/prefix"
binary="$prefix/bin/skillwick"
workspace="$temporary/workspace"

CARGO_TARGET_DIR="$temporary/target" cargo install --locked --path "$root" --root "$prefix"
test "$($binary --version)" = "skillwick $(awk -F '"' '/^version = / { print $2; exit }' "$root/Cargo.toml")"

mkdir -p "$workspace/.agents/skills/source-install" "$temporary/home"
printf '%s\n' '---' 'name: source-install' \
  'description: Verify a fresh source installation end to end.' '---' \
  >"$workspace/.agents/skills/source-install/SKILL.md"

filesystem_run() {
  env HOME="$temporary/home" CODEX_HOME="$temporary/codex-isolated" \
    XDG_CONFIG_HOME="$temporary/filesystem-config" \
    XDG_CACHE_HOME="$temporary/filesystem-cache" \
    XDG_STATE_HOME="$temporary/filesystem-state" \
    "$binary" --cwd "$workspace" "$@"
}
filesystem_run init --yes --agent none --inventory filesystem
filesystem_result=$(filesystem_run search 'fresh source installation end to end')
printf '%s\n' "$filesystem_result" | grep -q '^source-install@'
filesystem_id=$(printf '%s\n' "$filesystem_result" | sed -n 's/^\([^ ]*@[^ ]*\).*/\1/p')
filesystem_run read "$filesystem_id" | grep -q '^name: source-install$'
filesystem_run doctor --strict >/dev/null

test -d "$codex_home"
native_run() {
  native_workspace=$1
  shift
  env HOME="$temporary/home" CODEX_HOME="$codex_home" \
    XDG_CONFIG_HOME="$temporary/native-config" XDG_CACHE_HOME="$temporary/native-cache" \
    XDG_STATE_HOME="$temporary/native-state" \
    "$binary" --cwd "$native_workspace" "$@"
}
native_run "$root" init --yes --agent none --inventory codex \
  --codex-home "$codex_home" --codex-bin "$codex"
native_run "$root" doctor --strict >/dev/null
native_root=$(native_run "$root" --json list)
native_root_id=$(printf '%s\n' "$native_root" | jq -r \
  '.results[] | select(.source_kind == "codex") | .id' | sed -n '1p')
test -n "$native_root_id"
native_run "$root" read "$native_root_id" >/dev/null

native_first_query=$(native_run "$workspace" search 'fresh source installation end to end')
printf '%s\n' "$native_first_query" | grep -q '^source-install@'
native_run "$workspace" doctor --strict >/dev/null
native_workspace_inventory=$(native_run "$workspace" --json list)
native_workspace_id=$(printf '%s\n' "$native_workspace_inventory" | jq -r \
  '.results[] | select(.source_kind == "codex") | .id' | sed -n '1p')
test -n "$native_workspace_id"
native_run "$workspace" read "$native_workspace_id" >/dev/null
native_run "$workspace" refresh

echo "source-install release preflight passed"
