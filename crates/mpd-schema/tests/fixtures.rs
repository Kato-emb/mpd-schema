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
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

fn roundtrip(bytes: &[u8]) -> Result<Mpd, String> {
    let first = Mpd::from_slice(bytes).map_err(|error| format!("parse failed: {error}"))?;
    let output = first.to_string();
    let second =
        Mpd::from_str(&output).map_err(|error| format!("reparse failed: {error}\n{output}"))?;
    if first == second {
        Ok(first)
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
        let bytes = fs::read(&path).unwrap();
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
    for (file, marker) in [
        ("axinom-v7-multidrm-singlekey.mpd", "<cenc:pssh>"),
        ("playready-cbcs-bbb-1080p.mpd", "<mspr:pro>"),
    ] {
        let bytes = fs::read(dashif.join(file)).unwrap();
        let input = String::from_utf8(bytes.clone()).unwrap();
        let input_count = input.matches(marker).count();
        assert!(
            input_count > 0,
            "{file}: 入力に {marker} が無い（ベクタが想定と違う）"
        );

        let mpd = roundtrip(&bytes).unwrap_or_else(|message| panic!("{file}: {message}"));
        let output = mpd.to_string();
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
        let mpd = Mpd::from_slice(&fs::read(&path).unwrap()).unwrap();
        if let Err(message) = xmllint_validate(&schema, mpd.to_string().as_bytes()) {
            failures.push(format!("{}: {message}", file_name(&path)));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn xmllint_validate(schema: &Path, document: &[u8]) -> Result<(), String> {
    // DASH-MPD.xsd は W3C の xlink.xsd / xml.xsd を URL で import する。
    // fetch 済みのローカル複製を XML カタログで引き当て、--nonet で
    // ネットワーク非依存（w3.org のスロットリングと無縁）にする。
    let catalog = fixture_dir("w3c").join("catalog.xml");
    let spawned = Command::new("xmllint")
        .env("XML_CATALOG_FILES", &catalog)
        .arg("--nonet")
        .arg("--noout")
        .arg("--schema")
        .arg(schema)
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => panic!("xmllint を起動できない（libxml2-utils を導入する）: {error}"),
    };
    child.stdin.take().unwrap().write_all(document).unwrap();
    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}
