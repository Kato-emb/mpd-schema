//! Segment information types: `SegmentBase`, `SegmentList`,
//! `SegmentTemplate`, and `SegmentTimeline`.
//!
//! The XSD derives `MultipleSegmentBaseType` from `SegmentBaseType`, and
//! `SegmentListType` / `SegmentTemplateType` from `MultipleSegmentBaseType`;
//! the model represents each `xs:extension` step as an embedded base struct
//! (ADR-0002). Byte ranges of XSD type `SingleRFC7233RangeType` stay
//! [`String`]s: the presence of the `-` separator is significant
//! (`500` vs `500-`), so a numeric pair cannot represent the lexical space
//! losslessly. Template strings such as `SegmentTemplate@media` also stay
//! [`String`]s; expanding `$Number$` and friends is resolution-layer work
//! (ADR-0001).

use crate::model::element::Element;
use crate::model::types::XsDuration;

/// A `SegmentBase` element, and the base part of the other segment
/// information types (XSD `SegmentBaseType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SegmentBase {
    /// The `timescale` attribute.
    pub timescale: Option<u32>,
    /// The `eptDelta` attribute.
    pub ept_delta: Option<i64>,
    /// The `pdDelta` attribute.
    pub pd_delta: Option<i64>,
    /// The `presentationTimeOffset` attribute.
    pub presentation_time_offset: Option<u64>,
    /// The `presentationDuration` attribute.
    pub presentation_duration: Option<u64>,
    /// The `timeShiftBufferDepth` attribute.
    pub time_shift_buffer_depth: Option<XsDuration>,
    /// The `indexRange` attribute, a byte range such as `0-499`.
    pub index_range: Option<String>,
    /// The `indexRangeExact` attribute.
    pub index_range_exact: Option<bool>,
    /// The `availabilityTimeOffset` attribute.
    pub availability_time_offset: Option<f64>,
    /// The `availabilityTimeComplete` attribute.
    pub availability_time_complete: Option<bool>,
    /// The `Initialization` child.
    pub initialization: Option<Url>,
    /// The `RepresentationIndex` child.
    pub representation_index: Option<Url>,
    /// The `FailoverContent` child.
    pub failover_content: Option<FailoverContent>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl SegmentBase {
    /// Creates an empty segment base; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// The base part shared by `SegmentList` and `SegmentTemplate` (XSD
/// `MultipleSegmentBaseType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct MultipleSegmentBase {
    /// The embedded `SegmentBaseType` part, which also carries the catch-all
    /// fields for unknown content.
    pub base: SegmentBase,
    /// The `duration` attribute, in units of `timescale`.
    pub duration: Option<u32>,
    /// The `startNumber` attribute.
    pub start_number: Option<u32>,
    /// The `endNumber` attribute.
    pub end_number: Option<u32>,
    /// The `SegmentTimeline` child.
    pub segment_timeline: Option<SegmentTimeline>,
    /// The `BitstreamSwitching` child.
    pub bitstream_switching: Option<Url>,
}

impl MultipleSegmentBase {
    /// Creates an empty multiple-segment base; the schema requires no
    /// attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A URL/range pair used by the `Initialization`, `RepresentationIndex`,
/// and `BitstreamSwitching` children (XSD `URLType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Url {
    /// The `sourceURL` attribute.
    pub source_url: Option<String>,
    /// The `range` attribute, a byte range such as `0-499`.
    pub range: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl Url {
    /// Creates an empty URL element; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `FailoverContent` element (XSD `FailoverContentType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct FailoverContent {
    /// The `valid` attribute.
    pub valid: Option<bool>,
    /// The `FCS` children. The schema requires at least one; occurrence
    /// counts are not enforced by parsing.
    pub fcs_entries: Vec<Fcs>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl FailoverContent {
    /// Creates an empty failover description; the schema requires no
    /// attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// An `FCS` (failover content section) child of `FailoverContent`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Fcs {
    /// The required `t` attribute: the section's start time, in units of
    /// the enclosing `timescale`.
    pub t: u64,
    /// The `d` attribute: the section's duration; absent means the section
    /// lasts to the end of the period.
    pub d: Option<u64>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl Fcs {
    /// Creates a section starting at `t`; every other field starts empty.
    pub fn new(t: u64) -> Self {
        Self {
            t,
            d: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// A `SegmentList` element (XSD `SegmentListType`).
///
/// The `xlink:*` attributes have no typed fields yet and are preserved
/// through the catch-all on the embedded [`SegmentBase`].
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SegmentList {
    /// The embedded `MultipleSegmentBaseType` part.
    pub base: MultipleSegmentBase,
    /// The `SegmentURL` children.
    pub segment_urls: Vec<SegmentUrl>,
}

impl SegmentList {
    /// Creates an empty segment list; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `SegmentURL` child of `SegmentList` (XSD `SegmentURLType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SegmentUrl {
    /// The `media` attribute.
    pub media: Option<String>,
    /// The `mediaRange` attribute, a byte range such as `0-499`.
    pub media_range: Option<String>,
    /// The `index` attribute.
    pub index: Option<String>,
    /// The `indexRange` attribute, a byte range such as `0-499`.
    pub index_range: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl SegmentUrl {
    /// Creates an empty segment URL; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `SegmentTemplate` element (XSD `SegmentTemplateType`).
///
/// The template attributes are kept verbatim; identifier substitution such
/// as `$Number$` belongs to the resolution layer (ADR-0001).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SegmentTemplate {
    /// The embedded `MultipleSegmentBaseType` part.
    pub base: MultipleSegmentBase,
    /// The `media` template attribute.
    pub media: Option<String>,
    /// The `index` template attribute.
    pub index: Option<String>,
    /// The `initialization` template attribute.
    pub initialization: Option<String>,
    /// The `bitstreamSwitching` template attribute. Distinct from the
    /// `BitstreamSwitching` child element held by the embedded base.
    pub bitstream_switching: Option<String>,
}

impl SegmentTemplate {
    /// Creates an empty segment template; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A `SegmentTimeline` element (XSD `SegmentTimelineType`).
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SegmentTimeline {
    /// The `S` children.
    pub segments: Vec<S>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl SegmentTimeline {
    /// Creates an empty timeline; the schema requires no attribute.
    pub fn new() -> Self {
        Self::default()
    }
}

/// An `S` (segment) child of `SegmentTimeline`.
///
/// The single-letter field names mirror the attribute names defined by the
/// schema.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct S {
    /// The `t` attribute: the segment's start time, in units of the
    /// enclosing `timescale`.
    pub t: Option<u64>,
    /// The `n` attribute: the segment's number.
    pub n: Option<u64>,
    /// The required `d` attribute: the segment's duration, in units of the
    /// enclosing `timescale`.
    pub d: u64,
    /// The `r` attribute: the repeat count of contiguous segments with the
    /// same duration; `-1` means the repetition extends to the next `S`
    /// entry or to the end of the period.
    pub r: Option<i64>,
    /// The `k` attribute: the number of segments described by this entry's
    /// chunk pattern.
    pub k: Option<u64>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field.
    pub unknown_children: Vec<Element>,
}

impl S {
    /// Creates an entry of duration `d`; every other field starts empty.
    pub fn new(d: u64) -> Self {
        Self {
            t: None,
            n: None,
            d,
            r: None,
            k: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_required_attributes_and_leaves_the_rest_empty() {
        let entry = S::new(1_024);
        assert_eq!(entry.d, 1_024);
        assert_eq!(entry.t, None);
        assert_eq!(entry.r, None);

        let section = Fcs::new(900_000);
        assert_eq!(section.t, 900_000);
        assert_eq!(section.d, None);

        let template = SegmentTemplate::new();
        assert_eq!(template.base, MultipleSegmentBase::new());
        assert_eq!(template.media, None);
    }
}
