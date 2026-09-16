#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
binary=${1:-target/debug/skillwick}
case "$binary" in
  /*) ;;
  *) binary="$root/$binary" ;;
esac
[ -x "$binary" ] || { echo "distribution test requires an executable: $binary" >&2; exit 2; }
version=$("$binary" --version | awk '{print $2}')

case "$(uname -s):$(uname -m)" in
  Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
  Darwin:x86_64|Darwin:amd64) target=x86_64-apple-darwin ;;
  Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
  Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
  *) echo "unsupported test platform: $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    python3 - "$1" <<'PY'
import hashlib
import sys

digest = hashlib.sha256()
with open(sys.argv[1], "rb") as stream:
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
print(digest.hexdigest())
PY
  fi
}

tmp_parent=${TMPDIR:-/tmp}
[ -d "$tmp_parent" ] || { echo "temporary directory does not exist: $tmp_parent" >&2; exit 1; }
tmp=$(mktemp -d "${tmp_parent%/}/skillwick-distribution.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
install_home="$tmp/home"
server="$tmp/server"
archive_root="skillwick-$target"
archive="${archive_root}.tar.xz"
mkdir -p "$install_home/.agents/skills/packaged" "$server" "$tmp/config" "$tmp/cache" "$tmp/state" "$tmp/space parent"
printf '%s\n' '---' 'name: packaged' 'description: Verify the installed packaged executable.' '---' >"$install_home/.agents/skills/packaged/SKILL.md"

# Package only host target. Cross-target archives must come from corresponding
# build outputs; copying host binary under another target name proves wrong thing.
mkdir -p "$tmp/$archive_root"
cp "$binary" "$tmp/$archive_root/skillwick"
cp "$root/CHANGELOG.md" "$root/LICENSE-APACHE" "$root/LICENSE-MIT" "$root/README.md" "$tmp/$archive_root/"
COPYFILE_DISABLE=1 tar -cJf "$server/$archive" -C "$tmp" "$archive_root"
(cd "$server" && printf '%s  %s\n' "$(sha256 "$archive")" "$archive" >"$archive.sha256")
tar -tf "$server/$archive" | grep -Fxq "$archive_root/skillwick"

PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-release.py" \
  --version "$version" --archive-dir "$server" --target "$target" \
  --source-binary "$binary" --skip-formula --skip-execution --skip-installer

bad_server="$tmp/bad-server"
mkdir "$bad_server"
cp "$server"/* "$bad_server/"
bad_digest=$(printf '%064d' 0)
printf '%s  %s\n' "$bad_digest" "$archive" >"$bad_server/$archive.sha256"
if PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-release.py" \
  --version "$version" --archive-dir "$bad_server" --target "$target" \
  --skip-formula --skip-execution --skip-installer >"$tmp/bad-verifier.out" 2>&1; then
  echo "release verifier accepted a corrupted checksum" >&2
  exit 1
fi
grep -q 'checksum mismatch' "$tmp/bad-verifier.out"

# Installer default prefix is user-owned ~/.local tree and leaves shell startup
# files untouched.
HOME="$install_home" TMPDIR="$tmp/space parent" SKILLWICK_BASE_URL="file://$server" sh "$root/scripts/install.sh" --version "$version"
installed="$install_home/.local/bin/skillwick"
test -x "$installed"
test "$("$installed" --version)" = "skillwick $version"
test ! -e "$install_home/.profile"
test ! -e "$install_home/.bashrc"
test ! -e "$install_home/.zshrc"
env -u HOME SKILLWICK_BASE_URL="file://$server" sh "$root/scripts/install.sh" \
  --version "$version" --prefix "$tmp/explicit prefix"
test "$("$tmp/explicit prefix/bin/skillwick" --version)" = "skillwick $version"

run() {
  env HOME="$install_home" CODEX_HOME="$tmp/no-codex" PATH="$tmp/no-codex" XDG_CONFIG_HOME="$tmp/config" XDG_CACHE_HOME="$tmp/cache" XDG_STATE_HOME="$tmp/state" "$installed" --cwd "$install_home" "$@"
}
run init --yes --agent none --discovery explicit --root "$install_home/.agents/skills"
run list | grep -q '^1 skills in the current inventory\.$'
result=$(run search 'installed packaged executable')
grep -q '^packaged@' <<EOF
$result
EOF
identifier=$(printf '%s\n' "$result" | sed -n 's/^\([^ ]*@[^ ]*\).*/\1/p')
[ -n "$identifier" ]
run read --raw "$identifier" | grep -q '^---$'
run --json read packaged | python3 -c 'import json,sys; value=json.load(sys.stdin); assert value["version"] == 3; assert "content" in value["results"][0]'
run inspect "$identifier" --files | grep -q '^listing: complete$'
run doctor --strict --require packaged >/dev/null
for shell in bash zsh fish; do
  test -n "$(run completions "$shell")"
done

echo "distribution smoke test passed for $target"
