//! Event stream, labels, and related types appearing in Period and
//! `RepresentationBase` contexts.
//!
//! These types are gathered here because they represent the final phase of
//! schema coverage: `EventStream` / `Event`, `Label`, `Preselection`,
//! `Subset`, and the RepresentationBase-level children like `Switching`,
//! `RandomAccess`, `ProducerReferenceTime`, `ContentPopularityRate`, and
//! `Resync`.

use std::fmt;
use std::str::FromStr;

use crate::error::Error;
use crate::model::descriptor::Descriptor;
use crate::model::element::Element;
use crate::model::mpd::RepresentationBase;
use crate::model::types::{Sap, XsDuration, invalid_value};

/// An `EventStream` element (XSD `EventStreamType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct EventStream {
    /// The `schemeIdUri` attribute (required).
    pub scheme_id_uri: String,
    /// The `value` attribute.
    pub value: Option<String>,
    /// The `timescale` attribute.
    pub timescale: Option<u32>,
    /// The `presentationTimeOffset` attribute.
    pub presentation_time_offset: u64,
    /// The `xlink:href` attribute.
    pub href: Option<String>,
    /// The `xlink:actuate` attribute (default: "onRequest").
    pub actuate: Option<String>,
    /// The `Event` children.
    pub events: Vec<Event>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl EventStream {
    /// Creates an `EventStream` with the required `schemeIdUri`; other fields start empty.
    pub fn new(scheme_id_uri: impl Into<String>) -> Self {
        Self {
            scheme_id_uri: scheme_id_uri.into(),
            value: None,
            timescale: None,
            presentation_time_offset: 0,
            href: None,
            actuate: None,
            events: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// An `Event` element (XSD `EventType`).
///
/// The XSD marks this as `mixed="true"`, meaning it can contain both
/// text content and child elements. This is modeled as a text content field
/// plus a vector of unknown child elements (since the XSD `xs:any` is the
/// only specified child element type).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Event {
    /// The text content of the event (the mixed text part).
    pub text_content: Option<String>,
    /// The `presentationTime` attribute.
    pub presentation_time: u64,
    /// The `duration` attribute.
    pub duration: Option<u64>,
    /// The `id` attribute.
    pub id: Option<u32>,
    /// The `contentEncoding` attribute.
    pub content_encoding: Option<ContentEncoding>,
    /// The `messageData` attribute (deprecated).
    pub message_data: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Event {
    /// Creates an Event with defaults; all fields start empty/zero.
    pub fn new() -> Self {
        Self {
            text_content: None,
            presentation_time: 0,
            duration: None,
            id: None,
            content_encoding: None,
            message_data: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

/// The `contentEncoding` attribute (XSD `ContentEncodingType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ContentEncoding {
    /// Base64 encoding (`base64`).
    Base64,
}

impl FromStr for ContentEncoding {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "base64" => Ok(Self::Base64),
            _ => Err(invalid_value(input, "`base64`")),
        }
    }
}

impl fmt::Display for ContentEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base64 => formatter.write_str("base64"),
        }
    }
}

/// A `Label` or `GroupLabel` element (XSD `LabelType`).
///
/// This represents a simple content element with text and optional id/lang attributes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Label {
    /// The text content.
    pub text: String,
    /// The `id` attribute (default: 0).
    pub id: u32,
    /// The `lang` attribute.
    pub lang: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl Label {
    /// Creates a Label with the required text content; other fields start empty.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            id: 0,
            lang: None,
            unknown_attributes: Vec::new(),
        }
    }
}

/// A `Subset` element (XSD `SubsetType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Subset {
    /// The `contains` attribute (required), split on whitespace.
    pub contains: Vec<u32>,
    /// The `id` attribute.
    pub id: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl Subset {
    /// Creates a Subset with the required `contains` list; other fields start empty.
    pub fn new(contains: Vec<u32>) -> Self {
        Self {
            contains,
            id: None,
            unknown_attributes: Vec::new(),
        }
    }
}

/// A `Preselection` element (XSD `PreselectionType`).
///
/// This extends `RepresentationBaseType` and adds child descriptors and
/// attributes specific to preselection.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Preselection {
    /// The embedded `RepresentationBaseType` part.
    pub base: RepresentationBase,
    /// The `id` attribute (default: "1").
    pub id: String,
    /// The `preselectionComponents` attribute (required), split on whitespace.
    pub preselection_components: Vec<String>,
    /// The `lang` attribute.
    pub lang: Option<String>,
    /// The `order` attribute (default: "undefined").
    pub order: PreselectionOrder,
    /// The `Accessibility` children.
    pub accessibilities: Vec<Descriptor>,
    /// The `Role` children.
    pub roles: Vec<Descriptor>,
    /// The `Rating` children.
    pub ratings: Vec<Descriptor>,
    /// The `Viewpoint` children.
    pub viewpoints: Vec<Descriptor>,
}

impl Preselection {
    /// Creates a Preselection with the required attributes; other fields start empty.
    pub fn new(preselection_components: Vec<String>) -> Self {
        Self {
            base: RepresentationBase::new(),
            id: "1".to_string(),
            preselection_components,
            lang: None,
            order: PreselectionOrder::Undefined,
            accessibilities: Vec::new(),
            roles: Vec::new(),
            ratings: Vec::new(),
            viewpoints: Vec::new(),
        }
    }
}

/// The `order` attribute for Preselection (XSD `PreselectionOrderType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PreselectionOrder {
    /// Undefined order (`undefined`).
    Undefined,
}

impl FromStr for PreselectionOrder {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "undefined" => Ok(Self::Undefined),
            _ => Err(invalid_value(input, "`undefined`")),
        }
    }
}

impl fmt::Display for PreselectionOrder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Undefined => formatter.write_str("undefined"),
        }
    }
}

/// A `Switching` element (XSD `SwitchingType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Switching {
    /// The `interval` attribute (required).
    pub interval: u32,
    /// The `type` attribute (default: "media").
    pub switching_type: SwitchingType,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl Switching {
    /// Creates a Switching with the required `interval`; other fields start with defaults.
    pub fn new(interval: u32) -> Self {
        Self {
            interval,
            switching_type: SwitchingType::Media,
            unknown_attributes: Vec::new(),
        }
    }
}

/// The `type` attribute for Switching (XSD `SwitchingTypeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SwitchingType {
    /// Media switching (`media`).
    Media,
    /// Bitstream switching (`bitstream`).
    Bitstream,
}

impl FromStr for SwitchingType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "media" => Ok(Self::Media),
            "bitstream" => Ok(Self::Bitstream),
            _ => Err(invalid_value(input, "`media` or `bitstream`")),
        }
    }
}

impl fmt::Display for SwitchingType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Media => formatter.write_str("media"),
            Self::Bitstream => formatter.write_str("bitstream"),
        }
    }
}

/// A `RandomAccess` element (XSD `RandomAccessType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RandomAccess {
    /// The `interval` attribute (required).
    pub interval: u32,
    /// The `type` attribute (default: "closed").
    pub random_access_type: RandomAccessType,
    /// The `minBufferTime` attribute.
    pub min_buffer_time: Option<XsDuration>,
    /// The `bandwidth` attribute.
    pub bandwidth: Option<u32>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl RandomAccess {
    /// Creates a `RandomAccess` with the required `interval`; other fields start with defaults.
    pub fn new(interval: u32) -> Self {
        Self {
            interval,
            random_access_type: RandomAccessType::Closed,
            min_buffer_time: None,
            bandwidth: None,
            unknown_attributes: Vec::new(),
        }
    }
}

/// The `type` attribute for `RandomAccess` (XSD `RandomAccessTypeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RandomAccessType {
    /// Closed random access (`closed`).
    Closed,
    /// Open random access (`open`).
    Open,
}

impl FromStr for RandomAccessType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "closed" => Ok(Self::Closed),
            "open" => Ok(Self::Open),
            _ => Err(invalid_value(input, "`closed` or `open`")),
        }
    }
}

impl fmt::Display for RandomAccessType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("closed"),
            Self::Open => formatter.write_str("open"),
        }
    }
}

/// A `ProducerReferenceTime` element (XSD `ProducerReferenceTimeType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ProducerReferenceTime {
    /// The `id` attribute (required).
    pub id: u32,
    /// The `inband` attribute (default: false).
    pub inband: bool,
    /// The `type` attribute (default: "encoder").
    pub producer_reference_time_type: ProducerReferenceTimeType,
    /// The `applicationScheme` attribute.
    pub application_scheme: Option<String>,
    /// The `wallClockTime` attribute (required).
    pub wall_clock_time: String,
    /// The `presentationTime` attribute (required).
    pub presentation_time: u64,
    /// The `UTCTiming` child.
    pub utc_timing: Option<Descriptor>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ProducerReferenceTime {
    /// Creates a `ProducerReferenceTime` with the required attributes; other fields start empty.
    pub fn new(id: u32, wall_clock_time: impl Into<String>, presentation_time: u64) -> Self {
        Self {
            id,
            inband: false,
            producer_reference_time_type: ProducerReferenceTimeType::Encoder,
            application_scheme: None,
            wall_clock_time: wall_clock_time.into(),
            presentation_time,
            utc_timing: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// The `type` attribute for `ProducerReferenceTime` (XSD `ProducerReferenceTimeTypeType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProducerReferenceTimeType {
    /// Encoder type (`encoder`).
    Encoder,
    /// Captured type (`captured`).
    Captured,
    /// Application type (`application`).
    Application,
}

impl FromStr for ProducerReferenceTimeType {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "encoder" => Ok(Self::Encoder),
            "captured" => Ok(Self::Captured),
            "application" => Ok(Self::Application),
            _ => Err(invalid_value(
                input,
                "`encoder`, `captured`, or `application`",
            )),
        }
    }
}

impl fmt::Display for ProducerReferenceTimeType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encoder => formatter.write_str("encoder"),
            Self::Captured => formatter.write_str("captured"),
            Self::Application => formatter.write_str("application"),
        }
    }
}

/// A `ContentPopularityRate` element (XSD `ContentPopularityRateType`).
///
/// This contains a sequence of inline `PR` child elements with popularity rate data.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ContentPopularityRate {
    /// The `source` attribute (required).
    pub source: ContentPopularitySource,
    /// The `source_description` attribute.
    pub source_description: Option<String>,
    /// The `PR` children.
    pub rates: Vec<PopularityRate>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ContentPopularityRate {
    /// Creates a `ContentPopularityRate` with the required `source`; other fields start empty.
    pub fn new(source: ContentPopularitySource) -> Self {
        Self {
            source,
            source_description: None,
            rates: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// The `source` attribute for `ContentPopularityRate` (XSD inline restriction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ContentPopularitySource {
    /// Content source (`content`).
    Content,
    /// Statistics source (`statistics`).
    Statistics,
    /// Other source (`other`).
    Other,
}

impl FromStr for ContentPopularitySource {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self, Error> {
        match input {
            "content" => Ok(Self::Content),
            "statistics" => Ok(Self::Statistics),
            "other" => Ok(Self::Other),
            _ => Err(invalid_value(input, "`content`, `statistics`, or `other`")),
        }
    }
}

impl fmt::Display for ContentPopularitySource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content => formatter.write_str("content"),
            Self::Statistics => formatter.write_str("statistics"),
            Self::Other => formatter.write_str("other"),
        }
    }
}

/// A `PR` element within `ContentPopularityRate` (XSD inline complexType).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PopularityRate {
    /// The `popularityRate` attribute (1-100).
    pub popularity_rate: Option<u32>,
    /// The `start` attribute.
    pub start: Option<u64>,
    /// The `r` attribute (default: 0).
    pub r: i32,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl PopularityRate {
    /// Creates a `PopularityRate` with defaults; all fields start empty.
    pub fn new() -> Self {
        Self {
            popularity_rate: None,
            start: None,
            r: 0,
            unknown_attributes: Vec::new(),
        }
    }
}

impl Default for PopularityRate {
    fn default() -> Self {
        Self::new()
    }
}

/// A `Resync` element (XSD `ResyncType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Resync {
    /// The `type` attribute (default: "0").
    pub resync_type: Sap,
    /// The `dT` attribute.
    pub dt: Option<u32>,
    /// The `dImax` attribute.
    pub di_max: Option<f32>,
    /// The `dImin` attribute (default: 0).
    pub di_min: f32,
    /// The `marker` attribute (default: false).
    pub marker: bool,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl Resync {
    /// Creates a Resync with defaults; all fields start empty/zero.
    pub fn new() -> Self {
        Self {
            resync_type: Sap::default(),
            dt: None,
            di_max: None,
            di_min: 0.0,
            marker: false,
            unknown_attributes: Vec::new(),
        }
    }
}

impl Default for Resync {
    fn default() -> Self {
        Self::new()
    }
}

/// A `SubRepresentation` element (XSD `SubRepresentationType`).
///
/// This extends `RepresentationBaseType` and adds representation-specific children and attributes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SubRepresentation {
    /// The embedded `RepresentationBaseType` part.
    pub base: RepresentationBase,
    /// The `level` attribute.
    pub level: Option<u32>,
    /// The `dependencyLevel` attribute, split on whitespace. Empty means absent.
    pub dependency_level: Vec<u32>,
    /// The `bandwidth` attribute.
    pub bandwidth: Option<u32>,
    /// The `contentComponent` attribute, split on whitespace. Empty means absent.
    pub content_component: Vec<String>,
}

impl SubRepresentation {
    /// Creates a `SubRepresentation` with no required attributes; all fields start empty.
    pub fn new() -> Self {
        Self {
            base: RepresentationBase::new(),
            level: None,
            dependency_level: Vec::new(),
            bandwidth: None,
            content_component: Vec::new(),
        }
    }
}

impl Default for SubRepresentation {
    fn default() -> Self {
        Self::new()
    }
}

/// An `ExtendedBandwidth` element (XSD `ExtendedBandwidthType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ExtendedBandwidth {
    /// The `vbr` attribute (default: false).
    pub vbr: bool,
    /// The `ModelPair` children.
    pub model_pairs: Vec<ModelPair>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ExtendedBandwidth {
    /// Creates an `ExtendedBandwidth` with defaults; all fields start empty.
    pub fn new() -> Self {
        Self {
            vbr: false,
            model_pairs: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

impl Default for ExtendedBandwidth {
    fn default() -> Self {
        Self::new()
    }
}

/// A `ModelPair` element (XSD `ModelPairType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ModelPair {
    /// The `bufferTime` attribute (required).
    pub buffer_time: XsDuration,
    /// The `bandwidth` attribute (required).
    pub bandwidth: u32,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ModelPair {
    /// Creates a `ModelPair` with the required attributes; other fields start empty.
    pub fn new(buffer_time: XsDuration, bandwidth: u32) -> Self {
        Self {
            buffer_time,
            bandwidth,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// A `ContentComponent` element (XSD `ContentComponentType`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ContentComponent {
    /// The `id` attribute.
    pub id: Option<u32>,
    /// The `lang` attribute.
    pub lang: Option<String>,
    /// The `contentType` attribute.
    pub content_type: Option<String>,
    /// The `par` attribute.
    pub par: Option<crate::model::types::Ratio>,
    /// The `tag` attribute.
    pub tag: Option<String>,
    /// The `Accessibility` children.
    pub accessibilities: Vec<Descriptor>,
    /// The `Role` children.
    pub roles: Vec<Descriptor>,
    /// The `Rating` children.
    pub ratings: Vec<Descriptor>,
    /// The `Viewpoint` children.
    pub viewpoints: Vec<Descriptor>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ContentComponent {
    /// Creates a `ContentComponent` with no required attributes; all fields start empty.
    pub fn new() -> Self {
        Self {
            id: None,
            lang: None,
            content_type: None,
            par: None,
            tag: None,
            accessibilities: Vec::new(),
            roles: Vec::new(),
            ratings: Vec::new(),
            viewpoints: Vec::new(),
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

impl Default for ContentComponent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_stream_new_sets_required_and_defaults() {
        let es = EventStream::new("urn:example:scheme");
        assert_eq!(es.scheme_id_uri, "urn:example:scheme");
        assert_eq!(es.presentation_time_offset, 0);
        assert!(es.value.is_none());
        assert!(es.events.is_empty());
    }

    #[test]
    fn event_new_sets_defaults() {
        let ev = Event::new();
        assert_eq!(ev.presentation_time, 0);
        assert!(ev.text_content.is_none());
        assert!(ev.duration.is_none());
    }

    #[test]
    fn label_new_sets_text_and_defaults() {
        let label = Label::new("English");
        assert_eq!(label.text, "English");
        assert_eq!(label.id, 0);
        assert!(label.lang.is_none());
    }

    #[test]
    fn content_encoding_roundtrips() {
        assert_eq!(
            "base64".parse::<ContentEncoding>().unwrap(),
            ContentEncoding::Base64
        );
        assert_eq!(ContentEncoding::Base64.to_string(), "base64");
    }

    #[test]
    fn switching_type_roundtrips() {
        for (lex, val) in [
            ("media", SwitchingType::Media),
            ("bitstream", SwitchingType::Bitstream),
        ] {
            assert_eq!(lex.parse::<SwitchingType>().unwrap(), val);
            assert_eq!(val.to_string(), lex);
        }
    }

    #[test]
    fn random_access_type_roundtrips() {
        for (lex, val) in [
            ("closed", RandomAccessType::Closed),
            ("open", RandomAccessType::Open),
        ] {
            assert_eq!(lex.parse::<RandomAccessType>().unwrap(), val);
            assert_eq!(val.to_string(), lex);
        }
    }

    #[test]
    fn producer_reference_time_type_roundtrips() {
        for (lex, val) in [
            ("encoder", ProducerReferenceTimeType::Encoder),
            ("captured", ProducerReferenceTimeType::Captured),
            ("application", ProducerReferenceTimeType::Application),
        ] {
            assert_eq!(lex.parse::<ProducerReferenceTimeType>().unwrap(), val);
            assert_eq!(val.to_string(), lex);
        }
    }

    #[test]
    fn content_popularity_source_roundtrips() {
        for (lex, val) in [
            ("content", ContentPopularitySource::Content),
            ("statistics", ContentPopularitySource::Statistics),
            ("other", ContentPopularitySource::Other),
        ] {
            assert_eq!(lex.parse::<ContentPopularitySource>().unwrap(), val);
            assert_eq!(val.to_string(), lex);
        }
    }

    #[test]
    fn preselection_order_roundtrips() {
        assert_eq!(
            "undefined".parse::<PreselectionOrder>().unwrap(),
            PreselectionOrder::Undefined
        );
        assert_eq!(PreselectionOrder::Undefined.to_string(), "undefined");
    }
}
