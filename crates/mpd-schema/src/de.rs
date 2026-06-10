//! Event-to-struct deserialization.
//!
//! The deserializer pulls events from [`crate::backend::Reader`], matches
//! known elements by (namespace URI, local name), and captures everything
//! else into the generic [`Element`] tree with lexical names (ADR-0003). A
//! stack of (element name, sibling index) pairs is maintained while
//! descending so that errors carry a document path such as
//! `MPD > Period[0] @ start`.

use crate::backend::Reader;
use crate::error::{Error, ErrorKind, Result};
use crate::event::{Attribute, Event, StartElement};
use crate::model::element::{Element, Node};
use crate::model::mpd::{
    AdaptationSet, MPD_NAMESPACE, Mpd, Period, PresentationType, Representation, RepresentationBase,
};
use crate::model::types::{XsDateTime, XsDuration, invalid_value, parse_unsigned_digits};

/// 未知サブツリーは再帰で読むが、スキーマと違って深さに上限がないため、
/// 信頼できない入力によるスタック溢れを防ぐ上限を設ける。実在の MPD の
/// 未知ツリーは高々数段。
const MAX_UNKNOWN_ELEMENT_DEPTH: usize = 256;

pub(crate) fn mpd_from_slice(input: &[u8]) -> Result<Mpd> {
    let mut deserializer = Deserializer {
        reader: Reader::new(input)?,
        path: Vec::new(),
    };
    deserializer.parse_document()
}

struct Deserializer<'input> {
    reader: Reader<'input>,
    path: Vec<PathSegment>,
}

struct PathSegment {
    element_name: &'static str,
    /// The position among same-named siblings; present only for elements
    /// that may repeat.
    sibling_index: Option<usize>,
}

impl Deserializer<'_> {
    fn parse_document(&mut self) -> Result<Mpd> {
        let root = loop {
            match self.reader.read_event()? {
                Event::Start(start) => break start,
                Event::Text(text) if is_xml_whitespace(&text) => {}
                Event::Text(_) => return Err(Error::new(ErrorKind::UnexpectedText)),
                Event::End => {
                    return Err(Error::new(ErrorKind::Xml(
                        "end tag before any start tag".to_string(),
                    )));
                }
                Event::Eof => {
                    return Err(Error::new(ErrorKind::Xml(
                        "the document has no root element".to_string(),
                    )));
                }
            }
        };
        if !root.matches(MPD_NAMESPACE, "MPD") {
            return Err(Error::new(ErrorKind::UnexpectedElement { name: root.name }));
        }

        self.path.push(PathSegment {
            element_name: "MPD",
            sibling_index: None,
        });
        let mpd = self.parse_mpd(root)?;
        self.path.pop();

        loop {
            match self.reader.read_event()? {
                Event::Eof => return Ok(mpd),
                Event::Text(text) if is_xml_whitespace(&text) => {}
                Event::Text(_) => return Err(Error::new(ErrorKind::UnexpectedText)),
                Event::Start(start) => {
                    return Err(Error::new(ErrorKind::UnexpectedElement {
                        name: start.name,
                    }));
                }
                Event::End => {
                    return Err(Error::new(ErrorKind::Xml(
                        "end tag after the root element".to_string(),
                    )));
                }
            }
        }
    }

    fn parse_mpd(&mut self, start: StartElement) -> Result<Mpd> {
        let mut id: Option<String> = None;
        let mut profiles: Option<String> = None;
        let mut presentation_type: Option<PresentationType> = None;
        let mut availability_start_time: Option<XsDateTime> = None;
        let mut availability_end_time: Option<XsDateTime> = None;
        let mut publish_time: Option<XsDateTime> = None;
        let mut media_presentation_duration: Option<XsDuration> = None;
        let mut minimum_update_period: Option<XsDuration> = None;
        let mut min_buffer_time: Option<XsDuration> = None;
        let mut time_shift_buffer_depth: Option<XsDuration> = None;
        let mut suggested_presentation_delay: Option<XsDuration> = None;
        let mut max_segment_duration: Option<XsDuration> = None;
        let mut max_subsegment_duration: Option<XsDuration> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();

        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => id = Some(attribute.value),
                "profiles" => profiles = Some(attribute.value),
                "type" => {
                    presentation_type = Some(self.parse_attribute("type", &attribute.value)?);
                }
                "availabilityStartTime" => {
                    availability_start_time =
                        Some(self.parse_attribute("availabilityStartTime", &attribute.value)?);
                }
                "availabilityEndTime" => {
                    availability_end_time =
                        Some(self.parse_attribute("availabilityEndTime", &attribute.value)?);
                }
                "publishTime" => {
                    publish_time = Some(self.parse_attribute("publishTime", &attribute.value)?);
                }
                "mediaPresentationDuration" => {
                    media_presentation_duration =
                        Some(self.parse_attribute("mediaPresentationDuration", &attribute.value)?);
                }
                "minimumUpdatePeriod" => {
                    minimum_update_period =
                        Some(self.parse_attribute("minimumUpdatePeriod", &attribute.value)?);
                }
                "minBufferTime" => {
                    min_buffer_time =
                        Some(self.parse_attribute("minBufferTime", &attribute.value)?);
                }
                "timeShiftBufferDepth" => {
                    time_shift_buffer_depth =
                        Some(self.parse_attribute("timeShiftBufferDepth", &attribute.value)?);
                }
                "suggestedPresentationDelay" => {
                    suggested_presentation_delay =
                        Some(self.parse_attribute("suggestedPresentationDelay", &attribute.value)?);
                }
                "maxSegmentDuration" => {
                    max_segment_duration =
                        Some(self.parse_attribute("maxSegmentDuration", &attribute.value)?);
                }
                "maxSubsegmentDuration" => {
                    max_subsegment_duration =
                        Some(self.parse_attribute("maxSubsegmentDuration", &attribute.value)?);
                }
                // 既定名前空間宣言は保持しない。シリアライザがルート MPD に
                // MPD_NAMESPACE の宣言を再付与するため、保持すると重複した
                // 宣言を出力してしまう。
                "xmlns" => {}
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut mpd = Mpd::new(
            profiles.ok_or_else(|| self.missing_attribute("profiles"))?,
            min_buffer_time.ok_or_else(|| self.missing_attribute("minBufferTime"))?,
        );
        mpd.id = id;
        mpd.presentation_type = presentation_type;
        mpd.availability_start_time = availability_start_time;
        mpd.availability_end_time = availability_end_time;
        mpd.publish_time = publish_time;
        mpd.media_presentation_duration = media_presentation_duration;
        mpd.minimum_update_period = minimum_update_period;
        mpd.time_shift_buffer_depth = time_shift_buffer_depth;
        mpd.suggested_presentation_delay = suggested_presentation_delay;
        mpd.max_segment_duration = max_segment_duration;
        mpd.max_subsegment_duration = max_subsegment_duration;
        mpd.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Period") {
                self.path.push(PathSegment {
                    element_name: "Period",
                    sibling_index: Some(mpd.periods.len()),
                });
                let period = self.parse_period(child)?;
                self.path.pop();
                mpd.periods.push(period);
            } else {
                mpd.unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(mpd)
    }

    fn parse_period(&mut self, start: StartElement) -> Result<Period> {
        let mut period = Period::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => period.id = Some(attribute.value),
                "start" => period.start = Some(self.parse_attribute("start", &attribute.value)?),
                "duration" => {
                    period.duration = Some(self.parse_attribute("duration", &attribute.value)?);
                }
                "bitstreamSwitching" => {
                    period.bitstream_switching =
                        Some(self.in_attribute(
                            "bitstreamSwitching",
                            parse_xs_boolean(&attribute.value),
                        )?);
                }
                "xmlns" => {}
                _ => period
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "AdaptationSet") {
                self.path.push(PathSegment {
                    element_name: "AdaptationSet",
                    sibling_index: Some(period.adaptation_sets.len()),
                });
                let adaptation_set = self.parse_adaptation_set(child)?;
                self.path.pop();
                period.adaptation_sets.push(adaptation_set);
            } else {
                period
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(period)
    }

    fn parse_adaptation_set(&mut self, start: StartElement) -> Result<AdaptationSet> {
        let mut adaptation_set = AdaptationSet::new();
        for attribute in start.attributes {
            let Some(attribute) =
                self.apply_representation_base_attribute(&mut adaptation_set.base, attribute)?
            else {
                continue;
            };
            match attribute.name.as_str() {
                "id" => {
                    adaptation_set.id =
                        Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "group" => {
                    adaptation_set.group =
                        Some(self.in_attribute("group", parse_xs_unsigned_int(&attribute.value))?);
                }
                "lang" => adaptation_set.lang = Some(attribute.value),
                "contentType" => {
                    adaptation_set.content_type =
                        Some(self.parse_attribute("contentType", &attribute.value)?);
                }
                "par" => adaptation_set.par = Some(self.parse_attribute("par", &attribute.value)?),
                "minBandwidth" => {
                    adaptation_set.min_bandwidth = Some(
                        self.in_attribute("minBandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "maxBandwidth" => {
                    adaptation_set.max_bandwidth = Some(
                        self.in_attribute("maxBandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "minWidth" => {
                    adaptation_set.min_width = Some(
                        self.in_attribute("minWidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "maxWidth" => {
                    adaptation_set.max_width = Some(
                        self.in_attribute("maxWidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "minHeight" => {
                    adaptation_set.min_height = Some(
                        self.in_attribute("minHeight", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "maxHeight" => {
                    adaptation_set.max_height = Some(
                        self.in_attribute("maxHeight", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "minFrameRate" => {
                    adaptation_set.min_frame_rate =
                        Some(self.parse_attribute("minFrameRate", &attribute.value)?);
                }
                "maxFrameRate" => {
                    adaptation_set.max_frame_rate =
                        Some(self.parse_attribute("maxFrameRate", &attribute.value)?);
                }
                "segmentAlignment" => {
                    adaptation_set.segment_alignment = Some(
                        self.in_attribute("segmentAlignment", parse_xs_boolean(&attribute.value))?,
                    );
                }
                "subsegmentAlignment" => {
                    adaptation_set.subsegment_alignment =
                        Some(self.in_attribute(
                            "subsegmentAlignment",
                            parse_xs_boolean(&attribute.value),
                        )?);
                }
                "subsegmentStartsWithSAP" => {
                    adaptation_set.subsegment_starts_with_sap = Some(
                        self.in_attribute("subsegmentStartsWithSAP", parse_sap(&attribute.value))?,
                    );
                }
                "bitstreamSwitching" => {
                    adaptation_set.bitstream_switching =
                        Some(self.in_attribute(
                            "bitstreamSwitching",
                            parse_xs_boolean(&attribute.value),
                        )?);
                }
                "initializationSetRef" => {
                    adaptation_set.initialization_set_ref = self.in_attribute(
                        "initializationSetRef",
                        parse_uint_vector(&attribute.value),
                    )?;
                }
                "initializationPrincipal" => {
                    adaptation_set.initialization_principal = Some(attribute.value);
                }
                "xmlns" => {}
                _ => adaptation_set
                    .base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Representation") {
                self.path.push(PathSegment {
                    element_name: "Representation",
                    sibling_index: Some(adaptation_set.representations.len()),
                });
                let representation = self.parse_representation(child)?;
                self.path.pop();
                adaptation_set.representations.push(representation);
            } else {
                adaptation_set
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(adaptation_set)
    }

    fn parse_representation(&mut self, start: StartElement) -> Result<Representation> {
        let mut base = RepresentationBase::new();
        let mut id: Option<String> = None;
        let mut bandwidth: Option<u32> = None;
        let mut quality_ranking: Option<u32> = None;
        let mut dependency_id: Vec<String> = Vec::new();
        let mut association_id: Vec<String> = Vec::new();
        let mut association_type: Vec<String> = Vec::new();
        let mut media_stream_structure_id: Vec<String> = Vec::new();

        for attribute in start.attributes {
            let Some(attribute) = self.apply_representation_base_attribute(&mut base, attribute)?
            else {
                continue;
            };
            match attribute.name.as_str() {
                "id" => {
                    id = Some(
                        self.in_attribute("id", parse_string_no_whitespace(&attribute.value))?,
                    );
                }
                "bandwidth" => {
                    bandwidth = Some(
                        self.in_attribute("bandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "qualityRanking" => {
                    quality_ranking =
                        Some(self.in_attribute(
                            "qualityRanking",
                            parse_xs_unsigned_int(&attribute.value),
                        )?);
                }
                "dependencyId" => dependency_id = parse_string_vector(&attribute.value),
                "associationId" => association_id = parse_string_vector(&attribute.value),
                "associationType" => association_type = parse_string_vector(&attribute.value),
                "mediaStreamStructureId" => {
                    media_stream_structure_id = parse_string_vector(&attribute.value);
                }
                "xmlns" => {}
                _ => base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        let mut representation = Representation::new(
            id.ok_or_else(|| self.missing_attribute("id"))?,
            bandwidth.ok_or_else(|| self.missing_attribute("bandwidth"))?,
        );
        representation.base = base;
        representation.quality_ranking = quality_ranking;
        representation.dependency_id = dependency_id;
        representation.association_id = association_id;
        representation.association_type = association_type;
        representation.media_stream_structure_id = media_stream_structure_id;

        while let Some(child) = self.next_content_event()? {
            representation
                .base
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(representation)
    }

    /// Applies one attribute of an element extending `RepresentationBaseType`
    /// to the embedded base, returning the attribute unconsumed when it
    /// belongs to the extending type.
    fn apply_representation_base_attribute(
        &self,
        base: &mut RepresentationBase,
        attribute: Attribute,
    ) -> Result<Option<Attribute>> {
        match attribute.name.as_str() {
            "profiles" => base.profiles = Some(attribute.value),
            "width" => {
                base.width =
                    Some(self.in_attribute("width", parse_xs_unsigned_int(&attribute.value))?);
            }
            "height" => {
                base.height =
                    Some(self.in_attribute("height", parse_xs_unsigned_int(&attribute.value))?);
            }
            "sar" => base.sar = Some(self.parse_attribute("sar", &attribute.value)?),
            "frameRate" => {
                base.frame_rate = Some(self.parse_attribute("frameRate", &attribute.value)?);
            }
            "audioSamplingRate" => {
                base.audio_sampling_rate = self.in_attribute(
                    "audioSamplingRate",
                    parse_audio_sampling_rate(&attribute.value),
                )?;
            }
            "mimeType" => base.mime_type = Some(attribute.value),
            "segmentProfiles" => base.segment_profiles = parse_string_vector(&attribute.value),
            "codecs" => base.codecs = Some(attribute.value),
            "containerProfiles" => base.container_profiles = parse_string_vector(&attribute.value),
            "maximumSAPPeriod" => {
                base.maximum_sap_period =
                    Some(self.in_attribute("maximumSAPPeriod", parse_xs_double(&attribute.value))?);
            }
            "startWithSAP" => {
                base.start_with_sap =
                    Some(self.in_attribute("startWithSAP", parse_sap(&attribute.value))?);
            }
            "maxPlayoutRate" => {
                base.max_playout_rate =
                    Some(self.in_attribute("maxPlayoutRate", parse_xs_double(&attribute.value))?);
            }
            "codingDependency" => {
                base.coding_dependency = Some(
                    self.in_attribute("codingDependency", parse_xs_boolean(&attribute.value))?,
                );
            }
            "scanType" => {
                base.scan_type = Some(self.parse_attribute("scanType", &attribute.value)?)
            }
            "selectionPriority" => {
                base.selection_priority =
                    Some(self.in_attribute(
                        "selectionPriority",
                        parse_xs_unsigned_int(&attribute.value),
                    )?);
            }
            "tag" => base.tag = Some(attribute.value),
            _ => return Ok(Some(attribute)),
        }
        Ok(None)
    }

    /// Returns the next child element start, or `None` at the end of the
    /// enclosing element. Whitespace between elements is skipped; any other
    /// character data is rejected because the known elements all have
    /// element-only content.
    fn next_content_event(&mut self) -> Result<Option<StartElement>> {
        loop {
            match self.reader.read_event()? {
                Event::Start(start) => return Ok(Some(start)),
                Event::End => return Ok(None),
                Event::Text(text) if is_xml_whitespace(&text) => {}
                Event::Text(_) => return Err(self.element_error(ErrorKind::UnexpectedText)),
                Event::Eof => {
                    return Err(self
                        .element_error(ErrorKind::Xml("unexpected end of document".to_string())));
                }
            }
        }
    }

    fn parse_unknown_element(&mut self, start: StartElement, depth: usize) -> Result<Element> {
        if depth >= MAX_UNKNOWN_ELEMENT_DEPTH {
            return Err(self.element_error(ErrorKind::Xml(format!(
                "unknown elements nested deeper than {MAX_UNKNOWN_ELEMENT_DEPTH}"
            ))));
        }
        let StartElement {
            name,
            namespace,
            attributes,
        } = start;
        let mut element = Element::new(name);
        element.namespace = namespace;
        element.attributes = attributes
            .into_iter()
            .map(|attribute| (attribute.name, attribute.value))
            .collect();
        loop {
            match self.reader.read_event()? {
                Event::Start(child) => {
                    let child = self.parse_unknown_element(child, depth.saturating_add(1))?;
                    element.children.push(Node::Element(child));
                }
                Event::Text(text) if is_xml_whitespace(&text) => {}
                Event::Text(text) => element.children.push(Node::Text(text)),
                Event::End => return Ok(element),
                Event::Eof => {
                    return Err(self
                        .element_error(ErrorKind::Xml("unexpected end of document".to_string())));
                }
            }
        }
    }

    fn path_string(&self) -> String {
        let mut output = String::new();
        for (position, segment) in self.path.iter().enumerate() {
            if position > 0 {
                output.push_str(" > ");
            }
            output.push_str(segment.element_name);
            if let Some(index) = segment.sibling_index {
                output.push('[');
                output.push_str(&index.to_string());
                output.push(']');
            }
        }
        output
    }

    fn element_error(&self, kind: ErrorKind) -> Error {
        let mut error = Error::new(kind);
        error.path = self.path_string();
        error
    }

    fn parse_attribute<T>(&self, attribute_name: &str, value: &str) -> Result<T>
    where
        T: std::str::FromStr<Err = Error>,
    {
        self.in_attribute(attribute_name, value.parse())
    }

    fn in_attribute<T>(&self, attribute_name: &str, result: Result<T>) -> Result<T> {
        result.map_err(|mut error| {
            error.path = format!("{} @ {attribute_name}", self.path_string());
            error
        })
    }

    fn missing_attribute(&self, attribute_name: &str) -> Error {
        let mut error = Error::new(ErrorKind::MissingAttribute);
        error.path = format!("{} @ {attribute_name}", self.path_string());
        error
    }
}

fn is_xml_whitespace(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
}

fn parse_xs_boolean(value: &str) -> Result<bool> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(invalid_value(
            value,
            "an `xs:boolean` (`true`, `false`, `1`, or `0`)",
        )),
    }
}

fn parse_xs_unsigned_int(value: &str) -> Result<u32> {
    let digits = value.strip_prefix('+').unwrap_or(value);
    parse_unsigned_digits(digits).ok_or_else(|| invalid_value(value, "an `xs:unsignedInt`"))
}

fn parse_sap(value: &str) -> Result<u32> {
    let parsed = parse_xs_unsigned_int(value)?;
    if parsed <= 6 {
        Ok(parsed)
    } else {
        Err(invalid_value(value, "a SAP type in the range 0..=6"))
    }
}

fn parse_xs_double(value: &str) -> Result<f64> {
    const EXPECTED: &str = "an `xs:double`";
    match value {
        "INF" => Ok(f64::INFINITY),
        "-INF" => Ok(f64::NEG_INFINITY),
        "NaN" => Ok(f64::NAN),
        _ => {
            // Rust の float パーサは `inf` や `nan` も受理するため、
            // xs:double の字句空間に現れる文字だけを通す。
            if value
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
            {
                value.parse().map_err(|_| invalid_value(value, EXPECTED))
            } else {
                Err(invalid_value(value, EXPECTED))
            }
        }
    }
}

fn parse_uint_vector(value: &str) -> Result<Vec<u32>> {
    value
        .split_ascii_whitespace()
        .map(parse_xs_unsigned_int)
        .collect()
}

fn parse_string_vector(value: &str) -> Vec<String> {
    value.split_ascii_whitespace().map(str::to_string).collect()
}

fn parse_audio_sampling_rate(value: &str) -> Result<Vec<u32>> {
    let rates = parse_uint_vector(value)?;
    if (1..=2).contains(&rates.len()) {
        Ok(rates)
    } else {
        Err(invalid_value(value, "one or two unsigned integers"))
    }
}

fn parse_string_no_whitespace(value: &str) -> Result<String> {
    if value.chars().any(char::is_whitespace) {
        Err(invalid_value(value, "a string without whitespace"))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::element::Node;
    use crate::model::mpd::{ContentType, VideoScan};
    use crate::model::types::FrameRate;

    const MINIMAL: &str = concat!(
        r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
        r#"profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" minBufferTime="PT2S">"#,
        "<Period/>",
        "</MPD>",
    );

    #[test]
    fn minimal_mpd_parses() {
        let mpd = mpd_from_slice(MINIMAL.as_bytes()).unwrap();
        assert_eq!(mpd.profiles, "urn:mpeg:dash:profile:isoff-on-demand:2011");
        assert_eq!(mpd.min_buffer_time, "PT2S".parse().unwrap());
        assert_eq!(mpd.periods.len(), 1);
        assert!(mpd.unknown_attributes.is_empty());
        assert!(mpd.unknown_children.is_empty());
    }

    #[test]
    fn typed_attributes_parse_along_the_spine() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT1.5S" "#,
            r#"type="dynamic" availabilityStartTime="2026-06-10T00:00:00Z" "#,
            r#"mediaPresentationDuration="PT30M">"#,
            r#"<Period id="p0" start="PT0S" bitstreamSwitching="1">"#,
            r#"<AdaptationSet id="1" contentType="video" par="16:9" maxFrameRate="30000/1001" "#,
            r#"segmentAlignment="true" mimeType="video/mp4">"#,
            r#"<Representation id="v0" bandwidth="4800000" width="1920" height="1080" "#,
            r#"frameRate="25" sar="1:1" codecs="avc1.640028" audioSamplingRate="44100 48000" "#,
            r#"scanType="progressive" dependencyId="a b" startWithSAP="1"/>"#,
            "</AdaptationSet>",
            "</Period>",
            "</MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        assert_eq!(mpd.presentation_type, Some(PresentationType::Dynamic));
        assert_eq!(
            mpd.availability_start_time,
            Some("2026-06-10T00:00:00Z".parse().unwrap())
        );
        assert_eq!(
            mpd.media_presentation_duration,
            Some("PT30M".parse().unwrap())
        );

        let period = mpd.periods.first().unwrap();
        assert_eq!(period.id.as_deref(), Some("p0"));
        assert_eq!(period.start, Some("PT0S".parse().unwrap()));
        assert_eq!(period.bitstream_switching, Some(true));

        let adaptation_set = period.adaptation_sets.first().unwrap();
        assert_eq!(adaptation_set.id, Some(1));
        assert_eq!(adaptation_set.content_type, Some(ContentType::Video));
        assert_eq!(adaptation_set.par, Some("16:9".parse().unwrap()));
        assert_eq!(
            adaptation_set.max_frame_rate,
            Some("30000/1001".parse().unwrap())
        );
        assert_eq!(adaptation_set.segment_alignment, Some(true));
        assert_eq!(adaptation_set.base.mime_type.as_deref(), Some("video/mp4"));

        let representation = adaptation_set.representations.first().unwrap();
        assert_eq!(representation.id, "v0");
        assert_eq!(representation.bandwidth, 4_800_000);
        assert_eq!(representation.base.width, Some(1920));
        assert_eq!(representation.base.height, Some(1080));
        assert_eq!(representation.base.frame_rate, Some(FrameRate::new(25)));
        assert_eq!(representation.base.sar, Some("1:1".parse().unwrap()));
        assert_eq!(representation.base.codecs.as_deref(), Some("avc1.640028"));
        assert_eq!(
            representation.base.audio_sampling_rate,
            vec![44_100, 48_000]
        );
        assert_eq!(representation.base.scan_type, Some(VideoScan::Progressive));
        assert_eq!(representation.base.start_with_sap, Some(1));
        assert_eq!(
            representation.dependency_id,
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn missing_required_attribute_reports_the_path() {
        let input = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" minBufferTime="PT2S"/>"#;
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::MissingAttribute));
        assert_eq!(error.path, "MPD @ profiles");

        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><AdaptationSet>",
            r#"<Representation id="v0"/>"#,
            "</AdaptationSet></Period></MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::MissingAttribute));
        assert_eq!(
            error.path,
            "MPD > Period[0] > AdaptationSet[0] > Representation[0] @ bandwidth"
        );
    }

    #[test]
    fn invalid_attribute_value_reports_the_path_with_sibling_index() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period/>",
            r#"<Period start="oops"/>"#,
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::InvalidValue { .. }));
        assert_eq!(error.path, "MPD > Period[1] @ start");
    }

    #[test]
    fn prefixed_namespace_matches_known_elements() {
        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S">"#,
            "<ns1:Period/>",
            "</ns1:MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        assert_eq!(mpd.periods.len(), 1);
        assert_eq!(
            mpd.unknown_attributes,
            vec![(
                "xmlns:ns1".to_string(),
                "urn:mpeg:dash:schema:mpd:2011".to_string()
            )]
        );
    }

    #[test]
    fn unknown_elements_and_attributes_are_captured() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
            r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" "#,
            r#"xsi:schemaLocation="urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd" "#,
            r#"profiles="p" minBufferTime="PT2S">"#,
            "<ProgramInformation><Title>demo</Title></ProgramInformation>",
            "<Period>",
            r#"<AdaptationSet><ContentProtection xmlns:cenc="urn:mpeg:cenc:2013" "#,
            r#"schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc">"#,
            "<cenc:pssh>AAAA</cenc:pssh>",
            "</ContentProtection></AdaptationSet>",
            "</Period>",
            "</MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        assert_eq!(
            mpd.unknown_attributes,
            vec![
                (
                    "xmlns:xsi".to_string(),
                    "http://www.w3.org/2001/XMLSchema-instance".to_string()
                ),
                (
                    "xsi:schemaLocation".to_string(),
                    "urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd".to_string()
                ),
            ]
        );

        match mpd.unknown_children.as_slice() {
            [program_information] => {
                assert_eq!(program_information.name, "ProgramInformation");
                assert_eq!(
                    program_information.namespace.as_deref(),
                    Some("urn:mpeg:dash:schema:mpd:2011")
                );
                match program_information.children.as_slice() {
                    [Node::Element(title)] => {
                        assert_eq!(title.name, "Title");
                        assert_eq!(title.children, vec![Node::Text("demo".to_string())]);
                    }
                    other => panic!("unexpected children: {other:?}"),
                }
            }
            other => panic!("unexpected unknown children: {other:?}"),
        }

        let adaptation_set = mpd
            .periods
            .first()
            .unwrap()
            .adaptation_sets
            .first()
            .unwrap();
        match adaptation_set.base.unknown_children.as_slice() {
            [content_protection] => {
                assert_eq!(content_protection.name, "ContentProtection");
                match content_protection.children.as_slice() {
                    [Node::Element(pssh)] => {
                        assert_eq!(pssh.name, "cenc:pssh");
                        assert_eq!(pssh.namespace.as_deref(), Some("urn:mpeg:cenc:2013"));
                        assert_eq!(pssh.children, vec![Node::Text("AAAA".to_string())]);
                    }
                    other => panic!("unexpected children: {other:?}"),
                }
            }
            other => panic!("unexpected unknown children: {other:?}"),
        }
    }

    #[test]
    fn wrong_root_element_is_rejected() {
        let input = r#"<Patch xmlns="urn:mpeg:dash:schema:mpd:2011"/>"#;
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::UnexpectedElement { ref name } if name == "Patch"
        ));

        let input = "<MPD/>";
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::UnexpectedElement { .. }));
    }

    #[test]
    fn character_data_in_element_only_content_is_rejected() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "stray text",
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::UnexpectedText));
        assert_eq!(error.path, "MPD");
    }

    #[test]
    fn boolean_accepts_numeric_form_and_rejects_other_spellings() {
        let template = |value: &str| {
            format!(
                concat!(
                    r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
                    r#"minBufferTime="PT2S"><Period bitstreamSwitching="{}"/></MPD>"#,
                ),
                value
            )
        };
        for (value, expected) in [("1", true), ("0", false), ("true", true), ("false", false)] {
            let mpd = mpd_from_slice(template(value).as_bytes()).unwrap();
            assert_eq!(
                mpd.periods.first().unwrap().bitstream_switching,
                Some(expected)
            );
        }
        let error = mpd_from_slice(template("TRUE").as_bytes()).unwrap_err();
        assert_eq!(error.path, "MPD > Period[0] @ bitstreamSwitching");
    }

    #[test]
    fn sap_and_audio_sampling_rate_ranges_are_enforced() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<Period><AdaptationSet subsegmentStartsWithSAP="7"/></Period>"#,
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert_eq!(
            error.path,
            "MPD > Period[0] > AdaptationSet[0] @ subsegmentStartsWithSAP"
        );

        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<Period><AdaptationSet audioSamplingRate="1 2 3"/></Period>"#,
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::InvalidValue { .. }));
    }

    #[test]
    fn content_after_the_root_element_is_rejected() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period/></MPD><MPD/>",
        );
        assert!(mpd_from_slice(input.as_bytes()).is_err());
    }
}
