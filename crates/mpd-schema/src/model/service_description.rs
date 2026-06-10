//! Service description types (XSD `ServiceDescriptionType` and related).

use crate::model::descriptor::Descriptor;
use crate::model::element::Element;

/// A `ServiceDescription` element (XSD `ServiceDescriptionType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ServiceDescription {
    /// The `id` attribute.
    pub id: Option<u32>,
    /// The `Scope` child elements (Descriptors).
    pub scopes: Vec<Descriptor>,
    /// The `Latency` child elements.
    pub latencies: Vec<Latency>,
    /// The `PlaybackRate` child elements.
    pub playback_rates: Vec<PlaybackRate>,
    /// The `OperatingQuality` child elements.
    pub operating_qualities: Vec<OperatingQuality>,
    /// The `OperatingBandwidth` child elements.
    pub operating_bandwidths: Vec<OperatingBandwidth>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl ServiceDescription {
    /// Creates an empty service description; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `Latency` element (XSD `LatencyType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Latency {
    /// The `referenceId` attribute.
    pub reference_id: Option<u32>,
    /// The `target` attribute.
    pub target: Option<u32>,
    /// The `max` attribute.
    pub max: Option<u32>,
    /// The `min` attribute.
    pub min: Option<u32>,
    /// The `QualityLatency` child elements.
    pub quality_latencies: Vec<UIntPairsWithId>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Latency {
    /// Creates an empty latency; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `PlaybackRate` element (XSD `PlaybackRateType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct PlaybackRate {
    /// The `max` attribute.
    pub max: Option<f64>,
    /// The `min` attribute.
    pub min: Option<f64>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl PlaybackRate {
    /// Creates an empty playback rate; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Enum for the `mediaType` attribute of `OperatingQuality` (default: "any").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum OperatingQualityMediaType {
    /// `video`
    Video,
    /// `audio`
    Audio,
    /// `any` (default)
    #[default]
    Any,
}

impl std::str::FromStr for OperatingQualityMediaType {
    type Err = crate::error::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "any" => Ok(Self::Any),
            _ => Err(crate::model::types::invalid_value(
                input,
                "`video`, `audio`, or `any`",
            )),
        }
    }
}

impl std::fmt::Display for OperatingQualityMediaType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => formatter.write_str("video"),
            Self::Audio => formatter.write_str("audio"),
            Self::Any => formatter.write_str("any"),
        }
    }
}

/// An `OperatingQuality` element (XSD `OperatingQualityType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct OperatingQuality {
    /// The `mediaType` attribute, defaulting to `OperatingQualityMediaType::Any`.
    pub media_type: OperatingQualityMediaType,
    /// The `min` attribute.
    pub min: Option<u32>,
    /// The `max` attribute.
    pub max: Option<u32>,
    /// The `target` attribute.
    pub target: Option<u32>,
    /// The `type` attribute.
    pub quality_type: Option<String>,
    /// The `maxDifference` attribute.
    pub max_difference: Option<u32>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl OperatingQuality {
    /// Creates an empty operating quality; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Enum for the `mediaType` attribute of `OperatingBandwidth` (default: "all").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum OperatingBandwidthMediaType {
    /// `video`
    Video,
    /// `audio`
    Audio,
    /// `any`
    Any,
    /// `all` (default)
    #[default]
    All,
}

impl std::str::FromStr for OperatingBandwidthMediaType {
    type Err = crate::error::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "any" => Ok(Self::Any),
            "all" => Ok(Self::All),
            _ => Err(crate::model::types::invalid_value(
                input,
                "`video`, `audio`, `any`, or `all`",
            )),
        }
    }
}

impl std::fmt::Display for OperatingBandwidthMediaType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Video => formatter.write_str("video"),
            Self::Audio => formatter.write_str("audio"),
            Self::Any => formatter.write_str("any"),
            Self::All => formatter.write_str("all"),
        }
    }
}

/// An `OperatingBandwidth` element (XSD `OperatingBandwidthType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct OperatingBandwidth {
    /// The `mediaType` attribute, defaulting to `OperatingBandwidthMediaType::All`.
    pub media_type: OperatingBandwidthMediaType,
    /// The `min` attribute.
    pub min: Option<u32>,
    /// The `max` attribute.
    pub max: Option<u32>,
    /// The `target` attribute.
    pub target: Option<u32>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl OperatingBandwidth {
    /// Creates an empty operating bandwidth; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `UIntPairsWithId` element with simple content (XSD `UIntPairsWithIDType`).
///
/// Simple content is a whitespace-separated list of unsigned integers.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct UIntPairsWithId {
    /// The whitespace-separated list of unsigned integers.
    pub pairs: Vec<u32>,
    /// The `type` attribute.
    pub value_type: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl UIntPairsWithId {
    /// Creates a `UIntPairsWithId` with an empty list; other fields start empty.
    pub fn new() -> Self {
        Self {
            pairs: Vec::new(),
            value_type: None,
            unknown_attributes: Vec::new(),
        }
    }
}

impl Default for UIntPairsWithId {
    fn default() -> Self {
        Self::new()
    }
}

/// A `UIntVWithId` element with simple content (XSD `UIntVWithIDType`).
///
/// Simple content is a whitespace-separated list of unsigned integers.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct UIntVWithId {
    /// The required `id` attribute.
    pub id: u32,
    /// The whitespace-separated list of unsigned integers.
    pub values: Vec<u32>,
    /// The `profiles` attribute.
    pub profiles: Option<String>,
    /// The `contentType` attribute.
    pub content_type: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
}

impl UIntVWithId {
    /// Creates a `UIntVWithId` with the required `id` attribute; other fields
    /// start empty.
    pub fn new(id: u32) -> Self {
        Self {
            id,
            values: Vec::new(),
            profiles: None,
            content_type: None,
            unknown_attributes: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_rate_new_creates_empty_structure() {
        let rate = PlaybackRate::new();
        assert_eq!(rate.max, None);
        assert_eq!(rate.min, None);
    }

    #[test]
    fn operating_quality_media_type_roundtrips() {
        for (lexical, value) in [
            ("video", OperatingQualityMediaType::Video),
            ("audio", OperatingQualityMediaType::Audio),
            ("any", OperatingQualityMediaType::Any),
        ] {
            assert_eq!(lexical.parse::<OperatingQualityMediaType>().unwrap(), value);
            assert_eq!(value.to_string(), lexical);
        }
    }

    #[test]
    fn operating_bandwidth_media_type_roundtrips() {
        for (lexical, value) in [
            ("video", OperatingBandwidthMediaType::Video),
            ("audio", OperatingBandwidthMediaType::Audio),
            ("any", OperatingBandwidthMediaType::Any),
            ("all", OperatingBandwidthMediaType::All),
        ] {
            assert_eq!(
                lexical.parse::<OperatingBandwidthMediaType>().unwrap(),
                value
            );
            assert_eq!(value.to_string(), lexical);
        }
    }

    #[test]
    fn uint_v_with_id_new_creates_structure_with_id() {
        let v = UIntVWithId::new(42);
        assert_eq!(v.id, 42);
        assert!(v.values.is_empty());
    }
}
