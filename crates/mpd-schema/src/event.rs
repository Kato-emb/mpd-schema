//! Internal event vocabulary connecting `de`/`ser` to the XML backend.
//!
//! This is the seam mandated by ADR-0007: conversion logic depends only on
//! these types, never on quick-xml. The whole module is `pub(crate)` so the
//! event vocabulary stays out of the semver contract.

/// A pull event produced by the backend reader and consumed by the backend
/// writer.
///
/// Empty element tags (`<tag/>`) are expanded by the reader into a
/// [`Event::Start`] / [`Event::End`] pair, and consecutive character data
/// (text, CDATA sections, entity and character references) is coalesced into
/// a single [`Event::Text`], so consumers never see two `Text` events in a
/// row. Comments and processing instructions are discarded at read time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    /// An opening tag with its attributes.
    Start(StartElement),
    /// The closing tag matching the most recent unclosed [`Event::Start`].
    End,
    /// Character data with entity and character references resolved.
    Text(String),
    /// End of the document.
    Eof,
}

/// An opening tag.
///
/// The name is kept lexically (ADR-0003): [`StartElement::name`] is the
/// qualified name as written, while [`StartElement::namespace`] carries the
/// URI resolved at read time. Known elements are matched via
/// [`StartElement::matches`]; unknown elements are preserved using the
/// lexical side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartElement {
    /// The qualified name as written in the document (for example
    /// `"ns1:MPD"`).
    pub(crate) name: String,
    /// The namespace URI the name resolved to, if any.
    pub(crate) namespace: Option<String>,
    /// Attributes as written, in document order, including `xmlns:*`
    /// declarations.
    pub(crate) attributes: Vec<Attribute>,
}

impl StartElement {
    /// The local part of the qualified name.
    pub(crate) fn local_name(&self) -> &str {
        match self.name.split_once(':') {
            Some((_, local_name)) => local_name,
            None => &self.name,
        }
    }

    /// Whether this element resolves to the given namespace URI and local
    /// name.
    pub(crate) fn matches(&self, namespace_uri: &str, local_name: &str) -> bool {
        self.namespace.as_deref() == Some(namespace_uri) && self.local_name() == local_name
    }
}

/// An attribute of a [`StartElement`].
///
/// Names are lexical (qualified name as written); values have entity and
/// character references resolved and whitespace normalized per the XML
/// attribute-value rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Attribute {
    /// The qualified name as written in the document.
    pub(crate) name: String,
    /// The normalized attribute value.
    pub(crate) value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mpd_start(name: &str, namespace: Option<&str>) -> StartElement {
        StartElement {
            name: name.to_string(),
            namespace: namespace.map(str::to_string),
            attributes: Vec::new(),
        }
    }

    #[test]
    fn local_name_strips_prefix() {
        assert_eq!(mpd_start("ns1:MPD", None).local_name(), "MPD");
        assert_eq!(mpd_start("MPD", None).local_name(), "MPD");
    }

    #[test]
    fn matches_requires_resolved_namespace_and_local_name() {
        let namespace = "urn:mpeg:dash:schema:mpd:2011";
        assert!(mpd_start("ns1:MPD", Some(namespace)).matches(namespace, "MPD"));
        assert!(mpd_start("MPD", Some(namespace)).matches(namespace, "MPD"));
        assert!(!mpd_start("MPD", None).matches(namespace, "MPD"));
        assert!(!mpd_start("ns1:Period", Some(namespace)).matches(namespace, "MPD"));
    }
}
