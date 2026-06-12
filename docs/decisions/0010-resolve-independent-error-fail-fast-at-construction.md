---
status: accepted
date: 2026-06-12
decision-makers: r-kato
---
# mpd-resolve は独立した Error 型を持ち、列挙の構築時に fail-fast する

## Context and Problem Statement

mpd-schema には `Error { path, kind }`（fail-fast、手書き `Display`）がある。mpd-resolve のエラー型をこれと共有するか独立させるか。また解決のどの不整合を fail-fast 対象とするか、エラーをいつ返すか（列挙の構築時か、イテレーション中か）。

## Decision Drivers

* ADR-0001: 解決のバージョン進化は schema の semver から独立。公開 API に `mpd_schema::Error` を出すと major バンプが連動する
* エラーの性質が異なる: schema は字句⇔値空間の失敗、resolve は要素間の整合性（addressing mode、テンプレート識別子、導出に必要な属性の欠落）
* mpd-resolve は解析をしない（`&Mpd` を受けるだけ）ので、`mpd_schema::Error` を内包・変換する場面が存在しない
* イテレータの `Item` を `Result` にする設計は、利用者に「途中まで成功したリスト」の扱いを強いる

## Considered Options

* `mpd_schema::Error` を共有（resolve 用の kind を追加）
* 独立した Error 型 + `From<mpd_schema::Error>` 変換
* 完全に独立した Error 型（変換なし）

## Decision Outcome

Chosen option: 「完全に独立した Error 型」。設計言語は mpd-schema と揃える:

```rust
#[non_exhaustive]
pub struct Error {
    pub path: String,    // "MPD > Period[0] > AdaptationSet[2] > SegmentTemplate @ media"
    pub kind: ErrorKind, // #[non_exhaustive]
}
```

`thiserror` 不使用、`Display` / `std::error::Error` は手書き。

**fail-fast 対象**（確定分。`ErrorKind` は `#[non_exhaustive]` なので追加可能）:

* 未知のテンプレート識別子、不正な書式タグ、対応の取れない `$`
* addressing mode 不整合: 実効レベルでの複数 mode 競合、`$Time$` 使用かつ `SegmentTimeline` 不在、number ベース導出で `@duration` 不在
* `S@r="-1"` で展開の打ち切り点が導出不能: static では Period 終端（`Period@duration` も `MPD@mediaPresentationDuration` も無い等）。dynamic では可用ウィンドウ終端を now から導出するため、これ自体は正常系（ADR-0009）
* byte range（`indexRange` / `mediaRange` 等）の字句不正
* timescale 換算・時刻演算の u64 オーバーフロー
* `MPD@type="dynamic"` の文書を static 用エントリポイントに渡した（dynamic は now を取るエントリポイントで扱う、ADR-0009）
* dynamic の解決で `MPD@availabilityStartTime` が欠落、またはタイムゾーン無し（`XsDateTime::Unzoned`。now との減算が定義できない）
* dynamic の解決で `@availabilityTimeOffset`（`BaseURL` 上を含む）または `@availabilityTimeComplete="false"` に遭遇（v1 は低レイテンシ未対応、ADR-0009）
* (period, adaptation_set, representation) 指定の範囲外

`timescale` の欠落はエラーではない（23009-1 規定の既定値 1 を適用する）。

**検証タイミング**: エントリポイントが返す列挙オブジェクトの構築時に全検証（テンプレート解析・タイムライン展開パラメータの検査・境界の時刻演算）を済ませ、セグメントのイテレーションは無謬とする（`Item = MediaSegment` であり `Result` ではない）。

### Consequences

* Good, because 利用者のエラー処理は構築時の `?` 一箇所で済み、途中失敗のセグメントリストが存在しない
* Good, because 2クレートの semver が独立に進化できる
* Bad, because 検証コストが構築時に前払いになる。ただし展開は `S` 要素数のオーダーで検査でき、セグメント数のオーダーは要しない
* Neutral: lenient モードの余地は両 enum の `#[non_exhaustive]` が担保する（mpd-schema と同じ構え）
