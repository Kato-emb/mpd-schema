#!/usr/bin/env bash
set -euo pipefail

# DASHSchema は ISO カスタムライセンスで再配布不可のため、同梱せずここで取得する（ADR-0004）。
# ピン先は 5th edition の最終タグ `5th-Ed`（ISO/IEC 23009-1:2022）。
TARBALL_URL="https://github.com/MPEGGroup/DASHSchema/archive/refs/tags/5th-Ed.tar.gz"
TARBALL_SHA256="bf7868764bbd303d96e69a08fe01ad2a9434a66ebc3a483ff8b19bc9a46e3f85"

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
destination="$repository_root/fixtures/dashschema"

mkdir -p "$repository_root/fixtures/private"

# 単一ファイルを sha256 固定で取得する。既存ファイルも照合し、不一致なら
# 再取得する（ピン先は可変な vendor エンドポイントなので、pin 更新を
# ローカルにも伝播させる）。検証失敗時は staging（.download）を残さない。
fetch_pinned() {
    local url="$1"
    local sha256="$2"
    local target="$3"

    if [ -f "$target" ]; then
        if echo "$sha256  $target" | sha256sum -c --status -; then
            echo "already present: $target"
            return
        fi
        echo "sha256 mismatch, re-fetching: $target"
    fi

    echo "downloading $url"
    curl -fsSL --retry 3 -o "$target.download" "$url"
    if ! echo "$sha256  $target.download" | sha256sum -c --status -; then
        rm -f "$target.download"
        echo "sha256 verification failed: $url" >&2
        exit 1
    fi
    mv "$target.download" "$target"
}

if [ -f "$destination/DASH-MPD.xsd" ]; then
    echo "fixtures already present: $destination (delete the directory to re-fetch)"
else
    staging=""
    temporary_directory="$(mktemp -d)"
    trap 'rm -rf "$temporary_directory" "$staging"' EXIT

    tarball="$temporary_directory/dashschema.tar.gz"
    fetch_pinned "$TARBALL_URL" "$TARBALL_SHA256" "$tarball"

    # staging は fixtures/ 内（destination と同一ファイルシステム）に作り、
    # 展開完了後に rename する。tmpfs からのクロス FS な mv は非アトミックな
    # 再帰コピーで、中断すると sentinel（DASH-MPD.xsd）だけ残り samples 欠落の
    # 壊れた状態が固定化する。同一 FS の rename ならアトミックでこれを防ぐ。
    mkdir -p "$repository_root/fixtures"
    staging="$(mktemp -d "$repository_root/fixtures/.dashschema.XXXXXX")"
    tar -xzf "$tarball" -C "$staging" --strip-components=1

    rm -rf "$destination"
    mv "$staging" "$destination"

    echo "fetched DASHSchema 5th-Ed into $destination"
fi

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
