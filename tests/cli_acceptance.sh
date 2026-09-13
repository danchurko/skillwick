#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-cli.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
home="$temporary/home with spaces"
project="$temporary/project/child"
sibling="$temporary/sibling"
mkdir -p "$home/.agents/skills/cpp/references" "$home/.agents/skills/cpp/scripts" "$project/.agents/skills/agentcore" "$sibling/.agents/skills/leak" "$temporary/config" "$temporary/cache" "$temporary/state"

printf '%s\n' '---' 'name: C++' 'description: Build native C++ command line tools.' 'keywords: [cpp, C#, .NET, Node.js]' '---' 'Read references/guide.md.' > "$home/.agents/skills/cpp/SKILL.md"
printf '%s\n' 'relative reference' > "$home/.agents/skills/cpp/references/guide.md"
printf '%s\n' '#!/bin/sh' "touch '$temporary/support-script-ran'" > "$home/.agents/skills/cpp/scripts/check.sh"
printf '%s\n' '---' 'name: aws-agentcore' 'description: Deploy and debug AgentCore runtimes.' '---' > "$project/.agents/skills/agentcore/SKILL.md"
printf '%s\n' '---' 'name: leak' 'description: Never cross project boundaries.' '---' > "$sibling/.agents/skills/leak/SKILL.md"
ln -s "$sibling/.agents/skills/leak" "$home/.agents/skills/cpp/references/escape"

run() {
  env HOME="$home" XDG_CONFIG_HOME="$temporary/config" XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" "$binary" --cwd "$project" "$@"
}

run init --yes --agent none --inventory filesystem
run init --yes --agent none --inventory filesystem
test -f "$temporary/cache/skillwick/index-v2.sqlite"
test ! -e "$temporary/cache/skillwick/index-v2.sqlite-wal"
test ! -e "$temporary/cache/skillwick/index-v2.sqlite-shm"
run search C++ | grep -q 'C++@'
run search 'deploy AgentCore runtime' | grep -q 'aws-agentcore@'
! run list --all | grep -q 'leak@'
run list | grep -q '^2 skills in the current inventory\.$'
test "$(run list | grep -c '@')" -eq 2
run list --limit 1 | grep -q 'plain `skillwick list` prints every record'
run --json list --limit 1 | grep -q '"total":2'
test "$(run list --all | grep -c '@')" -eq 2
identifier=$(run list --all | sed -n 's/^\(C++@[0-9a-f]*\).*/\1/p')
run read "$identifier" | grep -q "base: $home/.agents/skills/cpp"
if run "read $identifier" >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
run inspect "$identifier" | grep -q '^description: Build native C++ command line tools\.'
! run inspect "$identifier" | grep -q '^package:'
run inspect "$identifier" --files | grep -Fq -- '- references/guide.md [markdown; file, .md]'
run inspect "$identifier" --files | grep -Fq -- '- scripts/check.sh [non-markdown; file, .sh]'
! run inspect "$identifier" --files | grep -q 'support-script-ran'
test ! -e "$temporary/support-script-ran"
run inspect "$identifier" --files | grep -Fq -- '- references/escape [non-markdown; symlink]'
! run inspect "$identifier" --files | grep -q 'escape/SKILL.md'
run inspect "$identifier" --files | grep -q '^counts: complete$'
run inspect "$identifier" --files | grep -q '^regular files: 3$'
run inspect "$identifier" --files | grep -q '^additional regular files: 2$'
run --json inspect "$identifier" --files | grep -q '"package"'
run --json inspect "$identifier" --files | grep -q '"counts_scope":"complete"'
run --json inspect "$identifier" --files | grep -q '"classification":"markdown"'
mkdir -p "$home/.agents/skills/cpp/flood"
i=0
while [ "$i" -le 256 ]; do
  printf '%s\n' "$i" > "$home/.agents/skills/cpp/flood/file-$i.txt"
  i=$((i + 1))
done
run inspect "$identifier" --files | grep -q 'truncated; max 256 entries'
run | grep -q 'Usage:'
run instructions | grep -q '^# Skillwick$'
run instructions | grep -q 'skillwick search "task"'
if run probe --limit 5 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
if run search test --bogus >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
run search --limit 1 C++ | grep -q 'C++@'
run search C++ --limit 1 | grep -q 'C++@'
if run search test --limit 6 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
test "$(run search no-such-skill)" = 'No matching skills.'
run --json search C++ | grep -q '"version":1,"results"'
direct=$(run search C++)
chained=$(true && run search C++)
test "$direct" = "$chained"
short_circuit="$temporary/short-circuit"
(false && run search C++ >"$short_circuit") || true
test ! -e "$short_circuit"

mkdir -p "$project/.agents/skills/oversized"
long_description=$(awk 'BEGIN { for (i = 0; i < 2500; i++) printf "x" }')
printf '%s\n' '---' 'name: oversized' "description: $long_description" '---' > "$project/.agents/skills/oversized/SKILL.md"
run refresh
long_output=$(run search oversized)
printf '%s' "$long_output" | grep -q '^oversized@'
test "$(printf '%s\n' "$long_output" | wc -c | tr -d ' ')" -le 2000
rm "$project/.agents/skills/agentcore/SKILL.md"
if run read aws-agentcore@missing >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi

dry="$temporary/dry"
mkdir -p "$dry"
env HOME="$dry" XDG_CONFIG_HOME="$dry/config" XDG_CACHE_HOME="$dry/cache" XDG_STATE_HOME="$dry/state" "$binary" init --dry-run --yes --agent none --inventory filesystem
test ! -e "$dry/config/skillwick/config.toml"
echo "CLI acceptance passed"
