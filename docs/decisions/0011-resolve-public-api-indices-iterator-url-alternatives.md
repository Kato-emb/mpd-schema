---
status: accepted
date: 2026-06-12
decision-makers: r-kato
---
# mpd-resolve の公開 API はインデックス指定 + 検証済み列挙オブジェクト、URL は全代替の `Vec<String>`

## Context and Problem Statement

入力は `&Mpd` と対象 Representation の指定。指定方法（インデックス / id / モデルへの参照）、出力の形（`Vec` / イテレータ）、URL の型（`url::Url` / `String`）、複数 `BaseURL`（代替）の扱いを決める。

## Decision Drivers

* 継承解決には祖先（Period・AdaptationSet）が必要 → `&Representation` 単体の参照渡しでは文脈が足りず、`&Mpd` 内での位置同定もポインタ比較頼みになる
* `Period@id` は XSD で optional → id 指定は一様に使えない
* 文書 URL が与えられず `BaseURL` も相対のとき、解決結果は相対参照のまま → `url::Url` は相対参照を表現できない（ADR-0012）
* メディアセグメント数は大きくなり得る（長尺 VoD）→ 全件 `Vec` の前払い割り当てを避けたい
* 複数 `BaseURL` は取得失敗・速度劣化時にプレーヤーが**取得時**に切替えるための代替（コーパスでは example_G1 の cdn1/cdn2 が典型）。どれを使うかは解決前に決まらないので、事前選択を求める API はユースケースに合わず、切替のたびに再解決と同一セグメントの再特定を強いる
* モデル層の構築規約（ADR-0002）との整合: 利用者が構築する入力型は `#[non_exhaustive]` + `new(必須)` + `pub` フィールドに従う。出力型は利用者が構築しないので `new()` は持たないが、フィールド追加を minor に収めるため `#[non_exhaustive]` は適用する

## Considered Options

* 指定方法: usize インデックス3つ組 / id 文字列 / モデルへの参照
* 出力: `Vec<MediaSegment>` / 構築時検証済みオブジェクト + 無謬イテレータ / `Item = Result` のイテレータ
* URL 型: `String` / `url::Url` / 独自 URL 型
* 複数 `BaseURL`: 各階層で先頭のみ採用 / クエリで事前にインデックス指定 / 全代替を列挙

## Decision Outcome

Chosen option: インデックス3つ組 + 検証済み列挙オブジェクト + 全代替の `Vec<String>`。スケッチ（名前は実装時に最終化、形をここで固定する）:

```rust
pub fn segments(mpd: &Mpd, query: &SegmentQuery<'_>) -> Result<Segments, Error>;

// dynamic 対応（ADR-0009）。static 文書も受理し、結果は segments() と一致する
pub fn segments_at(
    mpd: &Mpd,
    query: &SegmentQuery<'_>,
    now: std::time::SystemTime,
) -> Result<Segments, Error>;

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
    pub urls: Vec<String>, // 長さ >= 1。解決済み。絶対基底が無い場合は相対参照のまま
    pub byte_range: Option<ByteRange>,
}

#[non_exhaustive]
pub struct MediaSegment {
    pub urls: Vec<String>,     // SegmentRef.urls と同じ契約
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

複数 `BaseURL`（代替）は**全列挙**する。各階層の選択肢の直積（各階層で1つ選び、親の解決結果に対して解決する、の全組み合わせ）を `Segments` 構築時に解決し、各セグメントの `urls` に並べる。順序は API 契約に含める: 文書順の辞書式で、外側の階層（MPD 側）の選択が優先して変わる。先頭 = 全階層で先頭を選んだ組（仕様上の default）。展開後の参照が絶対 URL の場合など、同一 URL に潰れた代替は重複除去する。

フェイルオーバー・負荷分散はプレーヤーポリシーのまま: プレーヤーは `urls[0]` で取得し、失敗・速度劣化時にその場で残りへ切替える。再解決も `Mpd` の改変も要らない。順序契約により、モデルの `base_urls`（`serviceLocation` 等のメタデータ）との突き合わせもできる。

### Consequences

* Good, because イテレーションが無謬で利用者コードが単純（エラー処理は構築時の一箇所）
* Good, because フェイルオーバー・負荷分散が再解決なしで書ける。ライブラリは代替の選択ポリシーを持たない
* Good, because 相対参照のままの出力を型で排除しない
* Bad, because `String` は「これは URL である」という型保証を持たない。検証が必要な利用者は出力を `url` crate 等で再解析する
* Bad, because 代替が1つでも `Vec<String>` のアロケーションを払う。`SmallVec` 等のインライン最適化は、依存追加（ADR-0012）と公開フィールド型への第三者クレートの semver 結合を招くため採らない（セグメント取得はネットワーク I/O が支配的で、ここは効かない）
* Neutral: Representation の選択ロジック（bandwidth・codec による絞り込み等）は利用者が書く前提。選択ヘルパは v1 スコープ外

## More Information

責務境界の確認: 入力は解析済み `&Mpd` と文書 URL 文字列（dynamic では now も）、出力は純粋データ。I/O・wall clock なし — CONTEXT.md の「解決」に収まる。
