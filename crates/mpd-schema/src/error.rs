//! Error types shared across parsing and serialization.

use std::fmt;

/// Convenience alias for results produced by this crate.
pub type Result<T> = std::result::Result<T, Error>;

/// Error produced while parsing or serializing an MPD document.
///
/// Processing is fail-fast: the first error aborts the operation.
#[derive(Debug)]
#[non_exhaustive]
pub struct Error {
    /// Location in the document, such as
    /// `"MPD > Period[0] > AdaptationSet[2] @ minBufferTime"`.
    ///
    /// Empty when the error is not tied to a document location, for example
    /// when parsing a standalone attribute value via [`std::str::FromStr`].
    pub path: String,
    /// The category of failure.
    pub kind: ErrorKind,
}

impl Error {
    /// Creates an error that is not yet tied to a document location.
    ///
    /// The deserializer fills in [`Error::path`] when it propagates the error.
    pub(crate) fn new(kind: ErrorKind) -> Self {
        Self {
            path: String::new(),
            kind,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(formatter, "{}", self.kind)
        } else {
            write!(formatter, "{} (at {})", self.kind, self.path)
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ErrorKind::Io(source) => Some(source),
            _ => None,
        }
    }
}

/// The category of a parsing or serialization failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The input is not well-formed XML.
    Xml(String),
    /// An I/O operation failed while reading or writing a document.
    Io(std::io::Error),
    /// A required attribute is missing.
    ///
    /// The attribute is identified by [`Error::path`].
    MissingAttribute,
    /// A value does not conform to its expected lexical form.
    InvalidValue {
        /// The value as written in the document.
        value: String,
        /// A description of the expected form.
        expected: String,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(message) => write!(formatter, "malformed XML: {message}"),
            Self::Io(source) => write!(formatter, "I/O error: {source}"),
            Self::MissingAttribute => formatter.write_str("missing required attribute"),
            Self::InvalidValue { value, expected } => {
                write!(formatter, "invalid value `{value}`, expected {expected}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_without_path_omits_location() {
        let error = Error::new(ErrorKind::MissingAttribute);
        assert_eq!(error.to_string(), "missing required attribute");
    }

    #[test]
    fn display_with_path_appends_location() {
        let mut error = Error::new(ErrorKind::InvalidValue {
            value: "abc".to_string(),
            expected: "an unsigned integer".to_string(),
        });
        error.path = "MPD > Period[0] @ start".to_string();
        assert_eq!(
            error.to_string(),
            "invalid value `abc`, expected an unsigned integer (at MPD > Period[0] @ start)"
        );
    }

    #[test]
    fn io_error_is_exposed_as_source() {
        let error = Error::new(ErrorKind::Io(std::io::Error::other("broken pipe")));
        assert!(std::error::Error::source(&error).is_some());
    }
}
