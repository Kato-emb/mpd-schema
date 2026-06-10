#!/usr/bin/env bash
set -euo pipefail

# DASHSchema は ISO カスタムライセンスで再配布不可のため、同梱せずここで取得する（ADR-0004）。
# ピン先は 5th edition の最終タグ `5th-Ed`（ISO/IEC 23009-1:2022）。
TARBALL_URL="https://github.com/MPEGGroup/DASHSchema/archive/refs/tags/5th-Ed.tar.gz"
TARBALL_SHA256="bf7868764bbd303d96e69a08fe01ad2a9434a66ebc3a483ff8b19bc9a46e3f85"

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
destination="$repository_root/fixtures/dashschema"

mkdir -p "$repository_root/fixtures/private"

if [ -f "$destination/DASH-MPD.xsd" ]; then
    echo "fixtures already present: $destination (delete the directory to re-fetch)"
    exit 0
fi

temporary_directory="$(mktemp -d)"
trap 'rm -rf "$temporary_directory"' EXIT

tarball="$temporary_directory/dashschema.tar.gz"
echo "downloading $TARBALL_URL"
curl -fsSL --retry 3 -o "$tarball" "$TARBALL_URL"
echo "$TARBALL_SHA256  $tarball" | sha256sum -c -

mkdir -p "$temporary_directory/extracted"
tar -xzf "$tarball" -C "$temporary_directory/extracted" --strip-components=1

rm -rf "$destination"
mv "$temporary_directory/extracted" "$destination"

echo "fetched DASHSchema 5th-Ed into $destination"
