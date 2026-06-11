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
else
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
fi

fetch_pinned() {
    local url="$1"
    local sha256="$2"
    local target="$3"

    if [ -f "$target" ]; then
        echo "already present: $target (delete the file to re-fetch)"
        return
    fi

    echo "downloading $url"
    curl -fsSL --retry 3 -o "$target.download" "$url"
    echo "$sha256  $target.download" | sha256sum -c -
    mv "$target.download" "$target"
}

# DRM 付き実 MPD（未知ノード保持テスト用）。DASH-IF Test Vector Database 掲載の
# ベクタだが git リポジトリではなく散在 URL のため、ファイル単位で sha256 固定
# ダウンロードする（ADR-0004）。これらも再配布せず fixtures（gitignore）に置く。
dashif_destination="$repository_root/fixtures/dashif"
mkdir -p "$dashif_destination"

# Axinom multi-DRM ベクタ: cenc:pssh（Widevine / PlayReady）を含む
fetch_pinned \
    "https://media.axprod.net/TestVectors/v7-MultiDRM-SingleKey/Manifest_1080p.mpd" \
    "3905d8fcd37fa79adc34df2e8ff7c2471e3999c948914b8b8a0548ef095f055f" \
    "$dashif_destination/axinom-v7-multidrm-singlekey.mpd"

# Microsoft PlayReady ベクタ: mspr:pro を含む
fetch_pinned \
    "https://test.playready.microsoft.com/media/dash/APPLEENC_CBCS_BBB_1080p/1080p.mpd" \
    "4204b49bd96a5453a504e605fd154e582c7d9247850de046bacdb125bc84ba81" \
    "$dashif_destination/playready-cbcs-bbb-1080p.mpd"

# DASH-MPD.xsd が URL で import する W3C スキーマ。xmllint の XSD 検証を
# --nonet で決定的に走らせるため、ローカルへ取得し XML カタログで引き当てる。
w3c_destination="$repository_root/fixtures/w3c"
mkdir -p "$w3c_destination"

fetch_pinned \
    "https://www.w3.org/XML/2008/06/xlink.xsd" \
    "c83df86c7fdc16eb9c862b83dfb53fc1b1a4bcafd6e1d1217199e0188b82f24a" \
    "$w3c_destination/xlink.xsd"

fetch_pinned \
    "https://www.w3.org/2001/xml.xsd" \
    "61960fb3131e38022caad5360e2f33a3382578ab3c80cd58bd74320ede61b20c" \
    "$w3c_destination/xml.xsd"

cat > "$w3c_destination/catalog.xml" <<'EOF'
<?xml version="1.0"?>
<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
  <uri name="http://www.w3.org/XML/2008/06/xlink.xsd" uri="xlink.xsd"/>
  <uri name="http://www.w3.org/2001/xml.xsd" uri="xml.xsd"/>
</catalog>
EOF
