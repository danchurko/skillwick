#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
model=${SKILLWICK_INFERENCE_MODEL:-gpt-5.6-luna}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
tmp_parent=${TMPDIR:-/tmp}
[ -d "$tmp_parent" ] || { echo "temporary directory does not exist: $tmp_parent" >&2; exit 1; }
temporary=$(mktemp -d "${tmp_parent%/}/skillwick-inference.XXXXXX")
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
work="$temporary/work"
home="$temporary/home"
mkdir -p "$work/.agents/skills/cobalt-runtime" "$home" "$temporary/config" "$temporary/cache" "$temporary/state"
printf '%s\n' '---' 'name: cobalt-runtime' 'description: Deploy and diagnose the Cobalt runtime.' '---' 'Verification marker: COBALT_SKILL_LOADED.' > "$work/.agents/skills/cobalt-runtime/SKILL.md"

environment="HOME='$home' XDG_CONFIG_HOME='$temporary/config' XDG_CACHE_HOME='$temporary/cache' XDG_STATE_HOME='$temporary/state'"
# Filesystem roots are explicit; configure the temporary fixture before asking
# Codex to exercise search and read through the installed binary.
env HOME="$home" XDG_CONFIG_HOME="$temporary/config" \
  XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" \
  "$binary" --cwd "$work" init --yes --agent none --discovery explicit --root "$work/.agents/skills" >/dev/null
prompt="This is a Skillwick release smoke test. First run: env $environment '$binary' --cwd '$work' search 'deploy cobalt runtime'. Copy the returned ID. Then run: env $environment '$binary' --cwd '$work' read ID, replacing ID with that exact value. If the instruction contains COBALT_SKILL_LOADED, reply with exactly SKILLWICK_INFERENCE_OK. Do not modify files."
codex e "$prompt" --model "$model" > "$temporary/output"
grep -q '^SKILLWICK_INFERENCE_OK$' "$temporary/output"
echo "inference smoke test passed: $model"
