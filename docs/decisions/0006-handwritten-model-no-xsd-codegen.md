---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# モデル構造体は手書き、XSD からの自動生成はしない

## Context and Problem Statement

DASH-MPD.xsd という機械可読なスキーマ原本が存在する以上、構造体・変換コードの自動生成が自然な発想に見える。当初案も3レイヤ（XSD 自動生成 / ラッパー構造体 / バックエンド結合）だった。生成にするか手書きにするか?

## Decision Drivers

* complexType は約 60〜80 個。手書きしても数千行・一度きりの投資
* スキーマ改訂は数年に一度で、手作業追従で十分（edition は後方互換・追加が基本）
* 既存の汎用 XSD→Rust コンパイラは DASH スキーマの要件（継承、未知ノード受け皿、強い型付け）を満たさず、自作は本体より重い
* 生成コードはどうせ人間向け API としては粗く、ラップ層が要る = 二重維持になる

## Considered Options

* XSD からの自動生成（3レイヤ構成）
* 手書きモデル1層のみ

## Decision Outcome

Chosen option: 「手書きモデル1層のみ」。XSD 原本は手書き時の参照資料 + CI 検証材料（`xmllint --schema` で serialize 出力を検証）として使う。

### Consequences

* Good, because レイヤが1つで API 設計の自由度が高い（強い型付け、`#[non_exhaustive]` + `new()` 規約、受け皿フィールド）
* Good, because XSD コンパイラという本体より重い成果物を抱えない
* Bad, because 手書きモデルが XSD から乖離する事故があり得る — 代償として CI の XSD バリデーションを安全網に置く（[0004](./0004-fetch-fixtures-by-script-not-bundled.md) の fixtures がその入力）
* Bad, because スキーマ改訂のたびに手作業で差分を追う
