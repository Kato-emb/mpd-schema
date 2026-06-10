---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# mpd-schema は解決（Resolution）を含めず、姉妹クレートに分離する

## Context and Problem Statement

MPD ライブラリの利用者の最頻ユースケースにはセグメント URL 導出（`$Number$` 等のテンプレート展開、`BaseURL` 階層の解決）やタイムライン展開がある。これらは I/O を伴わない純粋計算なので「トランスポートは利用者責務」という原則では除外できない。`mpd-schema` のスコープに含めるか?

## Decision Drivers

* テンプレート構文の解析を入れると置換セマンティクス（`%0d` 書式、未知識別子）が芋づる式にスコープへ入り、schema クレートの責務が肥大する
* crate 名 `mpd-schema` が責務（スキーマの読み書き）を語っている
* 利用者ニーズは実在するため「永久に他人事」にもしたくない

## Considered Options

* スコープ外（永久に下流の別作者のクレートに委ねる）
* スコープ外、ただし同一リポジトリの姉妹クレートとして将来実装
* v1 から `mpd-schema` に含める

## Decision Outcome

Chosen option: 「同一リポジトリの姉妹クレートとして将来実装」。`mpd-schema` は byte⇔struct 変換に徹し、解決はワークスペース内の別クレート（`crates/` 配下）に分離する。この意図に基づき、リポジトリは最初から virtual workspace（ルート Cargo.toml は `[workspace]` のみ、本体は `crates/mpd-schema/`）とする。

### Consequences

* Good, because schema クレートの責務が単純なまま保たれ、解決ロジックのバージョン進化が schema の semver から独立する
* Good, because 最初からワークスペースにすることで後の移動による CI パス・doc リンクの churn を避けられる
* Bad, because v1 利用者はセグメント URL 導出を自前で書く必要がある
* Neutral: `SegmentTemplate@media` 等のテンプレート文字列は v1 では `String` のまま（強い型付けの明示的例外）

## More Information

`xlink:href` の分解: 属性の構造表現は schema クレート、参照の取得はトランスポート（利用者責務）。解決クレートの仕事でもない。
