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

run init --yes --agent codex --catalog native --codex-bin "$codex"
test "$(run list | grep -c '^skillwick@' || true)" -eq 0
test "$(run search 'requested working directory' | grep -c '^project-only@')" -eq 1
run doctor --strict >/dev/null
grep -Fxq "@$codex_home/SKILLWICK.md" "$codex_home/AGENTS.md"
grep -q 'Skillwick helps find relevant installed skills.' "$codex_home/SKILLWICK.md"
grep -Fq '`skillwick --json list`' "$codex_home/SKILLWICK.md"
grep -Fq 'skillwick search "deploy an AgentCore MCP server with TypeScript"' "$codex_home/SKILLWICK.md"
grep -Fq 'Use `skillwick search "task"`' "$codex_home/SKILLWICK.md"
grep -Fq 'choose only relevant results (or select' "$codex_home/SKILLWICK.md"
grep -Fq 'skillwick inspect ID --files' "$codex_home/SKILLWICK.md"
grep -q 'skillwick search.*--limit 3' "$codex_home/SKILLWICK.md"
grep -q 'skillwick read ID' "$codex_home/SKILLWICK.md"
grep -q 'skillwick inspect ID' "$codex_home/SKILLWICK.md"
grep -q 'skillwick refresh' "$codex_home/SKILLWICK.md"
grep -q 'skillwick doctor' "$codex_home/SKILLWICK.md"
grep -q 'skillwick doctor --strict' "$codex_home/SKILLWICK.md"
! grep -q 'skillwick init' "$codex_home/SKILLWICK.md"
! grep -Eiq 'delegate|sub.?agent|perspective|root agent|every turn|every prompt|model' \
  "$codex_home/SKILLWICK.md"
! grep -Eq 'skillwick "[^`]+"' "$codex_home/SKILLWICK.md"
# A new binary may replace its own unchanged context from an older release.
printf 'older owned context\n' >"$codex_home/SKILLWICK.md"
older_hash="$(shasum -a 256 "$codex_home/SKILLWICK.md" | awk '{print $1}')"
jq --arg hash "$older_hash" '.context_hash = $hash' \
  "$temporary/state/skillwick/integration.json" >"$temporary/older-journal.json"
mv "$temporary/older-journal.json" "$temporary/state/skillwick/integration.json"
run init --yes --agent codex --catalog native --codex-bin "$codex"
grep -q 'Skillwick helps find relevant installed skills.' "$codex_home/SKILLWICK.md"
grep -q 'include_instructions = false' "$codex_home/config.toml"
grep -q 'caveman' "$codex_home/hooks.json"

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
test ! -e "$codex_home/SKILLWICK.md"

# An external manager owns instructions and Codex settings while Skillwick owns
# only its configuration and inventory snapshot.
managed="$temporary/managed"
managed_home="$managed/home"
managed_codex="$managed/codex"
managed_work="$managed/work"
mkdir -p "$managed_home" "$managed_codex" "$managed_work/.agents/skills/managed-source"
printf '%s\n' '---' 'name: managed-source' 'description: Verify externally managed setup.' '---' \
  >"$managed_work/.agents/skills/managed-source/SKILL.md"
env HOME="$managed_home" CODEX_HOME="$managed_codex" "$binary" instructions \
  >"$managed_codex/SKILLWICK.md"
printf '@%s/SKILLWICK.md\n' "$managed_codex" >"$managed_codex/AGENTS.md"
printf '%s\n' 'skills.include_instructions = false' >"$managed_codex/config.toml"
managed_run() {
  env HOME="$managed_home" CODEX_HOME="$managed_codex" XDG_CONFIG_HOME="$managed/config" \
    XDG_CACHE_HOME="$managed/cache" XDG_STATE_HOME="$managed/state" \
    "$binary" --cwd "$managed_work" "$@"
}
managed_run init --yes --agent none --inventory codex --codex-bin "$codex"
managed_result=$(managed_run search 'externally managed setup')
printf '%s\n' "$managed_result" | grep -q '^managed-source@'
managed_id=$(printf '%s\n' "$managed_result" | sed -n 's/^\([^ ]*@[^ ]*\).*/\1/p')
managed_run read "$managed_id" | grep -q '^name: managed-source$'
managed_run doctor --strict >/dev/null
grep -Fxq "@$managed_codex/SKILLWICK.md" "$managed_codex/AGENTS.md"
grep -Fxq 'skills.include_instructions = false' "$managed_codex/config.toml"
managed_cache="$managed/cache/skillwick/index-v3.sqlite"
cp "$managed_cache" "$managed/cache.before-failure"
printf '%s\n' '#!/bin/sh' 'if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; else exit 1; fi' \
  >"$managed/codex-fail"
chmod +x "$managed/codex-fail"
if managed_run init --yes --agent none --inventory codex --codex-bin "$managed/codex-fail" \
  >/dev/null 2>&1; then
  exit 1
else
  test "$?" -eq 3
fi
cmp -s "$managed/cache.before-failure" "$managed_cache"
managed_run search 'externally managed setup' | grep -q '^managed-source@'

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
legacy_run init --yes --agent codex --catalog native --codex-bin "$codex"
# Simulate interruption after the v2 journal write but before AGENTS replacement.
printf '%s\n' '<!-- skillwick:begin -->' 'legacy instructions' '<!-- skillwick:end -->' \
  >"$legacy_codex/AGENTS.md"
legacy_run init --yes --agent codex --catalog native --codex-bin "$codex"
grep -Fxq "@$legacy_codex/SKILLWICK.md" "$legacy_codex/AGENTS.md"
! grep -q 'skillwick:begin' "$legacy_codex/AGENTS.md"
test ! -e "$legacy_router"
legacy_run uninstall
! grep -q "@$legacy_codex/SKILLWICK.md" "$legacy_codex/AGENTS.md"
test ! -e "$legacy_codex/SKILLWICK.md"

# A legacy journaled Skillwick hook is retired without touching an unrelated handler.
legacy_hooks="$temporary/legacy-hooks"
legacy_hooks_home="$legacy_hooks/home"
legacy_hooks_codex="$legacy_hooks/codex"
legacy_hooks_state="$legacy_hooks/state/skillwick"
legacy_hooks_command="'$binary' hook"
mkdir -p "$legacy_hooks_home" "$legacy_hooks_codex" "$legacy_hooks_state" \
  "$legacy_hooks/config" "$legacy_hooks/cache" "$legacy_hooks/work/.agents/skills/source"
printf '%s\n' '---' 'name: source' 'description: Existing specialist skill.' '---' \
  >"$legacy_hooks/work/.agents/skills/source/SKILL.md"
jq -n \
  --arg command "$legacy_hooks_command" \
  '{description:"existing hooks",hooks:{UserPromptSubmit:[
    {hooks:[{type:"command",command:"caveman"}]},
    {hooks:[{type:"command",command:$command,timeout:2,statusMessage:"Finding relevant skills with Skillwick"}]}
  ],Stop:[]}}' >"$legacy_hooks_codex/hooks.json"
jq -n \
  --arg instructions "$legacy_hooks_codex/AGENTS.md" \
  --arg config "$legacy_hooks_codex/config.toml" \
  --arg hook_file "$legacy_hooks_codex/hooks.json" \
  --arg command "$legacy_hooks_command" \
  '{version:2,instructions_file:$instructions,context_file:null,context_hash:null,context_file_created:false,reference:null,reference_added:false,legacy_block:false,router_file:null,codex_config:$config,previous_catalog:null,wrote_catalog:false,router_hash:null,hook_file:$hook_file,hook_command:$command,hook_file_created:false}' \
  >"$legacy_hooks_state/integration.json"
legacy_hooks_run() {
  env HOME="$legacy_hooks_home" CODEX_HOME="$legacy_hooks_codex" XDG_CONFIG_HOME="$legacy_hooks/config" \
    XDG_CACHE_HOME="$legacy_hooks/cache" XDG_STATE_HOME="$legacy_hooks/state" \
    "$binary" --cwd "$legacy_hooks/work" "$@"
}
legacy_hooks_run init --yes --agent codex --catalog native --codex-bin "$codex"
grep -Fq 'caveman' "$legacy_hooks_codex/hooks.json"
! grep -Fq "$legacy_hooks_command" "$legacy_hooks_codex/hooks.json"
grep -Fq '"Stop"' "$legacy_hooks_codex/hooks.json"
test "$(jq -r '.hook_command' "$legacy_hooks_state/integration.json")" = null

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
  --agent codex --catalog native --codex-bin "$codex" >/dev/null 2>&1; then
  exit 1
fi
grep -Fxq 'borrowed context' "$borrowed_codex/SKILLWICK.md"

failed="$temporary/failed"
mkdir -p "$failed/home" "$failed/codex" "$failed/work/.agents/skills/source"
printf '%s\n' '#!/bin/sh' 'if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; else exit 1; fi' > "$failed/codex-fail"
chmod +x "$failed/codex-fail"
printf '%s\n' '---' 'name: source' 'description: Existing specialist skill.' '---' > "$failed/work/.agents/skills/source/SKILL.md"
if env HOME="$failed/home" CODEX_HOME="$failed/codex" XDG_CONFIG_HOME="$failed/config" XDG_CACHE_HOME="$failed/cache" XDG_STATE_HOME="$failed/state" "$binary" --cwd "$failed/work" init --yes --agent codex --catalog native --codex-bin "$failed/codex-fail" >/dev/null 2>&1; then
  exit 1
fi
test ! -e "$failed/config/skillwick/config.toml"
test ! -e "$failed/codex/AGENTS.md"
test ! -e "$failed/codex/config.toml"
test ! -e "$failed/codex/SKILLWICK.md"

# Native snapshots are partitioned by normalized workspace and Codex home.
partitions="$temporary/partitions"
partition_home="$partitions/home"
partition_codex="$partitions/codex"
partition_work_a="$partitions/work-a"
partition_work_b="$partitions/work-b"
partition_work_c="$partitions/work-c"
partition_work_d="$partitions/work-d"
mkdir -p "$partition_home" "$partition_codex" "$partition_work_a" "$partition_work_b" \
  "$partition_work_c" "$partition_work_d" "$partitions/config" "$partitions/cache" \
  "$partitions/state"
for native_name in native-a native-b native-c native-d; do
  mkdir -p "$partition_codex/skills/$native_name"
  printf '%s\n' '---' "name: $native_name" "description: $native_name partition skill." '---' \
    >"$partition_codex/skills/$native_name/SKILL.md"
done
partition_codex_bin="$partitions/codex-bin"
printf '%s\n' \
  '#!/bin/sh' \
  'if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; exit 0; fi' \
  'if [ "$1" != "app-server" ]; then exit 1; fi' \
  'IFS= read -r initialize' \
  'IFS= read -r initialized' \
  'IFS= read -r request' \
  'workspace=$(printf "%s\n" "$request" | jq -r ".params.cwds[0]")' \
  'case "$workspace" in *work-a) native_name=native-a ;; *work-b) native_name=native-b ;; *work-c) native_name=native-c ;; *work-d) native_name=native-d ;; *) exit 1 ;; esac' \
  'printf "%s\n" "$workspace" >> "$CODEX_HOME/inventory.log"' \
  'sleep 0.1' \
  'skill_path="$CODEX_HOME/skills/$native_name/SKILL.md"' \
  'jq -cn --arg cwd "$workspace" --arg name "$native_name" --arg path "$skill_path" "{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"data\":[{\"cwd\":\$cwd,\"skills\":[{\"name\":\$name,\"description\":\$name,\"interface\":null,\"path\":\$path,\"scope\":\"user\",\"enabled\":true,\"pluginId\":null,\"shortDescription\":null}],\"errors\":[]}]}}"' \
  >"$partition_codex_bin"
chmod +x "$partition_codex_bin"
partition_run() {
  partition_workspace=$1
  shift
  env HOME="$partition_home" CODEX_HOME="$partition_codex" \
    XDG_CONFIG_HOME="$partitions/config" XDG_CACHE_HOME="$partitions/cache" \
    XDG_STATE_HOME="$partitions/state" "$binary" --cwd "$partition_workspace" "$@"
}
partition_run "$partition_work_a" init --yes --agent none --inventory codex \
  --codex-home "$partition_codex" --codex-bin "$partition_codex_bin" >/dev/null
partition_run "$partition_work_a" list | grep -q '^native-a@'
partition_run "$partition_work_a/." --json doctor >"$partitions/doctor-relative"
partition_total="$(partition_run "$partition_work_a" --json list --all | jq -r '.total')"
test "$(jq -r '.counts.model_discoverable' "$partitions/doctor-relative")" -eq "$partition_total"
test "$(jq -r '.native_snapshot_current' "$partitions/doctor-relative")" = true
ln -s "$partition_work_a" "$partitions/work-link"
partition_run "$partitions/work-link" --json doctor >"$partitions/doctor-symlink"
test "$(jq -r '.native_snapshot_current' "$partitions/doctor-symlink")" = true
calls_before="$(wc -l <"$partition_codex/inventory.log" | tr -d ' ')"
partition_run "$partition_work_b" list >"$partitions/list-b" 2>"$partitions/list-b.err"
grep -q '^native-b@' "$partitions/list-b"
! grep -q '^native-a@' "$partitions/list-b"
grep -Fxq 'notice: native inventory cache misses this context; refreshing' "$partitions/list-b.err"
calls_after_b="$(wc -l <"$partition_codex/inventory.log" | tr -d ' ')"
test "$calls_after_b" -eq $((calls_before + 1))
partition_run "$partition_work_a" list >"$partitions/list-a" 2>"$partitions/list-a.err"
grep -q '^native-a@' "$partitions/list-a"
! grep -q '^native-b@' "$partitions/list-a"
calls_after_a="$(wc -l <"$partition_codex/inventory.log" | tr -d ' ')"
test "$calls_after_a" -eq "$calls_after_b"

# A second Codex home gets its own partition without hiding the first home.
partition_alt_codex="$partitions/codex-alt"
partition_alt_config="$partitions/config-alt"
mkdir -p "$partition_alt_codex/skills/native-a" "$partition_alt_config"
cp "$partition_codex/skills/native-a/SKILL.md" "$partition_alt_codex/skills/native-a/SKILL.md"
partition_run_alt() {
  partition_workspace=$1
  shift
  env HOME="$partition_home" CODEX_HOME="$partition_alt_codex" \
    XDG_CONFIG_HOME="$partition_alt_config" XDG_CACHE_HOME="$partitions/cache" \
    XDG_STATE_HOME="$partitions/state" "$binary" --cwd "$partition_workspace" "$@"
}
partition_run_alt "$partition_work_a" init --yes --agent none --inventory codex \
  --codex-home "$partition_alt_codex" --codex-bin "$partition_codex_bin" >/dev/null
partition_run_alt "$partition_work_a" list | grep -q '^native-a@'
partition_run "$partition_work_b" list | grep -q '^native-b@'

# A failed refresh leaves the last published cache byte-for-byte intact.
partition_fail_bin="$partitions/codex-fail"
printf '%s\n' '#!/bin/sh' \
  'if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; exit 0; fi' \
  'exit 1' >"$partition_fail_bin"
chmod +x "$partition_fail_bin"
cp "$partitions/cache/skillwick/index-v3.sqlite" "$partitions/cache.before-failure"
mkdir -p "$partitions/work-fail"
if partition_run "$partitions/work-fail" init --yes --agent none --inventory codex \
  --codex-bin "$partition_fail_bin" >/dev/null 2>&1; then
  exit 1
else
  test "$?" -eq 3
fi
cmp -s "$partitions/cache.before-failure" "$partitions/cache/skillwick/index-v3.sqlite"
partition_run "$partition_work_a" list | grep -q '^native-a@'
partition_run "$partition_work_b" list | grep -q '^native-b@'

# Concurrent refreshes serialize publication and retain both new partitions.
partition_run "$partition_work_c" refresh >"$partitions/refresh-c" 2>&1 & refresh_c=$!
partition_run "$partition_work_d" refresh >"$partitions/refresh-d" 2>&1 & refresh_d=$!
wait "$refresh_c"
wait "$refresh_d"
partition_run "$partition_work_c" list | grep -q '^native-c@'
! partition_run "$partition_work_c" list | grep -q '^native-d@'
partition_run "$partition_work_d" list | grep -q '^native-d@'
! partition_run "$partition_work_d" list | grep -q '^native-c@'
echo "Codex integration passed"
