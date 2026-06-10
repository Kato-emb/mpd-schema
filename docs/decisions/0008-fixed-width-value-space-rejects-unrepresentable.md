---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# 属性値は固定幅の値空間に写像し、表現不能なスキーマ有効値は拒否する

## Context and Problem Statement

XSD の数値系単純型には字句空間に桁数上限がないものがある。`xs:duration` の小数秒は任意精度、`RatioType`（`[0-9]*:[0-9]*`）や `FrameRateType`（`[0-9]+(/[1-9][0-9]*)?`）はパターン制約のみで桁数無制限。一方モデル層は固定幅の Rust 型（小数秒はナノ秒 `u32`、比率は `u32` 等）に写像するため、スキーマ的に有効だが表現できない値が存在する。これをどう扱うか。

## Considered Options

* 字句のまま `String` で保持（値空間の制限なし、強い型付けを放棄）
* 任意精度型を導入（依存追加 or 自前実装）
* 固定幅の値空間 + 表現不能値は `InvalidValue` で拒否（fail-fast）
* 固定幅の値空間 + 表現不能値は切り捨て/丸めで受理

## Decision Outcome

Chosen option: 「固定幅の値空間 + 拒否」。

* 黙って精度を落とすと parse → serialize で入力と異なる値が出力され、roundtrip の「意味論的等価」（CONTEXT.md）と両立しない
* 任意精度・字句保持は、現実の MPD に出現しない極端値のために強い型付け（ARCHITECTURE.md モデル層規約 3）を損なう
* 例外として、`xs:duration` の小数秒 9 桁（ナノ秒）を超える部分が全て 0 の場合は情報落ちがないため受理する（`PT1.5000000000S` は可、`PT1.0000000005S` は不可）

### Consequences

* Good, because 受理した値は必ず無損失で roundtrip する
* Good, because モデル層の型は固定幅のまま単純に保てる
* Bad, because スキーマ的に有効な極端値（ピコ秒精度の duration、`u32::MAX` 超の Ratio 等）を含む文書はパース全体が失敗する。実コーパスで顕在化した場合はこの ADR を見直す
