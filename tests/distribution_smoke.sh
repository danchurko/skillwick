#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
version=0.1.1
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
home=$tmp/home
prefix=$tmp/prefix
server=$tmp/server
mkdir -p "$home" "$server"

cat > "$tmp/skillwick" <<EOF
#!/bin/sh
if [ "\${1:-}" = "--version" ]; then echo "skillwick $version"; exit 0; fi
exit 0
EOF
chmod 755 "$tmp/skillwick"
archive="skillwick-aarch64-apple-darwin.tar.xz"
mkdir -p "$tmp/skillwick-aarch64-apple-darwin"
mv "$tmp/skillwick" "$tmp/skillwick-aarch64-apple-darwin/skillwick"
tar -cJf "$server/$archive" -C "$tmp" skillwick-aarch64-apple-darwin
(cd "$server" && shasum -a 256 "$archive" > "$archive.sha256")

before=$(find "$root" -type f -print | sort | shasum -a 256)
HOME="$home" SKILLWICK_BASE_URL="file://$server" sh "$root/scripts/install.sh" --version "$version" --prefix "$prefix"
test "$("$prefix/bin/skillwick" --version)" = "skillwick $version"
test ! -e "$home/.config/skillwick/config.toml"
test ! -e "$home/.agents/AGENTS.md"
after=$(find "$root" -type f -print | sort | shasum -a 256)
test "$before" = "$after"

if [ "$#" -eq 1 ]; then
  real_prefix=$tmp/real-prefix
  HOME="$home" SKILLWICK_BASE_URL="file://$1" sh "$root/scripts/install.sh" --version "$version" --prefix "$real_prefix"
  test "$("$real_prefix/bin/skillwick" --version)" = "skillwick $version"
fi
echo "distribution smoke test passed"
