---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# DASHSchema 由来ファイルは同梱せず、fetch スクリプトで実行時に取得（5th-Ed-Final にピン）

## Context and Problem Statement

検証戦略（roundtrip fixtures、`xmllint --schema` CI 検証）は DASHSchema の XSD とサンプル MPD を入力に取る。しかし DASHSchema のライセンスは「ISO/IEC Directives に従う」カスタム条項で、MIT/Apache リポジトリへの同梱（再配布）を許す根拠がない。また idea.md の「5th edition 系の最新」に対し、DASHSchema のデフォルトブランチは既に 6th-Ed（FDIS 段階、未出版）へ進んでいた。

## Decision Drivers

* コンテンツの再配布は不可。テスト時に作業ツリーへ実体化する仕組みだけが必要
* 手書きモデルの参照資料が動く枝（FDIS）を指すのは追従コストの無駄
* DASH-IF テストベクタは git リポジトリではなく散在 URL のため、fetch スクリプトはどのみち必要

## Considered Options

* ピン先: `5th-Ed-Final` タグ / `6th-Ed` ブランチ追従
* 取得方式: git submodule / fetch スクリプト / リポジトリ同梱

## Decision Outcome

* **ピン先: `5th-Ed-Final` タグ**（最新の出版済み標準 = ISO/IEC 23009-1:2022）。6th edition 出版時に乗り換え、「最新 edition 一本」原則を維持。それまでの 6th-ed 要素は未知ノード受け皿が拾う。
* **取得方式: `scripts/fetch-fixtures.sh`**。タグの tarball を sha256 固定でダウンロードし、gitignore ディレクトリへ展開。fixtures 必須のテストは未取得時に fetch スクリプトの実行を促して fail する。CI は fetch ステップ + キャッシュ。

submodule を退けた決め手は機構の数: DASH-IF テストベクタでスクリプトの存在は避けられず、submodule 併用は fixtures 取得の2機構並走になる。タグ tarball + sha256 で再現性は submodule（git SHA）と同等。

### Consequences

* Good, because リポジトリは再配布物を一切含まず、ライセンス問題が消える（fetch して使うだけなら再配布に当たらない — DASH-IF ベクタも同様）
* Good, because fixtures 取得の仕組みが私有 MPD コーパス置き場（gitignore）と同型に揃う
* Bad, because 初回テスト実行前に1コマンド必要。bash スクリプト約30〜50行の保守が発生
* Bad, because 上流がタグや tarball を消すと取得不能（submodule でも同様のリスク）

## More Information

crates.io の `mpd-schema` は未使用で名前確保可能（2026-06-10 確認）。idea.md の「要確認: DASHSchema 由来ファイルの同梱可否」はこの ADR で解決済み。
