//! The crate's error type and its kinds.

use std::fmt;

/// A specialized [`Result`](std::result::Result) for resolution operations.
pub type Result<T> = std::result::Result<T, Error>;

/// An error raised while resolving an MPD.
///
/// `path` locates the element under resolution in the same style as
/// `mpd-schema`'s parse errors (`Period[0] > AdaptationSet[2] >
/// Representation[1] @ media`), and `kind` says what went wrong.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Error {
    /// The location of the offending element, or empty when not applicable.
    pub path: String,
    /// The category of failure.
    pub kind: ErrorKind,
}

impl Error {
    pub(crate) fn new(path: String, kind: ErrorKind) -> Self {
        Self { path, kind }
    }
}

/// The category of a resolution [`Error`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A `BaseURL` (or the manifest base) was not a valid URI reference, or a
    /// relative reference could not be resolved against the effective base.
    InvalidBaseUrl {
        /// The offending URI text.
        value: String,
    },
    /// A segment template referenced an identifier that DASH does not define.
    UnknownTemplateIdentifier {
        /// The unrecognized identifier, without the surrounding `$`.
        identifier: String,
    },
    /// A segment template's format tag was malformed (only `%0[width]d` is
    /// accepted), or the template was otherwise unparseable.
    InvalidTemplateFormat {
        /// The template string that failed to parse.
        template: String,
    },
    /// A segment template referenced an identifier that this version cannot
    /// supply a value for in the current addressing context.
    UnsupportedAddressing {
        /// Why the addressing could not be resolved.
        reason: String,
    },
    /// The Representation declared no usable addressing (no `SegmentTemplate`,
    /// `SegmentList`, `SegmentBase`, or `BaseURL`).
    MissingAddressing,
    /// The declared segment information was internally inconsistent (for
    /// example `$Time$` without a `SegmentTimeline`).
    InconsistentSegmentInfo {
        /// What the inconsistency was.
        reason: String,
    },
    /// An arithmetic computation over segment counts or times overflowed.
    Overflow,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.path.is_empty() {
            write!(formatter, "{}: ", self.path)?;
        }
        match &self.kind {
            ErrorKind::InvalidBaseUrl { value } => {
                write!(
                    formatter,
                    "invalid base URL or unresolvable reference `{value}`"
                )
            }
            ErrorKind::UnknownTemplateIdentifier { identifier } => {
                write!(formatter, "unknown template identifier `${identifier}$`")
            }
            ErrorKind::InvalidTemplateFormat { template } => {
                write!(formatter, "malformed segment template `{template}`")
            }
            ErrorKind::UnsupportedAddressing { reason } => {
                write!(formatter, "unsupported addressing: {reason}")
            }
            ErrorKind::MissingAddressing => {
                formatter.write_str("no segment addressing information")
            }
            ErrorKind::InconsistentSegmentInfo { reason } => {
                write!(formatter, "inconsistent segment information: {reason}")
            }
            ErrorKind::Overflow => formatter.write_str("segment computation overflowed"),
        }
    }
}

impl std::error::Error for Error {}
