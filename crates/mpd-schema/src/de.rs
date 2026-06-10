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
use crate::model::descriptor::{ContentProtection, Descriptor};
use crate::model::element::{Element, Node};
use crate::model::mpd::{
    AdaptationSet, MPD_NAMESPACE, Mpd, Period, PresentationType, Representation, RepresentationBase,
};
use crate::model::segment::{
    FailoverContent, Fcs, MultipleSegmentBase, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, SegmentUrl, Url,
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
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
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
            if child.matches(MPD_NAMESPACE, "ContentProtection") {
                self.path.push(PathSegment {
                    element_name: "ContentProtection",
                    sibling_index: Some(mpd.content_protections.len()),
                });
                let cp = self.parse_content_protection(child)?;
                self.path.pop();
                mpd.content_protections.push(cp);
            } else if child.matches(MPD_NAMESPACE, "Period") {
                self.path.push(PathSegment {
                    element_name: "Period",
                    sibling_index: Some(mpd.periods.len()),
                });
                let period = self.parse_period(child)?;
                self.path.pop();
                mpd.periods.push(period);
            } else if child.matches(MPD_NAMESPACE, "EssentialProperty") {
                self.path.push(PathSegment {
                    element_name: "EssentialProperty",
                    sibling_index: Some(mpd.essential_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                mpd.essential_properties.push(desc);
            } else if child.matches(MPD_NAMESPACE, "SupplementalProperty") {
                self.path.push(PathSegment {
                    element_name: "SupplementalProperty",
                    sibling_index: Some(mpd.supplemental_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                mpd.supplemental_properties.push(desc);
            } else if child.matches(MPD_NAMESPACE, "UTCTiming") {
                self.path.push(PathSegment {
                    element_name: "UTCTiming",
                    sibling_index: Some(mpd.utc_timings.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                mpd.utc_timings.push(desc);
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
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => period
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            let Some(child) = self.apply_segment_child(
                &mut period.segment_base,
                &mut period.segment_list,
                &mut period.segment_template,
                child,
            )?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "AssetIdentifier") {
                self.parse_singular_child(
                    &mut period.asset_identifier,
                    "AssetIdentifier",
                    child,
                    Self::parse_descriptor,
                )?;
            } else if child.matches(MPD_NAMESPACE, "ContentProtection") {
                self.path.push(PathSegment {
                    element_name: "ContentProtection",
                    sibling_index: Some(period.content_protections.len()),
                });
                let cp = self.parse_content_protection(child)?;
                self.path.pop();
                period.content_protections.push(cp);
            } else if child.matches(MPD_NAMESPACE, "AdaptationSet") {
                self.path.push(PathSegment {
                    element_name: "AdaptationSet",
                    sibling_index: Some(period.adaptation_sets.len()),
                });
                let adaptation_set = self.parse_adaptation_set(child)?;
                self.path.pop();
                period.adaptation_sets.push(adaptation_set);
            } else if child.matches(MPD_NAMESPACE, "SupplementalProperty") {
                self.path.push(PathSegment {
                    element_name: "SupplementalProperty",
                    sibling_index: Some(period.supplemental_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                period.supplemental_properties.push(desc);
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
                    adaptation_set.subsegment_starts_with_sap =
                        Some(self.parse_attribute("subsegmentStartsWithSAP", &attribute.value)?);
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
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => adaptation_set
                    .base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            let Some(child) = self.apply_segment_child(
                &mut adaptation_set.segment_base,
                &mut adaptation_set.segment_list,
                &mut adaptation_set.segment_template,
                child,
            )?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "Accessibility") {
                self.path.push(PathSegment {
                    element_name: "Accessibility",
                    sibling_index: Some(adaptation_set.accessibilities.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                adaptation_set.accessibilities.push(desc);
            } else if child.matches(MPD_NAMESPACE, "Role") {
                self.path.push(PathSegment {
                    element_name: "Role",
                    sibling_index: Some(adaptation_set.roles.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                adaptation_set.roles.push(desc);
            } else if child.matches(MPD_NAMESPACE, "Rating") {
                self.path.push(PathSegment {
                    element_name: "Rating",
                    sibling_index: Some(adaptation_set.ratings.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                adaptation_set.ratings.push(desc);
            } else if child.matches(MPD_NAMESPACE, "Viewpoint") {
                self.path.push(PathSegment {
                    element_name: "Viewpoint",
                    sibling_index: Some(adaptation_set.viewpoints.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                adaptation_set.viewpoints.push(desc);
            } else if child.matches(MPD_NAMESPACE, "Representation") {
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
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
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
            let Some(child) = self.apply_segment_child(
                &mut representation.segment_base,
                &mut representation.segment_list,
                &mut representation.segment_template,
                child,
            )?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "FramePacking") {
                self.path.push(PathSegment {
                    element_name: "FramePacking",
                    sibling_index: Some(representation.base.frame_packings.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                representation.base.frame_packings.push(desc);
            } else if child.matches(MPD_NAMESPACE, "AudioChannelConfiguration") {
                self.path.push(PathSegment {
                    element_name: "AudioChannelConfiguration",
                    sibling_index: Some(representation.base.audio_channel_configurations.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                representation.base.audio_channel_configurations.push(desc);
            } else if child.matches(MPD_NAMESPACE, "ContentProtection") {
                self.path.push(PathSegment {
                    element_name: "ContentProtection",
                    sibling_index: Some(representation.base.content_protections.len()),
                });
                let cp = self.parse_content_protection(child)?;
                self.path.pop();
                representation.base.content_protections.push(cp);
            } else if child.matches(MPD_NAMESPACE, "OutputProtection") {
                self.parse_singular_child(
                    &mut representation.base.output_protection,
                    "OutputProtection",
                    child,
                    Self::parse_descriptor,
                )?;
            } else if child.matches(MPD_NAMESPACE, "EssentialProperty") {
                self.path.push(PathSegment {
                    element_name: "EssentialProperty",
                    sibling_index: Some(representation.base.essential_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                representation.base.essential_properties.push(desc);
            } else if child.matches(MPD_NAMESPACE, "SupplementalProperty") {
                self.path.push(PathSegment {
                    element_name: "SupplementalProperty",
                    sibling_index: Some(representation.base.supplemental_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                representation.base.supplemental_properties.push(desc);
            } else {
                representation
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
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
                base.audio_sampling_rate =
                    Some(self.parse_attribute("audioSamplingRate", &attribute.value)?);
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
                base.start_with_sap = Some(self.parse_attribute("startWithSAP", &attribute.value)?);
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

    /// Handles the `SegmentBase` / `SegmentList` / `SegmentTemplate` children
    /// shared by `Period`, `AdaptationSet`, and `Representation`, returning
    /// the element unconsumed when it is none of the three.
    fn apply_segment_child(
        &mut self,
        segment_base: &mut Option<SegmentBase>,
        segment_list: &mut Option<SegmentList>,
        segment_template: &mut Option<SegmentTemplate>,
        child: StartElement,
    ) -> Result<Option<StartElement>> {
        if child.matches(MPD_NAMESPACE, "SegmentBase") {
            self.parse_singular_child(
                segment_base,
                "SegmentBase",
                child,
                Self::parse_segment_base,
            )?;
        } else if child.matches(MPD_NAMESPACE, "SegmentList") {
            self.parse_singular_child(
                segment_list,
                "SegmentList",
                child,
                Self::parse_segment_list,
            )?;
        } else if child.matches(MPD_NAMESPACE, "SegmentTemplate") {
            self.parse_singular_child(
                segment_template,
                "SegmentTemplate",
                child,
                Self::parse_segment_template,
            )?;
        } else {
            return Ok(Some(child));
        }
        Ok(None)
    }

    /// Parses a child the schema allows at most once into `slot`, rejecting
    /// a second occurrence and framing the path segment around `parse`.
    fn parse_singular_child<T>(
        &mut self,
        slot: &mut Option<T>,
        element_name: &'static str,
        child: StartElement,
        parse: impl FnOnce(&mut Self, StartElement) -> Result<T>,
    ) -> Result<()> {
        if slot.is_some() {
            return Err(self.duplicate_element(child.name));
        }
        self.path.push(PathSegment {
            element_name,
            sibling_index: None,
        });
        let parsed = parse(self, child)?;
        self.path.pop();
        *slot = Some(parsed);
        Ok(())
    }

    fn parse_segment_base(&mut self, start: StartElement) -> Result<SegmentBase> {
        let mut segment_base = SegmentBase::new();
        for attribute in start.attributes {
            if let Some(attribute) =
                self.apply_segment_base_attribute(&mut segment_base, attribute)?
            {
                match attribute.name.as_str() {
                    "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                    _ => segment_base
                        .unknown_attributes
                        .push((attribute.name, attribute.value)),
                }
            }
        }
        while let Some(child) = self.next_content_event()? {
            if let Some(child) = self.apply_segment_base_child(&mut segment_base, child)? {
                segment_base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(segment_base)
    }

    fn parse_segment_list(&mut self, start: StartElement) -> Result<SegmentList> {
        let mut segment_list = SegmentList::new();
        for attribute in start.attributes {
            if let Some(attribute) =
                self.apply_multiple_segment_base_attribute(&mut segment_list.base, attribute)?
            {
                match attribute.name.as_str() {
                    "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                    _ => segment_list
                        .base
                        .base
                        .unknown_attributes
                        .push((attribute.name, attribute.value)),
                }
            }
        }
        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "SegmentURL") {
                self.path.push(PathSegment {
                    element_name: "SegmentURL",
                    sibling_index: Some(segment_list.segment_urls.len()),
                });
                let segment_url = self.parse_segment_url(child)?;
                self.path.pop();
                segment_list.segment_urls.push(segment_url);
            } else if let Some(child) =
                self.apply_multiple_segment_base_child(&mut segment_list.base, child)?
            {
                segment_list
                    .base
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(segment_list)
    }

    fn parse_segment_template(&mut self, start: StartElement) -> Result<SegmentTemplate> {
        let mut segment_template = SegmentTemplate::new();
        for attribute in start.attributes {
            let Some(attribute) =
                self.apply_multiple_segment_base_attribute(&mut segment_template.base, attribute)?
            else {
                continue;
            };
            match attribute.name.as_str() {
                "media" => segment_template.media = Some(attribute.value),
                "index" => segment_template.index = Some(attribute.value),
                "initialization" => segment_template.initialization = Some(attribute.value),
                "bitstreamSwitching" => {
                    segment_template.bitstream_switching = Some(attribute.value);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => segment_template
                    .base
                    .base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        while let Some(child) = self.next_content_event()? {
            if let Some(child) =
                self.apply_multiple_segment_base_child(&mut segment_template.base, child)?
            {
                segment_template
                    .base
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(segment_template)
    }

    /// Applies one attribute of an element extending `SegmentBaseType` to the
    /// embedded base, returning the attribute unconsumed when it belongs to
    /// the extending type.
    fn apply_segment_base_attribute(
        &self,
        segment_base: &mut SegmentBase,
        attribute: Attribute,
    ) -> Result<Option<Attribute>> {
        match attribute.name.as_str() {
            "timescale" => {
                segment_base.timescale =
                    Some(self.in_attribute("timescale", parse_xs_unsigned_int(&attribute.value))?);
            }
            "eptDelta" => {
                segment_base.ept_delta =
                    Some(self.in_attribute("eptDelta", parse_xs_integer(&attribute.value))?);
            }
            "pdDelta" => {
                segment_base.pd_delta =
                    Some(self.in_attribute("pdDelta", parse_xs_integer(&attribute.value))?);
            }
            "presentationTimeOffset" => {
                segment_base.presentation_time_offset = Some(self.in_attribute(
                    "presentationTimeOffset",
                    parse_xs_unsigned_long(&attribute.value),
                )?);
            }
            "presentationDuration" => {
                segment_base.presentation_duration = Some(self.in_attribute(
                    "presentationDuration",
                    parse_xs_unsigned_long(&attribute.value),
                )?);
            }
            "timeShiftBufferDepth" => {
                segment_base.time_shift_buffer_depth =
                    Some(self.parse_attribute("timeShiftBufferDepth", &attribute.value)?);
            }
            "indexRange" => {
                segment_base.index_range = Some(
                    self.in_attribute("indexRange", parse_single_rfc7233_range(&attribute.value))?,
                );
            }
            "indexRangeExact" => {
                segment_base.index_range_exact =
                    Some(self.in_attribute("indexRangeExact", parse_xs_boolean(&attribute.value))?);
            }
            "availabilityTimeOffset" => {
                segment_base.availability_time_offset = Some(
                    self.in_attribute("availabilityTimeOffset", parse_xs_double(&attribute.value))?,
                );
            }
            "availabilityTimeComplete" => {
                segment_base.availability_time_complete = Some(self.in_attribute(
                    "availabilityTimeComplete",
                    parse_xs_boolean(&attribute.value),
                )?);
            }
            _ => return Ok(Some(attribute)),
        }
        Ok(None)
    }

    /// Applies one child of an element extending `SegmentBaseType` to the
    /// embedded base, returning the element unconsumed when it belongs to
    /// the extending type.
    fn apply_segment_base_child(
        &mut self,
        segment_base: &mut SegmentBase,
        child: StartElement,
    ) -> Result<Option<StartElement>> {
        if child.matches(MPD_NAMESPACE, "Initialization") {
            self.parse_singular_child(
                &mut segment_base.initialization,
                "Initialization",
                child,
                Self::parse_url,
            )?;
        } else if child.matches(MPD_NAMESPACE, "RepresentationIndex") {
            self.parse_singular_child(
                &mut segment_base.representation_index,
                "RepresentationIndex",
                child,
                Self::parse_url,
            )?;
        } else if child.matches(MPD_NAMESPACE, "FailoverContent") {
            self.parse_singular_child(
                &mut segment_base.failover_content,
                "FailoverContent",
                child,
                Self::parse_failover_content,
            )?;
        } else {
            return Ok(Some(child));
        }
        Ok(None)
    }

    /// Applies one attribute of an element extending
    /// `MultipleSegmentBaseType` to the embedded base, returning the
    /// attribute unconsumed when it belongs to the extending type.
    fn apply_multiple_segment_base_attribute(
        &self,
        base: &mut MultipleSegmentBase,
        attribute: Attribute,
    ) -> Result<Option<Attribute>> {
        let Some(attribute) = self.apply_segment_base_attribute(&mut base.base, attribute)? else {
            return Ok(None);
        };
        match attribute.name.as_str() {
            "duration" => {
                base.duration =
                    Some(self.in_attribute("duration", parse_xs_unsigned_int(&attribute.value))?);
            }
            "startNumber" => {
                base.start_number = Some(
                    self.in_attribute("startNumber", parse_xs_unsigned_int(&attribute.value))?,
                );
            }
            "endNumber" => {
                base.end_number =
                    Some(self.in_attribute("endNumber", parse_xs_unsigned_int(&attribute.value))?);
            }
            _ => return Ok(Some(attribute)),
        }
        Ok(None)
    }

    /// Applies one child of an element extending `MultipleSegmentBaseType`
    /// to the embedded base, returning the element unconsumed when it
    /// belongs to the extending type.
    fn apply_multiple_segment_base_child(
        &mut self,
        base: &mut MultipleSegmentBase,
        child: StartElement,
    ) -> Result<Option<StartElement>> {
        let Some(child) = self.apply_segment_base_child(&mut base.base, child)? else {
            return Ok(None);
        };
        if child.matches(MPD_NAMESPACE, "SegmentTimeline") {
            self.parse_singular_child(
                &mut base.segment_timeline,
                "SegmentTimeline",
                child,
                Self::parse_segment_timeline,
            )?;
        } else if child.matches(MPD_NAMESPACE, "BitstreamSwitching") {
            self.parse_singular_child(
                &mut base.bitstream_switching,
                "BitstreamSwitching",
                child,
                Self::parse_url,
            )?;
        } else {
            return Ok(Some(child));
        }
        Ok(None)
    }

    fn parse_url(&mut self, start: StartElement) -> Result<Url> {
        let mut url = Url::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "sourceURL" => url.source_url = Some(attribute.value),
                "range" => {
                    url.range = Some(
                        self.in_attribute("range", parse_single_rfc7233_range(&attribute.value))?,
                    );
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => url
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        while let Some(child) = self.next_content_event()? {
            url.unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(url)
    }

    fn parse_failover_content(&mut self, start: StartElement) -> Result<FailoverContent> {
        let mut failover_content = FailoverContent::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "valid" => {
                    failover_content.valid =
                        Some(self.in_attribute("valid", parse_xs_boolean(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => failover_content
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "FCS") {
                self.path.push(PathSegment {
                    element_name: "FCS",
                    sibling_index: Some(failover_content.fcs_entries.len()),
                });
                let fcs = self.parse_fcs(child)?;
                self.path.pop();
                failover_content.fcs_entries.push(fcs);
            } else {
                failover_content
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(failover_content)
    }

    fn parse_fcs(&mut self, start: StartElement) -> Result<Fcs> {
        let mut t: Option<u64> = None;
        let mut d: Option<u64> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "t" => t = Some(self.in_attribute("t", parse_xs_unsigned_long(&attribute.value))?),
                "d" => d = Some(self.in_attribute("d", parse_xs_unsigned_long(&attribute.value))?),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }
        let mut fcs = Fcs::new(t.ok_or_else(|| self.missing_attribute("t"))?);
        fcs.d = d;
        fcs.unknown_attributes = unknown_attributes;
        while let Some(child) = self.next_content_event()? {
            fcs.unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(fcs)
    }

    fn parse_descriptor(&mut self, start: StartElement) -> Result<Descriptor> {
        let mut scheme_id_uri: Option<String> = None;
        let mut value: Option<String> = None;
        let mut id: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "schemeIdUri" => scheme_id_uri = Some(attribute.value),
                "value" => value = Some(attribute.value),
                "id" => id = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }
        let mut descriptor =
            Descriptor::new(scheme_id_uri.ok_or_else(|| self.missing_attribute("schemeIdUri"))?);
        descriptor.value = value;
        descriptor.id = id;
        descriptor.unknown_attributes = unknown_attributes;
        while let Some(child) = self.next_content_event()? {
            descriptor
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(descriptor)
    }

    fn parse_content_protection(&mut self, start: StartElement) -> Result<ContentProtection> {
        let mut scheme_id_uri: Option<String> = None;
        let mut value: Option<String> = None;
        let mut id: Option<String> = None;
        let mut robustness: Option<String> = None;
        let mut ref_id: Option<String> = None;
        let mut r#ref: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "schemeIdUri" => scheme_id_uri = Some(attribute.value),
                "value" => value = Some(attribute.value),
                "id" => id = Some(attribute.value),
                "robustness" => robustness = Some(attribute.value),
                "refId" => ref_id = Some(attribute.value),
                "ref" => r#ref = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }
        let mut content_protection = ContentProtection::new(
            scheme_id_uri.ok_or_else(|| self.missing_attribute("schemeIdUri"))?,
        );
        content_protection.base.value = value;
        content_protection.base.id = id;
        content_protection.robustness = robustness;
        content_protection.ref_id = ref_id;
        content_protection.r#ref = r#ref;
        content_protection.base.unknown_attributes = unknown_attributes;
        while let Some(child) = self.next_content_event()? {
            content_protection
                .base
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(content_protection)
    }

    fn parse_segment_timeline(&mut self, start: StartElement) -> Result<SegmentTimeline> {
        let mut segment_timeline = SegmentTimeline::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => segment_timeline
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "S") {
                self.path.push(PathSegment {
                    element_name: "S",
                    sibling_index: Some(segment_timeline.segments.len()),
                });
                let segment = self.parse_s(child)?;
                self.path.pop();
                segment_timeline.segments.push(segment);
            } else {
                segment_timeline
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(segment_timeline)
    }

    fn parse_s(&mut self, start: StartElement) -> Result<S> {
        let mut t: Option<u64> = None;
        let mut n: Option<u64> = None;
        let mut d: Option<u64> = None;
        let mut r: Option<i64> = None;
        let mut k: Option<u64> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "t" => t = Some(self.in_attribute("t", parse_xs_unsigned_long(&attribute.value))?),
                "n" => n = Some(self.in_attribute("n", parse_xs_unsigned_long(&attribute.value))?),
                "d" => d = Some(self.in_attribute("d", parse_xs_unsigned_long(&attribute.value))?),
                "r" => r = Some(self.in_attribute("r", parse_xs_integer(&attribute.value))?),
                "k" => k = Some(self.in_attribute("k", parse_xs_unsigned_long(&attribute.value))?),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }
        let mut segment = S::new(d.ok_or_else(|| self.missing_attribute("d"))?);
        segment.t = t;
        segment.n = n;
        segment.r = r;
        segment.k = k;
        segment.unknown_attributes = unknown_attributes;
        while let Some(child) = self.next_content_event()? {
            segment
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(segment)
    }

    fn parse_segment_url(&mut self, start: StartElement) -> Result<SegmentUrl> {
        let mut segment_url = SegmentUrl::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "media" => segment_url.media = Some(attribute.value),
                "mediaRange" => {
                    segment_url.media_range = Some(self.in_attribute(
                        "mediaRange",
                        parse_single_rfc7233_range(&attribute.value),
                    )?);
                }
                "index" => segment_url.index = Some(attribute.value),
                "indexRange" => {
                    segment_url.index_range = Some(self.in_attribute(
                        "indexRange",
                        parse_single_rfc7233_range(&attribute.value),
                    )?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => segment_url
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        while let Some(child) = self.next_content_event()? {
            segment_url
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(segment_url)
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
        // 既知要素の直下（depth 0）では、どの名前空間にも属さない無接頭辞の
        // 未知要素を拒否する。シリアライザがルートに MPD_NAMESPACE の既定
        // 宣言を置くため、そのような要素は再パースで MPD 名前空間に解決され、
        // 名前が既知要素と一致すると型付きフィールドに化けて既知/未知の区分が
        // 往復で安定しない。自前の `xmlns` 宣言（`xmlns=""` を含む）を持つ
        // 要素は解決が文書の書き換えに依存しないため受理する。
        if depth == 0
            && start.namespace.is_none()
            && !start
                .attributes
                .iter()
                .any(|attribute| attribute.name == "xmlns")
        {
            return Err(self.element_error(ErrorKind::UnexpectedElement { name: start.name }));
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

    /// 既知要素上の既定名前空間宣言は保持されない（シリアライザがルート MPD
    /// に [`MPD_NAMESPACE`] を宣言し直す）ため、MPD 名前空間の宣言だけを
    /// 受理して捨てる。異なる名前空間の宣言を黙って捨てると、無接頭辞の
    /// 未知子孫要素の名前空間解決が roundtrip で変わるので拒否する。
    fn check_default_namespace_declaration(&self, value: &str) -> Result<()> {
        if value == MPD_NAMESPACE {
            Ok(())
        } else {
            self.in_attribute(
                "xmlns",
                Err(invalid_value(
                    value,
                    "the MPD namespace; a different default namespace on a known \
                     element does not survive serialization",
                )),
            )
        }
    }

    /// Error for a second occurrence of a child the schema allows at most
    /// once.
    fn duplicate_element(&self, name: String) -> Error {
        self.element_error(ErrorKind::UnexpectedElement { name })
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

fn parse_string_no_whitespace(value: &str) -> Result<String> {
    if value.chars().any(char::is_whitespace) {
        Err(invalid_value(value, "a string without whitespace"))
    } else {
        Ok(value.to_string())
    }
}

fn parse_xs_unsigned_long(value: &str) -> Result<u64> {
    const EXPECTED: &str = "an `xs:unsignedLong`";
    let digits = value.strip_prefix('+').unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_value(value, EXPECTED));
    }
    digits.parse().map_err(|_| invalid_value(value, EXPECTED))
}

/// `xs:integer` は桁数無制限だが、値空間は `i64` に固定し表現不能値は拒否する
/// （ADR-0008）。
fn parse_xs_integer(value: &str) -> Result<i64> {
    const EXPECTED: &str = "an `xs:integer` representable in 64 bits";
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_value(value, EXPECTED));
    }
    value.parse().map_err(|_| invalid_value(value, EXPECTED))
}

/// Validates the `SingleRFC7233RangeType` pattern
/// `([0-9]*)(-([0-9]*))?`; the value itself stays a string (see
/// `model/segment.rs`).
fn parse_single_rfc7233_range(value: &str) -> Result<String> {
    const EXPECTED: &str = "a byte range such as `0-499`";
    let (first, last) = match value.split_once('-') {
        Some(parts) => parts,
        None => (value, ""),
    };
    if first.bytes().all(|byte| byte.is_ascii_digit())
        && last.bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(value.to_string())
    } else {
        Err(invalid_value(value, EXPECTED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::element::Node;
    use crate::model::mpd::{ContentType, VideoScan};
    use crate::model::types::{AudioSamplingRate, FrameRate, Sap};

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
            Some(AudioSamplingRate::MinMax(44_100, 48_000))
        );
        assert_eq!(representation.base.scan_type, Some(VideoScan::Progressive));
        assert_eq!(
            representation.base.start_with_sap,
            Some(Sap::new(1).unwrap())
        );
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
    fn foreign_default_namespace_on_known_elements_is_rejected() {
        // レビュー再現例: 接頭辞で一致した既知要素の外来既定宣言を黙って
        // 捨てると、無接頭辞の未知子孫の解決が roundtrip で変わる。
        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" xmlns="urn:other" "#,
            r#"profiles="p" minBufferTime="PT2S"><Foo/></ns1:MPD>"#,
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(
            matches!(error.kind, ErrorKind::InvalidValue { ref value, .. } if value == "urn:other")
        );
        assert_eq!(error.path, "MPD @ xmlns");

        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S"><ns1:Period xmlns="urn:other"/></ns1:MPD>"#,
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert_eq!(error.path, "MPD > Period[0] @ xmlns");
    }

    #[test]
    fn redundant_mpd_default_namespace_on_known_elements_is_dropped() {
        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S">"#,
            r#"<ns1:Period xmlns="urn:mpeg:dash:schema:mpd:2011"/>"#,
            "</ns1:MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        let period = mpd.periods.first().unwrap();
        assert!(period.unknown_attributes.is_empty());
    }

    #[test]
    fn unbound_unknown_child_of_known_element_is_rejected() {
        // 無接頭辞かつどの名前空間にも属さない未知要素は、再シリアライズで
        // MPD 名前空間に入り、名前次第で型付き要素に化けるため受理しない。
        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S"><Period/></ns1:MPD>"#,
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::UnexpectedElement { ref name } if name == "Period"
        ));
        assert_eq!(error.path, "MPD");
    }

    #[test]
    fn undeclared_default_namespace_on_unknown_child_is_accepted() {
        // `xmlns=""` を自分で持つ要素は解決が書き換えに依存しないため受理。
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S"><Foo xmlns=""/></MPD>"#,
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        match mpd.unknown_children.as_slice() {
            [foo] => {
                assert_eq!(foo.name, "Foo");
                assert_eq!(foo.namespace, None);
                assert_eq!(foo.attributes, vec![("xmlns".to_string(), String::new())]);
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
    fn segment_template_with_timeline_parses() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><AdaptationSet>",
            r#"<SegmentTemplate timescale="90000" duration="180000" startNumber="1" "#,
            r#"presentationTimeOffset="900000" eptDelta="-100" "#,
            r#"media="seg-$RepresentationID$-$Number%05d$.m4s" initialization="init-$RepresentationID$.mp4">"#,
            r#"<SegmentTimeline><S t="0" d="180000" r="-1"/><S d="90000" k="2"/></SegmentTimeline>"#,
            "</SegmentTemplate>",
            "</AdaptationSet></Period>",
            "</MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        let adaptation_set = mpd
            .periods
            .first()
            .unwrap()
            .adaptation_sets
            .first()
            .unwrap();
        let segment_template = adaptation_set.segment_template.as_ref().unwrap();
        assert_eq!(segment_template.base.base.timescale, Some(90_000));
        assert_eq!(segment_template.base.duration, Some(180_000));
        assert_eq!(segment_template.base.start_number, Some(1));
        assert_eq!(
            segment_template.base.base.presentation_time_offset,
            Some(900_000)
        );
        assert_eq!(segment_template.base.base.ept_delta, Some(-100));
        assert_eq!(
            segment_template.media.as_deref(),
            Some("seg-$RepresentationID$-$Number%05d$.m4s")
        );
        assert_eq!(
            segment_template.initialization.as_deref(),
            Some("init-$RepresentationID$.mp4")
        );

        let timeline = segment_template.base.segment_timeline.as_ref().unwrap();
        match timeline.segments.as_slice() {
            [first, second] => {
                assert_eq!(first.t, Some(0));
                assert_eq!(first.d, 180_000);
                assert_eq!(first.r, Some(-1));
                assert_eq!(second.t, None);
                assert_eq!(second.d, 90_000);
                assert_eq!(second.k, Some(2));
            }
            other => panic!("unexpected segments: {other:?}"),
        }
    }

    #[test]
    fn segment_base_and_list_parse_on_representation() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><AdaptationSet>",
            r#"<Representation id="v0" bandwidth="1000">"#,
            r#"<SegmentBase timescale="48000" indexRange="0-499" indexRangeExact="true" "#,
            r#"availabilityTimeOffset="INF">"#,
            r#"<Initialization sourceURL="init.mp4" range="0-99"/>"#,
            r#"<FailoverContent valid="false"><FCS t="0" d="48000"/></FailoverContent>"#,
            "</SegmentBase>",
            "</Representation>",
            r#"<Representation id="v1" bandwidth="2000">"#,
            r#"<SegmentList duration="2"><SegmentURL media="s1.mp4" mediaRange="0-1"/>"#,
            r#"<SegmentURL media="s2.mp4"/></SegmentList>"#,
            "</Representation>",
            "</AdaptationSet></Period>",
            "</MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        let adaptation_set = mpd
            .periods
            .first()
            .unwrap()
            .adaptation_sets
            .first()
            .unwrap();

        let segment_base = adaptation_set
            .representations
            .first()
            .unwrap()
            .segment_base
            .as_ref()
            .unwrap();
        assert_eq!(segment_base.timescale, Some(48_000));
        assert_eq!(segment_base.index_range.as_deref(), Some("0-499"));
        assert_eq!(segment_base.index_range_exact, Some(true));
        assert_eq!(segment_base.availability_time_offset, Some(f64::INFINITY));
        let initialization = segment_base.initialization.as_ref().unwrap();
        assert_eq!(initialization.source_url.as_deref(), Some("init.mp4"));
        assert_eq!(initialization.range.as_deref(), Some("0-99"));
        let failover_content = segment_base.failover_content.as_ref().unwrap();
        assert_eq!(failover_content.valid, Some(false));
        match failover_content.fcs_entries.as_slice() {
            [fcs] => {
                assert_eq!(fcs.t, 0);
                assert_eq!(fcs.d, Some(48_000));
            }
            other => panic!("unexpected FCS entries: {other:?}"),
        }

        let segment_list = adaptation_set
            .representations
            .get(1)
            .unwrap()
            .segment_list
            .as_ref()
            .unwrap();
        assert_eq!(segment_list.base.duration, Some(2));
        match segment_list.segment_urls.as_slice() {
            [first, second] => {
                assert_eq!(first.media.as_deref(), Some("s1.mp4"));
                assert_eq!(first.media_range.as_deref(), Some("0-1"));
                assert_eq!(second.media.as_deref(), Some("s2.mp4"));
            }
            other => panic!("unexpected segment URLs: {other:?}"),
        }
    }

    #[test]
    fn duplicate_singular_segment_children_are_rejected() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><SegmentTemplate/><SegmentTemplate/></Period>",
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(
            error.kind,
            ErrorKind::UnexpectedElement { ref name } if name == "SegmentTemplate"
        ));
        assert_eq!(error.path, "MPD > Period[0]");

        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><SegmentBase><Initialization/><Initialization/></SegmentBase></Period>",
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::UnexpectedElement { .. }));
        assert_eq!(error.path, "MPD > Period[0] > SegmentBase");
    }

    #[test]
    fn missing_s_duration_reports_the_path() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><SegmentTemplate><SegmentTimeline>",
            r#"<S d="90000"/><S t="90000"/>"#,
            "</SegmentTimeline></SegmentTemplate></Period>",
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::MissingAttribute));
        assert_eq!(
            error.path,
            "MPD > Period[0] > SegmentTemplate > SegmentTimeline > S[1] @ d"
        );
    }

    #[test]
    fn fixed_width_integer_attributes_reject_unrepresentable_values() {
        let template = |attribute: &str| {
            format!(
                concat!(
                    r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
                    r#"minBufferTime="PT2S"><Period><SegmentBase {}/></Period></MPD>"#,
                ),
                attribute
            )
        };
        let mpd = mpd_from_slice(template(r#"eptDelta="-100""#).as_bytes()).unwrap();
        let segment_base = mpd.periods.first().unwrap().segment_base.as_ref().unwrap();
        assert_eq!(segment_base.ept_delta, Some(-100));

        let mpd =
            mpd_from_slice(template(r#"presentationTimeOffset="18446744073709551615""#).as_bytes())
                .unwrap();
        let segment_base = mpd.periods.first().unwrap().segment_base.as_ref().unwrap();
        assert_eq!(segment_base.presentation_time_offset, Some(u64::MAX));

        for attribute in [
            r#"eptDelta="9223372036854775808""#,
            r#"presentationTimeOffset="18446744073709551616""#,
            r#"presentationTimeOffset="-1""#,
        ] {
            let error = mpd_from_slice(template(attribute).as_bytes()).unwrap_err();
            assert!(matches!(error.kind, ErrorKind::InvalidValue { .. }));
        }
    }

    #[test]
    fn malformed_byte_ranges_are_rejected() {
        for range in ["abc", "1-2-3", "1_2"] {
            let input = format!(
                concat!(
                    r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
                    r#"minBufferTime="PT2S"><Period><SegmentBase indexRange="{}"/></Period></MPD>"#,
                ),
                range
            );
            let error = mpd_from_slice(input.as_bytes()).unwrap_err();
            assert!(matches!(error.kind, ErrorKind::InvalidValue { .. }));
            assert_eq!(error.path, "MPD > Period[0] > SegmentBase @ indexRange");
        }
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
