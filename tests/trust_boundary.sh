#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in
  /*) ;;
  *) binary="$(pwd)/$binary" ;;
esac
[ -x "$binary" ] || {
  echo "trust-boundary test requires an executable: $binary" >&2
  exit 2
}

temporary=$(mktemp -d /private/tmp/skillwick-trust.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM

home="$temporary/home"
work="$temporary/work"
root="$temporary/authorized"
outside="$temporary/outside"
config="$temporary/config"
cache="$temporary/cache"
state="$temporary/state"
mkdir -p "$home" "$work" "$root" "$outside" "$config" "$cache" "$state" \
  "$temporary/no-codex"

run() {
  env HOME="$home" CODEX_HOME="$temporary/no-codex-home" PATH="$temporary/no-codex" \
    XDG_CONFIG_HOME="$config" XDG_CACHE_HOME="$cache" XDG_STATE_HOME="$state" \
    "$binary" --cwd "$work" "$@"
}

write_skill() {
  directory=$1
  name=$2
  description=$3
  mkdir -p "$directory/$name"
  printf '%s\n' '---' "name: $name" "description: $description" '---' \
    >"$directory/$name/SKILL.md"
}

write_skill "$root" good "Valid trust-boundary fixture."
run init --yes --agent none --root "$root" >/dev/null
cache_file="$cache/skillwick/index-v3.sqlite"

# A malformed configured root record fails the current operation and cannot
# replace the last complete publication.
mkdir -p "$root/malformed"
printf '%s\n' '---' 'name: malformed' 'description: [unterminated' '---' \
  >"$root/malformed/SKILL.md"
cp "$cache_file" "$temporary/cache-before-malformed"
if malformed_output=$(run list 2>&1); then
  echo "malformed source unexpectedly succeeded" >&2
  exit 1
else
  malformed_status=$?
fi
test "$malformed_status" -eq 3
printf '%s\n' "$malformed_output" | grep -q 'list'
printf '%s\n' "$malformed_output" | grep -q 'malformed/SKILL.md'
cmp -s "$temporary/cache-before-malformed" "$cache_file"
rm -rf "$root/malformed"
run list | grep -q '^good@'

# Policy-only edits participate in the same ordinary reconciliation path.
mkdir -p "$root/good/agents"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' \
  >"$root/good/agents/openai.yaml"
run list >"$temporary/policy-denied"
! grep -q '^good@' "$temporary/policy-denied"
rm "$root/good/agents/openai.yaml"
run list | grep -q '^good@'

# Symlinked packages outside an authorized root are diagnosed and never leak
# into list, read, or inspect.
write_skill "$outside" escaped "Outside the authorized root."
ln -s "$outside/escaped" "$root/escape"
cp "$cache_file" "$temporary/cache-before-escape"
if escape_output=$(run list 2>&1); then
  echo "symlink escape unexpectedly succeeded" >&2
  exit 1
else
  escape_status=$?
fi
test "$escape_status" -eq 3
printf '%s\n' "$escape_output" | grep -q 'symlink escape'
! printf '%s\n' "$escape_output" | grep -q 'escaped@'
cmp -s "$temporary/cache-before-escape" "$cache_file"
rm "$root/escape"
run list | grep -q '^good@'

# Canonical aliases inside the same authorized root deduplicate safely.
write_skill "$root" canonical "Canonical identity fixture."
ln -s "$root/canonical" "$root/canonical-alias"
run list >"$temporary/canonical-list"
test "$(grep -c '^canonical@' "$temporary/canonical-list")" -eq 1
rm "$root/canonical-alias"

# Package inspection reports shape only and never executes or follows package
# scripts/symlinked directories.
package="$root/package"
mkdir -p "$package/references" "$package/scripts" "$package/flood"
printf '%s\n' '---' 'name: package' 'description: Bounded package inspection fixture.' '---' \
  >"$package/SKILL.md"
printf '%s\n' '#!/bin/sh' "touch '$temporary/inspection-executed'" \
  >"$package/scripts/check.sh"
printf '%s\n' 'secret' >"$outside/secret.txt"
ln -s "$outside" "$package/references/escape"
i=0
while [ "$i" -le 260 ]; do
  printf '%s\n' "$i" >"$package/flood/flood-$i.txt"
  i=$((i + 1))
done
run list >/dev/null
package_id=$(run list | sed -n 's/^\(package@[0-9a-f]*\).*/\1/p')
[ -n "$package_id" ]
package_json=$(run --json inspect "$package_id" --files)
printf '%s\n' "$package_json" | jq -e \
  '.version == 2 and .package.truncated == true and .package.counts_scope == "shown_subset" and (.package.entries | length) == 256' \
  >/dev/null
printf '%s\n' "$package_json" | jq -e '.package.entries[] | select(.path == "scripts/check.sh")' >/dev/null
! printf '%s\n' "$package_json" | jq -e '.package.entries[] | select(.path | startswith("references/escape/"))' >/dev/null
test ! -e "$temporary/inspection-executed"

# Removing an indexed source updates normal lookup, and old IDs cannot bypass
# the current configured-root scope.
good_id=$(run list | sed -n 's/^\(good@[0-9a-f]*\).*/\1/p')
[ -n "$good_id" ]
rm -rf "$root/good"
run list >"$temporary/list-after-remove"
! grep -q '^good@' "$temporary/list-after-remove"
if run read "$good_id" >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi
if run inspect "$good_id" >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi

# No Codex executable or state is consulted by filesystem inventory.
run doctor --strict >/dev/null
echo "Trust-boundary regression checks passed"
