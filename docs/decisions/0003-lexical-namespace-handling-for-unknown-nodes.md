---
status: accepted
date: 2026-06-10
decision-makers: r-kato
---
# 未知ノードの名前は字句保存 + 解決済み URI 併記、既知要素は名前空間解決でマッチ

## Context and Problem Statement

未知ノード（`cenc:pssh` 等)の名前をどう表現するか。また既知要素のマッチングは字句か名前空間解決か。`<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011">` のような接頭辞付き実 MPD は合法であり、字句マッチでは解析に失敗する。

## Considered Options

* 未知ノードも完全な名前空間認識（(URI, ローカル名) で保存、シリアライズ時に接頭辞再生成・宣言再配置）
* 未知ノードは字句のまま保存（qualified name + `xmlns:*` 宣言を未知属性として保持）、解決済み URI を読み取り専用で併記

## Decision Outcome

Chosen option: 「字句保存 + 解決済み URI 併記」。

* 既知要素は quick-xml の `NsReader` で (名前空間 URI, ローカル名) によりマッチする（正しさの要件、選択の余地なし)
* 未知 Element の名前は「書かれたままの qualified name」と「解析時に解決された URI（`Option<String>`）」の両方を持つ。書き戻しは字句側を使い、`xmlns:*` 宣言が受け皿に保持されるため整合する
* 接頭辞割当・宣言再配置の機構は schema クレートの本質ではなく v1 の複雑性に見合わない

### Consequences

* Good, because roundtrip が自明に成立し、機構が単純
* Good, because 利用者は解決済み URI で DRM スキーム判定等ができる
* Bad, because 未知部分の意味論的等価は字句ベース — 接頭辞が違うだけの文書は不等価扱い
* Bad, because 構築側で未知ノードを足す利用者は接頭辞と `xmlns` 宣言の整合に自分で責任を持つ
