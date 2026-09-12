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

run() {
  env HOME="$home" CODEX_HOME="$codex_home" XDG_CONFIG_HOME="$temporary/config" XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" "$binary" --cwd "$temporary/work" "$@"
}

run init --yes --agent codex --catalog native --hooks off --codex-bin "$codex"
test "$(run 'find relevant installed local skills' | grep -c '^skillwick@')" -eq 1
test "$(run 'requested working directory' | grep -c '^project-only@')" -eq 1
run doctor --strict >/dev/null
grep -q '<!-- skillwick:begin -->' "$codex_home/AGENTS.md"
grep -q 'include_instructions = false' "$codex_home/config.toml"

env HOME="$home" CODEX_HOME="$codex_home" "$codex" debug prompt-input -c skills.include_instructions=false 'find specialist guidance' > "$temporary/hidden.json"
env HOME="$home" CODEX_HOME="$codex_home" "$codex" debug prompt-input -c skills.include_instructions=true 'find specialist guidance' > "$temporary/native.json"
grep -q 'skillwick:begin' "$temporary/hidden.json"
! grep -Eq '<skills_instructions>|Available skills' "$temporary/hidden.json"
grep -Eq '<skills_instructions>|Available skills' "$temporary/native.json"
echo "prompt-input hidden_bytes=$(wc -c < "$temporary/hidden.json" | tr -d ' ') native_bytes=$(wc -c < "$temporary/native.json" | tr -d ' ')"

printf '%s\n' 'after setup instruction' >> "$codex_home/AGENTS.md"
printf '%s\n' 'after_setup = true' >> "$codex_home/config.toml"
run uninstall --purge-cache
grep -q 'after setup instruction' "$codex_home/AGENTS.md"
grep -q 'after_setup = true' "$codex_home/config.toml"
! grep -Eq 'skillwick:begin|include_instructions' "$codex_home/AGENTS.md" "$codex_home/config.toml"
test ! -e "$home/.agents/skills/skillwick/SKILL.md"

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
test ! -e "$failed/home/.agents/skills/skillwick/SKILL.md"
echo "Codex integration passed"
