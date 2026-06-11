# Decisions

- [0001](./0001-schema-crate-excludes-resolution.md) — **mpd-schema は解決を含めない** (accepted): セグメント URL 導出・タイムライン展開は将来のワークスペース内姉妹クレートへ分離。リポジトリは最初から virtual workspace。
- [0002](./0002-non-exhaustive-structs-with-new.md) — **`#[non_exhaustive]` + `new(必須属性)` + `pub` フィールド** (accepted): リテラル構築を捨て、Optional フィールド追加を minor に収める構築規約。
- [0003](./0003-lexical-namespace-handling-for-unknown-nodes.md) — **未知ノードは字句保存 + 解決済み URI 併記** (accepted): 既知要素は NsReader で名前空間解決マッチ、未知ノードの接頭辞正規化はしない。
- [0004](./0004-fetch-fixtures-by-script-not-bundled.md) — **fixtures は同梱せず fetch スクリプトで取得、`5th-Ed` にピン** (accepted): DASHSchema は ISO カスタムライセンスで再配布不可。sha256 固定の tarball 取得、6th edition は出版後に乗り換え。
- [0005](./0005-no-serde.md) — **serde 不採用** (accepted): `xs:extension` 継承と未知ノード保持が serde のデータモデルと合わず、自前のイベントベース変換を手書きする。
- [0006](./0006-handwritten-model-no-xsd-codegen.md) — **XSD 自動生成せず手書きモデル1層** (accepted): 60〜80 型は一度きりの投資、乖離事故は CI の XSD バリデーションが安全網。
- [0007](./0007-quick-xml-behind-internal-seam.md) — **quick-xml は内部の継ぎ目に隠す** (accepted): 自前 Event enum は `pub(crate)`、公開 API は構造体レベル（読み `&[u8]` / 書き `io::Write`）のみ。
- [0008](./0008-fixed-width-value-space-rejects-unrepresentable.md) — **属性値は固定幅の値空間に写像し、表現不能な値は拒否** (accepted): 黙った精度損失は意味論的等価と両立しないため fail-fast で拒否。小数秒の無損失な末尾ゼロのみ例外的に受理。
- [0009](./0009-resolve-v1-scope-static-segment-enumeration.md) — **mpd-resolve v1 は static MPD のセグメント列挙に限定** (accepted): BaseURL 解決・テンプレート展開・タイムライン展開・セグメント列挙の4機能。dynamic はエラーで拒否し、now 引数の別エントリポイントとして将来 additive に追加。
- [0010](./0010-resolve-independent-error-fail-fast-at-construction.md) — **mpd-resolve は独立 Error 型、列挙の構築時に fail-fast** (accepted): `mpd_schema::Error` と共有しない。検証は構築時に前払いし、イテレーションは無謬。
- [0011](./0011-resolve-public-api-indices-iterator-string-url.md) — **公開 API はインデックス指定 + 検証済み列挙 + `String` URL** (accepted): `&Mpd` + (period, adaptation_set, representation) 指定、複数 BaseURL は先頭採用、相対参照のままの出力を許す。
- [0012](./0012-resolve-deps-handwritten-rfc3986.md) — **mpd-resolve の依存は mpd-schema のみ、RFC 3986 §5 を自前実装** (accepted): url crate は WHATWG 実装かつ相対基底を扱えないため不採用。§5.4 テストベクタで準拠を担保。
