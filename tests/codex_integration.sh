#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
codex=${2:-codex}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
case "$codex" in /*) ;; *) codex=$(command -v "$codex") ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-codex.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
home="$temporary/home"
codex_home="$temporary/codex"
mkdir -p "$home" "$codex_home" "$temporary/config" "$temporary/cache" "$temporary/state" "$temporary/work"
mkdir -p "$temporary/work/.agents/skills/project-only"
printf '%s\n' '---' 'name: project-only' 'description: Verify init uses requested working directory.' '---' > "$temporary/work/.agents/skills/project-only/SKILL.md"
printf '%s\n' '{"description":"existing hooks","hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"caveman"}]}],"Stop":[]}}' > "$codex_home/hooks.json"

run() {
  env HOME="$home" CODEX_HOME="$codex_home" XDG_CONFIG_HOME="$temporary/config" XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" "$binary" --cwd "$temporary/work" "$@"
}

run init --yes --agent codex --catalog native --hooks suggest --codex-bin "$codex"
test "$(run list | grep -c '^skillwick@' || true)" -eq 0
test "$(run 'requested working directory' | grep -c '^project-only@')" -eq 1
run doctor --strict >/dev/null
grep -Fxq "@$codex_home/SKILLWICK.md" "$codex_home/AGENTS.md"
grep -q 'Skillwick is a skill helper' "$codex_home/SKILLWICK.md"
grep -q -- '--json list --all' "$codex_home/SKILLWICK.md"
grep -q 'brief task and important technologies' "$codex_home/SKILLWICK.md"
grep -q 'skillwick read ID' "$codex_home/SKILLWICK.md"
! grep -q 'skillwick init' "$codex_home/SKILLWICK.md"
# A new binary may replace its own unchanged context from an older release.
printf 'older owned context\n' >"$codex_home/SKILLWICK.md"
older_hash="$(shasum -a 256 "$codex_home/SKILLWICK.md" | awk '{print $1}')"
jq --arg hash "$older_hash" '.context_hash = $hash' \
  "$temporary/state/skillwick/integration.json" >"$temporary/older-journal.json"
mv "$temporary/older-journal.json" "$temporary/state/skillwick/integration.json"
run init --yes --agent codex --catalog native --hooks suggest --codex-bin "$codex"
grep -q 'Skillwick is a skill helper' "$codex_home/SKILLWICK.md"
grep -q 'include_instructions = false' "$codex_home/config.toml"
grep -q 'Finding relevant skills with Skillwick' "$codex_home/hooks.json"
grep -q 'caveman' "$codex_home/hooks.json"
printf '%s' '{"hook_event_name":"UserPromptSubmit","prompt":"requested working directory"}' | run hook | grep -q '"hookEventName":"UserPromptSubmit"'
{
  printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"skillwick-test","version":"1"},"capabilities":{"experimentalApi":true}}}' \
    '{"jsonrpc":"2.0","method":"initialized","params":{}}' \
    "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"hooks/list\",\"params\":{\"cwds\":[\"$temporary/work\"]}}"
  sleep 1
} | env HOME="$home" CODEX_HOME="$codex_home" "$codex" app-server --stdio > "$temporary/hooks-list.jsonl"
grep -q 'caveman' "$temporary/hooks-list.jsonl"
grep -q 'skillwick.*hook' "$temporary/hooks-list.jsonl"
run init --yes --agent codex --catalog native --hooks off --codex-bin "$codex"
grep -q 'caveman' "$codex_home/hooks.json"
! grep -q 'Finding relevant skills with Skillwick' "$codex_home/hooks.json"
run init --yes --agent codex --catalog native --hooks suggest --codex-bin "$codex"
grep -q 'Finding relevant skills with Skillwick' "$codex_home/hooks.json"

env HOME="$home" CODEX_HOME="$codex_home" "$codex" debug prompt-input -c skills.include_instructions=false 'find specialist guidance' > "$temporary/hidden.json"
env HOME="$home" CODEX_HOME="$codex_home" "$codex" debug prompt-input -c skills.include_instructions=true 'find specialist guidance' > "$temporary/native.json"
grep -q "@$codex_home/SKILLWICK.md" "$temporary/hidden.json"
! grep -Eq '<skills_instructions>|Available skills' "$temporary/hidden.json"
grep -Eq '<skills_instructions>|Available skills' "$temporary/native.json"
echo "prompt-input hidden_bytes=$(wc -c < "$temporary/hidden.json" | tr -d ' ') native_bytes=$(wc -c < "$temporary/native.json" | tr -d ' ')"

printf '%s\n' 'after setup instruction' >> "$codex_home/AGENTS.md"
printf '%s\n' 'after_setup = true' >> "$codex_home/config.toml"
run uninstall --purge-cache
grep -q 'after setup instruction' "$codex_home/AGENTS.md"
grep -q 'after_setup = true' "$codex_home/config.toml"
! grep -q "@$codex_home/SKILLWICK.md" "$codex_home/AGENTS.md"
! grep -q 'include_instructions' "$codex_home/config.toml"
grep -q 'caveman' "$codex_home/hooks.json"
! grep -q 'Finding relevant skills with Skillwick' "$codex_home/hooks.json"
test ! -e "$codex_home/SKILLWICK.md"

# Existing v1 integrations migrate without leaving the router skill or inline block.
legacy="$temporary/legacy"
legacy_home="$legacy/home"
legacy_codex="$legacy/codex"
legacy_router="$legacy_home/.agents/skills/skillwick/SKILL.md"
legacy_state="$legacy/state/skillwick"
mkdir -p "$legacy_home/.agents/skills/skillwick" "$legacy_state" "$legacy/config" "$legacy/cache" \
  "$legacy/work/.agents/skills/source" "$legacy_codex"
printf '%s\n' '---' 'name: skillwick' 'description: Legacy Skillwick router.' '---' \
  >"$legacy_router"
printf '%s\n' '<!-- skillwick:begin -->' 'legacy instructions' '<!-- skillwick:end -->' \
  >"$legacy_codex/AGENTS.md"
printf '%s\n' '---' 'name: source' 'description: Existing specialist skill.' '---' \
  >"$legacy/work/.agents/skills/source/SKILL.md"
legacy_hash="$(shasum -a 256 "$legacy_router" | awk '{print $1}')"
jq -n \
  --arg instructions "$legacy_codex/AGENTS.md" \
  --arg router "$legacy_router" \
  --arg config "$legacy_codex/config.toml" \
  --arg hash "$legacy_hash" \
  '{version:1,instructions_file:$instructions,router_file:$router,codex_config:$config,previous_catalog:null,wrote_catalog:false,router_hash:$hash}' \
  >"$legacy_state/integration.json"
legacy_run() {
  env HOME="$legacy_home" CODEX_HOME="$legacy_codex" XDG_CONFIG_HOME="$legacy/config" \
    XDG_CACHE_HOME="$legacy/cache" XDG_STATE_HOME="$legacy/state" \
    "$binary" --cwd "$legacy/work" "$@"
}
legacy_run init --yes --agent codex --catalog native --hooks off --codex-bin "$codex"
# Simulate interruption after the v2 journal write but before AGENTS replacement.
printf '%s\n' '<!-- skillwick:begin -->' 'legacy instructions' '<!-- skillwick:end -->' \
  >"$legacy_codex/AGENTS.md"
legacy_run init --yes --agent codex --catalog native --hooks off --codex-bin "$codex"
grep -Fxq "@$legacy_codex/SKILLWICK.md" "$legacy_codex/AGENTS.md"
! grep -q 'skillwick:begin' "$legacy_codex/AGENTS.md"
test ! -e "$legacy_router"
legacy_run uninstall
! grep -q "@$legacy_codex/SKILLWICK.md" "$legacy_codex/AGENTS.md"
test ! -e "$legacy_codex/SKILLWICK.md"

# A matching journal hash does not authorize replacing a borrowed context file.
borrowed="$temporary/borrowed"
borrowed_codex="$borrowed/codex"
mkdir -p "$borrowed/home" "$borrowed_codex" "$borrowed/config" "$borrowed/cache" \
  "$borrowed/state/skillwick" "$borrowed/work"
printf 'borrowed context\n' >"$borrowed_codex/SKILLWICK.md"
printf '@%s/SKILLWICK.md\n' "$borrowed_codex" >"$borrowed_codex/AGENTS.md"
borrowed_hash="$(shasum -a 256 "$borrowed_codex/SKILLWICK.md" | awk '{print $1}')"
jq -n \
  --arg instructions "$borrowed_codex/AGENTS.md" \
  --arg context "$borrowed_codex/SKILLWICK.md" \
  --arg reference "@$borrowed_codex/SKILLWICK.md" \
  --arg config "$borrowed_codex/config.toml" \
  --arg hash "$borrowed_hash" \
  '{version:2,instructions_file:$instructions,context_file:$context,context_hash:$hash,context_file_created:false,reference:$reference,reference_added:false,legacy_block:false,router_file:null,codex_config:$config,previous_catalog:null,wrote_catalog:false,router_hash:null}' \
  >"$borrowed/state/skillwick/integration.json"
if env HOME="$borrowed/home" CODEX_HOME="$borrowed_codex" \
  XDG_CONFIG_HOME="$borrowed/config" XDG_CACHE_HOME="$borrowed/cache" \
  XDG_STATE_HOME="$borrowed/state" "$binary" --cwd "$borrowed/work" init --yes \
  --agent codex --catalog native --hooks off --codex-bin "$codex" >/dev/null 2>&1; then
  exit 1
fi
grep -Fxq 'borrowed context' "$borrowed_codex/SKILLWICK.md"

failed="$temporary/failed"
mkdir -p "$failed/home" "$failed/codex" "$failed/work/.agents/skills/source"
printf '%s\n' '#!/bin/sh' 'if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; else exit 1; fi' > "$failed/codex-fail"
chmod +x "$failed/codex-fail"
printf '%s\n' '---' 'name: source' 'description: Existing specialist skill.' '---' > "$failed/work/.agents/skills/source/SKILL.md"
if env HOME="$failed/home" CODEX_HOME="$failed/codex" XDG_CONFIG_HOME="$failed/config" XDG_CACHE_HOME="$failed/cache" XDG_STATE_HOME="$failed/state" "$binary" --cwd "$failed/work" init --yes --agent codex --catalog native --hooks off --codex-bin "$failed/codex-fail" >/dev/null 2>&1; then
  exit 1
fi
test ! -e "$failed/config/skillwick/config.toml"
test ! -e "$failed/codex/AGENTS.md"
test ! -e "$failed/codex/config.toml"
test ! -e "$failed/codex/SKILLWICK.md"
echo "Codex integration passed"
