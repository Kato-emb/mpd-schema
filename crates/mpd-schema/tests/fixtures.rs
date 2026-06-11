//! `DASHSchema` サンプルと DASH-IF テストベクタに対する検証ハーネス。
//!
//! fixtures は再配布不可のため git 管理外で、`scripts/fetch-fixtures.sh` が
//! 取得する（ADR-0004）。未取得の場合、各テストはスクリプトの実行を促して
//! fail する。

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "テストでは失敗箇所を即座に特定するため unwrap / panic を許容する"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mpd_schema::Mpd;

fn fixture_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    assert!(
        dir.is_dir(),
        "fixtures が見つからない（{}）: `./scripts/fetch-fixtures.sh` を実行してから再実行する",
        dir.display()
    );
    dir
}

fn file_name(path: &Path) -> &str {
    path.file_name().unwrap().to_str().unwrap()
}

/// fixture ファイルを読む。`scripts/fetch-fixtures.sh` が dashif / w3c を
/// 空ディレクトリだけ作って download に失敗した場合、`fixture_dir` の
/// `is_dir()` は通ってしまうので、ファイル単位の読みでも取得手順を促す。
fn read_fixture(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| {
        panic!(
            "fixture が読めない（{}）: {error}\n`./scripts/fetch-fixtures.sh` を実行してから再実行する",
            path.display()
        )
    })
}

/// patch 系（`example_G21_patch*`、v1 非スコープ）を除く `DASHSchema` の
/// 全サンプル MPD。
fn dashschema_samples() -> Vec<PathBuf> {
    let mut samples: Vec<PathBuf> = fs::read_dir(fixture_dir("dashschema"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "mpd"))
        .filter(|path| !file_name(path).starts_with("example_G21_patch"))
        .collect();
    samples.sort();
    // 5th-Ed tarball は sha256 固定なので件数は既知（G 系 22 + H 系 3 + I 系 4）。
    // 展開不全やフィルタの書き損じで対象が静かに減るのを防ぐ。
    assert_eq!(
        samples.len(),
        29,
        "DASHSchema サンプル数が想定と違う: {samples:?}"
    );
    samples
}

/// `parse → serialize → parse` の意味論的等価を確認し、serialize 出力を返す。
/// 呼び出し側はこの出力をそのままマーカー数え等に使える（再 serialize して
/// 別文字列を検査する二度手間と、serialize 決定性への暗黙の依存を避ける）。
fn roundtrip(bytes: &[u8]) -> Result<String, String> {
    let first = Mpd::from_slice(bytes).map_err(|error| format!("parse failed: {error}"))?;
    let output = first.to_string();
    let second =
        Mpd::from_str(&output).map_err(|error| format!("reparse failed: {error}\n{output}"))?;
    if first == second {
        Ok(output)
    } else {
        Err("意味論的等価でない: 再解析結果が初回解析と一致しない".to_string())
    }
}

/// `DASHSchema` 全サンプルで `parse → serialize → parse` の意味論的等価
/// （定義は CONTEXT.md）を確認する。
#[test]
fn dashschema_samples_roundtrip() {
    let mut failures = Vec::new();
    for path in dashschema_samples() {
        let bytes = read_fixture(&path);
        if let Err(message) = roundtrip(&bytes) {
            failures.push(format!("{}: {message}", file_name(&path)));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// DRM 付き実 MPD で未知ノード受け皿の生存を確認する。`cenc:pssh` /
/// `mspr:pro` はスキーマ定義外の要素なので、受け皿が落とせば serialize
/// 出力から消失する。
#[test]
fn drm_test_vectors_preserve_unknown_nodes() {
    let dashif = fixture_dir("dashif");
    for (file, marker, has_bom) in [
        ("axinom-v7-multidrm-singlekey.mpd", "<cenc:pssh>", true),
        ("playready-cbcs-bbb-1080p.mpd", "<mspr:pro>", false),
    ] {
        let bytes = read_fixture(&dashif.join(file));
        if has_bom {
            assert!(
                bytes.starts_with(b"\xEF\xBB\xBF"),
                "{file}: 先頭の UTF-8 BOM が無い（ベクタが想定と違う。BOM 処理のカバレッジが失われる）"
            );
        }
        let input = std::str::from_utf8(&bytes).unwrap();
        let input_count = input.matches(marker).count();
        assert!(
            input_count > 0,
            "{file}: 入力に {marker} が無い（ベクタが想定と違う）"
        );

        let output = roundtrip(&bytes).unwrap_or_else(|message| panic!("{file}: {message}"));
        assert_eq!(
            output.matches(marker).count(),
            input_count,
            "{file}: {marker} が serialize 出力で増減した"
        );
    }
}

/// serialize 出力が DASH-MPD.xsd に適合することを xmllint で確認する。
/// 手書きモデルが XSD から乖離する事故への安全網（ADR-0006）。
#[test]
#[ignore = "xmllint（libxml2-utils）が必要。CI が --include-ignored で実行する"]
fn serialized_dashschema_samples_validate_against_xsd() {
    let schema = fixture_dir("dashschema").join("DASH-MPD.xsd");
    let mut failures = Vec::new();
    for path in dashschema_samples() {
        let mpd = Mpd::from_slice(&read_fixture(&path)).unwrap();
        if let Err(message) =
            xmllint_validate(&schema, file_name(&path), mpd.to_string().as_bytes())
        {
            failures.push(format!("{}: {message}", file_name(&path)));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn xmllint_validate(schema: &Path, name: &str, document: &[u8]) -> Result<(), String> {
    // DASH-MPD.xsd は W3C の xlink.xsd / xml.xsd を URL で import する。
    // fetch 済みのローカル複製を XML カタログで引き当て、--nonet で
    // ネットワーク非依存（w3.org のスロットリングと無縁）にする。
    let catalog = fixture_dir("w3c").join("catalog.xml");
    assert!(
        catalog.is_file(),
        "XML カタログが無い（{}）: `./scripts/fetch-fixtures.sh` を実行してから再実行する",
        catalog.display()
    );
    // ドキュメントは一時ファイル経由で渡す。stdin パイプだと、xmllint が
    // 読み取り前に（スキーマ/カタログ読込失敗等で）終了した際に write が
    // Broken pipe で panic し、原因を示す stderr を取り逃す。さらに大きな
    // 入力かつ大量のエラー出力では stdin/stderr 相互ブロックの恐れもある。
    let document_path = std::env::temp_dir().join(format!("mpd-schema-xsd-{name}"));
    fs::write(&document_path, document).unwrap_or_else(|error| {
        panic!(
            "一時ファイルに書けない（{}）: {error}",
            document_path.display()
        )
    });
    let result = Command::new("xmllint")
        .env("XML_CATALOG_FILES", &catalog)
        .arg("--nonet")
        .arg("--noout")
        .arg("--schema")
        .arg(schema)
        .arg(&document_path)
        .output();
    let _ = fs::remove_file(&document_path);
    let output = match result {
        Ok(output) => output,
        Err(error) => panic!("xmllint を起動できない（libxml2-utils を導入する）: {error}"),
    };
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}
