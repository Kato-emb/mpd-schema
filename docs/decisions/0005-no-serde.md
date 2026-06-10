---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# serde を使わず、自前のイベントベース変換で読み書きする

## Context and Problem Statement

Rust で XML⇔構造体変換といえば serde（+ quick-xml の serde サポート等）が定石であり、使わない選択は説明を要する。MPD スキーマを serde でモデル化できるか?

## Decision Drivers

* DASH スキーマは `xs:extension` による継承を多用するが、serde のデータモデルは継承関係を表現できず、フラット化や delegate 用のボイラープレートが膨大になる
* XML との双方向変換では serde 経由のパフォーマンスも悪い
* 未知ノード保持（汎用 Element ツリーへの受け皿）のような XML 固有の要件は serde の抽象とかみ合わない

## Considered Options

* serde + quick-xml の serde サポート
* 自前のイベントベース変換（`de`/`ser` モジュールが自前 Event enum と構造体を直接変換）

## Decision Outcome

Chosen option: 「自前のイベントベース変換」。継承は合成（base 構造体の埋め込み）で表現し、base 部分の読み書きを関数1つに共通化する。derive の利便性を捨てる代わりに、スキーマの構造に正確に沿った変換を直接書く。

### Consequences

* Good, because `xs:extension` の継承構造を XSD と 1:1 で表現でき、未知ノード受け皿・名前空間解決など XML 固有の要件を自由に実装できる
* Good, because serde のトレイトオブジェクト・中間表現を経由しない分、変換が速い
* Bad, because 60〜80 型ぶんの変換コードを手書きする（一度きりの投資。スキーマ改訂は数年に一度）
