#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-cli.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
home="$temporary/home with spaces"
project="$temporary/project/child"
sibling="$temporary/sibling"
mkdir -p "$home/.agents/skills/cpp/references" "$home/.agents/skills/cpp/scripts" "$home/.agents/skills/manual-only" "$home/.agents/skills/user-hidden" "$home/.agents/skills/policy-deny/agents" "$home/.agents/skills/malformed-policy" "$project/.agents/skills/agentcore" "$sibling/.agents/skills/leak" "$temporary/config" "$temporary/cache" "$temporary/state"

printf '%s\n' '---' 'name: C++' 'description: Build native C++ command line tools.' 'keywords: [cpp, C#, .NET, Node.js]' '---' 'Read references/guide.md.' > "$home/.agents/skills/cpp/SKILL.md"
printf '%s\n' 'relative reference' > "$home/.agents/skills/cpp/references/guide.md"
printf '%s\n' '#!/bin/sh' "touch '$temporary/support-script-ran'" > "$home/.agents/skills/cpp/scripts/check.sh"
printf '%s\n' '---' 'name: aws-agentcore' 'description: Deploy and debug AgentCore runtimes.' '---' > "$project/.agents/skills/agentcore/SKILL.md"
printf '%s\n' '---' 'name: manual-only' 'description: Manual only workflow.' 'disable-model-invocation: true' '---' > "$home/.agents/skills/manual-only/SKILL.md"
printf '%s\n' '---' 'name: user-hidden' 'description: Model discoverable workflow.' 'user-invocable: false' '---' > "$home/.agents/skills/user-hidden/SKILL.md"
printf '%s\n' '---' 'name: policy-deny' 'description: Policy denied workflow.' '---' > "$home/.agents/skills/policy-deny/SKILL.md"
printf '%s\n' 'policy:' '  allow_implicit_invocation: false' > "$home/.agents/skills/policy-deny/agents/openai.yaml"
printf '%s\n' '---' 'name: malformed-policy' 'description: Malformed policy workflow.' 'disable-model-invocation: maybe' '---' > "$home/.agents/skills/malformed-policy/SKILL.md"
printf '%s\n' '---' 'name: leak' 'description: Never cross project boundaries.' '---' > "$sibling/.agents/skills/leak/SKILL.md"
ln -s "$sibling/.agents/skills/leak" "$home/.agents/skills/cpp/references/escape"

run() {
  env HOME="$home" XDG_CONFIG_HOME="$temporary/config" XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" "$binary" --cwd "$project" "$@"
}

run init --yes --agent none --inventory filesystem
run init --yes --agent none --inventory filesystem
test -f "$temporary/cache/skillwick/index-v3.sqlite"
test ! -e "$temporary/cache/skillwick/index-v3.sqlite-wal"
test ! -e "$temporary/cache/skillwick/index-v3.sqlite-shm"
run search C++ | grep -q 'C++@'
run search 'deploy AgentCore runtime' | grep -q 'aws-agentcore@'
! run list | grep -q 'leak@'
run list | grep -q '^3 skills in the current inventory\.$'
test "$(run list | grep -c '@')" -eq 3
! run list | grep -q 'manual-only@'
! run list | grep -q 'policy-deny@'
run list | grep -q 'user-hidden@'
! run search 'manual only workflow' | grep -q 'manual-only@'
! run search 'policy denied workflow' | grep -q 'policy-deny@'
hidden_id="manual-only@$(printf 'filesystem:%s' "$home/.agents/skills/manual-only/SKILL.md" | shasum -a 256 | awk '{print substr($1,1,6)}')"
if run read "$hidden_id" >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi
if run inspect "$hidden_id" >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi
run refresh 2>&1 | grep -q 'invocation policy'
run doctor 2>&1 | grep -q 'filesystem:'
run doctor 2>&1 | grep -q 'native:'
run doctor 2>&1 | grep -q 'raw:'
run doctor 2>&1 | grep -q 'duplicates:'
run doctor 2>&1 | grep -q 'model-discoverable:'
run --json doctor | grep -q '"version":2'
if run list --limit 1 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
run --json list | grep -q '"version":2,"total":3'
test "$(run list | grep -c '@')" -eq 3
identifier=$(run list | sed -n 's/^\(C++@[0-9a-f]*\).*/\1/p')
run read "$identifier" | grep -q "base: $home/.agents/skills/cpp"
if run "read $identifier" >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
run inspect "$identifier" | grep -q '^description: Build native C++ command line tools\.'
! run inspect "$identifier" | grep -q '^package:'
run --json inspect "$identifier" | grep -q '"version":2,"results"'
run inspect "$identifier" --files | grep -Fq -- '- references/guide.md [markdown; file, .md]'
run inspect "$identifier" --files | grep -Fq -- '- scripts/check.sh [non-markdown; file, .sh]'
! run inspect "$identifier" --files | grep -q 'support-script-ran'
test ! -e "$temporary/support-script-ran"
run inspect "$identifier" --files | grep -Fq -- '- references/escape [non-markdown; symlink]'
! run inspect "$identifier" --files | grep -q 'escape/SKILL.md'
run inspect "$identifier" --files | grep -q '^counts: complete$'
run inspect "$identifier" --files | grep -q '^regular files: 3$'
run inspect "$identifier" --files | grep -q '^additional regular files: 2$'
run --json inspect "$identifier" --files | grep -q '"version":2'
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
run --help | grep -q 'version-2 JSON'
run search --help | grep -q '1-20'
run search --help | grep -q 'default: 5'
run inspect --help | grep -q 'does not execute files'
run refresh --help | grep -q 'disposable local index'
run init --help | grep -q 'required in non-interactive mode'
run doctor --help | grep -q 'exit code 3'
run uninstall --help | grep -q 'installed skills remain untouched'
run instructions | grep -q '^# Skillwick$'
run instructions | grep -q 'skillwick search "task"'
run instructions | grep -q 'another named skill through a skill tool'
if run probe --limit 5 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
if run search test --bogus >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
if run refresh --full >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
if run --json refresh >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
run search --limit 1 C++ | grep -q 'C++@'
run search C++ --limit 1 | grep -q 'C++@'
run search C++ --limit 20 | grep -q 'C++@'
if run search test --limit 21 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
test "$(run search no-such-skill)" = 'No matching skills.'
run --json search C++ | grep -q '"version":2,"results"'
direct=$(run search C++)
chained=$(true && run search C++)
test "$direct" = "$chained"
short_circuit="$temporary/short-circuit"
(false && run search C++ >"$short_circuit") || true
test ! -e "$short_circuit"

mkdir -p "$project/.agents/skills/oversized"
long_description=$(awk 'BEGIN { for (i = 0; i < 2500; i++) printf "x" }')
printf '%s\n' '---' 'name: oversized' "description: $long_description" '---' > "$project/.agents/skills/oversized/SKILL.md"
mkdir -p "$project/.agents/skills/oversized-two"
printf '%s\n' '---' 'name: oversized-two' "description: $long_description" '---' > "$project/.agents/skills/oversized-two/SKILL.md"
run refresh
long_output=$(run search oversized)
test "$(printf '%s\n' "$long_output" | grep -c '^oversized')" -eq 2
printf '%s\n' "$long_output" | grep -q '\[truncated\]'
printf '%s\n' "$long_output" | awk 'length($0) > 2000 { exit 1 } END { if (NR != 2) exit 1 }'
json_output=$(run --json search oversized --limit 2)
printf '%s\n' "$json_output" | grep -q '"version":2,"results"'
test "$(printf '%s\n' "$json_output" | grep -o '"id"' | wc -l | tr -d ' ')" -eq 2
rm "$project/.agents/skills/agentcore/SKILL.md"
if run read aws-agentcore@missing >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi

dry="$temporary/dry"
mkdir -p "$dry"
env HOME="$dry" XDG_CONFIG_HOME="$dry/config" XDG_CACHE_HOME="$dry/cache" XDG_STATE_HOME="$dry/state" "$binary" init --dry-run --yes --agent none --inventory filesystem
test ! -e "$dry/config/skillwick/config.toml"
echo "CLI acceptance passed"
