//! quick-xml adapter (ADR-0007).
//!
//! Converts between raw XML and the crate-internal [`Event`] vocabulary.
//! quick-xml types must not leak out of this file.

use std::collections::VecDeque;
use std::io;

use quick_xml::NsReader;
use quick_xml::XmlVersion;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event as XmlEvent};
use quick_xml::name::ResolveResult;

use crate::error::{Error, ErrorKind, Result};
use crate::event::{Attribute, Event, StartElement};

/// Pull reader producing [`Event`]s from a complete UTF-8 document.
#[derive(Debug)]
pub(crate) struct Reader<'input> {
    inner: NsReader<&'input [u8]>,
    /// Events synthesized ahead of time: the `End` of an expanded empty
    /// element tag, and the event that followed a coalesced text run.
    queue: VecDeque<Event>,
}

impl<'input> Reader<'input> {
    /// Creates a reader over the whole document.
    ///
    /// Rejects input that is not valid UTF-8; a non-UTF-8 encoding declared
    /// in the XML declaration is rejected later by [`Reader::read_event`].
    pub(crate) fn new(input: &'input [u8]) -> Result<Self> {
        if std::str::from_utf8(input).is_err() {
            return Err(Error::new(ErrorKind::Encoding(
                "input is not valid UTF-8".to_string(),
            )));
        }
        Ok(Self {
            inner: NsReader::from_reader(input),
            queue: VecDeque::new(),
        })
    }

    /// Returns the next event, or [`Event::Eof`] once the document ends.
    pub(crate) fn read_event(&mut self) -> Result<Event> {
        if let Some(event) = self.queue.pop_front() {
            return Ok(event);
        }
        let mut text = String::new();
        loop {
            let (resolution, xml_event) = self.inner.read_resolved_event().map_err(xml_error)?;
            match xml_event {
                XmlEvent::Start(start) => {
                    let event = Event::Start(convert_start(resolution, &start)?);
                    return Ok(self.flush_text(text, event));
                }
                XmlEvent::Empty(start) => {
                    let start_event = Event::Start(convert_start(resolution, &start)?);
                    let event = self.flush_text(text, start_event);
                    self.queue.push_back(Event::End);
                    return Ok(event);
                }
                XmlEvent::End(_) => return Ok(self.flush_text(text, Event::End)),
                XmlEvent::Text(content) => {
                    text.push_str(&content.xml10_content().map_err(xml_error)?);
                }
                XmlEvent::CData(content) => {
                    text.push_str(&content.xml10_content().map_err(xml_error)?);
                }
                XmlEvent::GeneralRef(reference) => {
                    if let Some(character) = reference.resolve_char_ref().map_err(xml_error)? {
                        text.push(character);
                    } else {
                        let name = reference.decode().map_err(xml_error)?;
                        match resolve_predefined_entity(&name) {
                            Some(replacement) => text.push_str(replacement),
                            None => {
                                return Err(Error::new(ErrorKind::Xml(format!(
                                    "unresolved entity reference `&{name};`"
                                ))));
                            }
                        }
                    }
                }
                XmlEvent::Decl(declaration) => check_declared_encoding(&declaration)?,
                XmlEvent::Comment(_) | XmlEvent::PI(_) | XmlEvent::DocType(_) => {}
                XmlEvent::Eof => return Ok(self.flush_text(text, Event::Eof)),
            }
        }
    }

    /// Returns the accumulated text run if there is one, queueing `event`
    /// behind it; otherwise returns `event` directly.
    fn flush_text(&mut self, text: String, event: Event) -> Event {
        if text.is_empty() {
            event
        } else {
            self.queue.push_back(event);
            Event::Text(text)
        }
    }
}

fn convert_start(resolution: ResolveResult<'_>, start: &BytesStart<'_>) -> Result<StartElement> {
    let name = utf8(start.name().into_inner())?.to_string();
    let namespace = match resolution {
        ResolveResult::Bound(namespace) => Some(utf8(namespace.into_inner())?.to_string()),
        ResolveResult::Unbound => None,
        ResolveResult::Unknown(prefix) => {
            return Err(Error::new(ErrorKind::Xml(format!(
                "undeclared namespace prefix `{}`",
                String::from_utf8_lossy(&prefix)
            ))));
        }
    };
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(xml_error)?;
        attributes.push(Attribute {
            name: utf8(attribute.key.into_inner())?.to_string(),
            value: attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(xml_error)?
                .into_owned(),
        });
    }
    Ok(StartElement {
        name,
        namespace,
        attributes,
    })
}

fn check_declared_encoding(declaration: &BytesDecl<'_>) -> Result<()> {
    match declaration.encoding() {
        Some(Ok(encoding)) => {
            if encoding.eq_ignore_ascii_case(b"utf-8") {
                Ok(())
            } else {
                Err(Error::new(ErrorKind::Encoding(format!(
                    "document declares `{}`; only UTF-8 is supported",
                    String::from_utf8_lossy(&encoding)
                ))))
            }
        }
        Some(Err(error)) => Err(xml_error(error)),
        None => Ok(()),
    }
}

fn utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|_| Error::new(ErrorKind::Encoding("input is not valid UTF-8".to_string())))
}

fn xml_error(source: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::Xml(source.to_string()))
}

/// Push writer consuming [`Event`]s and emitting XML to `sink`.
pub(crate) struct Writer<W: io::Write> {
    inner: quick_xml::Writer<W>,
    /// Names of currently open elements, used to emit closing tags because
    /// [`Event::End`] carries no name.
    open_element_names: Vec<String>,
}

impl<W: io::Write> Writer<W> {
    pub(crate) fn new(sink: W) -> Self {
        Self {
            inner: quick_xml::Writer::new(sink),
            open_element_names: Vec::new(),
        }
    }

    /// Creates a writer that indents each element by `spaces` spaces per level.
    pub(crate) fn new_with_indent(sink: W, spaces: usize) -> Self {
        Self {
            inner: quick_xml::Writer::new_with_indent(sink, b' ', spaces),
            open_element_names: Vec::new(),
        }
    }

    /// Writes one event. Text and attribute values are escaped here;
    /// [`Event::Eof`] is a no-op.
    pub(crate) fn write_event(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Start(start) => {
                let mut tag = BytesStart::new(start.name.as_str());
                for attribute in &start.attributes {
                    tag.push_attribute((attribute.name.as_str(), attribute.value.as_str()));
                }
                self.open_element_names.push(start.name.clone());
                self.inner
                    .write_event(XmlEvent::Start(tag))
                    .map_err(io_error)
            }
            Event::End => {
                let name = self.open_element_names.pop().ok_or_else(|| {
                    Error::new(ErrorKind::Xml(
                        "end event without a matching start event".to_string(),
                    ))
                })?;
                self.inner
                    .write_event(XmlEvent::End(BytesEnd::new(name)))
                    .map_err(io_error)
            }
            Event::Text(text) => self
                .inner
                .write_event(XmlEvent::Text(BytesText::new(text)))
                .map_err(io_error),
            Event::Eof => Ok(()),
        }
    }

    /// Consumes the writer, returning the sink.
    pub(crate) fn into_inner(self) -> W {
        self.inner.into_inner()
    }
}

// quick_xml::Writer が Debug を実装しないため derive できない。
impl<W: io::Write> std::fmt::Debug for Writer<W> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Writer")
            .field("open_element_names", &self.open_element_names)
            .finish_non_exhaustive()
    }
}

fn io_error(source: io::Error) -> Error {
    Error::new(ErrorKind::Io(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MPD_NAMESPACE: &str = "urn:mpeg:dash:schema:mpd:2011";

    fn read_all_events(input: &[u8]) -> Vec<Event> {
        let mut reader = Reader::new(input).unwrap();
        let mut events = Vec::new();
        loop {
            let event = reader.read_event().unwrap();
            let done = event == Event::Eof;
            events.push(event);
            if done {
                return events;
            }
        }
    }

    fn write_all_events(events: &[Event]) -> Vec<u8> {
        let mut writer = Writer::new(Vec::new());
        for event in events {
            writer.write_event(event).unwrap();
        }
        writer.into_inner()
    }

    #[test]
    fn events_roundtrip_through_writer() {
        let input = concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" minBufferTime="PT2S">"#,
            "\n  ",
            "<!-- comment is discarded -->",
            r#"<Period id="p&amp;1"/>"#,
            "\n  ",
            "<Title>a &amp; b <![CDATA[< c]]> &#x21;</Title>",
            "\n",
            "</MPD>",
        );
        let original_events = read_all_events(input.as_bytes());
        let output = write_all_events(&original_events);
        let reread_events = read_all_events(&output);
        assert_eq!(original_events, reread_events);
    }

    #[test]
    fn prefixed_mpd_namespace_resolves() {
        let input = r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011"/>"#;
        let events = read_all_events(input.as_bytes());
        match events.as_slice() {
            [Event::Start(start), Event::End, Event::Eof] => {
                assert_eq!(start.name, "ns1:MPD");
                assert_eq!(start.local_name(), "MPD");
                assert_eq!(start.namespace.as_deref(), Some(MPD_NAMESPACE));
                assert!(start.matches(MPD_NAMESPACE, "MPD"));
                assert_eq!(
                    start.attributes,
                    vec![Attribute {
                        name: "xmlns:ns1".to_string(),
                        value: MPD_NAMESPACE.to_string(),
                    }]
                );
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn default_namespace_resolves() {
        let input = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"></MPD>"#;
        let events = read_all_events(input.as_bytes());
        match events.as_slice() {
            [Event::Start(start), Event::End, Event::Eof] => {
                assert_eq!(start.name, "MPD");
                assert!(start.matches(MPD_NAMESPACE, "MPD"));
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn unprefixed_element_without_default_namespace_is_unbound() {
        let events = read_all_events(b"<MPD/>");
        match events.as_slice() {
            [Event::Start(start), Event::End, Event::Eof] => {
                assert_eq!(start.namespace, None);
                assert!(!start.matches(MPD_NAMESPACE, "MPD"));
            }
            other => panic!("unexpected events: {other:?}"),
        }
    }

    #[test]
    fn character_data_pieces_coalesce_into_one_text() {
        let input = "<a>pre &amp; <![CDATA[mid <>]]> &#x70;ost</a>";
        let events = read_all_events(input.as_bytes());
        assert_eq!(
            events,
            vec![
                Event::Start(StartElement {
                    name: "a".to_string(),
                    namespace: None,
                    attributes: Vec::new(),
                }),
                Event::Text("pre & mid <> post".to_string()),
                Event::End,
                Event::Eof,
            ]
        );
    }

    #[test]
    fn comments_and_processing_instructions_are_discarded() {
        let input = "<a><!-- c --><?pi data?></a>";
        let events = read_all_events(input.as_bytes());
        assert!(matches!(
            events.as_slice(),
            [Event::Start(_), Event::End, Event::Eof]
        ));
    }

    #[test]
    fn rejects_non_utf8_input() {
        let error = Reader::new(&[0xff, 0xfe, b'<', b'a', b'/', b'>']).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Encoding(_)));
    }

    #[test]
    fn rejects_declared_non_utf8_encoding() {
        let input = r#"<?xml version="1.0" encoding="ISO-8859-1"?><a/>"#;
        let mut reader = Reader::new(input.as_bytes()).unwrap();
        let error = reader.read_event().unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Encoding(_)));
    }

    #[test]
    fn rejects_undeclared_namespace_prefix() {
        let mut reader = Reader::new(b"<foo:bar/>").unwrap();
        let error = reader.read_event().unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Xml(_)));
    }

    #[test]
    fn rejects_unresolved_entity_reference() {
        let mut reader = Reader::new(b"<a>&undefined;</a>").unwrap();
        reader.read_event().unwrap();
        let error = reader.read_event().unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Xml(_)));
    }

    #[test]
    fn writer_escapes_text_and_attribute_values() {
        let events = vec![
            Event::Start(StartElement {
                name: "Period".to_string(),
                namespace: None,
                attributes: vec![Attribute {
                    name: "id".to_string(),
                    value: r#"a"<b"#.to_string(),
                }],
            }),
            Event::Text("x < y & z".to_string()),
            Event::End,
        ];
        let output = write_all_events(&events);
        let reread_events = read_all_events(&output);
        let mut expected = events.clone();
        expected.push(Event::Eof);
        assert_eq!(reread_events, expected);
    }

    #[test]
    fn writer_rejects_unbalanced_end() {
        let mut writer = Writer::new(Vec::new());
        let error = writer.write_event(&Event::End).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Xml(_)));
    }
}
