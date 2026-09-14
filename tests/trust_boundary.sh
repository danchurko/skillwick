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
config="$temporary/config"
cache="$temporary/cache"
state="$temporary/state"
mkdir -p "$home/.agents/skills" "$work" "$config" "$cache" "$state"

run_fs() {
  env HOME="$home" XDG_CONFIG_HOME="$config" XDG_CACHE_HOME="$cache" \
    XDG_STATE_HOME="$state" "$binary" --cwd "$work" "$@"
}

# Metadata failures are diagnostics, not permission to drop the prior valid
# record. The valid record gives the incomplete refresh a specialist source.
mkdir -p "$home/.agents/skills/good"
printf '%s\n' '---' 'name: good' 'description: Valid trust-boundary fixture.' '---' \
  >"$home/.agents/skills/good/SKILL.md"
run_fs init --yes --agent none --inventory filesystem >/dev/null 2>&1

mkdir -p "$home/.agents/skills/malformed" "$home/.agents/skills/oversized" \
  "$home/.agents/skills/invalid-utf8"
printf '%s\n' '---' 'name: malformed' 'description: [unterminated' '---' \
  >"$home/.agents/skills/malformed/SKILL.md"
awk 'BEGIN { printf "---\nname: oversized\ndescription: "; for (i = 0; i < 70000; i++) printf "x"; printf "\n---\n" }' \
  >"$home/.agents/skills/oversized/SKILL.md"
printf '%s\n' '---' 'name: invalid-utf8' 'description: invalid bytes' '---' \
  >"$home/.agents/skills/invalid-utf8/SKILL.md"
printf '\377\376\375' >>"$home/.agents/skills/invalid-utf8/SKILL.md"

metadata_errors="$temporary/metadata-errors"
run_fs refresh >"$temporary/metadata-out" 2>"$metadata_errors"
grep -q 'malformed/SKILL.md' "$metadata_errors"
grep -q 'frontmatter' "$metadata_errors"
grep -q 'oversized/SKILL.md' "$metadata_errors"
grep -q 'exceeds 64 KiB' "$metadata_errors"
grep -q 'invalid-utf8/SKILL.md' "$metadata_errors"
grep -q 'not UTF-8' "$metadata_errors"
run_fs list >"$temporary/metadata-list"
grep -q '^good@' "$temporary/metadata-list"
! grep -q '^malformed@' "$temporary/metadata-list"
! grep -q '^oversized@' "$temporary/metadata-list"
! grep -q '^invalid-utf8@' "$temporary/metadata-list"

# Canonical identity deduplicates an in-root symlink while rejecting an
# out-of-root directory symlink. The outside skill must never enter inventory.
mkdir -p "$home/.agents/skills/canonical-real" "$temporary/outside"
printf '%s\n' '---' 'name: canonical' 'description: Canonical identity fixture.' '---' \
  >"$home/.agents/skills/canonical-real/SKILL.md"
ln -s "canonical-real" "$home/.agents/skills/canonical-alias"
printf '%s\n' '---' 'name: escaped' 'description: Must remain outside the authorized root.' '---' \
  >"$temporary/outside/SKILL.md"
ln -s "$temporary/outside" "$home/.agents/skills/escape"
run_fs refresh >"$temporary/symlink-out" 2>"$temporary/symlink-err"
grep -q 'symlink escape' "$temporary/symlink-err"
test "$(run_fs list | grep -c '^canonical@')" -eq 1
! run_fs list | grep -q '^escaped@'

# Package inspection is bounded, reports shape only, and never executes a
# listed script or follows a symlinked directory.
package="$home/.agents/skills/package"
mkdir -p "$package/references" "$package/scripts" "$package/flood"
printf '%s\n' '---' 'name: package' 'description: Bounded package inspection fixture.' '---' \
  >"$package/SKILL.md"
printf '%s\n' 'reference secret' >"$temporary/outside/secret.txt"
printf '%s\n' '#!/bin/sh' "touch '$temporary/inspection-executed'" \
  >"$package/side-effect.sh"
ln -s "$temporary/outside" "$package/references/escape"
i=0
while [ "$i" -le 260 ]; do
  printf '%s\n' "$i" >"$package/flood/flood-$i.txt"
  i=$((i + 1))
done
run_fs refresh >/dev/null 2>"$temporary/package-refresh-err"
package_id=$(run_fs list | sed -n 's/^\(package@[0-9a-f]*\).*/\1/p')
[ -n "$package_id" ]
package_json=$(run_fs --json inspect "$package_id" --files)
printf '%s\n' "$package_json" | jq -e '.version == 2 and .package.truncated == true and .package.counts_scope == "shown_subset" and (.package.entries | length) == 256' >/dev/null
printf '%s\n' "$package_json" | jq -e '.package.entries[] | select(.path == "side-effect.sh")' >/dev/null
! printf '%s\n' "$package_json" | jq -e '.package.entries[] | select(.path | startswith("references/escape/"))' >/dev/null
test ! -e "$temporary/inspection-executed"

# A native provider fixture emits a notification before the response, then
# exposes a real SKILL.md. The remaining modes exercise each protocol failure
# while checking that the last published cache remains byte-for-byte intact.
codex_home="$temporary/codex"
native_work="$temporary/native-work"
native_skill="$codex_home/skills/native"
mkdir -p "$native_skill" "$native_work"
printf '%s\n' '---' 'name: native' 'description: Native trust-boundary fixture.' '---' \
  >"$native_skill/SKILL.md"
provider="$temporary/fake-codex"
cat >"$provider" <<'PROVIDER'
#!/bin/sh
if [ "$1" = "--version" ]; then echo "codex-cli 0.154.0"; exit 0; fi
[ "$1" = "app-server" ] || exit 1
IFS= read -r initialize || exit 1
IFS= read -r initialized || exit 1
IFS= read -r request || exit 1
case "${SKILLWICK_FAKE_BEHAVIOR:-good}" in
  good)
    printf '%s\n' '{"jsonrpc":"2.0","method":"window/logged","params":{"message":"fixture notification"}}'
    jq -cn --arg path "$SKILLWICK_NATIVE_PATH" '{"jsonrpc":"2.0","id":2,"result":{"data":[{"cwd":"fixture","skills":[{"name":"native","description":"Native trust-boundary fixture.","interface":null,"path":$path,"scope":"user","enabled":true,"pluginId":null,"shortDescription":null}],"errors":[]}]}}'
    ;;
  provider-error)
    jq -cn '{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"provider unavailable"}}'
    ;;
  source-error)
    jq -cn '{"jsonrpc":"2.0","id":2,"result":{"data":[{"cwd":"fixture","skills":[],"errors":[{"message":"source unavailable"}]}]}}'
    ;;
  malformed)
    printf '%s\n' '{not-json'
    ;;
  eof)
    exit 0
    ;;
  timeout)
    sleep 9
    ;;
  oversized)
    awk 'BEGIN { printf "{"; for (i = 0; i < 1048580; i++) printf "x"; printf "}\\n" }'
    ;;
  escape)
    jq -cn --arg path "$SKILLWICK_NATIVE_PATH" '{"jsonrpc":"2.0","id":2,"result":{"data":[{"cwd":"fixture","skills":[{"name":"native","description":"Native trust-boundary fixture.","interface":null,"path":$path,"scope":"user","enabled":true,"pluginId":null,"shortDescription":null}],"errors":[]}]}}'
    ;;
  *) exit 1 ;;
esac
PROVIDER
chmod +x "$provider"

run_native_with() {
  behavior=$1
  shift
  env HOME="$home" CODEX_HOME="$codex_home" XDG_CONFIG_HOME="$config" \
    XDG_CACHE_HOME="$cache" XDG_STATE_HOME="$state" \
    SKILLWICK_FAKE_BEHAVIOR="$behavior" SKILLWICK_NATIVE_PATH="$native_skill/SKILL.md" \
    "$binary" --cwd "$native_work" "$@"
}

run_native_with good init --yes --agent none --inventory codex \
  --codex-home "$codex_home" --codex-bin "$provider" >/dev/null 2>"$temporary/native-init-err"
run_native_with good list >"$temporary/native-list"
grep -q '^native@' "$temporary/native-list"
native_id=$(sed -n 's/^\(native@[0-9a-f]*\).*/\1/p' "$temporary/native-list")
[ -n "$native_id" ]
native_cache="$cache/skillwick/index-v3.sqlite"
[ -f "$native_cache" ]

cp "$native_skill/SKILL.md" "$temporary/native-original"
printf '%s\n' '---' 'name: native' 'description: Native content changed after indexing.' '---' \
  >"$native_skill/SKILL.md"
for target in "$native_id" native; do
  if native_changed=$(run_native_with good read "$target" 2>&1); then
    echo "changed native source was read" >&2
    exit 1
  else
    native_changed_status=$?
  fi
  test "$native_changed_status" -eq 3
  printf '%s\n' "$native_changed" | grep -q 'changed after indexing'
done
cp "$temporary/native-original" "$native_skill/SKILL.md"
rm "$native_skill/SKILL.md"
for target in "$native_id" native; do
  if native_unavailable=$(run_native_with good read "$target" 2>&1); then
    echo "unavailable native source was read" >&2
    exit 1
  else
    native_unavailable_status=$?
  fi
  test "$native_unavailable_status" -eq 3
  printf '%s\n' "$native_unavailable" | grep -q 'source is unavailable'
done
cp "$temporary/native-original" "$native_skill/SKILL.md"

before_hash=$(shasum -a 256 "$native_cache" | awk '{print $1}')
for behavior in provider-error source-error malformed eof timeout oversized; do
  if failure_output=$(run_native_with "$behavior" init --yes --agent none --inventory codex \
    --codex-home "$codex_home" --codex-bin "$provider" 2>&1); then
    echo "native $behavior unexpectedly succeeded" >&2
    exit 1
  else
    failure_status=$?
  fi
  test "$failure_status" -eq 3
  case "$behavior" in
    provider-error) printf '%s\n' "$failure_output" | grep -q 'provider unavailable' ;;
    source-error) printf '%s\n' "$failure_output" | grep -q 'source error(s)' ;;
    malformed) printf '%s\n' "$failure_output" | grep -q 'invalid Codex inventory JSON' ;;
    eof) printf '%s\n' "$failure_output" | grep -q 'reached EOF' ;;
    timeout) printf '%s\n' "$failure_output" | grep -q 'timed out' ;;
    oversized) printf '%s\n' "$failure_output" | grep -q 'message exceeds 1 MiB' ;;
  esac
  after_hash=$(shasum -a 256 "$native_cache" | awk '{print $1}')
  test "$after_hash" = "$before_hash"
done
run_native_with good list | grep -q '^native@'

# A symlinked native source changes canonical identity and is rejected before
# its outside content can be returned.
mkdir -p "$temporary/native-outside"
printf '%s\n' '---' 'name: outside' 'description: Outside native content.' '---' \
  >"$temporary/native-outside/SKILL.md"

# A provider-returned symlink to another package is rejected before metadata is
# indexed, and the previous native snapshot remains intact.
rm "$native_skill/SKILL.md"
ln -s "$temporary/native-outside/SKILL.md" "$native_skill/SKILL.md"
escape_cache_hash=$(shasum -a 256 "$native_cache" | awk '{print $1}')
if escape_output=$(run_native_with escape init --yes --agent none --inventory codex \
  --codex-home "$codex_home" --codex-bin "$provider" 2>&1); then
  echo "native symlink escape unexpectedly succeeded" >&2
  exit 1
else
  escape_status=$?
fi
test "$escape_status" -eq 3
printf '%s\n' "$escape_output" | grep -q 'escapes package'
test "$(shasum -a 256 "$native_cache" | awk '{print $1}')" = "$escape_cache_hash"
rm "$native_skill/SKILL.md"
cp "$temporary/native-original" "$native_skill/SKILL.md"

rm "$native_skill/SKILL.md"
ln -s "$temporary/native-outside/SKILL.md" "$native_skill/SKILL.md"
for target in "$native_id" native; do
  if native_path_changed=$(run_native_with good read "$target" 2>&1); then
    echo "changed native path was read" >&2
    exit 1
  else
    native_path_status=$?
  fi
  test "$native_path_status" -eq 3
  printf '%s\n' "$native_path_changed" | grep -q 'path changed'
done
rm "$native_skill/SKILL.md"
cp "$temporary/native-original" "$native_skill/SKILL.md"

echo "Trust-boundary regression checks passed"
