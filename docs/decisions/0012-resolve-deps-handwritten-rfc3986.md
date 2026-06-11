---
status: accepted
date: 2026-06-11
decision-makers: r-kato
---
# mpd-resolve の依存は mpd-schema のみとし、RFC 3986 参照解決を自前実装する

## Context and Problem Statement

mpd-schema は quick-xml + chrono のみ・proc-macro なし・WASM ビルド可を保っている。mpd-resolve の BaseURL 階層解決には URL の相対参照解決が必要で、`url` crate 等を依存に足すかが論点。

## Decision Drivers

* 23009-1 は BaseURL の解決を RFC 3986 で規定する。`url` crate は WHATWG URL Standard の実装で、RFC 3986 と挙動差がある（非特殊スキームの扱い、ルートを超える `..`、バックスラッシュの正規化等）
* `url::Url` は相対参照を表現できず、相対基底に対する解決もできない。ADR-0011 は「文書 URL 未提供 + 相対 `BaseURL`」で相対参照のままの出力を許すため、相対 × 相対の合成が必要
* 必要なのは RFC 3986 §5 の参照解決アルゴリズムだけで、§5.4 に正規・異常系のテストベクタが規定済み
* WASM・最小依存・proc-macro なしの方針を resolve でも維持したい

## Considered Options

* `url` crate（WHATWG URL Standard 実装）
* `iri-string` crate（RFC 3986/3987 準拠）
* RFC 3986 §5 を自前実装

## Decision Outcome

Chosen option: 「自前実装」。依存は `mpd-schema`（path + version 指定）のみ。chrono は v1（static のみ、ADR-0009）では不要で、dynamic 対応の着手時に再検討する。

実装範囲は §5.2 の参照解決（merge / remove_dot_segments）と Appendix B の5成分分解に限る。URL の妥当性検証・正規化（パーセントエンコード、IDNA 等）はしない — 入力の字句をそのまま合成する。基底が相対参照の場合（文書 URL 未提供）は §5.2 のパスマージを相対基底に準用し、結果を相対参照のまま返す（RFC 3986 は絶対基底を前提とするため、この準用は RFC の定義域外であることを doc に明記する）。

§5.4 のテストベクタ（normal examples + abnormal examples）を単体テストに全件写して準拠を担保する。

### Consequences

* Good, because 追加依存ゼロで WASM・最小依存の方針が保たれ、相対 × 相対の合成が可能になる
* Good, because RFC 準拠が §5.4 ベクタで機械的に検証できる
* Bad, because URL 解決コードの保守責任を負う。ただし対象は安定した RFC の固定アルゴリズムで、仕様変更による追従は想定しない
* Bad, because パーセントエンコードや IDN の検証はしないため、不正な URL 字句は不正なまま合成される。検証が必要な利用者は出力を `url` crate 等で再解析する（ADR-0011）
