---
status: accepted
date: 2026-06-11
decision-makers: r-kato
---
# mpd-resolve v1 は static MPD のセグメント列挙に限定する

## Context and Problem Statement

ADR-0001 で解決（Resolution）はワークスペース内の姉妹クレートに分離すると決めた。着手にあたり、クレート名と v1 に含める機能を確定する。候補は BaseURL 階層解決・テンプレート展開・SegmentTimeline 展開・セグメント列挙（init / media / index）。また dynamic MPD（`MPD@type="dynamic"`）の可用ウィンドウ計算（`availabilityStartTime` 基準、now は引数渡し）を v1 に含めるかが論点。

## Decision Drivers

* 利用者の最頻ユースケースは「Representation を選んでセグメント URL 列を得る」（ADR-0001）。テンプレート展開などの部品単体では足りず、Representation → AdaptationSet → Period の継承解決まで統合して初めて成立する
* dynamic の可用性計算は 23009-1 の別の大きな仕様面（可用ウィンドウ、`timeShiftBufferDepth`、`suggestedPresentationDelay`、Annex A）で、検証には live 系の実コーパスが要る。DASHSchema サンプルと取得済みテストベクタで検証できるのは static の列挙まで
* GOAL の「厳格側に倒す」: 中途半端な dynamic 対応で黙って誤ったセグメントリストを返すより、明示的に拒否する
* 純粋計算の原則（CONTEXT.md「解決」）: wall clock の取得はどの選択肢でもクレートに入れない

## Considered Options

* セグメント列挙までの4機能 + static のみ。dynamic はエラーで拒否
* 4機能 + dynamic の可用ウィンドウ計算（now 引数渡し）まで含める
* 部品（テンプレート展開・タイムライン展開）のみ公開し、列挙への統合は利用者に委ねる

## Decision Outcome

Chosen option: 「4機能 + static のみ」。クレート名は `mpd-resolve`（crates.io で 2026-06-11 時点未使用を確認）。

v1 に含める:

* **BaseURL 階層解決** — RFC 3986 の相対参照解決で MPD → Period → AdaptationSet → Representation の `BaseURL` を合成。文書 URL（MPD の取得元）は引数で受ける
* **テンプレート展開** — `$$` / `$RepresentationID$` / `$Number$` / `$Bandwidth$` / `$Time$` / `$SubNumber$` と書式タグ `%0[width]d`（23009-1 5.3.9.4.4）
* **SegmentTimeline 展開** — `S@t` / `@d` / `@r`（`-1` は次の `S` または Period 終端まで）/ `@n` / `@k`
* **セグメント列挙** — `SegmentBase` / `SegmentList` / `SegmentTemplate` の3階層の継承・上書き解決と addressing mode 判定を統合し、init / media / index セグメントの URL + byte range を列挙

v1 で拒否・除外:

* `MPD@type="dynamic"` の文書はエラーで拒否する。将来、now を引数に取る別エントリポイントとして additive に追加する（minor で出せる）
* Patch 適用（将来 additive、CONTEXT.md）、bitstream switching セグメントの列挙、MPD chaining
* xlink 参照の取得（トランスポート = 利用者責務、ADR-0001）

### Consequences

* Good, because v1 の検証材料が既存 fixtures（DASHSchema の static サンプル + DASH-IF テストベクタ）で完結する
* Good, because dynamic 対応・Patch 適用を API 追加で出せる形が温存される
* Bad, because live 配信の利用者は v1 では使えない
* Neutral: wall clock 非依存は dynamic 対応後も維持する（現在時刻が必要な計算は呼び出し側が now を渡す）
