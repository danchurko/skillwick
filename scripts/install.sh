#!/bin/sh
set -eu

DEFAULT_VERSION=0.4.0

usage() {
  echo "usage: $0 [--version VERSION] [--prefix PATH]" >&2
  exit 2
}

version=${SKILLWICK_VERSION:-$DEFAULT_VERSION}
prefix=${SKILLWICK_PREFIX:-}
while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      [ "$#" -ge 2 ] || usage
      version=$2
      shift 2
      ;;
    --prefix)
      [ "$#" -ge 2 ] || usage
      prefix=$2
      shift 2
      ;;
    -h|--help)
      usage
      ;;
    *)
      usage
      ;;
  esac
done

if [ -z "$prefix" ]; then
  install_home=${HOME:-}
  [ -n "$install_home" ] || { echo "HOME is required when --prefix is omitted" >&2; exit 2; }
  prefix="$install_home/.local"
fi

case "$version" in
  v*) version=${version#v} ;;
esac
case "$version" in
  ''|*[!0-9A-Za-z.+-]*) echo "invalid version: $version" >&2; exit 2 ;;
esac

os=$(uname -s)
machine=$(uname -m)
case "$os:$machine" in
  Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
  Darwin:x86_64|Darwin:amd64) target=x86_64-apple-darwin ;;
  Linux:aarch64|Linux:arm64) target=aarch64-unknown-linux-musl ;;
  Linux:x86_64|Linux:amd64) target=x86_64-unknown-linux-musl ;;
  *) echo "unsupported platform: $os $machine" >&2; exit 1 ;;
esac

tmp_parent=${TMPDIR:-/tmp}
[ -d "$tmp_parent" ] || { echo "temporary directory does not exist: $tmp_parent" >&2; exit 1; }
tmp=$(mktemp -d "${tmp_parent%/}/skillwick-install.XXXXXX")
staged=
cleanup() {
  [ -z "$staged" ] || rm -f "$staged"
  rm -rf "$tmp"
}
trap cleanup EXIT HUP INT TERM

download() {
  url=$1
  destination=$2
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$destination"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$destination"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$url" "$destination" <<'PY'
from pathlib import Path
import sys
import urllib.request

with urllib.request.urlopen(sys.argv[1], timeout=60) as response:
    Path(sys.argv[2]).write_bytes(response.read())
PY
  else
    echo "installer requires curl, wget, or python3" >&2
    exit 1
  fi
}

sha256() {
  path=$1
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$path" | awk '{print $NF}'
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$path" <<'PY'
import hashlib
import sys

digest = hashlib.sha256()
with open(sys.argv[1], "rb") as stream:
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
print(digest.hexdigest())
PY
  else
    echo "installer requires a SHA-256 implementation" >&2
    exit 1
  fi
}

base_url=${SKILLWICK_BASE_URL:-https://github.com/danchurko/skillwick/releases/download/v$version}
archive="skillwick-$target.tar.xz"
download "$base_url/$archive" "$tmp/$archive"
download "$base_url/$archive.sha256" "$tmp/$archive.sha256"

checksum_fields=$(awk '
  NF {
    count++
    if (count == 1 && NF == 2) {
      digest = $1
      name = $2
    } else {
      malformed = 1
    }
  }
  END {
    if (count != 1 || malformed || digest == "" || name == "") exit 1
    print digest
    print name
  }
' "$tmp/$archive.sha256") || {
  echo "invalid checksum file" >&2
  exit 1
}
expected=$(printf '%s\n' "$checksum_fields" | sed -n '1p')
checksum_name=$(printf '%s\n' "$checksum_fields" | sed -n '2p')
checksum_name=${checksum_name##*/}
case "$expected" in
  ''|*[!0-9A-Fa-f]*) echo "invalid checksum file" >&2; exit 1 ;;
esac
[ "${#expected}" -eq 64 ] || { echo "invalid checksum file" >&2; exit 1; }
[ "$checksum_name" = "$archive" ] || { echo "checksum names $checksum_name, expected $archive" >&2; exit 1; }
actual=$(sha256 "$tmp/$archive")
expected_lower=$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')
[ "$actual" = "$expected_lower" ] || {
  echo "checksum mismatch" >&2
  exit 1
}

archive_root="skillwick-$target"
members=$(tar -tJf "$tmp/$archive") || { echo "could not inspect $archive" >&2; exit 1; }
while IFS= read -r member; do
  case "$member" in
    "$archive_root"|"$archive_root"/*) ;;
    *) echo "archive contains unexpected path: $member" >&2; exit 1 ;;
  esac
  case "$member" in
    /*|*/../*|../*|*/..|..|*'\n'*) echo "archive contains unsafe path: $member" >&2; exit 1 ;;
  esac
done <<EOF
$members
EOF

extraction="$tmp/extracted"
mkdir -p "$extraction"
tar -xJf "$tmp/$archive" -C "$extraction"
executable="$extraction/$archive_root/skillwick"
[ -f "$executable" ] || { echo "archive has no executable" >&2; exit 1; }
if [ -L "$executable" ]; then
  echo "archive executable is a symlink" >&2
  exit 1
fi
chmod 755 "$executable"
reported=$("$executable" --version)
[ "$reported" = "skillwick $version" ] || {
  echo "archive executable reported $reported, expected skillwick $version" >&2
  exit 1
}

destination="$prefix/bin"
if [ -L "$destination" ]; then
  echo "refusing symlinked destination: $destination" >&2
  exit 1
fi
mkdir -p "$destination"
staged="$destination/.skillwick.$$"
if [ -e "$staged" ]; then
  echo "temporary install path already exists: $staged" >&2
  exit 1
fi
cp "$executable" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$destination/skillwick"
staged=
echo "$archive: checksum and version OK"
echo "installed $destination/skillwick"
