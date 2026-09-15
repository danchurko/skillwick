#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
developer_home=${HOME:?HOME is required}
shared_agents="$developer_home/.agents/skills"
managed_agents="$developer_home/.local/state/mac-state/agents/codex/skills"
codex_agents="$developer_home/.codex/skills"
temporary=$(mktemp -d /private/tmp/skillwick-source-install.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
prefix="$temporary/prefix"
binary="$prefix/bin/skillwick"

for skill_root in "$shared_agents" "$managed_agents" "$codex_agents"; do
  test -d "$skill_root" || {
    echo "required local skill root is unavailable: $skill_root" >&2
    exit 1
  }
done

CARGO_TARGET_DIR="$temporary/target" cargo install --locked --path "$root" --root "$prefix"
test "$($binary --version)" = "skillwick $(awk -F '"' '/^version = / { print $2; exit }' "$root/Cargo.toml")"

# Run the public fixtures with the installed executable. Together they cover
# freshness, isolation, recovery, concurrency, trust boundaries, live reads,
# bounded inspection, and package non-execution.
sh "$root/tests/cli_acceptance.sh" "$binary"
sh "$root/tests/trust_boundary.sh" "$binary"
sh "$root/tests/filesystem_integration.sh" "$binary"

hash_inputs() {
  for skill_root in "$shared_agents" "$managed_agents" "$codex_agents"; do
    find -L "$skill_root" -type f \( -name SKILL.md -o -path '*/agents/openai.yaml' \) -print
  done | LC_ALL=C sort | while IFS= read -r path; do
    shasum -a 256 "$path"
  done | shasum -a 256 | awk '{print $1}'
}

before_inputs=$(hash_inputs)
home="$temporary/home"
config_home="$temporary/config"
cache_home="$temporary/cache"
state_home="$temporary/state"
workspace="$temporary/workspace"
workspace_child="$workspace/nested/child"
workspace_b="$temporary/workspace-b"
project_root="$temporary/project-skills"
project_root_b="$temporary/project-skills-b"
sentinel="$temporary/sentinel"
mkdir -p "$home" "$config_home" "$cache_home" "$state_home" "$workspace_child" \
  "$workspace_b" "$project_root/source-install" "$project_root_b/source-install-b" "$sentinel"
printf '%s\n' '---' 'name: source-install' \
  'description: Verify a fresh installed binary against project scope.' '---' \
  >"$project_root/source-install/SKILL.md"
printf '%s\n' '---' 'name: source-install-b' \
  'description: Verify a second installed-binary project scope.' '---' \
  >"$project_root_b/source-install-b/SKILL.md"
printf '%s\n' '#!/bin/sh' "touch '$temporary/codex-invoked'" 'exit 99' >"$sentinel/codex"
chmod +x "$sentinel/codex"

run() {
  env HOME="$home" CODEX_HOME="$temporary/unavailable-codex-home" \
    PATH="$sentinel:$PATH" XDG_CONFIG_HOME="$config_home" \
    XDG_CACHE_HOME="$cache_home" XDG_STATE_HOME="$state_home" \
    "$binary" --cwd "$workspace" "$@"
}
run_child() {
  env HOME="$home" CODEX_HOME="$temporary/unavailable-codex-home" \
    PATH="$sentinel:$PATH" XDG_CONFIG_HOME="$config_home" \
    XDG_CACHE_HOME="$cache_home" XDG_STATE_HOME="$state_home" \
    "$binary" --cwd "$workspace_child" "$@"
}
run_b() {
  env HOME="$home" CODEX_HOME="$temporary/unavailable-codex-home" \
    PATH="$sentinel:$PATH" XDG_CONFIG_HOME="$config_home" \
    XDG_CACHE_HOME="$cache_home" XDG_STATE_HOME="$state_home" \
    "$binary" --cwd "$workspace_b" "$@"
}

run init --yes --agent none \
  --root "$shared_agents" \
  --root "$managed_agents" \
  --root "$codex_agents" \
  --project-root "$project_root" \
  --project-root "$shared_agents"

inventory=$(run --json list)
total=$(printf '%s\n' "$inventory" | jq -r '.total')
test "$total" -gt 0
project_id=$(printf '%s\n' "$inventory" | jq -r \
  '.results[] | select(.name == "source-install") | .id' | sed -n '1p')
test -n "$project_id"
run search 'fresh installed binary project scope' | grep -q '^source-install@'
run_child list | grep -q '^source-install@'
run read "$project_id" | grep -q '^name: source-install$'
run --json inspect "$project_id" | jq -e '.results[0].name == "source-install"' >/dev/null
run --json inspect "$project_id" --files | jq -e '.package.counts_scope' >/dev/null

real_id=$(printf '%s\n' "$inventory" | jq -r --arg project "$project_root" \
  '.results[] | select(.source != $project) | .id' | sed -n '1p')
real_name=$(printf '%s\n' "$inventory" | jq -r --arg id "$real_id" \
  '.results[] | select(.id == $id) | .name')
test -n "$real_id"
run search "$real_name" >/dev/null
run read "$real_id" >/dev/null
run --json inspect "$real_id" >/dev/null

doctor=$(run --json doctor --strict)
cache="$cache_home/skillwick/index-v3.sqlite"
canonical=$(sqlite3 "$cache" "SELECT count(*) FROM skills WHERE source_kind='filesystem';")
raw=$(sqlite3 "$cache" 'SELECT count(*) FROM skill_roots;')
discoverable=$(sqlite3 "$cache" \
  "SELECT count(*) FROM skills WHERE source_kind='filesystem' AND enabled=1 AND model_discoverable=1;")
test "$(printf '%s\n' "$doctor" | jq -r '.counts.filesystem')" -eq "$canonical"
test "$(printf '%s\n' "$doctor" | jq -r '.counts.raw')" -eq "$raw"
test "$total" -eq "$discoverable"
test "$raw" -ge "$canonical"

# Unchanged reconciliation must not replace SQLite.
before_cache=$(shasum -a 256 "$cache" | awk '{print $1}')
before_mtime=$(stat -f '%m' "$cache")
sleep 1
run list >/dev/null
test "$(shasum -a 256 "$cache" | awk '{print $1}')" = "$before_cache"
test "$(stat -f '%m' "$cache")" = "$before_mtime"

# Keep the real shared roots registered while controlled project material
# changes and two project scopes reconcile concurrently.
printf '%s\n' '---' 'name: source-install' \
  'description: Fresh project edit beside the real shared corpus.' '---' \
  >"$project_root/source-install/SKILL.md"
run search 'Fresh project edit beside the real shared corpus' | grep -q '^source-install@'
mkdir -p "$project_root/source-install/agents"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' \
  >"$project_root/source-install/agents/openai.yaml"
! run list | grep -q '^source-install@'
rm "$project_root/source-install/agents/openai.yaml"
run list | grep -q '^source-install@'
run_b init --yes --agent none --project-root "$project_root_b" >/dev/null
! run list | grep -q '^source-install-b@'
run_b list | grep -q '^source-install-b@'
run list >/dev/null & first=$!
run_b list >/dev/null & second=$!
wait "$first"
wait "$second"
! run list | grep -q '^source-install-b@'
run_b list | grep -q '^source-install-b@'

# A failed alternate configuration preserves the last publication and recovers
# on the next ordinary lookup after its controlled root is repaired.
missing="$temporary/missing-root"
missing_config="$temporary/missing.toml"
printf 'roots = ["%s", "%s"]\nagent = "none"\n' "$project_root" "$missing" >"$missing_config"
cp "$cache" "$temporary/cache-before-failure"
if run --config "$missing_config" list >"$temporary/missing.out" 2>&1; then
  echo "missing configured root unexpectedly succeeded" >&2
  exit 1
else
  test "$?" -eq 3
fi
grep -q 'configured root does not exist' "$temporary/missing.out"
cmp -s "$temporary/cache-before-failure" "$cache"
mkdir -p "$missing/recovered"
printf '%s\n' '---' 'name: recovered' 'description: Recovered source.' '---' \
  >"$missing/recovered/SKILL.md"
run --config "$missing_config" list | grep -q '^recovered@'

test ! -e "$temporary/codex-invoked"
after_inputs=$(hash_inputs)
test "$before_inputs" = "$after_inputs"
test "$(sqlite3 "$cache" 'PRAGMA integrity_check;')" = ok

printf '%s\n' \
  "installed binary: $binary" \
  "shared roots: $shared_agents | $managed_agents | $codex_agents" \
  "local inventory: discoverable=$total canonical=$canonical raw=$raw" \
  'native process invoked: no' \
  'installed source and local-corpus verification passed'
