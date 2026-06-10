---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# モデル構造体は `#[non_exhaustive]` + `new(必須属性)` + `pub` フィールド

## Context and Problem Statement

必須属性を非 `Option` で表す方針のもと、利用者が構造体をどう構築するか。スキーマ改訂の手作業追従と「実コーパスを見て必須属性を `Option` に緩める」裁量運用により、公開後もフィールドは増減する。構造体リテラルで構築する利用者はフィールド追加のたびに壊れる。

## Considered Options

* 全フィールド `pub`、`#[non_exhaustive]` なし（リテラル構築可、追加は常に major）
* `#[non_exhaustive]` + `new(必須属性...)` + `pub` フィールド（ミュータブル更新スタイル）
* builder パターン（derive crate 依存 or 60〜80 型ぶん手書き）

## Decision Outcome

Chosen option: 「`#[non_exhaustive]` + `new(必須属性...)` + `pub` フィールド」。構築は `let mut r = Representation::new(base, bandwidth); r.width = Some(...);` のスタイル。

* 必須性の型保証は `new` のシグネチャが担う（idea.md の「型システムが構築側の正しさを保証」を維持）
* Optional フィールドの追加が semver minor で済み、スキーマ改訂追従を躊躇しなくてよい
* builder は依存追加か大量ボイラープレートの二択で、ミュータブル更新で足りる用途に過剰

### Consequences

* Good, because 追加的なスキーマ改訂・`Option` フィールド追加を minor リリースで出せる
* Bad, because 構造体リテラル・FRU 構文は利用者に使えない（読み取り・ミュータブル更新は可能）
* Neutral: required→`Option` への緩和は `new` のシグネチャ変更となり major — これは選択肢によらず避けられず、頻度も低い
