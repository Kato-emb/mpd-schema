# mpd-schema

ISO/IEC 23009-1 (MPEG-DASH) の MPD を解析・構築する Rust ライブラリ。責務の境界を3層の用語で区別する。

## Language

### 責務境界

**スキーマ (Schema)**:
バイト列⇔構造体の双方向変換。`mpd-schema` クレートの唯一の責務。`xlink` 属性の構造表現もここに含む。
_Avoid_: パーサ（変換は双方向のため）

**解決 (Resolution)**:
解析済み MPD 上の純粋計算。セグメント URL 導出（`$Number$` 等のテンプレート展開、`BaseURL` 階層の解決）、`SegmentTimeline` のタイムライン展開など。将来の姉妹クレートの責務であり、`mpd-schema` には入れない。

**トランスポート (Transport)**:
I/O を伴う操作。HTTP 取得、`Location` 追跡、`minimumUpdatePeriod` での再取得、`xlink:href` 参照の取得。利用者の責務であり、このリポジトリのどのクレートにも入れない。

### ドメイン

**MPD (Media Presentation Description)**:
DASH のマニフェスト文書。XML Schema（XSD）で定義される。
_Avoid_: マニフェスト

**Patch**:
MPD 更新の差分配信用文書（`<Patch>` ルート、`DASH-MPD-UP.xsd` で定義）。v1 非スコープ。将来、文書の解析・構築はスキーマ（`mpd-schema`）、適用は解決クレートが担う。

**未知ノード**:
スキーマに定義のない要素・属性。各構造体の受け皿に汎用 Element ツリーとして保持される。DRM 関連（`cenc:pssh`、`mspr:pro` 等）はほぼ全てこれに該当する。

**意味論的等価**:
roundtrip 保証の水準。既知部分は型表現の一致、未知要素は名前・属性・テキスト・未知同士の相対順序の一致を意味する。既知要素との interleave 位置、コメント・PI、空白、属性順序、字句形（`PT120S` vs `PT2M`）は含まない。未知要素は既知の子の後ろにまとめて再出力され、その等価は字句ベース（名前空間接頭辞が異なれば不等価）。
_Avoid_: バイト等価、完全忠実
