//! Descriptor types: `Descriptor`, `ContentProtection`, and related elements.
//!
//! The XSD defines `DescriptorType` as a base type with attributes
//! `schemeIdUri` (required), `value`, and `id`, plus `xs:any` children and
//! `xs:anyAttribute` to preserve unknown content. `ContentProtectionType`
//! extends `DescriptorType` with attributes `robustness`, `refId`, and `ref`
//! (ADR-0002: extension represented by an embedded base struct, not `Deref`).

use crate::model::element::Element;

/// A `Descriptor` element (XSD `DescriptorType`).
///
/// Used by `EssentialProperty`, `SupplementalProperty`, `Accessibility`,
/// `Role`, `Rating`, `Viewpoint`, `FramePacking`, `AudioChannelConfiguration`,
/// `OutputProtection`, `UTCTiming`, and `AssetIdentifier` elements, each of
/// which are instantiated as `Descriptor` in the model.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Descriptor {
    /// The required `schemeIdUri` attribute.
    pub scheme_id_uri: String,
    /// The `value` attribute.
    pub value: Option<String>,
    /// The `id` attribute.
    pub id: Option<String>,
    /// Attributes without a typed field, as written.
    pub unknown_attributes: Vec<(String, String)>,
    /// Child elements without a typed field, re-serialized after the known
    /// children with their relative order preserved.
    pub unknown_children: Vec<Element>,
}

impl Descriptor {
    /// Creates a descriptor with the required `schemeIdUri`; every other field
    /// starts empty.
    pub fn new(scheme_id_uri: impl Into<String>) -> Self {
        Self {
            scheme_id_uri: scheme_id_uri.into(),
            value: None,
            id: None,
            unknown_attributes: Vec::new(),
            unknown_children: Vec::new(),
        }
    }
}

/// A `ContentProtection` element (XSD `ContentProtectionType`).
///
/// Extends `Descriptor` with attributes `robustness`, `refId`, and `ref`.
/// The XSD uses `xs:extension`; the model embeds the base struct
/// (ADR-0002).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ContentProtection {
    /// The embedded `DescriptorType` part, which carries `schemeIdUri`
    /// (required), `value`, `id`, and catch-all fields.
    pub base: Descriptor,
    /// The `robustness` attribute.
    pub robustness: Option<String>,
    /// The `refId` attribute.
    pub ref_id: Option<String>,
    /// The `ref` attribute.
    pub r#ref: Option<String>,
}

impl ContentProtection {
    /// Creates a content protection with the required `schemeIdUri` (via the
    /// base); every other field starts empty.
    pub fn new(scheme_id_uri: impl Into<String>) -> Self {
        Self {
            base: Descriptor::new(scheme_id_uri),
            robustness: None,
            ref_id: None,
            r#ref: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_new_sets_required_scheme_id_uri() {
        let descriptor = Descriptor::new("urn:mpeg:dash:test");
        assert_eq!(descriptor.scheme_id_uri, "urn:mpeg:dash:test");
        assert_eq!(descriptor.value, None);
        assert_eq!(descriptor.id, None);
        assert!(descriptor.unknown_attributes.is_empty());
        assert!(descriptor.unknown_children.is_empty());
    }

    #[test]
    fn content_protection_new_sets_required_scheme_id_uri() {
        let cp = ContentProtection::new("urn:uuid:12345678-1234-1234-1234-123456789012");
        assert_eq!(
            cp.base.scheme_id_uri,
            "urn:uuid:12345678-1234-1234-1234-123456789012"
        );
        assert_eq!(cp.robustness, None);
        assert_eq!(cp.ref_id, None);
        assert_eq!(cp.r#ref, None);
    }
}
