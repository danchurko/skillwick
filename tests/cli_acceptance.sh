#!/bin/sh
set -eu

binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$(pwd)/$binary" ;; esac
temporary=$(mktemp -d /private/tmp/skillwick-cli.XXXXXX)
trap 'rm -rf "$temporary"' EXIT HUP INT TERM
home="$temporary/home with spaces"
project="$temporary/project/child"
sibling="$temporary/sibling"
mkdir -p "$home/.agents/skills/cpp/references" "$project/.agents/skills/agentcore" "$sibling/.agents/skills/leak" "$temporary/config" "$temporary/cache" "$temporary/state"

printf '%s\n' '---' 'name: C++' 'description: Build native C++ command line tools.' 'keywords: [cpp, C#, .NET, Node.js]' '---' 'Read references/guide.md.' > "$home/.agents/skills/cpp/SKILL.md"
printf '%s\n' 'relative reference' > "$home/.agents/skills/cpp/references/guide.md"
printf '%s\n' '---' 'name: aws-agentcore' 'description: Deploy and debug AgentCore runtimes.' '---' > "$project/.agents/skills/agentcore/SKILL.md"
printf '%s\n' '---' 'name: leak' 'description: Never cross project boundaries.' '---' > "$sibling/.agents/skills/leak/SKILL.md"

run() {
  env HOME="$home" XDG_CONFIG_HOME="$temporary/config" XDG_CACHE_HOME="$temporary/cache" XDG_STATE_HOME="$temporary/state" "$binary" --cwd "$project" "$@"
}

run init --yes --agent none --inventory filesystem
run init --yes --agent none --inventory filesystem
run C++ | grep -q 'C++@'
run 'deploy AgentCore runtime' | grep -q 'aws-agentcore@'
! run list --all | grep -q 'leak@'
run list | grep -q '^2 skills in the current inventory\.$'
test "$(run list | grep -c '@')" -eq 2
run list --limit 1 | grep -q 'plain `skillwick list` prints every record'
run --json list --limit 1 | grep -q '"total":2'
test "$(run list --all | grep -c '@')" -eq 2
identifier=$(run list --all | sed -n 's/^\(C++@[0-9a-f]*\).*/\1/p')
run read "$identifier" | grep -q "base: $home/.agents/skills/cpp"
run -- init hooks | grep -q 'No matching skills.'
run | grep -q 'Usage:'
if run search test --limit 6 >/dev/null 2>&1; then exit 1; else test "$?" -eq 2; fi
test "$(run 'deploy AgentCore runtime' | wc -c | tr -d ' ')" -le 2000
rm "$project/.agents/skills/agentcore/SKILL.md"
if run read aws-agentcore@missing >/dev/null 2>&1; then exit 1; else test "$?" -eq 3; fi

dry="$temporary/dry"
mkdir -p "$dry"
env HOME="$dry" XDG_CONFIG_HOME="$dry/config" XDG_CACHE_HOME="$dry/cache" XDG_STATE_HOME="$dry/state" "$binary" init --dry-run --yes --agent none --inventory filesystem
test ! -e "$dry/config/skillwick/config.toml"
echo "CLI acceptance passed"
