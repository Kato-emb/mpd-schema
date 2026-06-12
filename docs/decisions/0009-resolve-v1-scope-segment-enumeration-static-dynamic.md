---
status: accepted
date: 2026-06-12
decision-makers: r-kato
---
# mpd-resolve v1 はセグメント列挙4機能 + dynamic の可用ウィンドウ計算

## Context and Problem Statement

ADR-0001 で解決（Resolution）はワークスペース内の姉妹クレートに分離すると決めた。着手にあたり、クレート名と v1 に含める機能を確定する。候補は BaseURL 階層解決・テンプレート展開・SegmentTimeline 展開・セグメント列挙（init / media / index）。また dynamic MPD（`MPD@type="dynamic"`）の可用ウィンドウ計算（`availabilityStartTime` 基準、now は引数渡し）を v1 に含めるかが論点。

## Decision Drivers

* 利用者の最頻ユースケースは「Representation を選んでセグメント URL 列を得る」（ADR-0001）。テンプレート展開などの部品単体では足りず、Representation → AdaptationSet → Period の継承解決まで統合して初めて成立する
* live 配信は dynamic 前提であり、static 限定では live 利用者が v1 を使えない。可用ウィンドウ計算は now を引数に取れば純粋計算（CONTEXT.md「解決」）に収まる
* live 配信では dynamic → static の遷移が正常系（イベント終了時に `MPD@type` が static に変わる）。エントリポイントが type ごとに分かれて利用者に分岐を強いると、この遷移を跨ぐプレーヤーコードが複雑になる
* dynamic の検証は「dynamic MPD スナップショット + 固定 now + 期待セグメントリスト」の形にすれば決定的にできる（DASH-IF livesim 等から採取）。実時間に依存するテストは要らない
* GOAL の「厳格側に倒す」: 可用時刻を黙って誤るくらいなら明示的に拒否する。低レイテンシ系の属性を中途半端に扱わない
* 純粋計算の原則（CONTEXT.md「解決」）: wall clock の取得はどの選択肢でもクレートに入れない

## Considered Options

* セグメント列挙までの4機能 + static のみ。dynamic はエラーで拒否
* 4機能 + dynamic の可用ウィンドウ計算（now 引数渡し）まで含める
* 部品（テンプレート展開・タイムライン展開）のみ公開し、列挙への統合は利用者に委ねる

## Decision Outcome

Chosen option: 「4機能 + dynamic の可用ウィンドウ計算」。クレート名は `mpd-resolve`（crates.io で 2026-06-11 時点未使用を確認）。

v1 に含める:

* **BaseURL 階層解決** — RFC 3986 の相対参照解決で MPD → Period → AdaptationSet → Representation の `BaseURL` を合成。文書 URL（MPD の取得元）は引数で受ける。複数 `BaseURL`（代替）は全列挙する（ADR-0011）
* **テンプレート展開** — `$$` / `$RepresentationID$` / `$Number$` / `$Bandwidth$` / `$Time$` / `$SubNumber$` と書式タグ `%0[width]d`（23009-1 5.3.9.4.4）
* **SegmentTimeline 展開** — `S@t` / `@d` / `@r`（`-1` は次の `S` または打ち切り点まで。static: Period 終端 / dynamic: 可用ウィンドウ終端）/ `@n` / `@k`
* **セグメント列挙** — `SegmentBase` / `SegmentList` / `SegmentTemplate` の3階層の継承・上書き解決と addressing mode 判定を統合し、init / media / index セグメントの URL + byte range を列挙
* **可用ウィンドウ計算（dynamic）** — now を必須引数に取る別エントリポイント（API 形状は ADR-0011）で、`availabilityStartTime` と `Period@start` を起点にセグメント可用時刻を計算し、now 時点で利用可能な media セグメントのみ列挙する。窓の開始は `timeShiftBufferDepth`（欠落時は制限なし）。結果は now 時点のスナップショットであり、`minimumUpdatePeriod` に従う MPD 再取得・再解決は利用者責務（トランスポート = 利用者責務、ADR-0001）。static 文書も同エントリポイントで受理し、結果は static 用エントリポイントと一致する（dynamic → static 遷移を単一コードパスで跨げる）

v1 で拒否・除外:

* dynamic 文書を static 用エントリポイントに渡した場合はエラー（可用性を無視したセグメントリストを黙って返さない）
* 低レイテンシ系: dynamic の解決で `@availabilityTimeOffset`（`BaseURL` 上を含む）または `@availabilityTimeComplete="false"` に遭遇したらエラーで拒否。なお `BaseURL@availabilityTimeOffset` は代替ごとに異なり得るため、将来の対応では代替ごとの可用時刻の注記が要る（ADR-0011 の全代替列挙と整合させる）
* dynamic で `availabilityStartTime` が欠落、またはタイムゾーン無し（`XsDateTime::Unzoned`、コーパスに実在する形）の場合はエラー（now との減算が定義できない）
* `suggestedPresentationDelay` に基づく提示時刻の計算（提示タイミングはプレーヤーポリシー。本クレートは可用性のみ扱う）
* UTCTiming による時計同期（now を渡すのは利用者）
* Patch 適用（将来 additive、CONTEXT.md）、bitstream switching セグメントの列挙、MPD chaining
* xlink 参照の取得（トランスポート = 利用者責務、ADR-0001）

### Consequences

* Good, because live 利用者が v1 から使える。dynamic → static 遷移も単一エントリポイントで跨げる
* Good, because 検証はスナップショット形式（dynamic MPD + 固定 now + 期待リスト）で決定的。fixtures への dynamic コーパス追加は実装フェーズで行う
* Bad, because now の渡し方と型が v1.0 で固定され、設計ミスは 2.0 を強制する。緩和: エントリポイント分離により static 側 API には波及しない
* Bad, because 低レイテンシ拒否などの仕様境界を fail-fast 一覧（ADR-0010）として維持するコストが乗る
* Neutral: wall clock 非依存は維持する（現在時刻が必要な計算は呼び出し側が now を渡す）

## More Information

now の型は `std::time::SystemTime`（UTC の瞬間）。chrono の型を公開 API に出すと chrono のメジャーバージョンが mpd-resolve の semver 契約に結合し、依存方針（ADR-0012）とも整合しないため std のみとする。`availabilityStartTime`（`XsDateTime`）との演算はエポック基準の整数演算に落とす。
