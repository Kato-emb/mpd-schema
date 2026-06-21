//! Bidirectional conversion between MPEG-DASH MPD documents and Rust structs.
//!
//! This crate parses MPD (Media Presentation Description) documents as
//! defined by ISO/IEC 23009-1 into typed structs and serializes them back to
//! XML. It deliberately stops at the document level: resolution (segment URL
//! derivation, timeline expansion) and transport (HTTP fetching, `Location`
//! following) are out of scope.
//!
//! Unknown elements and attributes — vendor extensions, DRM payloads such as
//! `cenc:pssh` — are preserved verbatim in each struct's `unknown_attributes`
//! / `unknown_children` fields and written back on serialization.
//!
//! # Parsing and serializing
//!
//! ```
//! use mpd_schema::Mpd;
//!
//! let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
//!     profiles="urn:mpeg:dash:profile:isoff-on-demand:2011"
//!     minBufferTime="PT2S"><Period/></MPD>"#;
//!
//! let mut mpd = Mpd::from_str(xml)?;
//! assert_eq!(mpd.periods.len(), 1);
//!
//! mpd.id = Some("manifest".to_string());
//! let output = mpd.to_string();
//! assert!(output.contains(r#"id="manifest""#));
//! # Ok::<(), mpd_schema::Error>(())
//! ```
//!
//! # Building a document from scratch
//!
//! Required attributes are taken by each struct's `new`; everything else
//! starts empty and is set through public fields.
//!
//! ```
//! use mpd_schema::{Mpd, Period};
//!
//! let mut mpd = Mpd::new(
//!     "urn:mpeg:dash:profile:isoff-on-demand:2011",
//!     "PT2S".parse()?,
//! );
//! mpd.periods.push(Period::new());
//! let xml = mpd.to_string();
//! assert!(xml.starts_with(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011""#));
//! # Ok::<(), mpd_schema::Error>(())
//! ```

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::panic,
        clippy::arithmetic_side_effects,
        reason = "テストでは失敗箇所を即座に特定するため unwrap / panic を許容し、期待値計算の素朴な算術を許容する"
    )
)]

mod backend;
mod de;
pub mod error;
mod event;
pub mod model;
mod ser;

use std::fmt;
use std::io;

pub use error::{Error, ErrorKind, Result};
pub use model::descriptor::{ContentProtection, Descriptor};
pub use model::element::{Element, Node};
pub use model::mpd::{
    AdaptationSet, BaseUrl, ContentType, InitializationSet, LeapSecondInformation, MPD_NAMESPACE,
    Metrics, Mpd, PatchLocation, Period, PresentationType, ProgramInformation, Range,
    Representation, RepresentationBase, VideoScan,
};
pub use model::period_representation::{
    ContentComponent, ContentEncoding, ContentPopularityRate, ContentPopularitySource, Event,
    EventStream, ExtendedBandwidth, Label, ModelPair, PopularityRate, Preselection,
    PreselectionOrder, ProducerReferenceTime, ProducerReferenceTimeType, RandomAccess,
    RandomAccessType, Resync, SubRepresentation, Subset, Switching, SwitchingType,
};
pub use model::segment::{
    FailoverContent, Fcs, MultipleSegmentBase, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, SegmentUrl, Url,
};
pub use model::service_description::{
    Latency, OperatingBandwidth, OperatingBandwidthMediaType, OperatingQuality,
    OperatingQualityMediaType, PlaybackRate, ServiceDescription, UIntPairsWithId, UIntVWithId,
};
pub use model::types::{
    AudioSamplingRate, ConditionalUint, FrameRate, Ratio, Sap, XsDateTime, XsDuration,
};

impl Mpd {
    /// Parses an MPD document from a byte slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the input is not well-formed UTF-8 XML, the root
    /// element is not `MPD` in the DASH namespace, a required attribute is
    /// missing, or an attribute value does not conform to its lexical form.
    /// For attribute and element-structure errors, [`Error::path`] identifies
    /// where in the document the failure occurred; other classes (malformed
    /// XML, encoding) leave it empty.
    pub fn from_slice(bytes: &[u8]) -> Result<Mpd> {
        de::mpd_from_slice(bytes)
    }

    /// Parses an MPD document from a string slice.
    ///
    /// # Errors
    ///
    /// Same as [`Mpd::from_slice`].
    #[allow(
        clippy::should_implement_trait,
        reason = "ARCHITECTURE.md が規定する公開 API。from_slice / from_reader と対称な \
                  固有メソッドとして、トレイト import なしで呼べる形を保つ"
    )]
    pub fn from_str(input: &str) -> Result<Mpd> {
        Self::from_slice(input.as_bytes())
    }

    /// Reads a reader to its end and parses the contents as an MPD document.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::Io`] if reading fails; otherwise same as
    /// [`Mpd::from_slice`].
    pub fn from_reader<R: io::Read>(mut reader: R) -> Result<Mpd> {
        let mut buffer = Vec::new();
        reader
            .read_to_end(&mut buffer)
            .map_err(|source| Error::new(ErrorKind::Io(source)))?;
        Self::from_slice(&buffer)
    }

    /// Serializes the document as UTF-8 XML into a writer.
    ///
    /// The output is written without internal buffering: wrap raw `File` or
    /// socket sinks in [`io::BufWriter`] to avoid one small write per XML
    /// event.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::Io`] if writing to `writer` fails,
    /// [`ErrorKind::InvalidValue`] if an unknown attribute or element name
    /// (set by hand through an `unknown_attributes` / `unknown_children`
    /// field) is not a well-formed XML name, and [`ErrorKind::Xml`] if
    /// unknown elements are nested deeper than the shared depth limit.
    pub fn write_to<W: io::Write>(&self, writer: W) -> Result<()> {
        ser::write_mpd(self, writer)?;
        Ok(())
    }

    /// Serializes the document as indented, human-readable UTF-8 XML.
    ///
    /// This is a debugging aid: elements are placed on their own lines and
    /// indented two spaces per level. Unlike [`Mpd::write_to`], the output is
    /// not guaranteed to round-trip to an equivalent document — the inserted
    /// whitespace becomes text content of elements that hold none of their
    /// own. Use [`Mpd::write_to`] when the output will be parsed again.
    ///
    /// # Errors
    ///
    /// Same as [`Mpd::write_to`].
    pub fn write_to_pretty<W: io::Write>(&self, writer: W) -> Result<()> {
        ser::write_mpd_indented(self, writer, PRETTY_INDENT_SPACES)?;
        Ok(())
    }

    /// Serializes the document as an indented, human-readable XML string.
    ///
    /// The string counterpart of [`Mpd::write_to_pretty`]; the same
    /// round-trip caveat applies.
    ///
    /// # Errors
    ///
    /// The serialization errors of [`Mpd::write_to`] (other than
    /// [`ErrorKind::Io`], which cannot arise when writing to a `String`), plus
    /// [`ErrorKind::Encoding`] in the unreachable case that the serialized
    /// bytes are not UTF-8.
    pub fn to_string_pretty(&self) -> Result<String> {
        let bytes = ser::write_mpd_indented(self, Vec::new(), PRETTY_INDENT_SPACES)?;
        String::from_utf8(bytes).map_err(|source| {
            Error::new(ErrorKind::Encoding(format!(
                "serialized output is not UTF-8: {source}"
            )))
        })
    }
}

/// Spaces per nesting level used by the pretty serializers.
const PRETTY_INDENT_SPACES: usize = 2;

/// Serializes the document as UTF-8 XML, making `to_string` available.
///
/// Serialization into a `String` can fail only when the document contains
/// hand-built unknown nodes whose names are not well-formed XML names or
/// whose nesting exceeds the shared depth limit. `fmt::Error` is returned
/// in those cases, which makes `to_string` / `format!` panic (standard
/// library behavior); call [`Mpd::write_to`] to obtain the underlying
/// [`Error`] instead.
impl fmt::Display for Mpd {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Vec への書き込みで Io エラーは起き得ないため、ここで fmt::Error に
        // 潰れるのは未知ノードの検証エラー（名前・深度）のみ（impl の doc に
        // 明記）。
        let bytes = ser::write_mpd(self, Vec::new()).map_err(|_| fmt::Error)?;
        let xml = String::from_utf8(bytes).map_err(|_| fmt::Error)?;
        formatter.write_str(&xml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = concat!(
        r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
        r#"profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" minBufferTime="PT2S">"#,
        "<Period/>",
        "</MPD>",
    );

    #[test]
    fn from_reader_write_to_roundtrip() {
        let mpd = Mpd::from_reader(MINIMAL.as_bytes()).unwrap();
        let mut output = Vec::new();
        mpd.write_to(&mut output).unwrap();
        let reparsed = Mpd::from_slice(&output).unwrap();
        assert_eq!(mpd, reparsed);
    }

    /// `Display`（→ `to_string`）が `write_to` と同じ serializer を通り、
    /// 検証可能な文書では失敗しないことを固定する。将来 ser にデータ依存の
    /// エラー経路が増えて両者が乖離した場合、ここで検出する。
    #[test]
    fn to_string_matches_write_to_output() {
        let mpd = Mpd::from_str(MINIMAL).unwrap();
        let mut output = Vec::new();
        mpd.write_to(&mut output).unwrap();
        assert_eq!(mpd.to_string().as_bytes(), output.as_slice());
    }

    /// pretty serializer がネストにインデントを与えつつ、葉要素のテキストを
    /// 無改変で1行に保つ（空白注入で text を壊さない）ことを固定する。
    #[test]
    fn pretty_indents_and_preserves_leaf_text() {
        const XML: &str = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
            r#"profiles="p" minBufferTime="PT2S">"#,
            "<BaseURL>https://cdn.example.com/base/</BaseURL>",
            "<Period/>",
            "</MPD>",
        );
        let mpd = Mpd::from_str(XML).unwrap();
        let pretty = mpd.to_string_pretty().unwrap();

        assert!(pretty.contains("\n  <BaseURL>"));
        assert!(pretty.contains("<BaseURL>https://cdn.example.com/base/</BaseURL>"));

        let mut bytes = Vec::new();
        mpd.write_to_pretty(&mut bytes).unwrap();
        assert_eq!(pretty.as_bytes(), bytes.as_slice());
    }

    /// crate ルートの再エクスポート一覧が model.rs の一覧と同期している
    /// ことを、両ファイルの `pub use` 文の突き合わせで検証する。model.rs
    /// にだけ型を足すと `mpd_schema::X` が欠けたままコンパイルが通るため、
    /// ドリフトはテストでしか検出できない。
    #[test]
    fn crate_root_reexports_stay_in_sync_with_model() {
        let root = pub_use_entries(include_str!("lib.rs"), "model::");
        let model = pub_use_entries(include_str!("model.rs"), "");
        assert!(!root.is_empty());
        assert_eq!(root, model);
    }

    #[test]
    #[should_panic(expected = "brace なしの pub use")]
    fn braceless_pub_use_is_detected_as_drift_risk() {
        pub_use_entries("pub use segment::Foo;", "");
    }

    /// `pub use <prefix><module>::{Name, ...};` 文から (module, Name) の
    /// 組を列挙する。prefix に一致しない文（`error::` 等）は無視する。
    fn pub_use_entries(source: &str, prefix: &str) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        for part in source.split("pub use ").skip(1) {
            let statement = part
                .split(';')
                .next()
                .unwrap()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let Some(statement) = statement.strip_prefix(prefix) else {
                continue;
            };
            // brace なしの `pub use foo::Bar;` は下の解析から漏れ、片側の
            // 一覧にだけ存在してもドリフトとして検出されないため、ここで
            // 落とす。
            assert!(
                !statement.contains("::") || statement.contains("::{"),
                "brace なしの pub use を検出: `pub use {statement};` — brace 形式に統一する"
            );
            let Some((module, names)) = statement.split_once("::{") else {
                continue;
            };
            let Some(names) = names.strip_suffix('}') else {
                continue;
            };
            for name in names.split(',') {
                let name = name.trim();
                if !name.is_empty() {
                    entries.push((module.to_string(), name.to_string()));
                }
            }
        }
        entries.sort();
        entries
    }
}
