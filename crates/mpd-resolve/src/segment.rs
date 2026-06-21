//! The resolved-segment value types returned to the caller.

use url::Url;

use crate::error::{Error, ErrorKind};

/// One resolved media segment, in 1:1 correspondence with a real segment.
///
/// The addressing mode that produced it (`SegmentTemplate`, `SegmentList`,
/// `SegmentBase`, or a bare `BaseURL`) is not exposed: every mode folds into
/// this one shape.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ResolvedSegment {
    /// The candidate URLs in document order, one per effective `BaseURL`
    /// alternative. Always non-empty; the first is the document-order primary.
    pub urls: Vec<CandidateUrl>,
    /// The byte range within the resource, when the addressing restricts it.
    pub byte_range: Option<ByteRange>,
    /// The segment's start and duration in `timescale` ticks, when known.
    pub time: Option<SegmentTime>,
    /// The segment number, when the addressing assigns one. This is a stable
    /// identity for re-syncing across MPD refreshes and for seek correlation,
    /// not an input to fetching (the URL already encodes it).
    pub number: Option<u64>,
}

impl ResolvedSegment {
    pub(crate) fn new(urls: Vec<CandidateUrl>) -> Self {
        Self {
            urls,
            byte_range: None,
            time: None,
            number: None,
        }
    }
}

/// A single candidate URL for a segment, paired with its `serviceLocation`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CandidateUrl {
    /// The absolute, RFC 3986-resolved URL.
    pub url: Url,
    /// The `serviceLocation` of the deepest `BaseURL` that contributed this
    /// candidate, used by callers for sticky failover.
    pub service_location: Option<String>,
}

impl CandidateUrl {
    pub(crate) fn new(url: Url, service_location: Option<String>) -> Self {
        Self {
            url,
            service_location,
        }
    }
}

/// A byte range within a resource, as carried by `indexRange`, `mediaRange`,
/// `range`, and `byteRange` attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ByteRange {
    /// The first byte offset (inclusive).
    pub start: u64,
    /// The last byte offset (inclusive), or `None` for an open range such as
    /// `500-` that runs to the end of the resource.
    pub end: Option<u64>,
}

impl ByteRange {
    /// Parses a `first-last` byte range such as `0-499` or the open `500-`.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::InconsistentSegmentInfo`] with `path` set to the
    /// caller-supplied location when the text is not a valid byte range.
    pub(crate) fn parse(text: &str, path: &str) -> Result<Self, Error> {
        let malformed = || {
            Error::new(
                path.to_string(),
                ErrorKind::InconsistentSegmentInfo {
                    reason: format!("malformed byte range `{text}`"),
                },
            )
        };
        let (first, last) = text.split_once('-').ok_or_else(malformed)?;
        let start = first.parse::<u64>().map_err(|_| malformed())?;
        let end = if last.is_empty() {
            None
        } else {
            Some(last.parse::<u64>().map_err(|_| malformed())?)
        };
        Ok(Self { start, end })
    }
}

/// A segment's position on the media timeline, in `timescale` ticks.
///
/// Times stay in ticks rather than seconds so the mapping is lossless and
/// matches the `$Time$` identifier's own units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SegmentTime {
    /// The start time in `timescale` ticks on the media timeline. This is the
    /// value `$Time$` substitutes and includes `presentationTimeOffset`.
    pub start: u64,
    /// The duration in `timescale` ticks.
    pub duration: u64,
    /// The number of ticks per second.
    pub timescale: u32,
}

impl SegmentTime {
    pub(crate) fn new(start: u64, duration: u64, timescale: u32) -> Self {
        Self {
            start,
            duration,
            timescale,
        }
    }
}
