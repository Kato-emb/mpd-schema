# アーキテクチャ

`mpd-schema` の構造設計。個々の決定の経緯は [decisions/index.md](./decisions/index.md)、用語は [CONTEXT.md](./CONTEXT.md) を正とする。

## 全体像

リポジトリは最初から virtual workspace（[ADR-0001](./decisions/0001-schema-crate-excludes-resolution.md)）。

```
/
├── Cargo.toml              # [workspace] のみ
├── crates/
│   └── mpd-schema/         # 本体。将来、解決クレート（命名は着手時に決定。候補: mpd-resolve）が並ぶ
├── scripts/
│   └── fetch-fixtures.sh   # DASHSchema 5th-Ed-Final を sha256 固定で取得（ADR-0004）
├── fixtures/               # gitignore。fetch スクリプトの展開先 + 私有 MPD コーパス
└── docs/
```

責務は3層に切る（CONTEXT.md の用語）:

| 責務 | 内容 | 置き場 |
|---|---|---|
| スキーマ | byte⇔struct 双方向変換 | `mpd-schema`（このクレート） |
| 解決 | セグメント URL 導出・タイムライン展開・Patch 適用 | 将来の姉妹クレート |
| トランスポート | HTTP 取得・`Location` 追跡・`xlink:href` 取得 | 利用者 |

コアは I/O 非依存で `wasm32-unknown-unknown` でビルド可能に保つ。

## データフロー

```
解析:   &[u8] ──quick-xml NsReader──▶ Event ──de──▶ モデル構造体
        (backend/ 内に閉じる)      (pub(crate))

構築:   モデル構造体 ──ser──▶ Event ──quick-xml Writer──▶ io::Write
                            (pub(crate))   (backend/ 内に閉じる)
```

quick-xml の型は `backend/` の外に出ない。`de`/`ser` は自前 Event enum のみに依存する（[ADR-0007](./decisions/0007-quick-xml-behind-internal-seam.md)）。

## モジュール構成

```
crates/mpd-schema/src/
├── lib.rs          # 公開 API の re-export
├── model.rs        # サブモジュール宣言と re-export
├── model/          # 手書き構造体（XSD complexType と 1:1、約 60〜80 型）
│   ├── mpd.rs      #   XSD の領域ごとに数ファイルへまとめる
│   ├── segment.rs  #   （型ごとの細分はしない。例: SegmentBase/List/Template/Timeline を1ファイルに）
│   ├── descriptor.rs
│   ├── types.rs    # XsDuration, FrameRate, Ratio, ConditionalUint, ...
│   └── element.rs  # 未知ノード用の汎用 Element ツリー
├── event.rs        # 自前 Event enum（pub(crate)）
├── de.rs           # Event → 構造体（要素パスのスタックを保持しエラーに焼き込む）
├── ser.rs          # 構造体 → Event
├── backend.rs      # quick-xml アダプタ（Event との相互変換のみ）
└── error.rs        # Error / ErrorKind
```

`model/` 内の区分けは XSD の論理領域単位。実装が進んで肥大したファイルを割るのは可。

モデルは自動生成しない。XSD 原本は手書き時の参照資料 + CI 検証材料（[ADR-0006](./decisions/0006-handwritten-model-no-xsd-codegen.md)）。serde は使わない（[ADR-0005](./decisions/0005-no-serde.md)）。

## 公開 API

公開面は構造体レベルのみ。Event 層・ストリーミングは公開しない。

```rust
impl Mpd {
    pub fn from_slice(bytes: &[u8]) -> Result<Mpd>;      // 読みのコア
    pub fn from_str(s: &str) -> Result<Mpd>;             // as_bytes() するだけ
    pub fn from_reader<R: io::Read>(r: R) -> Result<Mpd>; // read_to_end → from_slice
    pub fn write_to<W: io::Write>(&self, w: W) -> Result<()>; // 書きのコア
    pub fn to_string(&self) -> String;                   // Vec<u8> に向ける便宜
}
```

- 読み書きの非対称はデータの性質由来: 読みは入力全長が呼び出し時点で確定（スラープに漸近的な損なし）、書きは出力長未知（`io::Write` が可変長書きの上位互換）
- エンコーディングは v1 は UTF-8 のみ。非 UTF-8 はエラー
- ルート型は v1 では `Mpd` のみ。`Patch` 文書は将来 additive に追加（minor で出せる）

## モデル層の規約

全構造体に共通する4つの規約:

1. **構築**（[ADR-0002](./decisions/0002-non-exhaustive-structs-with-new.md)）: `#[non_exhaustive]` + `new(必須属性...)` + `pub` フィールド。

   ```rust
   let mut r = Representation::new(base, bandwidth);
   r.width = Some(1920);
   ```

   必須性の型保証は `new` のシグネチャが担い、Optional フィールド追加は semver 互換に収まる。

2. **継承は合成**: `xs:extension` は base 構造体の埋め込みで表現（`struct Representation { pub base: RepresentationBase, ... }`）。`Deref` 透過はやらない。base 部分の de/ser は関数1つに共通化。

3. **強い型付け**: 属性値は専用型に解析する。`XsDuration`（年・月を表現するため自前）、`FrameRate { num, den }`、`Ratio`、`ConditionalUint`。`xs:dateTime` は `chrono`。例外は `SegmentTemplate@media` 等のテンプレート文字列で、v1 は `String` のまま（解析は解決クレートの領分、ADR-0001）。

4. **未知ノード受け皿**: 各構造体が汎用 Element ツリーの受け皿フィールドを持つ。

   ```rust
   pub struct Element {
       pub name: String,              // 書かれたままの qualified name（"cenc:pssh"）
       pub namespace: Option<String>, // 解析時に解決された URI（読み取り用おまけ）
       pub attributes: Vec<(String, String)>, // xmlns:* 宣言もここに保持される
       pub children: Vec<Node>,
   }
   pub enum Node { Element(Element), Text(String) } // CDATA は Text に正規化、コメント・PI は破棄
   ```

   - 既知要素のマッチは `NsReader` による (名前空間 URI, ローカル名) 解決。未知ノードは字句保存で、書き戻しは字句側を使う（[ADR-0003](./decisions/0003-lexical-namespace-handling-for-unknown-nodes.md)）
   - 再シリアライズ時、未知要素は既知の子の**後ろ**にまとめて出力（DASH-MPD.xsd の `xs:any` が sequence 末尾にあるため、これが XSD 的に正しい位置）。未知同士の相対順序のみ保持

roundtrip の保証水準は「意味論的等価」（定義は CONTEXT.md）。コメント・空白・属性順序・字句形（`PT120S` vs `PT2M`）は保持しない。

## エラー

```rust
#[non_exhaustive]
pub struct Error {
    pub path: String,    // "MPD > Period[0] > AdaptationSet[2] @ minBufferTime"
    pub kind: ErrorKind, // #[non_exhaustive]: Xml(..), MissingAttribute,
}                        //   InvalidValue { value, expected }, Io(..), ...
```

- strict 一本、最初のエラーで即停止（fail-fast）
- `path` は `de` が降下時に持つ (要素名, 同名インデックス) スタックからエラー時のみ構築
- `thiserror` 不使用。`Display` / `std::error::Error` は手書き
- lenient モードは将来の別エントリポイント（`from_slice_lenient() -> (Mpd, Vec<Error>)` 想定）。両 enum の `#[non_exhaustive]` がその余地

## 依存

`quick-xml`（backend/ に閉じる）と `chrono`（`xs:dateTime`）のみ。proc-macro 系依存なし。MSRV は公開時に両依存に合わせて明記。

## 検証・CI

| 検証 | 入力 | 内容 |
|---|---|---|
| roundtrip テスト | DASHSchema サンプル MPD（patch 系は除外）+ DASH-IF テストベクタ | `parse → serialize → parse` で意味論的等価を確認 |
| 未知ノード保持 | DRM 付き実 MPD | `cenc:pssh` 等が受け皿経由で生き残ることを確認 |
| XSD バリデーション | serialize 出力 + DASH-MPD.xsd | `xmllint --schema`。手書きモデルの XSD 乖離事故への安全網（ADR-0006 の代償） |
| WASM ビルド | — | `cargo build --target wasm32-unknown-unknown` |

- fixtures は `scripts/fetch-fixtures.sh` で取得（[ADR-0004](./decisions/0004-fetch-fixtures-by-script-not-bundled.md)）。未取得なら fixtures 必須テストはスクリプト実行を促して fail。CI は fetch ステップ + キャッシュ
- 私有・実サービスの MPD は gitignore したローカル専用ディレクトリで
- property-based テストは v1 以降

## 対象スキーマとバージョン運用

- モデル化対象は DASHSchema `5th-Ed-Final` タグ（ISO/IEC 23009-1:2022、最新の出版済み edition）。6th edition は出版後に乗り換え、「最新 edition 一本」を維持。それまでの 6th-ed 要素は未知ノード受け皿が拾う
- 初版 0.1.0。1.0 の条件: (1) 5th-Ed-Final の complexType 全カバー、(2) 公開 fixtures の roundtrip green、(3) 実コーパス由来の「required → `Option` 緩和」が一巡していること。緩和は `new()` のシグネチャ変更 = breaking なので、0.x のうちに使い切る
- crates.io 名 `mpd-schema` は確保可能（2026-06-10 確認）。ライセンス MIT OR Apache-2.0。fixtures は package に含めない
