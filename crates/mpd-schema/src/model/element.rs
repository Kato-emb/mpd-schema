//! Generic element tree that preserves unknown nodes.

/// An XML element with no schema definition, preserved as written.
///
/// Every model struct carries a catch-all field of this type so that unknown
/// content (DRM elements such as `cenc:pssh`, future schema additions, ...)
/// survives a parse/serialize roundtrip. Names are kept lexically;
/// serialization writes [`Element::name`] back as-is, which stays consistent
/// because `xmlns:*` declarations are preserved in [`Element::attributes`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Element {
    /// The qualified name as written in the document (for example
    /// `"cenc:pssh"`).
    pub name: String,
    /// The namespace URI resolved at parse time.
    ///
    /// Read-only convenience for consumers (for example DRM scheme
    /// detection); serialization ignores it and writes the lexical
    /// [`Element::name`]. Setting or mutating this field therefore has no
    /// effect on output: code that builds unknown nodes by hand is
    /// responsible for keeping the prefix in [`Element::name`] and the
    /// `xmlns:*` declarations in [`Element::attributes`] consistent
    /// (ADR-0003).
    pub namespace: Option<String>,
    /// Attributes as written, in document order, including `xmlns:*`
    /// declarations.
    pub attributes: Vec<(String, String)>,
    /// Child nodes in document order.
    pub children: Vec<Node>,
}

impl Element {
    /// Creates an element with the given qualified name and no namespace,
    /// attributes, or children.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: None,
            attributes: Vec::new(),
            children: Vec::new(),
        }
    }
}

/// A node in an unknown-content tree.
///
/// CDATA sections are normalized to [`Node::Text`]; comments and processing
/// instructions are discarded at parse time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Node {
    /// A child element.
    Element(Element),
    /// Character data.
    Text(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_an_empty_element() {
        let element = Element::new("cenc:pssh");
        assert_eq!(element.name, "cenc:pssh");
        assert_eq!(element.namespace, None);
        assert!(element.attributes.is_empty());
        assert!(element.children.is_empty());
    }

    #[test]
    fn tree_can_be_built_by_mutation() {
        let mut pssh = Element::new("cenc:pssh");
        pssh.namespace = Some("urn:mpeg:cenc:2013".to_string());
        pssh.children.push(Node::Text("AAAA".to_string()));

        let mut protection = Element::new("ContentProtection");
        protection
            .attributes
            .push(("xmlns:cenc".to_string(), "urn:mpeg:cenc:2013".to_string()));
        protection.children.push(Node::Element(pssh));

        assert_eq!(protection.children.len(), 1);
        match protection.children.first() {
            Some(Node::Element(child)) => {
                assert_eq!(child.children, vec![Node::Text("AAAA".to_string())]);
            }
            other => panic!("unexpected child: {other:?}"),
        }
    }
}
