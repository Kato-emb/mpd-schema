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
    /// [`Error::path`] identifies where in the document the failure occurred.
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
    /// # Errors
    ///
    /// Returns [`ErrorKind::Io`] if writing to `writer` fails.
    pub fn write_to<W: io::Write>(&self, writer: W) -> Result<()> {
        ser::write_mpd(self, writer)?;
        Ok(())
    }
}

/// Serializes the document as UTF-8 XML, making `to_string` available.
impl fmt::Display for Mpd {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Vec への書き込みは Io エラーを起こさず、ser が生成するイベント列は
        // 常に整合するため、このエラー写像は実際には到達しない。
        let bytes = ser::write_mpd(self, Vec::new()).map_err(|_| fmt::Error)?;
        let xml = String::from_utf8(bytes).map_err(|_| fmt::Error)?;
        formatter.write_str(&xml)
    }
}
