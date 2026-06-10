//! Document types along the `MPD` → `Period` → `AdaptationSet` →
//! `Representation` spine of `DASH-MPD.xsd`.
//!
//! Every struct follows the model-layer conventions (ADR-0002,
//! ARCHITECTURE.md): `#[non_exhaustive]` with `new(required attributes...)`
//! and `pub` fields, `xs:extension` represented by an embedded base struct,
//! and catch-all fields that preserve unknown content. Elements of the MPD
//! namespace that have no typed field yet are also preserved through the
//! catch-all; they migrate to typed fields as coverage grows.

use std::fmt;
use std::str::FromStr;

use crate::error::Error;
use crate::model::descriptor::{ContentProtection, Descriptor};
use crate::model::element::Element;
use crate::model::segment::{SegmentBase, SegmentList, SegmentTemplate};
use crate::model::service_description::{ServiceDescription, UIntVWithId};
use crate::model::types::{
    AudioSamplingRate, FrameRate, Ratio, Sap, XsDateTime, XsDuration, invalid_value,
};

/// The XML namespace that `DASH-MPD.xsd` assigns to MPD documents.
pub const MPD_NAMESPACE: &str = "urn:mpeg:dash:schema:mpd:2011";

/// The `MPD` root element (XSD `MPDtype`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Mpd {
    /// The `id` attribute.
    pub id: Option<String>,
    /// The required `profiles` attribute, kept as the comma-separated list
    /// written in the document.
    pub profiles: String,
    /// The `type` attribute.
    pub presentation_type: Option<PresentationType>,
    /// The `availabilityStartTime` attribute.
    pub availability_start_time: Option<XsDateTime>,
    /// The `availabilityEndTime` attribute.
    pub availability_end_time: Option<XsDateTime>,
    /// The `publishTime` attribute.
    pub publish_time: Option<XsDateTime>,
    /// The `mediaPresentationDuration` attribute.
    pub media_presentation_duration: Option<XsDuration>,
    /// The `minimumUpdatePeriod` attribute.
    pub minimum_update_period: Option<XsDuration>,
    /// The required `minBufferTime` attribute.
    pub min_buffer_time: XsDuration,
    /// The `timeShiftBufferDepth` attribute.
    pub time_shift_buffer_depth: Option<XsDuration>,
    /// The `suggestedPresentationDelay` attribute.
    pub suggested_presentation_delay: Option<XsDuration>,
    /// The `maxSegmentDuration` attribute.
    pub max_segment_duration: Option<XsDuration>,
    /// The `maxSubsegmentDuration` attribute.
    pub max_subsegment_duration: Option<XsDuration>,
    /// The `ProgramInformation` children.
    pub program_informations: Vec<ProgramInformation>,
    /// The `BaseURL` children.
    pub base_urls: Vec<BaseUrl>,
    /// The `Location` children (plain strings).
    pub locations: Vec<String>,
    /// The `PatchLocation` children.
    pub patch_locations: Vec<PatchLocation>,
    /// The `ServiceDescription` children.
    pub service_descriptions: Vec<ServiceDescription>,
    /// The `InitializationSet` children.
    pub initialization_sets: Vec<InitializationSet>,
    /// The `InitializationGroup` children.
    pub initialization_groups: Vec<UIntVWithId>,
    /// The `InitializationPresentation` children.
    pub initialization_presentations: Vec<UIntVWithId>,
    /// The `ContentProtection` children.
    pub content_protections: Vec<ContentProtection>,
    /// The `Period` children. The schema requires at least one; occurrence
    /// counts are not enforced by parsing.
    pub periods: Vec<Period>,
    /// The `Metrics` children.
    pub metrics: Vec<Metrics>,
    /// The `EssentialProperty` children.
    pub essential_properties: Vec<Descriptor>,
    /// The `SupplementalProperty` children.
    pub supplemental_properties: Vec<Descriptor>,
    /// The `UTCTiming` children.
    pub utc_timings: Vec<Descriptor>,
    /// The `LeapSecondInformation` child.
    pub leap_second_information: Option<LeapSecondInformation>,
    /// Attributes without a typed field, as written, including `xmlns:*`
    /// declarations. The default `xmlns` declaration is not kept: parsing
    /// drops it and serialization re-adds [`MPD_NAMESPACE`] on the root.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Mpd {
    /// Creates an MPD with the required attributes; every other field starts
    /// empty.
    pub fn new(profiles: impl Into<String>, min_buffer_time: XsDuration) -> Self {
        Self {
            id: None,
            profiles: profiles.into(),
            presentation_type: None,
            availability_start_time: None,
            availability_end_time: None,
            publish_time: None,
            media_presentation_duration: None,
            minimum_update_period: None,
            min_buffer_time,
            time_shift_buffer_depth: None,
            suggested_presentation_delay: None,
            max_segment_duration: None,
            max_subsegment_duration: None,
            program_informations: Vec::new(),
            base_urls: Vec::new(),
            locations: Vec::new(),
            patch_locations: Vec::new(),
            service_descriptions: Vec::new(),
            initialization_sets: Vec::new(),
            initialization_groups: Vec::new(),
            initialization_presentations: Vec::new(),
            content_protections: Vec::new(),
            periods: Vec::new(),
            metrics: Vec::new(),
            essential_properties: Vec::new(),
            supplemental_properties: Vec::new(),
            utc_timings: Vec::new(),
            leap_second_information: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// The `MPD@type` attribute (XSD `PresentationType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PresentationType {
    /// A static presentation (`static`).
    Static,
    /// A dynamic presentation (`dynamic`).
    Dynamic,
}

impl FromStr for PresentationType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "static" => Ok(Self::Static),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err(invalid_value(input, "`static` or `dynamic`")),
        }
    }
}

impl fmt::Display for PresentationType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static => formatter.write_str("static"),
            Self::Dynamic => formatter.write_str("dynamic"),
        }
    }
}

/// A `Period` element (XSD `PeriodType`).
///
/// The `xlink:*` attributes have no typed fields yet and are preserved
/// through [`Period::unknown_attributes`].
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Period {
    /// The `id` attribute.
    pub id: Option<String>,
    /// The `start` attribute.
    pub start: Option<XsDuration>,
    /// The `duration` attribute.
    pub duration: Option<XsDuration>,
    /// The `bitstreamSwitching` attribute.
    pub bitstream_switching: Option<bool>,
    /// The `BaseURL` children.
    pub base_urls: Vec<BaseUrl>,
    /// The `SegmentBase` child.
    pub segment_base: Option<SegmentBase>,
    /// The `SegmentList` child.
    pub segment_list: Option<SegmentList>,
    /// The `SegmentTemplate` child.
    pub segment_template: Option<SegmentTemplate>,
    /// The `AssetIdentifier` child.
    pub asset_identifier: Option<Descriptor>,
    /// The `ServiceDescription` children.
    pub service_descriptions: Vec<ServiceDescription>,
    /// The `ContentProtection` children.
    pub content_protections: Vec<ContentProtection>,
    /// The `AdaptationSet` children.
    pub adaptation_sets: Vec<AdaptationSet>,
    /// The `SupplementalProperty` children.
    pub supplemental_properties: Vec<Descriptor>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Period {
    /// Creates an empty period; the schema requires no attribute.
    pub fn new() -> Self {
        Self {
            id: None,
            start: None,
            duration: None,
            bitstream_switching: None,
            base_urls: Vec::new(),
            segment_base: None,
            segment_list: None,
            segment_template: None,
            asset_identifier: None,
            service_descriptions: Vec::new(),
            content_protections: Vec::new(),
            adaptation_sets: Vec::new(),
            supplemental_properties: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// The attributes and children shared by `AdaptationSet`, `Representation`,
/// and `SubRepresentation` (XSD `RepresentationBaseType`).
///
/// The XSD expresses the sharing as `xs:extension`; the model embeds this
/// struct in the extending types instead (ADR-0002). The catch-all fields of
/// an extending element live here, because the `xs:any` particle and
/// `xs:anyAttribute` are declared on the base type.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct RepresentationBase {
    /// The `profiles` attribute.
    pub profiles: Option<String>,
    /// The `width` attribute.
    pub width: Option<u32>,
    /// The `height` attribute.
    pub height: Option<u32>,
    /// The `sar` attribute.
    pub sar: Option<Ratio>,
    /// The `frameRate` attribute.
    pub frame_rate: Option<FrameRate>,
    /// The `audioSamplingRate` attribute.
    pub audio_sampling_rate: Option<AudioSamplingRate>,
    /// The `mimeType` attribute.
    pub mime_type: Option<String>,
    /// The `segmentProfiles` attribute, split on whitespace. Empty means
    /// absent.
    pub segment_profiles: Vec<String>,
    /// The `codecs` attribute.
    pub codecs: Option<String>,
    /// The `containerProfiles` attribute, split on whitespace. Empty means
    /// absent.
    pub container_profiles: Vec<String>,
    /// The `maximumSAPPeriod` attribute.
    pub maximum_sap_period: Option<f64>,
    /// The `startWithSAP` attribute.
    pub start_with_sap: Option<Sap>,
    /// The `maxPlayoutRate` attribute.
    pub max_playout_rate: Option<f64>,
    /// The `codingDependency` attribute.
    pub coding_dependency: Option<bool>,
    /// The `scanType` attribute.
    pub scan_type: Option<VideoScan>,
    /// The `selectionPriority` attribute.
    pub selection_priority: Option<u32>,
    /// The `tag` attribute.
    pub tag: Option<String>,
    /// The `FramePacking` children.
    pub frame_packings: Vec<Descriptor>,
    /// The `AudioChannelConfiguration` children.
    pub audio_channel_configurations: Vec<Descriptor>,
    /// The `ContentProtection` children.
    pub content_protections: Vec<ContentProtection>,
    /// The `OutputProtection` child.
    pub output_protection: Option<Descriptor>,
    /// The `EssentialProperty` children.
    pub essential_properties: Vec<Descriptor>,
    /// The `SupplementalProperty` children.
    pub supplemental_properties: Vec<Descriptor>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl RepresentationBase {
    /// Creates an empty base; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// The `scanType` attribute (XSD `VideoScanType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VideoScan {
    /// Progressive scan (`progressive`).
    Progressive,
    /// Interlaced scan (`interlaced`).
    Interlaced,
    /// Unknown scan type (`unknown`).
    Unknown,
}

impl FromStr for VideoScan {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "progressive" => Ok(Self::Progressive),
            "interlaced" => Ok(Self::Interlaced),
            "unknown" => Ok(Self::Unknown),
            _ => Err(invalid_value(
                input,
                "`progressive`, `interlaced`, or `unknown`",
            )),
        }
    }
}

impl fmt::Display for VideoScan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Progressive => formatter.write_str("progressive"),
            Self::Interlaced => formatter.write_str("interlaced"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

/// An `AdaptationSet` element (XSD `AdaptationSetType`).
///
/// The `xlink:*` attributes have no typed fields yet and are preserved
/// through the catch-all on [`AdaptationSet::base`].
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct AdaptationSet {
    /// The embedded `RepresentationBaseType` part, which also carries the
    /// catch-all fields for unknown content.
    pub base: RepresentationBase,
    /// The `id` attribute.
    pub id: Option<u32>,
    /// The `group` attribute.
    pub group: Option<u32>,
    /// The `lang` attribute.
    pub lang: Option<String>,
    /// The `contentType` attribute.
    pub content_type: Option<ContentType>,
    /// The `par` attribute.
    pub par: Option<Ratio>,
    /// The `minBandwidth` attribute.
    pub min_bandwidth: Option<u32>,
    /// The `maxBandwidth` attribute.
    pub max_bandwidth: Option<u32>,
    /// The `minWidth` attribute.
    pub min_width: Option<u32>,
    /// The `maxWidth` attribute.
    pub max_width: Option<u32>,
    /// The `minHeight` attribute.
    pub min_height: Option<u32>,
    /// The `maxHeight` attribute.
    pub max_height: Option<u32>,
    /// The `minFrameRate` attribute.
    pub min_frame_rate: Option<FrameRate>,
    /// The `maxFrameRate` attribute.
    pub max_frame_rate: Option<FrameRate>,
    /// The `segmentAlignment` attribute.
    pub segment_alignment: Option<bool>,
    /// The `subsegmentAlignment` attribute.
    pub subsegment_alignment: Option<bool>,
    /// The `subsegmentStartsWithSAP` attribute.
    pub subsegment_starts_with_sap: Option<Sap>,
    /// The `bitstreamSwitching` attribute.
    pub bitstream_switching: Option<bool>,
    /// The `initializationSetRef` attribute, split on whitespace. Empty
    /// means absent.
    pub initialization_set_ref: Vec<u32>,
    /// The `initializationPrincipal` attribute.
    pub initialization_principal: Option<String>,
    /// The `Accessibility` children.
    pub accessibilities: Vec<Descriptor>,
    /// The `Role` children.
    pub roles: Vec<Descriptor>,
    /// The `Rating` children.
    pub ratings: Vec<Descriptor>,
    /// The `Viewpoint` children.
    pub viewpoints: Vec<Descriptor>,
    /// The `BaseURL` children.
    pub base_urls: Vec<BaseUrl>,
    /// The `SegmentBase` child.
    pub segment_base: Option<SegmentBase>,
    /// The `SegmentList` child.
    pub segment_list: Option<SegmentList>,
    /// The `SegmentTemplate` child.
    pub segment_template: Option<SegmentTemplate>,
    /// The `Representation` children.
    pub representations: Vec<Representation>,
}

impl AdaptationSet {
    /// Creates an empty adaptation set; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// The `contentType` attribute (XSD `RFC6838ContentTypeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ContentType {
    /// The `text` top-level media type.
    Text,
    /// The `image` top-level media type.
    Image,
    /// The `audio` top-level media type.
    Audio,
    /// The `video` top-level media type.
    Video,
    /// The `application` top-level media type.
    Application,
    /// The `font` top-level media type.
    Font,
}

impl FromStr for ContentType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "text" => Ok(Self::Text),
            "image" => Ok(Self::Image),
            "audio" => Ok(Self::Audio),
            "video" => Ok(Self::Video),
            "application" => Ok(Self::Application),
            "font" => Ok(Self::Font),
            _ => Err(invalid_value(
                input,
                "an RFC 6838 top-level media type such as `video`",
            )),
        }
    }
}

impl fmt::Display for ContentType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text => formatter.write_str("text"),
            Self::Image => formatter.write_str("image"),
            Self::Audio => formatter.write_str("audio"),
            Self::Video => formatter.write_str("video"),
            Self::Application => formatter.write_str("application"),
            Self::Font => formatter.write_str("font"),
        }
    }
}

/// A `Representation` element (XSD `RepresentationType`).
///
/// ```
/// use mpd_schema::model::Representation;
///
/// let mut representation = Representation::new("video-1080p", 4_800_000);
/// representation.base.width = Some(1920);
/// representation.base.height = Some(1080);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Representation {
    /// The embedded `RepresentationBaseType` part, which also carries the
    /// catch-all fields for unknown content.
    pub base: RepresentationBase,
    /// The required `id` attribute, a string without whitespace.
    pub id: String,
    /// The required `bandwidth` attribute.
    pub bandwidth: u32,
    /// The `qualityRanking` attribute.
    pub quality_ranking: Option<u32>,
    /// The `dependencyId` attribute, split on whitespace. Empty means
    /// absent.
    pub dependency_id: Vec<String>,
    /// The `associationId` attribute, split on whitespace. Empty means
    /// absent.
    pub association_id: Vec<String>,
    /// The `associationType` attribute, split on whitespace. Empty means
    /// absent.
    pub association_type: Vec<String>,
    /// The `mediaStreamStructureId` attribute, split on whitespace. Empty
    /// means absent.
    pub media_stream_structure_id: Vec<String>,
    /// The `BaseURL` children.
    pub base_urls: Vec<BaseUrl>,
    /// The `SegmentBase` child.
    pub segment_base: Option<SegmentBase>,
    /// The `SegmentList` child.
    pub segment_list: Option<SegmentList>,
    /// The `SegmentTemplate` child.
    pub segment_template: Option<SegmentTemplate>,
}

impl Representation {
    /// Creates a representation with the required attributes; every other
    /// field starts empty.
    pub fn new(id: impl Into<String>, bandwidth: u32) -> Self {
        Self {
            base: RepresentationBase::new(),
            id: id.into(),
            bandwidth,
            quality_ranking: None,
            dependency_id: Vec::new(),
            association_id: Vec::new(),
            association_type: Vec::new(),
            media_stream_structure_id: Vec::new(),
            base_urls: Vec::new(),
            segment_base: None,
            segment_list: None,
            segment_template: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_required_attributes_and_leaves_the_rest_empty() {
        let mpd = Mpd::new(
            "urn:mpeg:dash:profile:isoff-on-demand:2011",
            XsDuration::new(),
        );
        assert_eq!(mpd.profiles, "urn:mpeg:dash:profile:isoff-on-demand:2011");
        assert_eq!(mpd.min_buffer_time, XsDuration::new());
        assert_eq!(mpd.id, None);
        assert!(mpd.periods.is_empty());

        let representation = Representation::new("video-1080p", 4_800_000);
        assert_eq!(representation.id, "video-1080p");
        assert_eq!(representation.bandwidth, 4_800_000);
        assert_eq!(representation.base, RepresentationBase::new());
    }

    #[test]
    fn presentation_type_roundtrips_through_lexical_form() {
        for (lexical, value) in [
            ("static", PresentationType::Static),
            ("dynamic", PresentationType::Dynamic),
        ] {
            assert_eq!(lexical.parse::<PresentationType>().unwrap(), value);
            assert_eq!(value.to_string(), lexical);
        }
        assert!("STATIC".parse::<PresentationType>().is_err());
    }

    #[test]
    fn content_type_roundtrips_through_lexical_form() {
        for (lexical, value) in [
            ("text", ContentType::Text),
            ("image", ContentType::Image),
            ("audio", ContentType::Audio),
            ("video", ContentType::Video),
            ("application", ContentType::Application),
            ("font", ContentType::Font),
        ] {
            assert_eq!(lexical.parse::<ContentType>().unwrap(), value);
            assert_eq!(value.to_string(), lexical);
        }
        assert!("model".parse::<ContentType>().is_err());
    }

    #[test]
    fn video_scan_roundtrips_through_lexical_form() {
        for (lexical, value) in [
            ("progressive", VideoScan::Progressive),
            ("interlaced", VideoScan::Interlaced),
            ("unknown", VideoScan::Unknown),
        ] {
            assert_eq!(lexical.parse::<VideoScan>().unwrap(), value);
            assert_eq!(value.to_string(), lexical);
        }
        assert!("Progressive".parse::<VideoScan>().is_err());
    }
}

/// A `BaseURL` element with simple content (XSD `BaseURLType`).
///
/// Represents a URI with optional attributes for availability timing and
/// byte range information.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct BaseUrl {
    /// The URI text content.
    pub url: String,
    /// The `serviceLocation` attribute.
    pub service_location: Option<String>,
    /// The `byteRange` attribute.
    pub byte_range: Option<String>,
    /// The `availabilityTimeOffset` attribute.
    pub availability_time_offset: Option<f64>,
    /// The `availabilityTimeComplete` attribute.
    pub availability_time_complete: Option<bool>,
    /// The `timeShiftBufferDepth` attribute.
    pub time_shift_buffer_depth: Option<XsDuration>,
    /// The `rangeAccess` attribute, defaulting to `false`.
    pub range_access: bool,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl BaseUrl {
    /// Creates a `BaseURL` with the text content; other fields start empty.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            service_location: None,
            byte_range: None,
            availability_time_offset: None,
            availability_time_complete: None,
            time_shift_buffer_depth: None,
            range_access: false,
            unknown_attributes: Vec::new(),
        }
    }
}

/// A `ProgramInformation` element (XSD `ProgramInformationType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ProgramInformation {
    /// The `lang` attribute.
    pub lang: Option<String>,
    /// The `moreInformationURL` attribute.
    pub more_information_url: Option<String>,
    /// The `Title` child element.
    pub title: Option<String>,
    /// The `Source` child element.
    pub source: Option<String>,
    /// The `Copyright` child element.
    pub copyright: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ProgramInformation {
    /// Creates an empty program information; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `PatchLocation` element with simple content (XSD `PatchLocationType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PatchLocation {
    /// The URI text content.
    pub url: String,
    /// The `ttl` attribute.
    pub ttl: Option<f64>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl PatchLocation {
    /// Creates a `PatchLocation` with the text content; other fields start empty.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ttl: None,
            unknown_attributes: Vec::new(),
        }
    }
}

/// A `Range` element for `Metrics` (XSD `RangeType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Range {
    /// The `starttime` attribute.
    pub starttime: Option<XsDuration>,
    /// The `duration` attribute.
    pub duration: Option<XsDuration>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl Range {
    /// Creates an empty range; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `Metrics` element (XSD `MetricsType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Metrics {
    /// The required `metrics` attribute.
    pub metrics: String,
    /// The `Range` child elements.
    pub ranges: Vec<Range>,
    /// The `Reporting` child elements (Descriptors).
    pub reportings: Vec<Descriptor>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Metrics {
    /// Creates a `Metrics` with the required attribute; other fields start empty.
    pub fn new(metrics: impl Into<String>) -> Self {
        Self {
            metrics: metrics.into(),
            ranges: Vec::new(),
            reportings: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// An `InitializationSet` element (XSD `InitializationSetType`).
///
/// Extends `RepresentationBaseType` with additional descriptor children.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct InitializationSet {
    /// The embedded `RepresentationBaseType` part, which also carries the
    /// catch-all fields for unknown content.
    pub base: RepresentationBase,
    /// The required `id` attribute.
    pub id: u32,
    /// The `inAllPeriods` attribute, defaulting to `true`.
    pub in_all_periods: bool,
    /// The `contentType` attribute.
    pub content_type: Option<ContentType>,
    /// The `par` attribute.
    pub par: Option<Ratio>,
    /// The `maxWidth` attribute.
    pub max_width: Option<u32>,
    /// The `maxHeight` attribute.
    pub max_height: Option<u32>,
    /// The `maxFrameRate` attribute.
    pub max_frame_rate: Option<FrameRate>,
    /// The `initialization` attribute.
    pub initialization: Option<String>,
    /// The `Accessibility` child elements (Descriptors).
    pub accessibilities: Vec<Descriptor>,
    /// The `Role` child elements (Descriptors).
    pub roles: Vec<Descriptor>,
    /// The `Rating` child elements (Descriptors).
    pub ratings: Vec<Descriptor>,
    /// The `Viewpoint` child elements (Descriptors).
    pub viewpoints: Vec<Descriptor>,
}

impl InitializationSet {
    /// Creates an `InitializationSet` with the required `id` attribute; other
    /// fields start empty.
    pub fn new(id: u32) -> Self {
        Self {
            base: RepresentationBase::new(),
            id,
            in_all_periods: true,
            content_type: None,
            par: None,
            max_width: None,
            max_height: None,
            max_frame_rate: None,
            initialization: None,
            accessibilities: Vec::new(),
            roles: Vec::new(),
            ratings: Vec::new(),
            viewpoints: Vec::new(),
        }
    }
}

/// A `LeapSecondInformation` element (XSD `LeapSecondInformationType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LeapSecondInformation {
    /// The required `availabilityStartLeapOffset` attribute.
    pub availability_start_leap_offset: i64,
    /// The `nextAvailabilityStartLeapOffset` attribute.
    pub next_availability_start_leap_offset: Option<i64>,
    /// The `nextLeapChangeTime` attribute.
    pub next_leap_change_time: Option<XsDateTime>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl LeapSecondInformation {
    /// Creates a `LeapSecondInformation` with the required attribute; other
    /// fields start empty.
    pub fn new(availability_start_leap_offset: i64) -> Self {
        Self {
            availability_start_leap_offset,
            next_availability_start_leap_offset: None,
            next_leap_change_time: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}
