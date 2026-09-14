#!/bin/sh
set -eu

usage() {
  echo "usage: $0 --version VERSION [--prefix PATH]" >&2
  exit 2
}

version=
prefix=/usr/local
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) [ "$#" -ge 2 ] || usage; version=$2; shift 2 ;;
    --prefix) [ "$#" -ge 2 ] || usage; prefix=$2; shift 2 ;;
    -h|--help) usage ;;
    *) usage ;;
  esac
done
[ -n "$version" ] || usage
case "$version" in *[!A-Za-z0-9._-]*) echo "invalid version" >&2; exit 2 ;; esac

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) target=aarch64-apple-darwin ;;
  Darwin:x86_64) target=x86_64-apple-darwin ;;
  *) echo "unsupported platform: $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

base_url=${SKILLWICK_BASE_URL:-https://github.com/danchurko/skillwick/releases/download/v$version}
archive="skillwick-$target.tar.xz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
curl -fsSL "$base_url/$archive" -o "$tmp/$archive"
curl -fsSL "$base_url/$archive.sha256" -o "$tmp/$archive.sha256"
expected=$(awk 'NF { print $1; exit }' "$tmp/$archive.sha256")
case "$expected" in *[!0-9a-fA-F]*|'') echo "invalid checksum file" >&2; exit 1 ;; esac
test "${#expected}" -eq 64 || { echo "invalid checksum length" >&2; exit 1; }
actual=$(shasum -a 256 "$tmp/$archive" | awk '{ print $1 }')
test "$actual" = "$expected" || { echo "checksum mismatch" >&2; exit 1; }
echo "$archive: OK"
tar -xJf "$tmp/$archive" -C "$tmp"
executable="$tmp/skillwick-$target/skillwick"
test -f "$executable"
mkdir -p "$prefix/bin"
install -m 755 "$executable" "$prefix/bin/skillwick"
echo "installed $prefix/bin/skillwick"
