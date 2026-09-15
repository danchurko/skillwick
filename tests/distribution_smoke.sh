#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
binary=${1:-target/debug/skillwick}
case "$binary" in /*) ;; *) binary="$root/$binary" ;; esac
version=$($binary --version | awk '{print $2}')
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
home=$tmp/home
prefix=$tmp/prefix
server=$tmp/server
mkdir -p "$home/.agents/skills/packaged" "$server" "$tmp/config" "$tmp/cache" "$tmp/state"
printf '%s\n' '---' 'name: packaged' 'description: Verify the installed packaged executable.' '---' \
  >"$home/.agents/skills/packaged/SKILL.md"

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) target=aarch64-apple-darwin ;;
  Darwin:x86_64) target=x86_64-apple-darwin ;;
  *) echo "unsupported test platform" >&2; exit 1 ;;
esac
archive="skillwick-$target.tar.xz"
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  archive="skillwick-$target.tar.xz"
  mkdir -p "$tmp/skillwick-$target"
  cp "$binary" "$tmp/skillwick-$target/skillwick"
  cp "$root/LICENSE-APACHE" "$root/LICENSE-MIT" "$root/README.md" "$tmp/skillwick-$target/"
  COPYFILE_DISABLE=1 tar -cJf "$server/$archive" -C "$tmp" "skillwick-$target"
  (cd "$server" && shasum -a 256 "$archive" >"$archive.sha256")
  tar -tf "$server/$archive" | grep -Fxq "skillwick-$target/skillwick"
done

arm_digest=$(shasum -a 256 "$server/skillwick-aarch64-apple-darwin.tar.xz" | awk '{print $1}')
x86_digest=$(shasum -a 256 "$server/skillwick-x86_64-apple-darwin.tar.xz" | awk '{print $1}')
formula="$tmp/skillwick.rb"
printf '%s\n' \
  'class Skillwick < Formula' \
  '  homepage "https://github.com/danchurko/skillwick"' \
  "  version \"$version\"" \
  '  if Hardware::CPU.arm?' \
  "    url \"https://github.com/danchurko/skillwick/releases/download/v$version/skillwick-aarch64-apple-darwin.tar.xz\"" \
  "    sha256 \"$arm_digest\"" \
  '  else' \
  "    url \"https://github.com/danchurko/skillwick/releases/download/v$version/skillwick-x86_64-apple-darwin.tar.xz\"" \
  "    sha256 \"$x86_digest\"" \
  '  end' \
  '  def install' \
  '    bin.install "skillwick"' \
  '  end' \
  'end' >"$formula"

PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-release.py" \
  --version "$version" --archive-dir "$server" --formula "$formula" \
  --source-binary "$binary" --skip-execution

bad_server="$tmp/bad-server"
cp -R "$server" "$bad_server"
case "$arm_digest" in
  0*) bad_digest="1${arm_digest#?}" ;;
  *) bad_digest="0${arm_digest#?}" ;;
esac
printf '%s  %s\n' "$bad_digest" "skillwick-aarch64-apple-darwin.tar.xz" \
  >"$bad_server/skillwick-aarch64-apple-darwin.tar.xz.sha256"
if PYTHONDONTWRITEBYTECODE=1 python3 "$root/scripts/verify-release.py" \
  --version "$version" --archive-dir "$bad_server" --formula "$formula" \
  --skip-execution --skip-installer >"$tmp/bad-verifier.out" 2>&1; then
  echo "release verifier accepted a corrupted checksum" >&2
  exit 1
fi
grep -q 'checksum mismatch' "$tmp/bad-verifier.out"

before=$(find "$root" -type f -print | sort | shasum -a 256)
HOME="$home" SKILLWICK_BASE_URL="file://$server" sh "$root/scripts/install.sh" --version "$version" --prefix "$prefix"
test "$("$prefix/bin/skillwick" --version)" = "skillwick $version"
test ! -e "$home/.config/skillwick/config.toml"
test ! -e "$home/.agents/AGENTS.md"

run() {
  env HOME="$home" XDG_CONFIG_HOME="$tmp/config" XDG_CACHE_HOME="$tmp/cache" \
    XDG_STATE_HOME="$tmp/state" "$prefix/bin/skillwick" --cwd "$home" "$@"
}
run init --yes --agent none --root "$home/.agents/skills"
result=$(run search 'installed packaged executable')
printf '%s\n' "$result" | grep -q '^packaged@'
identifier=$(printf '%s\n' "$result" | sed -n 's/^\([^ ]*@[^ ]*\).*/\1/p')
run read "$identifier" | grep -q '^name: packaged$'
run inspect "$identifier" --files | grep -q '^listing: complete$'
run doctor --strict >/dev/null

after=$(find "$root" -type f -print | sort | shasum -a 256)
test "$before" = "$after"
echo "distribution smoke test passed"
