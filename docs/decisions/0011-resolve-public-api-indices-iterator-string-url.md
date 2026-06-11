---
status: accepted
date: 2026-06-11
decision-makers: r-kato
---
# mpd-resolve の公開 API はインデックス指定 + 検証済み列挙オブジェクト、URL は `String`

## Context and Problem Statement

入力は `&Mpd` と対象 Representation の指定。指定方法（インデックス / id / モデルへの参照）、出力の形（`Vec` / イテレータ）、URL の型（`url::Url` / `String`）を決める。

## Decision Drivers

* 継承解決には祖先（Period・AdaptationSet）が必要 → `&Representation` 単体の参照渡しでは文脈が足りず、`&Mpd` 内での位置同定もポインタ比較頼みになる
* `Period@id` は XSD で optional → id 指定は一様に使えない
* 文書 URL が与えられず `BaseURL` も相対のとき、解決結果は相対参照のまま → `url::Url` は相対参照を表現できない（ADR-0012）
* メディアセグメント数は大きくなり得る（長尺 VoD）→ 全件 `Vec` の前払い割り当てを避けたい
* モデル層の構築規約（ADR-0002）との整合: 利用者が構築する入力型は `#[non_exhaustive]` + `new(必須)` + `pub` フィールドに従う。出力型は利用者が構築しないので `new()` は持たないが、フィールド追加を minor に収めるため `#[non_exhaustive]` は適用する

## Considered Options

* 指定方法: usize インデックス3つ組 / id 文字列 / モデルへの参照
* 出力: `Vec<MediaSegment>` / 構築時検証済みオブジェクト + 無謬イテレータ / `Item = Result` のイテレータ
* URL 型: `String` / `url::Url` / 独自 URL 型

## Decision Outcome

Chosen option: インデックス3つ組 + 検証済み列挙オブジェクト + `String`。スケッチ（名前は実装時に最終化、形をここで固定する）:

```rust
pub fn segments(mpd: &Mpd, query: &SegmentQuery<'_>) -> Result<Segments, Error>;

#[non_exhaustive]
pub struct SegmentQuery<'a> {
    pub period: usize,
    pub adaptation_set: usize,
    pub representation: usize,
    pub document_url: Option<&'a str>, // MPD の取得元。トランスポートは利用者責務なので引数で受ける
}
// ADR-0002 に従い SegmentQuery::new(period, adaptation_set, representation)

#[non_exhaustive]
pub struct Segments {
    // 構築時に全検証済み(ADR-0010)
    pub initialization: Option<SegmentRef>,
    pub index: Option<SegmentRef>,
    pub timescale: u32,
    pub presentation_time_offset: u64,
}
impl Segments {
    pub fn media(&self) -> Media<'_>; // Iterator<Item = MediaSegment>、無謬
}

#[non_exhaustive]
pub struct SegmentRef {
    pub url: String, // 解決済み。絶対基底が無い場合は相対参照のまま
    pub byte_range: Option<ByteRange>,
}

#[non_exhaustive]
pub struct MediaSegment {
    pub url: String,
    pub byte_range: Option<ByteRange>,
    pub number: Option<u64>,   // number ベースの addressing 時
    pub time: Option<u64>,     // time ベースの addressing 時（timescale 単位）
    pub duration: Option<u64>, // timescale 単位
}

#[non_exhaustive]
pub struct ByteRange {
    pub first: u64,
    pub last: Option<u64>, // None は open-ended（"500-"）
}
```

複数 `BaseURL`（代替）は各階層で**先頭を採用**する。代替の選択（フェイルオーバー・負荷分散）はプレイヤーのポリシーであり解決の責務外。代替はモデルの `base_urls` から読めるので情報は失われず、別の代替で解決したい利用者は `Mpd` 側を並べ替えて渡す。

### Consequences

* Good, because イテレーションが無謬で利用者コードが単純（エラー処理は構築時の一箇所）
* Good, because 相対参照のままの出力を型で排除しない
* Bad, because `String` は「これは URL である」という型保証を持たない。検証が必要な利用者は出力を `url` crate 等で再解析する
* Neutral: Representation の選択ロジック（bandwidth・codec による絞り込み等）は利用者が書く前提。選択ヘルパは v1 スコープ外

## More Information

責務境界の確認: 入力は解析済み `&Mpd` と文書 URL 文字列、出力は純粋データ。I/O・wall clock なし — CONTEXT.md の「解決」に収まる。
