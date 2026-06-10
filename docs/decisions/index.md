# Decisions

- [0001](./0001-schema-crate-excludes-resolution.md) — **mpd-schema は解決を含めない** (accepted): セグメント URL 導出・タイムライン展開は将来のワークスペース内姉妹クレートへ分離。リポジトリは最初から virtual workspace。
- [0002](./0002-non-exhaustive-structs-with-new.md) — **`#[non_exhaustive]` + `new(必須属性)` + `pub` フィールド** (accepted): リテラル構築を捨て、Optional フィールド追加を minor に収める構築規約。
- [0003](./0003-lexical-namespace-handling-for-unknown-nodes.md) — **未知ノードは字句保存 + 解決済み URI 併記** (accepted): 既知要素は NsReader で名前空間解決マッチ、未知ノードの接頭辞正規化はしない。
- [0004](./0004-fetch-fixtures-by-script-not-bundled.md) — **fixtures は同梱せず fetch スクリプトで取得、`5th-Ed-Final` にピン** (accepted): DASHSchema は ISO カスタムライセンスで再配布不可。sha256 固定の tarball 取得、6th edition は出版後に乗り換え。
- [0005](./0005-no-serde.md) — **serde 不採用** (accepted): `xs:extension` 継承と未知ノード保持が serde のデータモデルと合わず、自前のイベントベース変換を手書きする。
- [0006](./0006-handwritten-model-no-xsd-codegen.md) — **XSD 自動生成せず手書きモデル1層** (accepted): 60〜80 型は一度きりの投資、乖離事故は CI の XSD バリデーションが安全網。
- [0007](./0007-quick-xml-behind-internal-seam.md) — **quick-xml は内部の継ぎ目に隠す** (accepted): 自前 Event enum は `pub(crate)`、公開 API は構造体レベル（読み `&[u8]` / 書き `io::Write`）のみ。
