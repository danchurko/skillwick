#!/bin/sh
set -eu

binary=${1:-target/release/skillwick}
codex=${2:-codex}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
case "$codex" in /*) ;; *) codex=$(command -v "$codex") ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-benchmark.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
codex_home=${CODEX_HOME:-"$HOME/.codex"}

run() {
  env CODEX_HOME="$codex_home" \
    XDG_CACHE_HOME="$temporary/cache" \
    XDG_CONFIG_HOME="$temporary/config" \
    XDG_STATE_HOME="$temporary/state" \
    "$binary" --config "$temporary/config/config.toml" "$@"
}

run init --yes --agent none --inventory codex \
  --codex-home "$codex_home" --codex-bin "$codex" >/dev/null 2>&1
run benchmark --native-only --dataset benchmarks/local-skills-v1.json
