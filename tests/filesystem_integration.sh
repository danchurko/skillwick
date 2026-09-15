#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-filesystem.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

home="$temporary/home"
shared="$temporary/shared"
project="$temporary/project"
child="$project/nested/child"
config="$temporary/config"
cache="$temporary/cache"
state="$temporary/state"
no_codex="$temporary/no-codex"
mkdir -p "$home" "$shared" "$child" "$config" "$cache" "$state" "$no_codex"

write_skill() {
  directory=$1
  name=$2
  description=$3
  mkdir -p "$directory/$name"
  printf '%s\n' '---' "name: $name" "description: $description" '---' \
    >"$directory/$name/SKILL.md"
}
write_skill "$shared" shared "Shared filesystem source."
write_skill "$project" project "Project filesystem source."

run() {
  env HOME="$home" CODEX_HOME="$temporary/missing-codex-home" PATH="$no_codex" \
    XDG_CONFIG_HOME="$config" XDG_CACHE_HOME="$cache" XDG_STATE_HOME="$state" \
    "$binary" --cwd "$project" "$@"
}
run_child() {
  env HOME="$home" CODEX_HOME="$temporary/missing-codex-home" PATH="$no_codex" \
    XDG_CONFIG_HOME="$config" XDG_CACHE_HOME="$cache" XDG_STATE_HOME="$state" \
    "$binary" --cwd "$child" "$@"
}

# The old Codex-specific integration fixture is deliberately gone. This
# end-to-end binary check proves that no executable, CODEX_HOME, or provider
# state is needed for filesystem inventory.
run init --yes --agent none --root "$shared" --project-root "$project"
run list | grep -q '^2 skills in the current inventory\.$'
run search 'Project filesystem source' | grep -q '^project@'
run_child list | grep -q '^project@'
run doctor --strict >/dev/null

cache_file="$cache/skillwick/index-v3.sqlite"
before_hash=$(shasum -a 256 "$cache_file" | awk '{print $1}')
run_child list >/dev/null
test "$(shasum -a 256 "$cache_file" | awk '{print $1}')" = "$before_hash"

# Concurrent ordinary refreshes remain serialized and publish a valid merged
# snapshot without cross-project leakage.
project_b="$temporary/project-b"
write_skill "$project_b" project-b "Second project source."
run_b() {
  env HOME="$home" CODEX_HOME="$temporary/missing-codex-home" PATH="$no_codex" \
    XDG_CONFIG_HOME="$config" XDG_CACHE_HOME="$cache" XDG_STATE_HOME="$state" \
    "$binary" --cwd "$project_b" "$@"
}
run_b init --yes --agent none --root "$shared" --project-root "$project_b" >/dev/null
run refresh >/dev/null & first=$!
run_b refresh >/dev/null & second=$!
wait "$first"
wait "$second"
run list | grep -q '^2 skills in the current inventory\.$'
run list | grep -q '^shared@'
run list | grep -q '^project@'
! run list | grep -q '^project-b@'
run_b list | grep -q '^2 skills in the current inventory\.$'
run_b list | grep -q '^shared@'
run_b list | grep -q '^project-b@'
! run_b list | grep -q '^project@'
test "$(sqlite3 "$cache_file" 'PRAGMA integrity_check;')" = ok
test "$(sqlite3 "$cache_file" "SELECT count(*) FROM skills WHERE source_kind='filesystem';")" -eq 3

# Provider-specific flags are rejected instead of silently reintroducing the
# removed native mode.
for obsolete in \
  '--inventory codex' '--catalog native' '--codex-home /tmp/codex' '--codex-bin /tmp/codex'; do
  if run $obsolete list >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
done

echo "Filesystem integration passed"
