---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# quick-xml は内部の継ぎ目に隠し、公開 API は構造体レベルのみ

## Context and Problem Statement

XML バックエンドに quick-xml を使うが、その型を公開 API に漏らすと quick-xml のバージョンアップ・乗り換えが全て breaking change になる。バックエンドへの依存をどこで切るか。また Event 層・ストリーミング API をどこまで公開するか。

## Decision Drivers

* quick-xml の semver にこのクレートの semver を縛られたくない
* MPD は高々数 MB で、ストリーミング解析の実需がない
* クレート分割や公開トレイト化はイベント語彙の公開 = semver 契約を強制する
* WASM（`wasm32-unknown-unknown`）でビルド可能に保つ（quick-xml はそのままコンパイル可能、コアは I/O 非依存）

## Considered Options

* quick-xml の型を公開 API に露出（最薄ラッパー）
* 自前 Event enum を内部の継ぎ目にし、quick-xml は `backend/` に閉じ込める。公開 API は構造体レベルのみ
* バックエンドを公開トレイト化・別クレート化して差し替え可能にする
* roxmltree（DOM）をバックエンドにする

## Decision Outcome

Chosen option: 「自前 Event enum を内部の継ぎ目に、公開 API は構造体レベルのみ」。

* `de`/`ser` の変換ロジックは自前 Event enum のみに依存し、quick-xml の型は `backend/` の外に漏らさない
* Event 層は `pub(crate)`。公開 API は `Mpd::from_slice` / `from_str` / `from_reader` / `write_to` / `to_string` 相当のみ
* 入出力境界は std の型のみ: 読みは `&[u8]` コア（出力の構造体ツリーが全載りする以上、入力スラープに漸近的な損はない）、書きは `io::Write` コア（出力長未知のため）
* 公開トレイト化・クレート分割は複数バックエンドの需要が出てから。roxmltree は読み取り専用 DOM で pull 型の変換と合わず不採用

### Consequences

* Good, because quick-xml のメジャーアップや乗り換えが内部変更で済む
* Good, because イベント語彙を semver 契約にしない自由を保つ
* Bad, because ストリーミング処理を求める利用者には応えない（実需が出たら Event 層の公開を再検討）
