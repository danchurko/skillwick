#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-cli.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

home="$temporary/home"
shared="$temporary/shared"
project_a="$temporary/project-a"
project_b="$temporary/project-b"
project_a_child="$project_a/packages/child"
config_home="$temporary/config"
cache_home="$temporary/cache"
state_home="$temporary/state"
mkdir -p "$home" "$shared" "$project_a_child" "$project_b" "$config_home" \
  "$cache_home" "$state_home" "$temporary/no-codex"

write_skill() {
  directory=$1
  name=$2
  description=$3
  mkdir -p "$directory/$name"
  printf '%s\n' '---' "name: $name" "description: $description" '---' \
    >"$directory/$name/SKILL.md"
}

write_skill "$shared" shared "Shared workspace guidance."
write_skill "$project_a" project-a "Project A guidance."
write_skill "$project_b" project-b "Project B guidance."

run_a() {
  env HOME="$home" CODEX_HOME="$temporary/no-codex-home" PATH="$temporary/no-codex" \
    XDG_CONFIG_HOME="$config_home" XDG_CACHE_HOME="$cache_home" \
    XDG_STATE_HOME="$state_home" "$binary" --cwd "$project_a" "$@"
}
run_a_child() {
  env HOME="$home" CODEX_HOME="$temporary/no-codex-home" PATH="$temporary/no-codex" \
    XDG_CONFIG_HOME="$config_home" XDG_CACHE_HOME="$cache_home" \
    XDG_STATE_HOME="$state_home" "$binary" --cwd "$project_a_child" "$@"
}
run_b() {
  env HOME="$home" CODEX_HOME="$temporary/no-codex-home" PATH="$temporary/no-codex" \
    XDG_CONFIG_HOME="$config_home" XDG_CACHE_HOME="$cache_home" \
    XDG_STATE_HOME="$state_home" "$binary" --cwd "$project_b" "$@"
}

# Shared roots are visible everywhere; project roots apply to their project and
# descendants only. No implicit HOME or ancestor discovery is permitted.
run_a init --yes --agent none --root "$shared" --project-root "$project_a"
run_b init --yes --agent none --root "$shared" --project-root "$project_b"
grep -Fq '[[projects]]' "$config_home/skillwick/config.toml"
grep -Fq "path = \"$project_a\"" "$config_home/skillwick/config.toml"
grep -Fq "path = \"$project_b\"" "$config_home/skillwick/config.toml"

run_a list >"$temporary/list-a"
grep -q '^2 skills in the current inventory\.$' "$temporary/list-a"
grep -q '^shared@' "$temporary/list-a"
grep -q '^project-a@' "$temporary/list-a"
! grep -q '^project-b@' "$temporary/list-a"
run_a_child list >"$temporary/list-a-child"
grep -q '^project-a@' "$temporary/list-a-child"
! grep -q '^project-b@' "$temporary/list-a-child"
run_b list >"$temporary/list-b"
grep -q '^2 skills in the current inventory\.$' "$temporary/list-b"
grep -q '^shared@' "$temporary/list-b"
grep -q '^project-b@' "$temporary/list-b"
! grep -q '^project-a@' "$temporary/list-b"

# Ordinary lookup reconciles current files, but an unchanged inventory does
# not replace the durable snapshot.
cache="$cache_home/skillwick/index-v3.sqlite"
before_hash=$(shasum -a 256 "$cache" | awk '{print $1}')
before_mtime=$(stat -f '%m' "$cache")
sleep 1
run_a list >/dev/null
test "$(shasum -a 256 "$cache" | awk '{print $1}')" = "$before_hash"
test "$(stat -f '%m' "$cache")" = "$before_mtime"

# Add, edit, rename, remove, and policy-only changes are visible on the next
# ordinary command without a human refresh step.
printf '%s\n' '---' 'name: project-a' \
  'description: Project A changed in place.' '---' >"$project_a/project-a/SKILL.md"
run_a search 'changed in place' | grep -q '^project-a@'
mv "$project_a/project-a/SKILL.md" "$project_a/project-a/renamed.tmp"
printf '%s\n' '---' 'name: project-a-renamed' \
  'description: Project A renamed.' '---' >"$project_a/project-a/SKILL.md"
rm "$project_a/project-a/renamed.tmp"
run_a list >"$temporary/list-renamed"
grep -q '^project-a-renamed@' "$temporary/list-renamed"
! grep -q '^project-a@' "$temporary/list-renamed"
mkdir -p "$shared/shared/agents"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' \
  >"$shared/shared/agents/openai.yaml"
run_a list >"$temporary/list-policy-denied"
! grep -q '^shared@' "$temporary/list-policy-denied"
rm "$shared/shared/agents/openai.yaml"
run_a list | grep -q '^shared@'
rm "$project_a/project-a/SKILL.md"
run_a list >"$temporary/list-removed"
! grep -q '^project-a-renamed@' "$temporary/list-removed"

# A record from another project cannot be read or inspected by ID or name.
project_b_id=$(run_b list | sed -n 's/^\(project-b@[0-9a-f]*\).*/\1/p')
[ -n "$project_b_id" ]
if run_a read "$project_b_id" >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi
if run_a inspect project-b >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi

# A missing configured root fails the affected operation and preserves the
# last published snapshot. Making it valid lets the next operation recover.
missing="$temporary/missing-root"
missing_config="$temporary/missing.toml"
printf '%s\n' "roots = [\"$shared\", \"$missing\"]" 'agent = "none"' \
  >"$missing_config"
cp "$cache" "$temporary/cache-before-missing"
if missing_output=$(run_a --config "$missing_config" list 2>&1); then
  echo "missing configured root unexpectedly succeeded" >&2
  exit 1
else
  missing_status=$?
fi
test "$missing_status" -eq 3
printf '%s\n' "$missing_output" | grep -q 'list'
printf '%s\n' "$missing_output" | grep -q 'configured root does not exist'
cmp -s "$temporary/cache-before-missing" "$cache"
write_skill "$missing" recovered "Recovered configured root."
run_a --config "$missing_config" list | grep -q '^recovered@'

# An explicitly valid empty root set is a successful empty inventory.
empty_config="$temporary/empty.toml"
printf '%s\n' 'roots = []' 'agent = "none"' >"$empty_config"
run_a --config "$empty_config" list | grep -q '^0 skills in the current inventory\.$'

# Native inventory modes and provider-specific options are gone, with no
# compatibility alias that could reintroduce an executable/state dependency.
for obsolete in \
  '--inventory codex' '--catalog native' '--codex-home /tmp/codex' '--codex-bin /tmp/codex'; do
  if run_a $obsolete list >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
done

run_a --help | grep -q -- '--project-root'
! run_a --help | grep -q -- '--catalog'
! run_a --help | grep -q -- '--codex-bin'
echo "CLI acceptance passed"
