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
            if child.matches(MPD_NAMESPACE, "ProgramInformation") {
                self.path.push(PathSegment {
                    element_name: "ProgramInformation",
                    sibling_index: Some(mpd.program_informations.len()),
                });
                let program_information = self.parse_program_information(child)?;
                self.path.pop();
                mpd.program_informations.push(program_information);
            } else if child.matches(MPD_NAMESPACE, "BaseURL") {
                self.path.push(PathSegment {
                    element_name: "BaseURL",
                    sibling_index: Some(mpd.base_urls.len()),
                });
                let base_url = self.parse_base_url(child)?;
                self.path.pop();
                mpd.base_urls.push(base_url);
            } else if child.matches(MPD_NAMESPACE, "Location") {
                self.path.push(PathSegment {
                    element_name: "Location",
                    sibling_index: Some(mpd.locations.len()),
                });
                for attribute in &child.attributes {
                    if attribute.name == "xmlns" {
                        self.check_default_namespace_declaration(&attribute.value)?;
                    }
                }
                let location = self.parse_text_content()?;
                self.path.pop();
                mpd.locations.push(location);
            } else if child.matches(MPD_NAMESPACE, "PatchLocation") {
                self.path.push(PathSegment {
                    element_name: "PatchLocation",
                    sibling_index: Some(mpd.patch_locations.len()),
                });
                let patch_location = self.parse_patch_location(child)?;
                self.path.pop();
                mpd.patch_locations.push(patch_location);
            } else if child.matches(MPD_NAMESPACE, "ServiceDescription") {
                self.path.push(PathSegment {
                    element_name: "ServiceDescription",
                    sibling_index: Some(mpd.service_descriptions.len()),
                });
                let service_description = self.parse_service_description(child)?;
                self.path.pop();
                mpd.service_descriptions.push(service_description);
            } else if child.matches(MPD_NAMESPACE, "InitializationSet") {
                self.path.push(PathSegment {
                    element_name: "InitializationSet",
                    sibling_index: Some(mpd.initialization_sets.len()),
                });
                let initialization_set = self.parse_initialization_set(child)?;
                self.path.pop();
                mpd.initialization_sets.push(initialization_set);
            } else if child.matches(MPD_NAMESPACE, "InitializationGroup") {
                self.path.push(PathSegment {
                    element_name: "InitializationGroup",
                    sibling_index: Some(mpd.initialization_groups.len()),
                });
                let initialization_group = self.parse_uint_v_with_id(child)?;
                self.path.pop();
                mpd.initialization_groups.push(initialization_group);
            } else if child.matches(MPD_NAMESPACE, "InitializationPresentation") {
                self.path.push(PathSegment {
                    element_name: "InitializationPresentation",
                    sibling_index: Some(mpd.initialization_presentations.len()),
                });
                let initialization_presentation = self.parse_uint_v_with_id(child)?;
                self.path.pop();
                mpd.initialization_presentations
                    .push(initialization_presentation);
            } else if child.matches(MPD_NAMESPACE, "ContentProtection") {
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
            } else if child.matches(MPD_NAMESPACE, "Metrics") {
                self.path.push(PathSegment {
                    element_name: "Metrics",
                    sibling_index: Some(mpd.metrics.len()),
                });
                let metrics = self.parse_metrics(child)?;
                self.path.pop();
                mpd.metrics.push(metrics);
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
            } else if child.matches(MPD_NAMESPACE, "LeapSecondInformation") {
                if mpd.leap_second_information.is_some() {
                    return Err(self.duplicate_element(child.name));
                }
                self.path.push(PathSegment {
                    element_name: "LeapSecondInformation",
                    sibling_index: None,
                });
                let leap_second = self.parse_leap_second_information(child)?;
                self.path.pop();
                mpd.leap_second_information = Some(leap_second);
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
            if child.matches(MPD_NAMESPACE, "BaseURL") {
                self.path.push(PathSegment {
                    element_name: "BaseURL",
                    sibling_index: Some(period.base_urls.len()),
                });
                let base_url = self.parse_base_url(child)?;
                self.path.pop();
                period.base_urls.push(base_url);
                continue;
            }

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
            } else if child.matches(MPD_NAMESPACE, "ServiceDescription") {
                self.path.push(PathSegment {
                    element_name: "ServiceDescription",
                    sibling_index: Some(period.service_descriptions.len()),
                });
                let service_description = self.parse_service_description(child)?;
                self.path.pop();
                period.service_descriptions.push(service_description);
            } else if child.matches(MPD_NAMESPACE, "ContentProtection") {
                self.path.push(PathSegment {
                    element_name: "ContentProtection",
                    sibling_index: Some(period.content_protections.len()),
                });
                let cp = self.parse_content_protection(child)?;
                self.path.pop();
                period.content_protections.push(cp);
            } else if child.matches(MPD_NAMESPACE, "EventStream") {
                self.path.push(PathSegment {
                    element_name: "EventStream",
                    sibling_index: Some(period.event_streams.len()),
                });
                let event_stream = self.parse_event_stream(child)?;
                self.path.pop();
                period.event_streams.push(event_stream);
            } else if child.matches(MPD_NAMESPACE, "AdaptationSet") {
                self.path.push(PathSegment {
                    element_name: "AdaptationSet",
                    sibling_index: Some(period.adaptation_sets.len()),
                });
                let adaptation_set = self.parse_adaptation_set(child)?;
                self.path.pop();
                period.adaptation_sets.push(adaptation_set);
            } else if child.matches(MPD_NAMESPACE, "Subset") {
                self.path.push(PathSegment {
                    element_name: "Subset",
                    sibling_index: Some(period.subsets.len()),
                });
                let subset = self.parse_subset(child)?;
                self.path.pop();
                period.subsets.push(subset);
            } else if child.matches(MPD_NAMESPACE, "SupplementalProperty") {
                self.path.push(PathSegment {
                    element_name: "SupplementalProperty",
                    sibling_index: Some(period.supplemental_properties.len()),
                });
                let desc = self.parse_descriptor(child)?;
                self.path.pop();
                period.supplemental_properties.push(desc);
            } else if child.matches(MPD_NAMESPACE, "EmptyAdaptationSet") {
                self.path.push(PathSegment {
                    element_name: "EmptyAdaptationSet",
                    sibling_index: Some(period.empty_adaptation_sets.len()),
                });
                let adaptation_set = self.parse_adaptation_set(child)?;
                self.path.pop();
                period.empty_adaptation_sets.push(adaptation_set);
            } else if child.matches(MPD_NAMESPACE, "GroupLabel") {
                self.path.push(PathSegment {
                    element_name: "GroupLabel",
                    sibling_index: Some(period.group_labels.len()),
                });
                let label = self.parse_label(child)?;
                self.path.pop();
                period.group_labels.push(label);
            } else if child.matches(MPD_NAMESPACE, "Preselection") {
                self.path.push(PathSegment {
                    element_name: "Preselection",
                    sibling_index: Some(period.preselections.len()),
                });
                let preselection = self.parse_preselection(child)?;
                self.path.pop();
                period.preselections.push(preselection);
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
            if child.matches(MPD_NAMESPACE, "BaseURL") {
                self.path.push(PathSegment {
                    element_name: "BaseURL",
                    sibling_index: Some(adaptation_set.base_urls.len()),
                });
                let base_url = self.parse_base_url(child)?;
                self.path.pop();
                adaptation_set.base_urls.push(base_url);
                continue;
            }

            let Some(child) =
                self.apply_representation_base_child(&mut adaptation_set.base, child)?
            else {
                continue;
            };

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
            } else if child.matches(MPD_NAMESPACE, "ContentComponent") {
                self.path.push(PathSegment {
                    element_name: "ContentComponent",
                    sibling_index: Some(adaptation_set.content_components.len()),
                });
                let content_component = self.parse_content_component(child)?;
                self.path.pop();
                adaptation_set.content_components.push(content_component);
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
            if child.matches(MPD_NAMESPACE, "BaseURL") {
                self.path.push(PathSegment {
                    element_name: "BaseURL",
                    sibling_index: Some(representation.base_urls.len()),
                });
                let base_url = self.parse_base_url(child)?;
                self.path.pop();
                representation.base_urls.push(base_url);
                continue;
            }

            let Some(child) =
                self.apply_representation_base_child(&mut representation.base, child)?
            else {
                continue;
            };

            let Some(child) = self.apply_segment_child(
                &mut representation.segment_base,
                &mut representation.segment_list,
                &mut representation.segment_template,
                child,
            )?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "ExtendedBandwidth") {
                self.path.push(PathSegment {
                    element_name: "ExtendedBandwidth",
                    sibling_index: Some(representation.extended_bandwidths.len()),
                });
                let extended_bandwidth = self.parse_extended_bandwidth(child)?;
                self.path.pop();
                representation.extended_bandwidths.push(extended_bandwidth);
            } else if child.matches(MPD_NAMESPACE, "SubRepresentation") {
                self.path.push(PathSegment {
                    element_name: "SubRepresentation",
                    sibling_index: Some(representation.sub_representations.len()),
                });
                let sub_representation = self.parse_sub_representation(child)?;
                self.path.pop();
                representation.sub_representations.push(sub_representation);
            } else {
                representation
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(representation)
    }

    fn parse_program_information(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ProgramInformation> {
        use crate::model::ProgramInformation;

        let mut pi = ProgramInformation::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "lang" => pi.lang = Some(attribute.value),
                "moreInformationURL" => pi.more_information_url = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => pi
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Title") {
                if pi.title.is_some() {
                    return Err(self.duplicate_element(child.name));
                }
                pi.title = Some(self.parse_text_content()?);
            } else if child.matches(MPD_NAMESPACE, "Source") {
                if pi.source.is_some() {
                    return Err(self.duplicate_element(child.name));
                }
                pi.source = Some(self.parse_text_content()?);
            } else if child.matches(MPD_NAMESPACE, "Copyright") {
                if pi.copyright.is_some() {
                    return Err(self.duplicate_element(child.name));
                }
                pi.copyright = Some(self.parse_text_content()?);
            } else {
                pi.unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(pi)
    }

    fn parse_base_url(&mut self, start: StartElement) -> Result<crate::model::BaseUrl> {
        use crate::model::BaseUrl;

        let mut base_url = BaseUrl::new("");
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "serviceLocation" => base_url.service_location = Some(attribute.value),
                "byteRange" => base_url.byte_range = Some(attribute.value),
                "availabilityTimeOffset" => {
                    base_url.availability_time_offset = Some(self.in_attribute(
                        "availabilityTimeOffset",
                        parse_xs_double(&attribute.value),
                    )?);
                }
                "availabilityTimeComplete" => {
                    base_url.availability_time_complete = Some(self.in_attribute(
                        "availabilityTimeComplete",
                        parse_xs_boolean(&attribute.value),
                    )?);
                }
                "timeShiftBufferDepth" => {
                    base_url.time_shift_buffer_depth =
                        Some(self.parse_attribute("timeShiftBufferDepth", &attribute.value)?);
                }
                "rangeAccess" => {
                    base_url.range_access =
                        self.in_attribute("rangeAccess", parse_xs_boolean(&attribute.value))?;
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => base_url
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        let url = self.parse_text_content()?;
        base_url.url = url;
        Ok(base_url)
    }

    fn parse_patch_location(&mut self, start: StartElement) -> Result<crate::model::PatchLocation> {
        use crate::model::PatchLocation;

        let mut patch = PatchLocation::new("");
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "ttl" => {
                    patch.ttl = Some(self.in_attribute("ttl", parse_xs_double(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => patch
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        let url = self.parse_text_content()?;
        patch.url = url;
        Ok(patch)
    }

    fn parse_range(&mut self, start: StartElement) -> Result<crate::model::Range> {
        use crate::model::Range;

        let mut range = Range::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "starttime" => {
                    range.starttime = Some(self.parse_attribute("starttime", &attribute.value)?);
                }
                "duration" => {
                    range.duration = Some(self.parse_attribute("duration", &attribute.value)?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => range
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        self.consume_empty_element()?;
        Ok(range)
    }

    fn parse_metrics(&mut self, start: StartElement) -> Result<crate::model::Metrics> {
        use crate::model::Metrics;

        let mut metrics_attr: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();

        for attribute in start.attributes {
            match attribute.name.as_str() {
                "metrics" => metrics_attr = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut metrics =
            Metrics::new(metrics_attr.ok_or_else(|| self.missing_attribute("metrics"))?);
        metrics.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Range") {
                let range = self.parse_range(child)?;
                metrics.ranges.push(range);
            } else if child.matches(MPD_NAMESPACE, "Reporting") {
                let descriptor = self.parse_descriptor(child)?;
                metrics.reportings.push(descriptor);
            } else {
                metrics
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(metrics)
    }

    fn parse_leap_second_information(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::LeapSecondInformation> {
        use crate::model::LeapSecondInformation;
        use crate::model::types::XsDateTime;

        let mut leap_offset: Option<i64> = None;
        let mut next_leap_offset: Option<i64> = None;
        let mut next_leap_time: Option<XsDateTime> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();

        for attribute in start.attributes {
            match attribute.name.as_str() {
                "availabilityStartLeapOffset" => {
                    leap_offset = Some(self.in_attribute(
                        "availabilityStartLeapOffset",
                        parse_xs_integer(&attribute.value),
                    )?);
                }
                "nextAvailabilityStartLeapOffset" => {
                    next_leap_offset = Some(self.in_attribute(
                        "nextAvailabilityStartLeapOffset",
                        parse_xs_integer(&attribute.value),
                    )?);
                }
                "nextLeapChangeTime" => {
                    next_leap_time =
                        Some(self.parse_attribute("nextLeapChangeTime", &attribute.value)?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut leap_second = LeapSecondInformation::new(
            leap_offset.ok_or_else(|| self.missing_attribute("availabilityStartLeapOffset"))?,
        );
        leap_second.next_availability_start_leap_offset = next_leap_offset;
        leap_second.next_leap_change_time = next_leap_time;
        leap_second.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            leap_second
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(leap_second)
    }

    fn parse_service_description(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ServiceDescription> {
        use crate::model::ServiceDescription;

        let mut service_desc = ServiceDescription::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => {
                    service_desc.id =
                        Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => service_desc
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Scope") {
                self.path.push(PathSegment {
                    element_name: "Scope",
                    sibling_index: Some(service_desc.scopes.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                service_desc.scopes.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Latency") {
                self.path.push(PathSegment {
                    element_name: "Latency",
                    sibling_index: Some(service_desc.latencies.len()),
                });
                let latency = self.parse_latency(child)?;
                self.path.pop();
                service_desc.latencies.push(latency);
            } else if child.matches(MPD_NAMESPACE, "PlaybackRate") {
                self.path.push(PathSegment {
                    element_name: "PlaybackRate",
                    sibling_index: Some(service_desc.playback_rates.len()),
                });
                let playback_rate = self.parse_playback_rate(child)?;
                self.path.pop();
                service_desc.playback_rates.push(playback_rate);
            } else if child.matches(MPD_NAMESPACE, "OperatingQuality") {
                self.path.push(PathSegment {
                    element_name: "OperatingQuality",
                    sibling_index: Some(service_desc.operating_qualities.len()),
                });
                let quality = self.parse_operating_quality(child)?;
                self.path.pop();
                service_desc.operating_qualities.push(quality);
            } else if child.matches(MPD_NAMESPACE, "OperatingBandwidth") {
                self.path.push(PathSegment {
                    element_name: "OperatingBandwidth",
                    sibling_index: Some(service_desc.operating_bandwidths.len()),
                });
                let bandwidth = self.parse_operating_bandwidth(child)?;
                self.path.pop();
                service_desc.operating_bandwidths.push(bandwidth);
            } else {
                service_desc
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(service_desc)
    }

    fn parse_latency(&mut self, start: StartElement) -> Result<crate::model::Latency> {
        use crate::model::Latency;

        let mut latency = Latency::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "referenceId" => {
                    latency.reference_id = Some(
                        self.in_attribute("referenceId", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "target" => {
                    latency.target =
                        Some(self.in_attribute("target", parse_xs_unsigned_int(&attribute.value))?);
                }
                "max" => {
                    latency.max =
                        Some(self.in_attribute("max", parse_xs_unsigned_int(&attribute.value))?);
                }
                "min" => {
                    latency.min =
                        Some(self.in_attribute("min", parse_xs_unsigned_int(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => latency
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "QualityLatency") {
                let quality_latency = self.parse_uint_pairs_with_id(child)?;
                latency.quality_latencies.push(quality_latency);
            } else {
                latency
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(latency)
    }

    fn parse_playback_rate(&mut self, start: StartElement) -> Result<crate::model::PlaybackRate> {
        use crate::model::PlaybackRate;

        let mut rate = PlaybackRate::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "max" => {
                    rate.max = Some(self.in_attribute("max", parse_xs_double(&attribute.value))?);
                }
                "min" => {
                    rate.min = Some(self.in_attribute("min", parse_xs_double(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => rate
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        self.consume_empty_element()?;
        Ok(rate)
    }

    fn parse_operating_quality(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::OperatingQuality> {
        use crate::model::OperatingQuality;

        let mut quality = OperatingQuality::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "mediaType" => {
                    quality.media_type = self.parse_attribute("mediaType", &attribute.value)?;
                }
                "min" => {
                    quality.min =
                        Some(self.in_attribute("min", parse_xs_unsigned_int(&attribute.value))?);
                }
                "max" => {
                    quality.max =
                        Some(self.in_attribute("max", parse_xs_unsigned_int(&attribute.value))?);
                }
                "target" => {
                    quality.target =
                        Some(self.in_attribute("target", parse_xs_unsigned_int(&attribute.value))?);
                }
                "type" => quality.quality_type = Some(attribute.value),
                "maxDifference" => {
                    quality.max_difference =
                        Some(self.in_attribute(
                            "maxDifference",
                            parse_xs_unsigned_int(&attribute.value),
                        )?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => quality
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        self.consume_empty_element()?;
        Ok(quality)
    }

    fn parse_operating_bandwidth(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::OperatingBandwidth> {
        use crate::model::OperatingBandwidth;

        let mut bandwidth = OperatingBandwidth::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "mediaType" => {
                    bandwidth.media_type = self.parse_attribute("mediaType", &attribute.value)?;
                }
                "min" => {
                    bandwidth.min =
                        Some(self.in_attribute("min", parse_xs_unsigned_int(&attribute.value))?);
                }
                "max" => {
                    bandwidth.max =
                        Some(self.in_attribute("max", parse_xs_unsigned_int(&attribute.value))?);
                }
                "target" => {
                    bandwidth.target =
                        Some(self.in_attribute("target", parse_xs_unsigned_int(&attribute.value))?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => bandwidth
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        self.consume_empty_element()?;
        Ok(bandwidth)
    }

    fn parse_uint_pairs_with_id(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::UIntPairsWithId> {
        use crate::model::UIntPairsWithId;

        let mut pairs_obj = UIntPairsWithId::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "type" => pairs_obj.value_type = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => pairs_obj
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        let pairs_text = self.parse_text_content()?;
        pairs_obj.pairs = parse_uint_vector(&pairs_text)?;
        Ok(pairs_obj)
    }

    fn parse_uint_v_with_id(&mut self, start: StartElement) -> Result<crate::model::UIntVWithId> {
        use crate::model::UIntVWithId;

        let mut id: Option<u32> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        let mut profiles: Option<String> = None;
        let mut content_type: Option<String> = None;

        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => {
                    id = Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "profiles" => profiles = Some(attribute.value),
                "contentType" => content_type = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut v = UIntVWithId::new(id.ok_or_else(|| self.missing_attribute("id"))?);
        v.profiles = profiles;
        v.content_type = content_type;
        v.unknown_attributes = unknown_attributes;

        let values_text = self.parse_text_content()?;
        v.values = parse_uint_vector(&values_text)?;
        Ok(v)
    }

    fn parse_initialization_set(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::InitializationSet> {
        use crate::model::InitializationSet;

        let mut init_set = InitializationSet::new(0);
        let mut id: Option<u32> = None;

        for attribute in start.attributes {
            let Some(attribute) =
                self.apply_representation_base_attribute(&mut init_set.base, attribute)?
            else {
                continue;
            };
            match attribute.name.as_str() {
                "id" => {
                    id = Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "inAllPeriods" => {
                    init_set.in_all_periods =
                        self.in_attribute("inAllPeriods", parse_xs_boolean(&attribute.value))?;
                }
                "contentType" => {
                    init_set.content_type =
                        Some(self.parse_attribute("contentType", &attribute.value)?);
                }
                "par" => {
                    init_set.par = Some(self.parse_attribute("par", &attribute.value)?);
                }
                "maxWidth" => {
                    init_set.max_width = Some(
                        self.in_attribute("maxWidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "maxHeight" => {
                    init_set.max_height = Some(
                        self.in_attribute("maxHeight", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "maxFrameRate" => {
                    init_set.max_frame_rate =
                        Some(self.parse_attribute("maxFrameRate", &attribute.value)?);
                }
                "initialization" => init_set.initialization = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => init_set
                    .base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        init_set.id = id.ok_or_else(|| self.missing_attribute("id"))?;

        while let Some(child) = self.next_content_event()? {
            let Some(child) = self.apply_representation_base_child(&mut init_set.base, child)?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "Accessibility") {
                self.path.push(PathSegment {
                    element_name: "Accessibility",
                    sibling_index: Some(init_set.accessibilities.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                init_set.accessibilities.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Role") {
                self.path.push(PathSegment {
                    element_name: "Role",
                    sibling_index: Some(init_set.roles.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                init_set.roles.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Rating") {
                self.path.push(PathSegment {
                    element_name: "Rating",
                    sibling_index: Some(init_set.ratings.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                init_set.ratings.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Viewpoint") {
                self.path.push(PathSegment {
                    element_name: "Viewpoint",
                    sibling_index: Some(init_set.viewpoints.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                init_set.viewpoints.push(descriptor);
            } else {
                init_set
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(init_set)
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

    /// Handles the `FramePacking`, `AudioChannelConfiguration`, `ContentProtection`,
    /// `OutputProtection`, `EssentialProperty`, and `SupplementalProperty` children
    /// shared by `Period`, `AdaptationSet`, and `Representation` through the
    /// embedded `RepresentationBase`, returning the element unconsumed when it is
    /// none of those six.
    fn apply_representation_base_child(
        &mut self,
        base: &mut RepresentationBase,
        child: StartElement,
    ) -> Result<Option<StartElement>> {
        if child.matches(MPD_NAMESPACE, "FramePacking") {
            self.path.push(PathSegment {
                element_name: "FramePacking",
                sibling_index: Some(base.frame_packings.len()),
            });
            let desc = self.parse_descriptor(child)?;
            self.path.pop();
            base.frame_packings.push(desc);
        } else if child.matches(MPD_NAMESPACE, "AudioChannelConfiguration") {
            self.path.push(PathSegment {
                element_name: "AudioChannelConfiguration",
                sibling_index: Some(base.audio_channel_configurations.len()),
            });
            let desc = self.parse_descriptor(child)?;
            self.path.pop();
            base.audio_channel_configurations.push(desc);
        } else if child.matches(MPD_NAMESPACE, "ContentProtection") {
            self.path.push(PathSegment {
                element_name: "ContentProtection",
                sibling_index: Some(base.content_protections.len()),
            });
            let cp = self.parse_content_protection(child)?;
            self.path.pop();
            base.content_protections.push(cp);
        } else if child.matches(MPD_NAMESPACE, "OutputProtection") {
            self.parse_singular_child(
                &mut base.output_protection,
                "OutputProtection",
                child,
                Self::parse_descriptor,
            )?;
        } else if child.matches(MPD_NAMESPACE, "EssentialProperty") {
            self.path.push(PathSegment {
                element_name: "EssentialProperty",
                sibling_index: Some(base.essential_properties.len()),
            });
            let desc = self.parse_descriptor(child)?;
            self.path.pop();
            base.essential_properties.push(desc);
        } else if child.matches(MPD_NAMESPACE, "SupplementalProperty") {
            self.path.push(PathSegment {
                element_name: "SupplementalProperty",
                sibling_index: Some(base.supplemental_properties.len()),
            });
            let desc = self.parse_descriptor(child)?;
            self.path.pop();
            base.supplemental_properties.push(desc);
        } else if child.matches(MPD_NAMESPACE, "InbandEventStream") {
            self.path.push(PathSegment {
                element_name: "InbandEventStream",
                sibling_index: Some(base.inband_event_streams.len()),
            });
            let event_stream = self.parse_event_stream(child)?;
            self.path.pop();
            base.inband_event_streams.push(event_stream);
        } else if child.matches(MPD_NAMESPACE, "Switching") {
            self.path.push(PathSegment {
                element_name: "Switching",
                sibling_index: Some(base.switchings.len()),
            });
            let switching = self.parse_switching(child)?;
            self.path.pop();
            base.switchings.push(switching);
        } else if child.matches(MPD_NAMESPACE, "RandomAccess") {
            self.path.push(PathSegment {
                element_name: "RandomAccess",
                sibling_index: Some(base.random_accesses.len()),
            });
            let random_access = self.parse_random_access(child)?;
            self.path.pop();
            base.random_accesses.push(random_access);
        } else if child.matches(MPD_NAMESPACE, "GroupLabel") {
            self.path.push(PathSegment {
                element_name: "GroupLabel",
                sibling_index: Some(base.group_labels.len()),
            });
            let label = self.parse_label(child)?;
            self.path.pop();
            base.group_labels.push(label);
        } else if child.matches(MPD_NAMESPACE, "Label") {
            self.path.push(PathSegment {
                element_name: "Label",
                sibling_index: Some(base.labels.len()),
            });
            let label = self.parse_label(child)?;
            self.path.pop();
            base.labels.push(label);
        } else if child.matches(MPD_NAMESPACE, "ProducerReferenceTime") {
            self.path.push(PathSegment {
                element_name: "ProducerReferenceTime",
                sibling_index: Some(base.producer_reference_times.len()),
            });
            let producer_reference_time = self.parse_producer_reference_time(child)?;
            self.path.pop();
            base.producer_reference_times.push(producer_reference_time);
        } else if child.matches(MPD_NAMESPACE, "ContentPopularityRate") {
            self.path.push(PathSegment {
                element_name: "ContentPopularityRate",
                sibling_index: Some(base.content_popularity_rates.len()),
            });
            let content_popularity_rate = self.parse_content_popularity_rate(child)?;
            self.path.pop();
            base.content_popularity_rates.push(content_popularity_rate);
        } else if child.matches(MPD_NAMESPACE, "Resync") {
            self.path.push(PathSegment {
                element_name: "Resync",
                sibling_index: Some(base.resyncs.len()),
            });
            let resync = self.parse_resync(child)?;
            self.path.pop();
            base.resyncs.push(resync);
        } else {
            return Ok(Some(child));
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

    fn parse_event_stream(&mut self, start: StartElement) -> Result<crate::model::EventStream> {
        use crate::model::EventStream;

        let mut scheme_id_uri: Option<String> = None;
        let mut event_stream_value: Option<String> = None;
        let mut timescale: Option<u32> = None;
        let mut presentation_time_offset: u64 = 0;
        let mut href: Option<String> = None;
        let mut actuate: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "schemeIdUri" => scheme_id_uri = Some(attribute.value),
                "value" => event_stream_value = Some(attribute.value),
                "timescale" => {
                    timescale = Some(
                        self.in_attribute("timescale", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "presentationTimeOffset" => {
                    presentation_time_offset = self.in_attribute(
                        "presentationTimeOffset",
                        parse_xs_unsigned_long(&attribute.value),
                    )?;
                }
                "xlink:href" => href = Some(attribute.value),
                "xlink:actuate" => actuate = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut event_stream =
            EventStream::new(scheme_id_uri.ok_or_else(|| self.missing_attribute("schemeIdUri"))?);
        event_stream.value = event_stream_value;
        event_stream.timescale = timescale;
        event_stream.presentation_time_offset = presentation_time_offset;
        event_stream.href = href;
        event_stream.actuate = actuate;
        event_stream.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Event") {
                self.path.push(PathSegment {
                    element_name: "Event",
                    sibling_index: Some(event_stream.events.len()),
                });
                let event = self.parse_event(child)?;
                self.path.pop();
                event_stream.events.push(event);
            } else {
                event_stream
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(event_stream)
    }

    fn parse_event(&mut self, start: StartElement) -> Result<crate::model::Event> {
        let mut event = crate::model::Event::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "presentationTime" => {
                    event.presentation_time = self.in_attribute(
                        "presentationTime",
                        parse_xs_unsigned_long(&attribute.value),
                    )?;
                }
                "duration" => {
                    event.duration = Some(
                        self.in_attribute("duration", parse_xs_unsigned_long(&attribute.value))?,
                    );
                }
                "id" => {
                    event.id =
                        Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "contentEncoding" => {
                    event.content_encoding =
                        Some(self.parse_attribute("contentEncoding", &attribute.value)?);
                }
                "messageData" => event.message_data = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => event
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        // `EventType` is `mixed="true"`. The model keeps the text content and
        // the child elements in separate fields, so text chunks interleaved
        // between child elements are concatenated and the serializer re-emits
        // all text before the children. A document that alternates text and
        // elements therefore round-trips to an equivalent model but not to a
        // byte-identical document; no real-world Event payload relies on that
        // interleaving.
        let mut text = String::new();
        loop {
            match self.reader.read_event()? {
                Event::Start(child) => {
                    event
                        .unknown_children
                        .push(self.parse_unknown_element(child, 0)?);
                }
                Event::Text(chunk) => text.push_str(&chunk),
                Event::End => break,
                Event::Eof => {
                    return Err(self
                        .element_error(ErrorKind::Xml("unexpected end of document".to_string())));
                }
            }
        }
        if !text.is_empty() {
            event.text_content = Some(text);
        }
        Ok(event)
    }

    fn parse_label(&mut self, start: StartElement) -> Result<crate::model::Label> {
        use crate::model::Label;

        let mut id: u32 = 0;
        let mut lang: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => id = self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?,
                "lang" => lang = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut label = Label::new(self.parse_text_content()?);
        label.id = id;
        label.lang = lang;
        label.unknown_attributes = unknown_attributes;
        Ok(label)
    }

    fn parse_subset(&mut self, start: StartElement) -> Result<crate::model::Subset> {
        use crate::model::Subset;

        let mut contains: Option<Vec<u32>> = None;
        let mut id: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "contains" => {
                    contains =
                        Some(self.in_attribute("contains", parse_uint_vector(&attribute.value))?);
                }
                "id" => id = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut subset = Subset::new(contains.ok_or_else(|| self.missing_attribute("contains"))?);
        subset.id = id;
        subset.unknown_attributes = unknown_attributes;
        self.consume_empty_element()?;
        Ok(subset)
    }

    fn parse_switching(&mut self, start: StartElement) -> Result<crate::model::Switching> {
        use crate::model::Switching;

        let mut interval: Option<u32> = None;
        let mut switching_type = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "interval" => {
                    interval = Some(
                        self.in_attribute("interval", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "type" => switching_type = Some(self.parse_attribute("type", &attribute.value)?),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut switching =
            Switching::new(interval.ok_or_else(|| self.missing_attribute("interval"))?);
        if let Some(switching_type) = switching_type {
            switching.switching_type = switching_type;
        }
        switching.unknown_attributes = unknown_attributes;
        self.consume_empty_element()?;
        Ok(switching)
    }

    fn parse_random_access(&mut self, start: StartElement) -> Result<crate::model::RandomAccess> {
        use crate::model::RandomAccess;

        let mut interval: Option<u32> = None;
        let mut random_access_type = None;
        let mut min_buffer_time = None;
        let mut bandwidth: Option<u32> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "interval" => {
                    interval = Some(
                        self.in_attribute("interval", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "type" => {
                    random_access_type = Some(self.parse_attribute("type", &attribute.value)?)
                }
                "minBufferTime" => {
                    min_buffer_time =
                        Some(self.parse_attribute("minBufferTime", &attribute.value)?);
                }
                "bandwidth" => {
                    bandwidth = Some(
                        self.in_attribute("bandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut random_access =
            RandomAccess::new(interval.ok_or_else(|| self.missing_attribute("interval"))?);
        if let Some(random_access_type) = random_access_type {
            random_access.random_access_type = random_access_type;
        }
        random_access.min_buffer_time = min_buffer_time;
        random_access.bandwidth = bandwidth;
        random_access.unknown_attributes = unknown_attributes;
        self.consume_empty_element()?;
        Ok(random_access)
    }

    fn parse_producer_reference_time(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ProducerReferenceTime> {
        use crate::model::ProducerReferenceTime;

        let mut id: Option<u32> = None;
        let mut inband: Option<bool> = None;
        let mut producer_reference_time_type = None;
        let mut application_scheme: Option<String> = None;
        let mut wall_clock_time: Option<String> = None;
        let mut presentation_time: Option<u64> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => {
                    id = Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?)
                }
                "inband" => {
                    inband = Some(self.in_attribute("inband", parse_xs_boolean(&attribute.value))?);
                }
                "type" => {
                    producer_reference_time_type =
                        Some(self.parse_attribute("type", &attribute.value)?);
                }
                "applicationScheme" => application_scheme = Some(attribute.value),
                "wallClockTime" => wall_clock_time = Some(attribute.value),
                "presentationTime" => {
                    presentation_time = Some(self.in_attribute(
                        "presentationTime",
                        parse_xs_unsigned_long(&attribute.value),
                    )?);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut producer_reference_time = ProducerReferenceTime::new(
            id.ok_or_else(|| self.missing_attribute("id"))?,
            wall_clock_time.ok_or_else(|| self.missing_attribute("wallClockTime"))?,
            presentation_time.ok_or_else(|| self.missing_attribute("presentationTime"))?,
        );
        if let Some(inband) = inband {
            producer_reference_time.inband = inband;
        }
        if let Some(producer_reference_time_type) = producer_reference_time_type {
            producer_reference_time.producer_reference_time_type = producer_reference_time_type;
        }
        producer_reference_time.application_scheme = application_scheme;
        producer_reference_time.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "UTCTiming") {
                self.parse_singular_child(
                    &mut producer_reference_time.utc_timing,
                    "UTCTiming",
                    child,
                    Self::parse_descriptor,
                )?;
            } else {
                producer_reference_time
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(producer_reference_time)
    }

    fn parse_content_popularity_rate(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ContentPopularityRate> {
        use crate::model::ContentPopularityRate;

        let mut source = None;
        let mut source_description: Option<String> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "source" => source = Some(self.parse_attribute("source", &attribute.value)?),
                "source_description" => source_description = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut content_popularity_rate =
            ContentPopularityRate::new(source.ok_or_else(|| self.missing_attribute("source"))?);
        content_popularity_rate.source_description = source_description;
        content_popularity_rate.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "PR") {
                self.path.push(PathSegment {
                    element_name: "PR",
                    sibling_index: Some(content_popularity_rate.rates.len()),
                });
                let rate = self.parse_popularity_rate(child)?;
                self.path.pop();
                content_popularity_rate.rates.push(rate);
            } else {
                content_popularity_rate
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(content_popularity_rate)
    }

    fn parse_popularity_rate(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::PopularityRate> {
        use crate::model::PopularityRate;

        let mut rate = PopularityRate::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "popularityRate" => {
                    rate.popularity_rate =
                        Some(self.in_attribute(
                            "popularityRate",
                            parse_xs_unsigned_int(&attribute.value),
                        )?);
                }
                "start" => {
                    rate.start =
                        Some(self.in_attribute("start", parse_xs_unsigned_long(&attribute.value))?);
                }
                "r" => rate.r = self.in_attribute("r", parse_xs_int(&attribute.value))?,
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => rate
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        self.consume_empty_element()?;
        Ok(rate)
    }

    fn parse_resync(&mut self, start: StartElement) -> Result<crate::model::Resync> {
        use crate::model::Resync;

        let mut resync = Resync::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "type" => resync.resync_type = self.parse_attribute("type", &attribute.value)?,
                "dT" => {
                    resync.dt =
                        Some(self.in_attribute("dT", parse_xs_unsigned_int(&attribute.value))?);
                }
                "dImax" => {
                    resync.di_max =
                        Some(self.in_attribute("dImax", parse_xs_float(&attribute.value))?);
                }
                "dImin" => {
                    resync.di_min = self.in_attribute("dImin", parse_xs_float(&attribute.value))?;
                }
                "marker" => {
                    resync.marker =
                        self.in_attribute("marker", parse_xs_boolean(&attribute.value))?;
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => resync
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }
        self.consume_empty_element()?;
        Ok(resync)
    }

    fn parse_content_component(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ContentComponent> {
        use crate::model::ContentComponent;

        let mut content_component = ContentComponent::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "id" => {
                    content_component.id =
                        Some(self.in_attribute("id", parse_xs_unsigned_int(&attribute.value))?);
                }
                "lang" => content_component.lang = Some(attribute.value),
                "contentType" => {
                    content_component.content_type =
                        Some(self.parse_attribute("contentType", &attribute.value)?);
                }
                "par" => {
                    content_component.par = Some(self.parse_attribute("par", &attribute.value)?)
                }
                "tag" => content_component.tag = Some(attribute.value),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => content_component
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "Accessibility") {
                self.path.push(PathSegment {
                    element_name: "Accessibility",
                    sibling_index: Some(content_component.accessibilities.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                content_component.accessibilities.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Role") {
                self.path.push(PathSegment {
                    element_name: "Role",
                    sibling_index: Some(content_component.roles.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                content_component.roles.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Rating") {
                self.path.push(PathSegment {
                    element_name: "Rating",
                    sibling_index: Some(content_component.ratings.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                content_component.ratings.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Viewpoint") {
                self.path.push(PathSegment {
                    element_name: "Viewpoint",
                    sibling_index: Some(content_component.viewpoints.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                content_component.viewpoints.push(descriptor);
            } else {
                content_component
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(content_component)
    }

    fn parse_extended_bandwidth(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::ExtendedBandwidth> {
        use crate::model::ExtendedBandwidth;

        let mut extended_bandwidth = ExtendedBandwidth::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "vbr" => {
                    extended_bandwidth.vbr =
                        self.in_attribute("vbr", parse_xs_boolean(&attribute.value))?;
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => extended_bandwidth
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if child.matches(MPD_NAMESPACE, "ModelPair") {
                self.path.push(PathSegment {
                    element_name: "ModelPair",
                    sibling_index: Some(extended_bandwidth.model_pairs.len()),
                });
                let model_pair = self.parse_model_pair(child)?;
                self.path.pop();
                extended_bandwidth.model_pairs.push(model_pair);
            } else {
                extended_bandwidth
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(extended_bandwidth)
    }

    fn parse_model_pair(&mut self, start: StartElement) -> Result<crate::model::ModelPair> {
        use crate::model::ModelPair;

        let mut buffer_time = None;
        let mut bandwidth: Option<u32> = None;
        let mut unknown_attributes: Vec<(String, String)> = Vec::new();
        for attribute in start.attributes {
            match attribute.name.as_str() {
                "bufferTime" => {
                    buffer_time = Some(self.parse_attribute("bufferTime", &attribute.value)?);
                }
                "bandwidth" => {
                    bandwidth = Some(
                        self.in_attribute("bandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => unknown_attributes.push((attribute.name, attribute.value)),
            }
        }

        let mut model_pair = ModelPair::new(
            buffer_time.ok_or_else(|| self.missing_attribute("bufferTime"))?,
            bandwidth.ok_or_else(|| self.missing_attribute("bandwidth"))?,
        );
        model_pair.unknown_attributes = unknown_attributes;

        while let Some(child) = self.next_content_event()? {
            model_pair
                .unknown_children
                .push(self.parse_unknown_element(child, 0)?);
        }
        Ok(model_pair)
    }

    fn parse_preselection(&mut self, start: StartElement) -> Result<crate::model::Preselection> {
        use crate::model::Preselection;

        let mut base = RepresentationBase::new();
        let mut id: Option<String> = None;
        let mut preselection_components: Option<Vec<String>> = None;
        let mut lang: Option<String> = None;
        let mut order = None;
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
                "preselectionComponents" => {
                    preselection_components = Some(parse_string_vector(&attribute.value));
                }
                "lang" => lang = Some(attribute.value),
                "order" => order = Some(self.parse_attribute("order", &attribute.value)?),
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        let mut preselection = Preselection::new(
            preselection_components
                .ok_or_else(|| self.missing_attribute("preselectionComponents"))?,
        );
        preselection.base = base;
        if let Some(id) = id {
            preselection.id = id;
        }
        preselection.lang = lang;
        if let Some(order) = order {
            preselection.order = order;
        }

        while let Some(child) = self.next_content_event()? {
            let Some(child) =
                self.apply_representation_base_child(&mut preselection.base, child)?
            else {
                continue;
            };
            if child.matches(MPD_NAMESPACE, "Accessibility") {
                self.path.push(PathSegment {
                    element_name: "Accessibility",
                    sibling_index: Some(preselection.accessibilities.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                preselection.accessibilities.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Role") {
                self.path.push(PathSegment {
                    element_name: "Role",
                    sibling_index: Some(preselection.roles.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                preselection.roles.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Rating") {
                self.path.push(PathSegment {
                    element_name: "Rating",
                    sibling_index: Some(preselection.ratings.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                preselection.ratings.push(descriptor);
            } else if child.matches(MPD_NAMESPACE, "Viewpoint") {
                self.path.push(PathSegment {
                    element_name: "Viewpoint",
                    sibling_index: Some(preselection.viewpoints.len()),
                });
                let descriptor = self.parse_descriptor(child)?;
                self.path.pop();
                preselection.viewpoints.push(descriptor);
            } else {
                preselection
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(preselection)
    }

    fn parse_sub_representation(
        &mut self,
        start: StartElement,
    ) -> Result<crate::model::SubRepresentation> {
        use crate::model::SubRepresentation;

        let mut sub_representation = SubRepresentation::new();
        for attribute in start.attributes {
            let Some(attribute) =
                self.apply_representation_base_attribute(&mut sub_representation.base, attribute)?
            else {
                continue;
            };
            match attribute.name.as_str() {
                "level" => {
                    sub_representation.level =
                        Some(self.in_attribute("level", parse_xs_unsigned_int(&attribute.value))?);
                }
                "dependencyLevel" => {
                    sub_representation.dependency_level =
                        self.in_attribute("dependencyLevel", parse_uint_vector(&attribute.value))?;
                }
                "bandwidth" => {
                    sub_representation.bandwidth = Some(
                        self.in_attribute("bandwidth", parse_xs_unsigned_int(&attribute.value))?,
                    );
                }
                "contentComponent" => {
                    sub_representation.content_component = parse_string_vector(&attribute.value);
                }
                "xmlns" => self.check_default_namespace_declaration(&attribute.value)?,
                _ => sub_representation
                    .base
                    .unknown_attributes
                    .push((attribute.name, attribute.value)),
            }
        }

        while let Some(child) = self.next_content_event()? {
            if let Some(child) =
                self.apply_representation_base_child(&mut sub_representation.base, child)?
            {
                sub_representation
                    .base
                    .unknown_children
                    .push(self.parse_unknown_element(child, 0)?);
            }
        }
        Ok(sub_representation)
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

    fn parse_text_content(&mut self) -> Result<String> {
        let mut text = String::new();
        loop {
            match self.reader.read_event()? {
                Event::Start(_) => {
                    return Err(self.element_error(ErrorKind::UnexpectedElement {
                        name: "unexpected child element".to_string(),
                    }));
                }
                Event::End => return Ok(text),
                Event::Text(t) => text.push_str(&t),
                Event::Eof => {
                    return Err(self
                        .element_error(ErrorKind::Xml("unexpected end of document".to_string())));
                }
            }
        }
    }

    fn consume_empty_element(&mut self) -> Result<()> {
        loop {
            match self.reader.read_event()? {
                Event::End => return Ok(()),
                Event::Text(text) if is_xml_whitespace(&text) => {}
                Event::Text(_) => return Err(self.element_error(ErrorKind::UnexpectedText)),
                Event::Start(_) => {
                    return Err(self.element_error(ErrorKind::UnexpectedElement {
                        name: "unexpected child element".to_string(),
                    }));
                }
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

fn parse_xs_float(value: &str) -> Result<f32> {
    const EXPECTED: &str = "an `xs:float`";
    match value {
        "INF" => Ok(f32::INFINITY),
        "-INF" => Ok(f32::NEG_INFINITY),
        "NaN" => Ok(f32::NAN),
        _ => {
            // Rust の float パーサは `inf` や `nan` も受理するため、
            // xs:float の字句空間に現れる文字だけを通す。
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

fn parse_xs_int(value: &str) -> Result<i32> {
    const EXPECTED: &str = "an `xs:int`";
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_value(value, EXPECTED));
    }
    value.parse().map_err(|_| invalid_value(value, EXPECTED))
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
            "<FutureExtension><Detail>demo</Detail></FutureExtension>",
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
            [future_extension] => {
                assert_eq!(future_extension.name, "FutureExtension");
                assert_eq!(
                    future_extension.namespace.as_deref(),
                    Some("urn:mpeg:dash:schema:mpd:2011")
                );
                match future_extension.children.as_slice() {
                    [Node::Element(detail)] => {
                        assert_eq!(detail.name, "Detail");
                        assert_eq!(detail.children, vec![Node::Text("demo".to_string())]);
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
        match adaptation_set.base.content_protections.as_slice() {
            [content_protection] => {
                assert_eq!(
                    content_protection.base.scheme_id_uri,
                    "urn:mpeg:dash:mp4protection:2011"
                );
                match content_protection.base.unknown_children.as_slice() {
                    [pssh] => {
                        assert_eq!(pssh.name, "cenc:pssh");
                        assert_eq!(pssh.namespace.as_deref(), Some("urn:mpeg:cenc:2013"));
                        assert_eq!(pssh.children, vec![Node::Text("AAAA".to_string())]);
                    }
                    other => panic!("unexpected children: {other:?}"),
                }
            }
            other => panic!("unexpected content protections: {other:?}"),
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
    fn deep_path_segments_for_manifest_level_elements() {
        // ServiceDescription > Latency with invalid referenceId attribute
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<ServiceDescription><Latency referenceId="invalid"/></ServiceDescription>"#,
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(
            error.path.contains("ServiceDescription[0]")
                && error.path.contains("Latency[0]")
                && error.path.contains("referenceId"),
            "expected deep path with ServiceDescription and Latency: {}",
            error.path
        );
    }

    #[test]
    fn initialization_set_missing_id_shows_path() {
        // InitializationSet without required id attribute
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<InitializationSet contentType="video"/>"#,
            "</MPD>",
        );
        let error = mpd_from_slice(input.as_bytes()).unwrap_err();
        assert!(
            error.path.contains("InitializationSet[0]") && error.path.contains("@ id"),
            "expected path with InitializationSet and missing id: {}",
            error.path
        );
    }

    #[test]
    fn content_after_the_root_element_is_rejected() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period/></MPD><MPD/>",
        );
        assert!(mpd_from_slice(input.as_bytes()).is_err());
    }

    #[test]
    fn adaptation_set_with_representation_base_children_roundtrips() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period>",
            r#"<AdaptationSet contentType="video">"#,
            r#"<FramePacking schemeIdUri="test:fp" value="1"/>"#,
            r#"<ContentProtection schemeIdUri="test:cp" value="2"/>"#,
            r#"<AudioChannelConfiguration schemeIdUri="test:acc" value="3"/>"#,
            r#"<Accessibility schemeIdUri="test:acc1"/>"#,
            r#"<Role schemeIdUri="test:role"/>"#,
            "</AdaptationSet>",
            "</Period>",
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
        assert_eq!(adaptation_set.base.frame_packings.len(), 1);
        assert_eq!(adaptation_set.base.content_protections.len(), 1);
        assert_eq!(adaptation_set.base.audio_channel_configurations.len(), 1);
        assert_eq!(adaptation_set.accessibilities.len(), 1);
        assert_eq!(adaptation_set.roles.len(), 1);
    }

    #[test]
    fn initialization_set_with_representation_base_children_roundtrips() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<InitializationSet id="1" contentType="video">"#,
            r#"<FramePacking schemeIdUri="test:fp" value="1"/>"#,
            r#"<AudioChannelConfiguration schemeIdUri="test:acc" value="2"/>"#,
            r#"<EssentialProperty schemeIdUri="test:ep" value="3"/>"#,
            r#"<Accessibility schemeIdUri="test:acc1"/>"#,
            r#"<Role schemeIdUri="test:role"/>"#,
            "</InitializationSet>",
            "</MPD>",
        );
        let mpd = mpd_from_slice(input.as_bytes()).unwrap();
        let init_set = mpd.initialization_sets.first().unwrap();
        assert_eq!(init_set.base.frame_packings.len(), 1);
        assert_eq!(init_set.base.audio_channel_configurations.len(), 1);
        assert_eq!(init_set.base.essential_properties.len(), 1);
        assert_eq!(init_set.accessibilities.len(), 1);
        assert_eq!(init_set.roles.len(), 1);
    }
}
