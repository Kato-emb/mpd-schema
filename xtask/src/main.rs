//! Development automation for the workspace.
//!
//! Run via `cargo xtask <command>`. The only command is `fetch-fixtures`,
//! which downloads the non-redistributable DASHSchema corpus, the DRM test
//! vectors, and the W3C schemas the XSD validation needs (ADR-0004), each
//! pinned by sha256. It replaces the former `scripts/fetch-fixtures.sh` so the
//! step runs on Windows without bash, curl, or sha256sum.

use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

// Pinned to the 5th edition final tag `5th-Ed` (ISO/IEC 23009-1:2022).
const TARBALL_URL: &str = "https://github.com/MPEGGroup/DASHSchema/archive/refs/tags/5th-Ed.tar.gz";
const TARBALL_SHA256: &str = "bf7868764bbd303d96e69a08fe01ad2a9434a66ebc3a483ff8b19bc9a46e3f85";

// Per-file pins (url, sha256, path relative to `fixtures/`).
const PINNED_FILES: &[(&str, &str, &str)] = &[
    (
        "https://media.axprod.net/TestVectors/v7-MultiDRM-SingleKey/Manifest_1080p.mpd",
        "3905d8fcd37fa79adc34df2e8ff7c2471e3999c948914b8b8a0548ef095f055f",
        "dashif/axinom-v7-multidrm-singlekey.mpd",
    ),
    (
        "https://test.playready.microsoft.com/media/dash/APPLEENC_CBCS_BBB_1080p/1080p.mpd",
        "4204b49bd96a5453a504e605fd154e582c7d9247850de046bacdb125bc84ba81",
        "dashif/playready-cbcs-bbb-1080p.mpd",
    ),
    (
        "https://www.w3.org/XML/2008/06/xlink.xsd",
        "c83df86c7fdc16eb9c862b83dfb53fc1b1a4bcafd6e1d1217199e0188b82f24a",
        "w3c/xlink.xsd",
    ),
    (
        "https://www.w3.org/2001/xml.xsd",
        "61960fb3131e38022caad5360e2f33a3382578ab3c80cd58bd74320ede61b20c",
        "w3c/xml.xsd",
    ),
];

const CATALOG: &str = r#"<?xml version="1.0"?>
<catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
  <uri name="http://www.w3.org/XML/2008/06/xlink.xsd" uri="xlink.xsd"/>
  <uri name="http://www.w3.org/2001/xml.xsd" uri="xml.xsd"/>
</catalog>
"#;

fn main() {
    let command = std::env::args().nth(1);
    let result = match command.as_deref() {
        Some("fetch-fixtures") => fetch_fixtures(),
        _ => {
            eprintln!("usage: cargo xtask fetch-fixtures");
            std::process::exit(2);
        }
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn fetch_fixtures() -> Result<(), Box<dyn Error>> {
    let fixtures = repository_root().join("fixtures");
    fs::create_dir_all(fixtures.join("private"))?;

    fetch_dashschema(&fixtures)?;

    for (url, sha256, relative_path) in PINNED_FILES {
        let target = fixtures.join(relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fetch_pinned(url, sha256, &target)?;
    }

    fs::write(fixtures.join("w3c/catalog.xml"), CATALOG)?;
    Ok(())
}

/// Fetches and extracts the DASHSchema tarball, skipping when already present.
fn fetch_dashschema(fixtures: &Path) -> Result<(), Box<dyn Error>> {
    let destination = fixtures.join("dashschema");
    if destination.join("DASH-MPD.xsd").is_file() {
        println!(
            "fixtures already present: {} (delete the directory to re-fetch)",
            destination.display()
        );
        return Ok(());
    }

    println!("downloading {TARBALL_URL}");
    let tarball = fetch_bytes(TARBALL_URL)?;
    verify_sha256(&tarball, TARBALL_SHA256, TARBALL_URL)?;

    // Extract into a staging directory on the same filesystem as the
    // destination, then rename: a partially extracted tree never replaces a
    // good one, and the rename is atomic (Unix and Windows alike).
    let staging = destination.with_file_name(".dashschema.staging");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    extract_strip_one(&tarball, &staging)?;

    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(&staging, &destination)?;
    println!("fetched DASHSchema 5th-Ed into {}", destination.display());
    Ok(())
}

/// Unpacks a gzipped tar, dropping the leading path component of each entry
/// (the archive's top-level directory), like `tar --strip-components=1`.
fn extract_strip_one(tarball: &[u8], staging: &Path) -> Result<(), Box<dyn Error>> {
    let mut archive = Archive::new(GzDecoder::new(tarball));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let stripped: PathBuf = path.components().skip(1).collect();
        if stripped.as_os_str().is_empty() {
            continue;
        }
        entry.unpack(staging.join(stripped))?;
    }
    Ok(())
}

/// Downloads `url` to `target` when absent or sha256-mismatched.
fn fetch_pinned(url: &str, sha256: &str, target: &Path) -> Result<(), Box<dyn Error>> {
    if target.is_file() && sha256_hex(&fs::read(target)?) == sha256 {
        println!("already present: {}", target.display());
        return Ok(());
    }

    println!("downloading {url}");
    let bytes = fetch_bytes(url)?;
    verify_sha256(&bytes, sha256, url)?;

    // Write to a sibling staging file, then rename, so a failed download never
    // leaves a half-written target behind.
    let staging = target.with_extension("download");
    fs::write(&staging, &bytes)?;
    fs::rename(&staging, target)?;
    Ok(())
}

fn fetch_bytes(url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut last_error: Option<Box<dyn Error>> = None;
    for attempt in 1..=3 {
        match try_fetch(url) {
            Ok(bytes) => return Ok(bytes),
            Err(error) => {
                eprintln!("attempt {attempt}/3 failed: {error}");
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| format!("could not download {url}").into()))
}

fn try_fetch(url: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let response = ureq::get(url).call()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn verify_sha256(bytes: &[u8], expected: &str, url: &str) -> Result<(), Box<dyn Error>> {
    let actual = sha256_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(format!("sha256 mismatch for {url}: expected {expected}, got {actual}").into())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The workspace root, derived from the xtask crate directory that cargo
/// exports at runtime.
fn repository_root() -> PathBuf {
    match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(manifest_dir) => Path::new(&manifest_dir)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}
