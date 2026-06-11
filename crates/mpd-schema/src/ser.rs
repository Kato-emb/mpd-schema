//! Struct-to-event serialization.
//!
//! Known elements are written unprefixed in the canonical XSD order with the
//! default namespace declared on the root, while unknown content is written
//! back lexically after the known children (ADR-0003, ARCHITECTURE.md).

use std::fmt;
use std::io;

use crate::backend::Writer;
use crate::error::{Error, ErrorKind, Result};
use crate::event::{Attribute, Event, StartElement};
use crate::model::descriptor::{ContentProtection, Descriptor};
use crate::model::element::{Element, MAX_UNKNOWN_ELEMENT_DEPTH, Node};
use crate::model::mpd::{
    AdaptationSet, MPD_NAMESPACE, Mpd, Period, Representation, RepresentationBase,
};
use crate::model::segment::{
    FailoverContent, Fcs, MultipleSegmentBase, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, SegmentUrl, Url,
};
use crate::model::types::invalid_value;

pub(crate) fn write_mpd<W: io::Write>(mpd: &Mpd, sink: W) -> Result<W> {
    let mut writer = Writer::new(sink);
    emit_mpd(&mut writer, mpd)?;
    Ok(writer.into_inner())
}

fn emit_mpd<W: io::Write>(writer: &mut Writer<W>, mpd: &Mpd) -> Result<()> {
    let mut attributes = Vec::new();
    // パース時に既定名前空間宣言を保持しない方針（de.rs 参照）の対であり、
    // ルートで宣言し直す。手組みの構造体が受け皿に `xmlns` を持つ場合だけ
    // 重複宣言を避けるため譲る。
    if !mpd
        .unknown_attributes
        .iter()
        .any(|(name, _)| name == "xmlns")
    {
        push_attribute(&mut attributes, "xmlns", MPD_NAMESPACE);
    }
    push_optional(&mut attributes, "id", mpd.id.as_ref());
    push_attribute(&mut attributes, "profiles", &mpd.profiles);
    push_optional(&mut attributes, "type", mpd.presentation_type.as_ref());
    push_optional(
        &mut attributes,
        "availabilityStartTime",
        mpd.availability_start_time.as_ref(),
    );
    push_optional(
        &mut attributes,
        "availabilityEndTime",
        mpd.availability_end_time.as_ref(),
    );
    push_optional(&mut attributes, "publishTime", mpd.publish_time.as_ref());
    push_optional(
        &mut attributes,
        "mediaPresentationDuration",
        mpd.media_presentation_duration.as_ref(),
    );
    push_optional(
        &mut attributes,
        "minimumUpdatePeriod",
        mpd.minimum_update_period.as_ref(),
    );
    push_attribute(&mut attributes, "minBufferTime", mpd.min_buffer_time);
    push_optional(
        &mut attributes,
        "timeShiftBufferDepth",
        mpd.time_shift_buffer_depth.as_ref(),
    );
    push_optional(
        &mut attributes,
        "suggestedPresentationDelay",
        mpd.suggested_presentation_delay.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxSegmentDuration",
        mpd.max_segment_duration.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxSubsegmentDuration",
        mpd.max_subsegment_duration.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &mpd.unknown_attributes)?;

    start_element(writer, "MPD", attributes)?;
    for pi in &mpd.program_informations {
        emit_program_information(writer, pi)?;
    }
    for base_url in &mpd.base_urls {
        emit_base_url(writer, base_url)?;
    }
    for location in &mpd.locations {
        start_element(writer, "Location", Vec::new())?;
        writer.write_event(&Event::Text(location.clone()))?;
        writer.write_event(&Event::End)?;
    }
    for patch_loc in &mpd.patch_locations {
        emit_patch_location(writer, patch_loc)?;
    }
    for sd in &mpd.service_descriptions {
        emit_service_description(writer, sd)?;
    }
    for init_set in &mpd.initialization_sets {
        emit_initialization_set(writer, init_set)?;
    }
    for init_group in &mpd.initialization_groups {
        emit_uint_v_with_id(writer, "InitializationGroup", init_group)?;
    }
    for init_pres in &mpd.initialization_presentations {
        emit_uint_v_with_id(writer, "InitializationPresentation", init_pres)?;
    }
    for cp in &mpd.content_protections {
        emit_content_protection(writer, cp)?;
    }
    for period in &mpd.periods {
        emit_period(writer, period)?;
    }
    for metrics in &mpd.metrics {
        emit_metrics(writer, metrics)?;
    }
    for desc in &mpd.essential_properties {
        emit_descriptor(writer, desc, "EssentialProperty")?;
    }
    for desc in &mpd.supplemental_properties {
        emit_descriptor(writer, desc, "SupplementalProperty")?;
    }
    for desc in &mpd.utc_timings {
        emit_descriptor(writer, desc, "UTCTiming")?;
    }
    if let Some(leap_sec) = &mpd.leap_second_information {
        emit_leap_second_information(writer, leap_sec)?;
    }
    emit_unknown_children(writer, &mpd.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_period<W: io::Write>(writer: &mut Writer<W>, period: &Period) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "id", period.id.as_ref());
    push_optional(&mut attributes, "start", period.start.as_ref());
    push_optional(&mut attributes, "duration", period.duration.as_ref());
    push_optional(
        &mut attributes,
        "bitstreamSwitching",
        period.bitstream_switching.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &period.unknown_attributes)?;

    start_element(writer, "Period", attributes)?;
    for base_url in &period.base_urls {
        emit_base_url(writer, base_url)?;
    }
    emit_segment_children(
        writer,
        period.segment_base.as_ref(),
        period.segment_list.as_ref(),
        period.segment_template.as_ref(),
    )?;
    if let Some(desc) = &period.asset_identifier {
        emit_descriptor(writer, desc, "AssetIdentifier")?;
    }
    for es in &period.event_streams {
        emit_event_stream(writer, es, "EventStream")?;
    }
    for sd in &period.service_descriptions {
        emit_service_description(writer, sd)?;
    }
    for cp in &period.content_protections {
        emit_content_protection(writer, cp)?;
    }
    for adaptation_set in &period.adaptation_sets {
        emit_adaptation_set(writer, adaptation_set, "AdaptationSet")?;
    }
    for subset in &period.subsets {
        emit_subset(writer, subset)?;
    }
    for desc in &period.supplemental_properties {
        emit_descriptor(writer, desc, "SupplementalProperty")?;
    }
    for empty_as in &period.empty_adaptation_sets {
        emit_adaptation_set(writer, empty_as, "EmptyAdaptationSet")?;
    }
    for label in &period.group_labels {
        emit_label(writer, label, "GroupLabel")?;
    }
    for preselection in &period.preselections {
        emit_preselection(writer, preselection)?;
    }
    emit_unknown_children(writer, &period.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_adaptation_set<W: io::Write>(
    writer: &mut Writer<W>,
    adaptation_set: &AdaptationSet,
    element_name: &str,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_representation_base_attributes(&mut attributes, &adaptation_set.base);
    push_optional(&mut attributes, "id", adaptation_set.id.as_ref());
    push_optional(&mut attributes, "group", adaptation_set.group.as_ref());
    push_optional(&mut attributes, "lang", adaptation_set.lang.as_ref());
    push_optional(
        &mut attributes,
        "contentType",
        adaptation_set.content_type.as_ref(),
    );
    push_optional(&mut attributes, "par", adaptation_set.par.as_ref());
    push_optional(
        &mut attributes,
        "minBandwidth",
        adaptation_set.min_bandwidth.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxBandwidth",
        adaptation_set.max_bandwidth.as_ref(),
    );
    push_optional(
        &mut attributes,
        "minWidth",
        adaptation_set.min_width.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxWidth",
        adaptation_set.max_width.as_ref(),
    );
    push_optional(
        &mut attributes,
        "minHeight",
        adaptation_set.min_height.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxHeight",
        adaptation_set.max_height.as_ref(),
    );
    push_optional(
        &mut attributes,
        "minFrameRate",
        adaptation_set.min_frame_rate.as_ref(),
    );
    push_optional(
        &mut attributes,
        "maxFrameRate",
        adaptation_set.max_frame_rate.as_ref(),
    );
    push_optional(
        &mut attributes,
        "segmentAlignment",
        adaptation_set.segment_alignment.as_ref(),
    );
    push_optional(
        &mut attributes,
        "subsegmentAlignment",
        adaptation_set.subsegment_alignment.as_ref(),
    );
    push_optional(
        &mut attributes,
        "subsegmentStartsWithSAP",
        adaptation_set.subsegment_starts_with_sap.as_ref(),
    );
    push_optional(
        &mut attributes,
        "bitstreamSwitching",
        adaptation_set.bitstream_switching.as_ref(),
    );
    push_list(
        &mut attributes,
        "initializationSetRef",
        &adaptation_set.initialization_set_ref,
    );
    push_optional(
        &mut attributes,
        "initializationPrincipal",
        adaptation_set.initialization_principal.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &adaptation_set.base.unknown_attributes)?;

    start_element(writer, element_name, attributes)?;
    emit_representation_base_children(writer, &adaptation_set.base)?;
    for desc in &adaptation_set.accessibilities {
        emit_descriptor(writer, desc, "Accessibility")?;
    }
    for desc in &adaptation_set.roles {
        emit_descriptor(writer, desc, "Role")?;
    }
    for desc in &adaptation_set.ratings {
        emit_descriptor(writer, desc, "Rating")?;
    }
    for desc in &adaptation_set.viewpoints {
        emit_descriptor(writer, desc, "Viewpoint")?;
    }
    for cc in &adaptation_set.content_components {
        emit_content_component(writer, cc)?;
    }
    for base_url in &adaptation_set.base_urls {
        emit_base_url(writer, base_url)?;
    }
    emit_segment_children(
        writer,
        adaptation_set.segment_base.as_ref(),
        adaptation_set.segment_list.as_ref(),
        adaptation_set.segment_template.as_ref(),
    )?;
    for representation in &adaptation_set.representations {
        emit_representation(writer, representation)?;
    }
    emit_unknown_children(writer, &adaptation_set.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_representation<W: io::Write>(
    writer: &mut Writer<W>,
    representation: &Representation,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_representation_base_attributes(&mut attributes, &representation.base);
    push_attribute(&mut attributes, "id", &representation.id);
    push_attribute(&mut attributes, "bandwidth", representation.bandwidth);
    push_optional(
        &mut attributes,
        "qualityRanking",
        representation.quality_ranking.as_ref(),
    );
    push_list(
        &mut attributes,
        "dependencyId",
        &representation.dependency_id,
    );
    push_list(
        &mut attributes,
        "associationId",
        &representation.association_id,
    );
    push_list(
        &mut attributes,
        "associationType",
        &representation.association_type,
    );
    push_list(
        &mut attributes,
        "mediaStreamStructureId",
        &representation.media_stream_structure_id,
    );
    push_unknown_attributes(&mut attributes, &representation.base.unknown_attributes)?;

    start_element(writer, "Representation", attributes)?;
    emit_representation_base_children(writer, &representation.base)?;
    for base_url in &representation.base_urls {
        emit_base_url(writer, base_url)?;
    }
    for eb in &representation.extended_bandwidths {
        emit_extended_bandwidth(writer, eb)?;
    }
    for sr in &representation.sub_representations {
        emit_sub_representation(writer, sr)?;
    }
    emit_segment_children(
        writer,
        representation.segment_base.as_ref(),
        representation.segment_list.as_ref(),
        representation.segment_template.as_ref(),
    )?;
    emit_unknown_children(writer, &representation.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_representation_base_children<W: io::Write>(
    writer: &mut Writer<W>,
    base: &RepresentationBase,
) -> Result<()> {
    for desc in &base.frame_packings {
        emit_descriptor(writer, desc, "FramePacking")?;
    }
    for desc in &base.audio_channel_configurations {
        emit_descriptor(writer, desc, "AudioChannelConfiguration")?;
    }
    for cp in &base.content_protections {
        emit_content_protection(writer, cp)?;
    }
    if let Some(desc) = &base.output_protection {
        emit_descriptor(writer, desc, "OutputProtection")?;
    }
    for desc in &base.essential_properties {
        emit_descriptor(writer, desc, "EssentialProperty")?;
    }
    for desc in &base.supplemental_properties {
        emit_descriptor(writer, desc, "SupplementalProperty")?;
    }
    for es in &base.inband_event_streams {
        emit_event_stream(writer, es, "InbandEventStream")?;
    }
    for sw in &base.switchings {
        emit_switching(writer, sw)?;
    }
    for ra in &base.random_accesses {
        emit_random_access(writer, ra)?;
    }
    for label in &base.group_labels {
        emit_label(writer, label, "GroupLabel")?;
    }
    for label in &base.labels {
        emit_label(writer, label, "Label")?;
    }
    for prt in &base.producer_reference_times {
        emit_producer_reference_time(writer, prt)?;
    }
    for cpr in &base.content_popularity_rates {
        emit_content_popularity_rate(writer, cpr)?;
    }
    for resync in &base.resyncs {
        emit_resync(writer, resync)?;
    }
    Ok(())
}

fn emit_program_information<W: io::Write>(
    writer: &mut Writer<W>,
    pi: &crate::model::ProgramInformation,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "lang", pi.lang.as_ref());
    push_optional(
        &mut attributes,
        "moreInformationURL",
        pi.more_information_url.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &pi.unknown_attributes)?;
    start_element(writer, "ProgramInformation", attributes)?;
    if let Some(title) = &pi.title {
        start_element(writer, "Title", Vec::new())?;
        writer.write_event(&Event::Text(title.clone()))?;
        writer.write_event(&Event::End)?;
    }
    if let Some(source) = &pi.source {
        start_element(writer, "Source", Vec::new())?;
        writer.write_event(&Event::Text(source.clone()))?;
        writer.write_event(&Event::End)?;
    }
    if let Some(copyright) = &pi.copyright {
        start_element(writer, "Copyright", Vec::new())?;
        writer.write_event(&Event::Text(copyright.clone()))?;
        writer.write_event(&Event::End)?;
    }
    emit_unknown_children(writer, &pi.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_base_url<W: io::Write>(
    writer: &mut Writer<W>,
    base_url: &crate::model::BaseUrl,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(
        &mut attributes,
        "serviceLocation",
        base_url.service_location.as_ref(),
    );
    push_optional(&mut attributes, "byteRange", base_url.byte_range.as_ref());
    if let Some(availability_time_offset) = base_url.availability_time_offset {
        push_attribute(
            &mut attributes,
            "availabilityTimeOffset",
            xs_double_lexical(availability_time_offset),
        );
    }
    push_optional(
        &mut attributes,
        "availabilityTimeComplete",
        base_url.availability_time_complete.as_ref(),
    );
    push_optional(
        &mut attributes,
        "timeShiftBufferDepth",
        base_url.time_shift_buffer_depth.as_ref(),
    );
    if base_url.range_access {
        push_attribute(&mut attributes, "rangeAccess", "true");
    }
    push_unknown_attributes(&mut attributes, &base_url.unknown_attributes)?;
    start_element(writer, "BaseURL", attributes)?;
    writer.write_event(&Event::Text(base_url.url.clone()))?;
    writer.write_event(&Event::End)
}

fn emit_patch_location<W: io::Write>(
    writer: &mut Writer<W>,
    patch: &crate::model::PatchLocation,
) -> Result<()> {
    let mut attributes = Vec::new();
    if let Some(ttl) = patch.ttl {
        push_attribute(&mut attributes, "ttl", xs_double_lexical(ttl));
    }
    push_unknown_attributes(&mut attributes, &patch.unknown_attributes)?;
    start_element(writer, "PatchLocation", attributes)?;
    writer.write_event(&Event::Text(patch.url.clone()))?;
    writer.write_event(&Event::End)
}

fn emit_service_description<W: io::Write>(
    writer: &mut Writer<W>,
    sd: &crate::model::ServiceDescription,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "id", sd.id.as_ref());
    push_unknown_attributes(&mut attributes, &sd.unknown_attributes)?;
    start_element(writer, "ServiceDescription", attributes)?;
    for scope in &sd.scopes {
        emit_descriptor(writer, scope, "Scope")?;
    }
    for latency in &sd.latencies {
        emit_latency(writer, latency)?;
    }
    for rate in &sd.playback_rates {
        emit_playback_rate(writer, rate)?;
    }
    for quality in &sd.operating_qualities {
        emit_operating_quality(writer, quality)?;
    }
    for bandwidth in &sd.operating_bandwidths {
        emit_operating_bandwidth(writer, bandwidth)?;
    }
    emit_unknown_children(writer, &sd.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_latency<W: io::Write>(
    writer: &mut Writer<W>,
    latency: &crate::model::Latency,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(
        &mut attributes,
        "referenceId",
        latency.reference_id.as_ref(),
    );
    push_optional(&mut attributes, "target", latency.target.as_ref());
    push_optional(&mut attributes, "max", latency.max.as_ref());
    push_optional(&mut attributes, "min", latency.min.as_ref());
    push_unknown_attributes(&mut attributes, &latency.unknown_attributes)?;
    start_element(writer, "Latency", attributes)?;
    for quality_latency in &latency.quality_latencies {
        emit_uint_pairs_with_id(writer, quality_latency)?;
    }
    emit_unknown_children(writer, &latency.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_playback_rate<W: io::Write>(
    writer: &mut Writer<W>,
    rate: &crate::model::PlaybackRate,
) -> Result<()> {
    let mut attributes = Vec::new();
    if let Some(min) = rate.min {
        push_attribute(&mut attributes, "min", xs_double_lexical(min));
    }
    if let Some(max) = rate.max {
        push_attribute(&mut attributes, "max", xs_double_lexical(max));
    }
    push_unknown_attributes(&mut attributes, &rate.unknown_attributes)?;
    start_element(writer, "PlaybackRate", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_operating_quality<W: io::Write>(
    writer: &mut Writer<W>,
    quality: &crate::model::OperatingQuality,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "mediaType", quality.media_type);
    push_optional(&mut attributes, "min", quality.min.as_ref());
    push_optional(&mut attributes, "max", quality.max.as_ref());
    push_optional(&mut attributes, "target", quality.target.as_ref());
    push_optional(&mut attributes, "type", quality.quality_type.as_ref());
    push_optional(
        &mut attributes,
        "maxDifference",
        quality.max_difference.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &quality.unknown_attributes)?;
    start_element(writer, "OperatingQuality", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_operating_bandwidth<W: io::Write>(
    writer: &mut Writer<W>,
    bw: &crate::model::OperatingBandwidth,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "mediaType", bw.media_type);
    push_optional(&mut attributes, "min", bw.min.as_ref());
    push_optional(&mut attributes, "max", bw.max.as_ref());
    push_optional(&mut attributes, "target", bw.target.as_ref());
    push_unknown_attributes(&mut attributes, &bw.unknown_attributes)?;
    start_element(writer, "OperatingBandwidth", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_uint_pairs_with_id<W: io::Write>(
    writer: &mut Writer<W>,
    pairs: &crate::model::UIntPairsWithId,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "type", pairs.value_type.as_ref());
    push_unknown_attributes(&mut attributes, &pairs.unknown_attributes)?;
    start_element(writer, "QualityLatency", attributes)?;
    let pairs_str = pairs
        .pairs
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    writer.write_event(&Event::Text(pairs_str))?;
    writer.write_event(&Event::End)
}

fn emit_uint_v_with_id<W: io::Write>(
    writer: &mut Writer<W>,
    tag: &str,
    v: &crate::model::UIntVWithId,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "id", v.id);
    push_optional(&mut attributes, "profiles", v.profiles.as_ref());
    push_optional(&mut attributes, "contentType", v.content_type.as_ref());
    push_unknown_attributes(&mut attributes, &v.unknown_attributes)?;
    start_element(writer, tag, attributes)?;
    let values_str = v
        .values
        .iter()
        .map(|val| val.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    writer.write_event(&Event::Text(values_str))?;
    writer.write_event(&Event::End)
}

fn emit_metrics<W: io::Write>(
    writer: &mut Writer<W>,
    metrics: &crate::model::Metrics,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "metrics", &metrics.metrics);
    push_unknown_attributes(&mut attributes, &metrics.unknown_attributes)?;
    start_element(writer, "Metrics", attributes)?;
    for range in &metrics.ranges {
        emit_range(writer, range)?;
    }
    for reporting in &metrics.reportings {
        emit_descriptor(writer, reporting, "Reporting")?;
    }
    emit_unknown_children(writer, &metrics.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_range<W: io::Write>(writer: &mut Writer<W>, range: &crate::model::Range) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "starttime", range.starttime.as_ref());
    push_optional(&mut attributes, "duration", range.duration.as_ref());
    push_unknown_attributes(&mut attributes, &range.unknown_attributes)?;
    start_element(writer, "Range", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_initialization_set<W: io::Write>(
    writer: &mut Writer<W>,
    init_set: &crate::model::InitializationSet,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_representation_base_attributes(&mut attributes, &init_set.base);
    push_attribute(&mut attributes, "id", init_set.id);
    if !init_set.in_all_periods {
        push_attribute(&mut attributes, "inAllPeriods", "false");
    }
    push_optional(
        &mut attributes,
        "contentType",
        init_set.content_type.as_ref(),
    );
    push_optional(&mut attributes, "par", init_set.par.as_ref());
    push_optional(&mut attributes, "maxWidth", init_set.max_width.as_ref());
    push_optional(&mut attributes, "maxHeight", init_set.max_height.as_ref());
    push_optional(
        &mut attributes,
        "maxFrameRate",
        init_set.max_frame_rate.as_ref(),
    );
    push_optional(
        &mut attributes,
        "initialization",
        init_set.initialization.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &init_set.base.unknown_attributes)?;
    start_element(writer, "InitializationSet", attributes)?;
    emit_representation_base_children(writer, &init_set.base)?;
    for accessibility in &init_set.accessibilities {
        emit_descriptor(writer, accessibility, "Accessibility")?;
    }
    for role in &init_set.roles {
        emit_descriptor(writer, role, "Role")?;
    }
    for rating in &init_set.ratings {
        emit_descriptor(writer, rating, "Rating")?;
    }
    for viewpoint in &init_set.viewpoints {
        emit_descriptor(writer, viewpoint, "Viewpoint")?;
    }
    emit_unknown_children(writer, &init_set.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_leap_second_information<W: io::Write>(
    writer: &mut Writer<W>,
    leap_sec: &crate::model::LeapSecondInformation,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(
        &mut attributes,
        "availabilityStartLeapOffset",
        leap_sec.availability_start_leap_offset,
    );
    push_optional(
        &mut attributes,
        "nextAvailabilityStartLeapOffset",
        leap_sec.next_availability_start_leap_offset.as_ref(),
    );
    push_optional(
        &mut attributes,
        "nextLeapChangeTime",
        leap_sec.next_leap_change_time.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &leap_sec.unknown_attributes)?;
    start_element(writer, "LeapSecondInformation", attributes)?;
    emit_unknown_children(writer, &leap_sec.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_segment_children<W: io::Write>(
    writer: &mut Writer<W>,
    segment_base: Option<&SegmentBase>,
    segment_list: Option<&SegmentList>,
    segment_template: Option<&SegmentTemplate>,
) -> Result<()> {
    if let Some(segment_base) = segment_base {
        emit_segment_base(writer, segment_base)?;
    }
    if let Some(segment_list) = segment_list {
        emit_segment_list(writer, segment_list)?;
    }
    if let Some(segment_template) = segment_template {
        emit_segment_template(writer, segment_template)?;
    }
    Ok(())
}

fn emit_segment_base<W: io::Write>(
    writer: &mut Writer<W>,
    segment_base: &SegmentBase,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_segment_base_attributes(&mut attributes, segment_base);
    push_unknown_attributes(&mut attributes, &segment_base.unknown_attributes)?;
    start_element(writer, "SegmentBase", attributes)?;
    emit_segment_base_children(writer, segment_base)?;
    emit_unknown_children(writer, &segment_base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_segment_list<W: io::Write>(
    writer: &mut Writer<W>,
    segment_list: &SegmentList,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_multiple_segment_base_attributes(&mut attributes, &segment_list.base);
    push_unknown_attributes(&mut attributes, &segment_list.base.base.unknown_attributes)?;
    start_element(writer, "SegmentList", attributes)?;
    emit_multiple_segment_base_children(writer, &segment_list.base)?;
    for segment_url in &segment_list.segment_urls {
        emit_segment_url(writer, segment_url)?;
    }
    emit_unknown_children(writer, &segment_list.base.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_segment_template<W: io::Write>(
    writer: &mut Writer<W>,
    segment_template: &SegmentTemplate,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_multiple_segment_base_attributes(&mut attributes, &segment_template.base);
    push_optional(&mut attributes, "media", segment_template.media.as_ref());
    push_optional(&mut attributes, "index", segment_template.index.as_ref());
    push_optional(
        &mut attributes,
        "initialization",
        segment_template.initialization.as_ref(),
    );
    push_optional(
        &mut attributes,
        "bitstreamSwitching",
        segment_template.bitstream_switching.as_ref(),
    );
    push_unknown_attributes(
        &mut attributes,
        &segment_template.base.base.unknown_attributes,
    )?;
    start_element(writer, "SegmentTemplate", attributes)?;
    emit_multiple_segment_base_children(writer, &segment_template.base)?;
    emit_unknown_children(writer, &segment_template.base.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn push_segment_base_attributes(attributes: &mut Vec<Attribute>, segment_base: &SegmentBase) {
    push_optional(attributes, "timescale", segment_base.timescale.as_ref());
    push_optional(attributes, "eptDelta", segment_base.ept_delta.as_ref());
    push_optional(attributes, "pdDelta", segment_base.pd_delta.as_ref());
    push_optional(
        attributes,
        "presentationTimeOffset",
        segment_base.presentation_time_offset.as_ref(),
    );
    push_optional(
        attributes,
        "presentationDuration",
        segment_base.presentation_duration.as_ref(),
    );
    push_optional(
        attributes,
        "timeShiftBufferDepth",
        segment_base.time_shift_buffer_depth.as_ref(),
    );
    push_optional(attributes, "indexRange", segment_base.index_range.as_ref());
    push_optional(
        attributes,
        "indexRangeExact",
        segment_base.index_range_exact.as_ref(),
    );
    if let Some(availability_time_offset) = segment_base.availability_time_offset {
        push_attribute(
            attributes,
            "availabilityTimeOffset",
            xs_double_lexical(availability_time_offset),
        );
    }
    push_optional(
        attributes,
        "availabilityTimeComplete",
        segment_base.availability_time_complete.as_ref(),
    );
}

fn push_multiple_segment_base_attributes(
    attributes: &mut Vec<Attribute>,
    base: &MultipleSegmentBase,
) {
    push_segment_base_attributes(attributes, &base.base);
    push_optional(attributes, "duration", base.duration.as_ref());
    push_optional(attributes, "startNumber", base.start_number.as_ref());
    push_optional(attributes, "endNumber", base.end_number.as_ref());
}

fn emit_segment_base_children<W: io::Write>(
    writer: &mut Writer<W>,
    segment_base: &SegmentBase,
) -> Result<()> {
    if let Some(initialization) = &segment_base.initialization {
        emit_url(writer, "Initialization", initialization)?;
    }
    if let Some(representation_index) = &segment_base.representation_index {
        emit_url(writer, "RepresentationIndex", representation_index)?;
    }
    if let Some(failover_content) = &segment_base.failover_content {
        emit_failover_content(writer, failover_content)?;
    }
    Ok(())
}

fn emit_multiple_segment_base_children<W: io::Write>(
    writer: &mut Writer<W>,
    base: &MultipleSegmentBase,
) -> Result<()> {
    emit_segment_base_children(writer, &base.base)?;
    if let Some(segment_timeline) = &base.segment_timeline {
        emit_segment_timeline(writer, segment_timeline)?;
    }
    if let Some(bitstream_switching) = &base.bitstream_switching {
        emit_url(writer, "BitstreamSwitching", bitstream_switching)?;
    }
    Ok(())
}

fn emit_url<W: io::Write>(writer: &mut Writer<W>, name: &str, url: &Url) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "sourceURL", url.source_url.as_ref());
    push_optional(&mut attributes, "range", url.range.as_ref());
    push_unknown_attributes(&mut attributes, &url.unknown_attributes)?;
    start_element(writer, name, attributes)?;
    emit_unknown_children(writer, &url.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_failover_content<W: io::Write>(
    writer: &mut Writer<W>,
    failover_content: &FailoverContent,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "valid", failover_content.valid.as_ref());
    push_unknown_attributes(&mut attributes, &failover_content.unknown_attributes)?;
    start_element(writer, "FailoverContent", attributes)?;
    for fcs in &failover_content.fcs_entries {
        emit_fcs(writer, fcs)?;
    }
    emit_unknown_children(writer, &failover_content.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_fcs<W: io::Write>(writer: &mut Writer<W>, fcs: &Fcs) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "t", fcs.t);
    push_optional(&mut attributes, "d", fcs.d.as_ref());
    push_unknown_attributes(&mut attributes, &fcs.unknown_attributes)?;
    start_element(writer, "FCS", attributes)?;
    emit_unknown_children(writer, &fcs.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_segment_timeline<W: io::Write>(
    writer: &mut Writer<W>,
    segment_timeline: &SegmentTimeline,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_unknown_attributes(&mut attributes, &segment_timeline.unknown_attributes)?;
    start_element(writer, "SegmentTimeline", attributes)?;
    for segment in &segment_timeline.segments {
        emit_s(writer, segment)?;
    }
    emit_unknown_children(writer, &segment_timeline.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_s<W: io::Write>(writer: &mut Writer<W>, segment: &S) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "t", segment.t.as_ref());
    push_optional(&mut attributes, "n", segment.n.as_ref());
    push_attribute(&mut attributes, "d", segment.d);
    push_optional(&mut attributes, "r", segment.r.as_ref());
    push_optional(&mut attributes, "k", segment.k.as_ref());
    push_unknown_attributes(&mut attributes, &segment.unknown_attributes)?;
    start_element(writer, "S", attributes)?;
    emit_unknown_children(writer, &segment.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_segment_url<W: io::Write>(writer: &mut Writer<W>, segment_url: &SegmentUrl) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "media", segment_url.media.as_ref());
    push_optional(
        &mut attributes,
        "mediaRange",
        segment_url.media_range.as_ref(),
    );
    push_optional(&mut attributes, "index", segment_url.index.as_ref());
    push_optional(
        &mut attributes,
        "indexRange",
        segment_url.index_range.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &segment_url.unknown_attributes)?;
    start_element(writer, "SegmentURL", attributes)?;
    emit_unknown_children(writer, &segment_url.unknown_children)?;
    writer.write_event(&Event::End)
}

fn push_representation_base_attributes(attributes: &mut Vec<Attribute>, base: &RepresentationBase) {
    push_optional(attributes, "profiles", base.profiles.as_ref());
    push_optional(attributes, "width", base.width.as_ref());
    push_optional(attributes, "height", base.height.as_ref());
    push_optional(attributes, "sar", base.sar.as_ref());
    push_optional(attributes, "frameRate", base.frame_rate.as_ref());
    push_optional(
        attributes,
        "audioSamplingRate",
        base.audio_sampling_rate.as_ref(),
    );
    push_optional(attributes, "mimeType", base.mime_type.as_ref());
    push_list(attributes, "segmentProfiles", &base.segment_profiles);
    push_optional(attributes, "codecs", base.codecs.as_ref());
    push_list(attributes, "containerProfiles", &base.container_profiles);
    if let Some(maximum_sap_period) = base.maximum_sap_period {
        push_attribute(
            attributes,
            "maximumSAPPeriod",
            xs_double_lexical(maximum_sap_period),
        );
    }
    push_optional(attributes, "startWithSAP", base.start_with_sap.as_ref());
    if let Some(max_playout_rate) = base.max_playout_rate {
        push_attribute(
            attributes,
            "maxPlayoutRate",
            xs_double_lexical(max_playout_rate),
        );
    }
    push_optional(
        attributes,
        "codingDependency",
        base.coding_dependency.as_ref(),
    );
    push_optional(attributes, "scanType", base.scan_type.as_ref());
    push_optional(
        attributes,
        "selectionPriority",
        base.selection_priority.as_ref(),
    );
    push_optional(attributes, "tag", base.tag.as_ref());
}

fn emit_unknown_children<W: io::Write>(writer: &mut Writer<W>, children: &[Element]) -> Result<()> {
    for element in children {
        emit_unknown_element(writer, element, 0)?;
    }
    Ok(())
}

fn emit_unknown_element<W: io::Write>(
    writer: &mut Writer<W>,
    element: &Element,
    depth: usize,
) -> Result<()> {
    if depth >= MAX_UNKNOWN_ELEMENT_DEPTH {
        return Err(Error::new(ErrorKind::Xml(format!(
            "unknown elements nested deeper than {MAX_UNKNOWN_ELEMENT_DEPTH}"
        ))));
    }
    validate_xml_name(&element.name)?;
    let attributes = element
        .attributes
        .iter()
        .map(|(name, value)| {
            validate_xml_name(name)?;
            Ok(Attribute {
                name: name.clone(),
                value: value.clone(),
            })
        })
        .collect::<Result<_>>()?;
    writer.write_event(&Event::Start(StartElement {
        name: element.name.clone(),
        namespace: element.namespace.clone(),
        attributes,
    }))?;
    for child in &element.children {
        match child {
            Node::Element(child_element) => {
                emit_unknown_element(writer, child_element, depth.saturating_add(1))?;
            }
            Node::Text(text) => writer.write_event(&Event::Text(text.clone()))?,
        }
    }
    writer.write_event(&Event::End)
}

fn emit_descriptor<W: io::Write>(
    writer: &mut Writer<W>,
    descriptor: &Descriptor,
    element_name: &str,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "schemeIdUri", &descriptor.scheme_id_uri);
    push_optional(&mut attributes, "value", descriptor.value.as_ref());
    push_optional(&mut attributes, "id", descriptor.id.as_ref());
    push_unknown_attributes(&mut attributes, &descriptor.unknown_attributes)?;
    start_element(writer, element_name, attributes)?;
    emit_unknown_children(writer, &descriptor.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_content_protection<W: io::Write>(
    writer: &mut Writer<W>,
    content_protection: &ContentProtection,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(
        &mut attributes,
        "schemeIdUri",
        &content_protection.base.scheme_id_uri,
    );
    push_optional(
        &mut attributes,
        "value",
        content_protection.base.value.as_ref(),
    );
    push_optional(&mut attributes, "id", content_protection.base.id.as_ref());
    push_optional(
        &mut attributes,
        "robustness",
        content_protection.robustness.as_ref(),
    );
    push_optional(&mut attributes, "refId", content_protection.ref_id.as_ref());
    push_optional(&mut attributes, "ref", content_protection.r#ref.as_ref());
    push_unknown_attributes(&mut attributes, &content_protection.base.unknown_attributes)?;
    start_element(writer, "ContentProtection", attributes)?;
    emit_unknown_children(writer, &content_protection.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn start_element<W: io::Write>(
    writer: &mut Writer<W>,
    name: &str,
    attributes: Vec<Attribute>,
) -> Result<()> {
    writer.write_event(&Event::Start(StartElement {
        name: name.to_string(),
        namespace: None,
        attributes,
    }))
}

fn push_attribute(attributes: &mut Vec<Attribute>, name: &str, value: impl fmt::Display) {
    attributes.push(Attribute {
        name: name.to_string(),
        value: value.to_string(),
    });
}

fn push_optional<T: fmt::Display>(attributes: &mut Vec<Attribute>, name: &str, value: Option<&T>) {
    if let Some(value) = value {
        push_attribute(attributes, name, value);
    }
}

fn push_list<T: fmt::Display>(attributes: &mut Vec<Attribute>, name: &str, values: &[T]) {
    if values.is_empty() {
        return;
    }
    let lexical = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    push_attribute(attributes, name, lexical);
}

fn push_unknown_attributes(
    attributes: &mut Vec<Attribute>,
    unknown: &[(String, String)],
) -> Result<()> {
    for (name, value) in unknown {
        validate_xml_name(name)?;
        attributes.push(Attribute {
            name: name.clone(),
            value: value.clone(),
        });
    }
    Ok(())
}

/// 未知ノードの名前は公開フィールド経由で任意の文字列を取り得るため、
/// 書き出し前に XML 1.0 の Name 生成規則で検証する。素通しすると整形式
/// でない XML を黙って生成し、再パースできない出力になる。既知要素・既知
/// 属性の名前は静的文字列なので検証しない。
fn validate_xml_name(name: &str) -> Result<()> {
    let mut characters = name.chars();
    let well_formed = match characters.next() {
        Some(first) => is_name_start_char(first) && characters.all(is_name_char),
        None => false,
    };
    if well_formed {
        Ok(())
    } else {
        Err(invalid_value(name, "a well-formed XML name"))
    }
}

// XML 1.0 5th edition の NameStartChar / NameChar の文字範囲そのまま。
fn is_name_start_char(character: char) -> bool {
    matches!(character,
        ':' | '_' | 'A'..='Z' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(character: char) -> bool {
    is_name_start_char(character)
        || matches!(character,
            '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

fn xs_double_lexical(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "INF"
        } else {
            "-INF"
        }
        .to_string()
    } else {
        value.to_string()
    }
}

fn xs_float_lexical(value: f32) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "INF"
        } else {
            "-INF"
        }
        .to_string()
    } else {
        value.to_string()
    }
}

fn emit_event_stream<W: io::Write>(
    writer: &mut Writer<W>,
    event_stream: &crate::model::period_representation::EventStream,
    element_name: &str,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "schemeIdUri", &event_stream.scheme_id_uri);
    push_optional(&mut attributes, "value", event_stream.value.as_ref());
    push_optional(
        &mut attributes,
        "timescale",
        event_stream.timescale.as_ref(),
    );
    if event_stream.presentation_time_offset != 0 {
        push_attribute(
            &mut attributes,
            "presentationTimeOffset",
            event_stream.presentation_time_offset,
        );
    }
    push_optional(&mut attributes, "xlink:href", event_stream.href.as_ref());
    push_optional(
        &mut attributes,
        "xlink:actuate",
        event_stream.actuate.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &event_stream.unknown_attributes)?;

    start_element(writer, element_name, attributes)?;
    for event in &event_stream.events {
        emit_event(writer, event)?;
    }
    emit_unknown_children(writer, &event_stream.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_event<W: io::Write>(
    writer: &mut Writer<W>,
    event: &crate::model::period_representation::Event,
) -> Result<()> {
    let mut attributes = Vec::new();
    if event.presentation_time != 0 {
        push_attribute(&mut attributes, "presentationTime", event.presentation_time);
    }
    push_optional(&mut attributes, "duration", event.duration.as_ref());
    push_optional(&mut attributes, "id", event.id.as_ref());
    push_optional(
        &mut attributes,
        "contentEncoding",
        event.content_encoding.as_ref(),
    );
    push_optional(&mut attributes, "messageData", event.message_data.as_ref());
    push_unknown_attributes(&mut attributes, &event.unknown_attributes)?;

    start_element(writer, "Event", attributes)?;
    if let Some(text) = &event.text_content {
        writer.write_event(&Event::Text(text.clone()))?;
    }
    emit_unknown_children(writer, &event.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_label<W: io::Write>(
    writer: &mut Writer<W>,
    label: &crate::model::period_representation::Label,
    element_name: &str,
) -> Result<()> {
    let mut attributes = Vec::new();
    if label.id != 0 {
        push_attribute(&mut attributes, "id", label.id);
    }
    push_optional(&mut attributes, "lang", label.lang.as_ref());
    push_unknown_attributes(&mut attributes, &label.unknown_attributes)?;

    start_element(writer, element_name, attributes)?;
    writer.write_event(&Event::Text(label.text.clone()))?;
    writer.write_event(&Event::End)
}

fn emit_subset<W: io::Write>(
    writer: &mut Writer<W>,
    subset: &crate::model::period_representation::Subset,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_list(&mut attributes, "contains", &subset.contains);
    push_optional(&mut attributes, "id", subset.id.as_ref());
    push_unknown_attributes(&mut attributes, &subset.unknown_attributes)?;

    start_element(writer, "Subset", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_preselection<W: io::Write>(
    writer: &mut Writer<W>,
    preselection: &crate::model::period_representation::Preselection,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_representation_base_attributes(&mut attributes, &preselection.base);
    if preselection.id != "1" {
        push_attribute(&mut attributes, "id", &preselection.id);
    }
    push_list(
        &mut attributes,
        "preselectionComponents",
        &preselection.preselection_components,
    );
    push_optional(&mut attributes, "lang", preselection.lang.as_ref());
    push_attribute(&mut attributes, "order", preselection.order.to_string());
    push_unknown_attributes(&mut attributes, &preselection.base.unknown_attributes)?;

    start_element(writer, "Preselection", attributes)?;
    emit_representation_base_children(writer, &preselection.base)?;
    for desc in &preselection.accessibilities {
        emit_descriptor(writer, desc, "Accessibility")?;
    }
    for desc in &preselection.roles {
        emit_descriptor(writer, desc, "Role")?;
    }
    for desc in &preselection.ratings {
        emit_descriptor(writer, desc, "Rating")?;
    }
    for desc in &preselection.viewpoints {
        emit_descriptor(writer, desc, "Viewpoint")?;
    }
    emit_unknown_children(writer, &preselection.base.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_switching<W: io::Write>(
    writer: &mut Writer<W>,
    switching: &crate::model::period_representation::Switching,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "interval", switching.interval);
    push_attribute(
        &mut attributes,
        "type",
        switching.switching_type.to_string(),
    );
    push_unknown_attributes(&mut attributes, &switching.unknown_attributes)?;

    start_element(writer, "Switching", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_random_access<W: io::Write>(
    writer: &mut Writer<W>,
    random_access: &crate::model::period_representation::RandomAccess,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "interval", random_access.interval);
    push_attribute(
        &mut attributes,
        "type",
        random_access.random_access_type.to_string(),
    );
    push_optional(
        &mut attributes,
        "minBufferTime",
        random_access.min_buffer_time.as_ref(),
    );
    push_optional(
        &mut attributes,
        "bandwidth",
        random_access.bandwidth.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &random_access.unknown_attributes)?;

    start_element(writer, "RandomAccess", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_producer_reference_time<W: io::Write>(
    writer: &mut Writer<W>,
    prt: &crate::model::period_representation::ProducerReferenceTime,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "id", prt.id);
    if prt.inband {
        push_attribute(&mut attributes, "inband", "true");
    }
    push_attribute(
        &mut attributes,
        "type",
        prt.producer_reference_time_type.to_string(),
    );
    push_optional(
        &mut attributes,
        "applicationScheme",
        prt.application_scheme.as_ref(),
    );
    push_attribute(&mut attributes, "wallClockTime", &prt.wall_clock_time);
    push_attribute(&mut attributes, "presentationTime", prt.presentation_time);
    push_unknown_attributes(&mut attributes, &prt.unknown_attributes)?;

    start_element(writer, "ProducerReferenceTime", attributes)?;
    if let Some(desc) = &prt.utc_timing {
        emit_descriptor(writer, desc, "UTCTiming")?;
    }
    emit_unknown_children(writer, &prt.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_content_popularity_rate<W: io::Write>(
    writer: &mut Writer<W>,
    cpr: &crate::model::period_representation::ContentPopularityRate,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "source", cpr.source.to_string());
    push_optional(
        &mut attributes,
        "source_description",
        cpr.source_description.as_ref(),
    );
    push_unknown_attributes(&mut attributes, &cpr.unknown_attributes)?;

    start_element(writer, "ContentPopularityRate", attributes)?;
    for pr in &cpr.rates {
        emit_popularity_rate(writer, pr)?;
    }
    emit_unknown_children(writer, &cpr.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_popularity_rate<W: io::Write>(
    writer: &mut Writer<W>,
    pr: &crate::model::period_representation::PopularityRate,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(
        &mut attributes,
        "popularityRate",
        pr.popularity_rate.as_ref(),
    );
    push_optional(&mut attributes, "start", pr.start.as_ref());
    if pr.r != 0 {
        push_attribute(&mut attributes, "r", pr.r);
    }
    push_unknown_attributes(&mut attributes, &pr.unknown_attributes)?;

    start_element(writer, "PR", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_resync<W: io::Write>(
    writer: &mut Writer<W>,
    resync: &crate::model::period_representation::Resync,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "type", resync.resync_type.to_string());
    push_optional(&mut attributes, "dT", resync.dt.as_ref());
    if let Some(di_max) = resync.di_max {
        push_attribute(&mut attributes, "dImax", xs_float_lexical(di_max));
    }
    if resync.di_min != 0.0 {
        push_attribute(&mut attributes, "dImin", xs_float_lexical(resync.di_min));
    }
    if resync.marker {
        push_attribute(&mut attributes, "marker", "true");
    }
    push_unknown_attributes(&mut attributes, &resync.unknown_attributes)?;

    start_element(writer, "Resync", attributes)?;
    writer.write_event(&Event::End)
}

fn emit_content_component<W: io::Write>(
    writer: &mut Writer<W>,
    cc: &crate::model::period_representation::ContentComponent,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_optional(&mut attributes, "id", cc.id.as_ref());
    push_optional(&mut attributes, "lang", cc.lang.as_ref());
    push_optional(&mut attributes, "contentType", cc.content_type.as_ref());
    push_optional(&mut attributes, "par", cc.par.as_ref());
    push_optional(&mut attributes, "tag", cc.tag.as_ref());
    push_unknown_attributes(&mut attributes, &cc.unknown_attributes)?;

    start_element(writer, "ContentComponent", attributes)?;
    for desc in &cc.accessibilities {
        emit_descriptor(writer, desc, "Accessibility")?;
    }
    for desc in &cc.roles {
        emit_descriptor(writer, desc, "Role")?;
    }
    for desc in &cc.ratings {
        emit_descriptor(writer, desc, "Rating")?;
    }
    for desc in &cc.viewpoints {
        emit_descriptor(writer, desc, "Viewpoint")?;
    }
    emit_unknown_children(writer, &cc.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_extended_bandwidth<W: io::Write>(
    writer: &mut Writer<W>,
    eb: &crate::model::period_representation::ExtendedBandwidth,
) -> Result<()> {
    let mut attributes = Vec::new();
    if eb.vbr {
        push_attribute(&mut attributes, "vbr", "true");
    }
    push_unknown_attributes(&mut attributes, &eb.unknown_attributes)?;

    start_element(writer, "ExtendedBandwidth", attributes)?;
    for mp in &eb.model_pairs {
        emit_model_pair(writer, mp)?;
    }
    emit_unknown_children(writer, &eb.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_model_pair<W: io::Write>(
    writer: &mut Writer<W>,
    mp: &crate::model::period_representation::ModelPair,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_attribute(&mut attributes, "bufferTime", mp.buffer_time);
    push_attribute(&mut attributes, "bandwidth", mp.bandwidth);
    push_unknown_attributes(&mut attributes, &mp.unknown_attributes)?;

    start_element(writer, "ModelPair", attributes)?;
    emit_unknown_children(writer, &mp.unknown_children)?;
    writer.write_event(&Event::End)
}

fn emit_sub_representation<W: io::Write>(
    writer: &mut Writer<W>,
    sr: &crate::model::period_representation::SubRepresentation,
) -> Result<()> {
    let mut attributes = Vec::new();
    push_representation_base_attributes(&mut attributes, &sr.base);
    push_optional(&mut attributes, "level", sr.level.as_ref());
    push_list(&mut attributes, "dependencyLevel", &sr.dependency_level);
    push_optional(&mut attributes, "bandwidth", sr.bandwidth.as_ref());
    push_list(&mut attributes, "contentComponent", &sr.content_component);
    push_unknown_attributes(&mut attributes, &sr.base.unknown_attributes)?;

    start_element(writer, "SubRepresentation", attributes)?;
    emit_representation_base_children(writer, &sr.base)?;
    emit_unknown_children(writer, &sr.base.unknown_children)?;
    writer.write_event(&Event::End)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::de::mpd_from_slice;
    use crate::model::types::XsDuration;

    fn serialize(mpd: &Mpd) -> String {
        let output = write_mpd(mpd, Vec::new()).unwrap();
        String::from_utf8(output).unwrap()
    }

    /// parse → serialize → parse の意味論的等価（CONTEXT.md）をモデル比較で
    /// 確認する。
    fn assert_roundtrip(input: &str) -> Mpd {
        let first = mpd_from_slice(input.as_bytes()).unwrap();
        let output = serialize(&first);
        let second = mpd_from_slice(output.as_bytes()).unwrap();
        assert_eq!(first, second, "roundtrip output:\n{output}");
        first
    }

    #[test]
    fn hand_built_mpd_serializes_with_default_namespace() {
        let mut mpd = Mpd::new(
            "urn:mpeg:dash:profile:isoff-on-demand:2011",
            "PT2S".parse::<XsDuration>().unwrap(),
        );
        mpd.periods.push(Period::new());
        let output = serialize(&mpd);
        assert_eq!(
            output,
            concat!(
                r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
                r#"profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" minBufferTime="PT2S">"#,
                "<Period></Period>",
                "</MPD>",
            )
        );
    }

    #[test]
    fn minimal_mpd_roundtrips() {
        assert_roundtrip(concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
            r#"profiles="urn:mpeg:dash:profile:isoff-on-demand:2011" minBufferTime="PT2S">"#,
            "\n  <Period/>\n",
            "</MPD>",
        ));
    }

    #[test]
    fn typed_attributes_roundtrip_along_the_spine() {
        assert_roundtrip(concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" id="m1" profiles="p" "#,
            r#"type="dynamic" availabilityStartTime="2026-06-10T00:00:00Z" "#,
            r#"publishTime="2026-06-10T01:02:03.5Z" minimumUpdatePeriod="PT30S" "#,
            r#"minBufferTime="PT1.5S" timeShiftBufferDepth="PT1H" "#,
            r#"suggestedPresentationDelay="PT10S" maxSegmentDuration="PT4S">"#,
            r#"<Period id="p0" start="PT0S" duration="PT30M" bitstreamSwitching="false">"#,
            r#"<AdaptationSet id="1" group="2" lang="en" contentType="video" par="16:9" "#,
            r#"minBandwidth="1000000" maxBandwidth="5000000" minWidth="640" maxWidth="1920" "#,
            r#"minHeight="360" maxHeight="1080" minFrameRate="25" maxFrameRate="30000/1001" "#,
            r#"segmentAlignment="true" subsegmentAlignment="false" subsegmentStartsWithSAP="1" "#,
            r#"bitstreamSwitching="true" initializationSetRef="1 2" "#,
            r#"initializationPrincipal="https://example.com/init.mpd" mimeType="video/mp4" "#,
            r#"codecs="avc1.640028" maximumSAPPeriod="2.5" startWithSAP="1" "#,
            r#"maxPlayoutRate="2" codingDependency="false" scanType="progressive" "#,
            r#"selectionPriority="3" tag="main">"#,
            r#"<Representation id="v0" bandwidth="4800000" qualityRanking="1" "#,
            r#"dependencyId="a b" associationId="c" associationType="cdsc" "#,
            r#"mediaStreamStructureId="s1" width="1920" height="1080" sar="1:1" "#,
            r#"frameRate="30000/1001" audioSamplingRate="44100 48000" "#,
            r#"segmentProfiles="cmfc cmff" containerProfiles="cmfc"/>"#,
            "</AdaptationSet>",
            "</Period>",
            "</MPD>",
        ));
    }

    #[test]
    fn unknown_content_roundtrips_after_known_children() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
            r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" "#,
            r#"xsi:schemaLocation="urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd" "#,
            r#"profiles="p" minBufferTime="PT2S">"#,
            "<FutureExtension><Detail>demo</Detail></FutureExtension>",
            "<Period>",
            r#"<AdaptationSet mimeType="video/mp4">"#,
            r#"<ContentProtection xmlns:cenc="urn:mpeg:cenc:2013" "#,
            r#"schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc">"#,
            "<cenc:pssh>AAAA</cenc:pssh>",
            "</ContentProtection>",
            r#"<Representation id="v0" bandwidth="1000"/>"#,
            "</AdaptationSet>",
            "</Period>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);

        let period_position = output.find("<Period").unwrap();
        let future_extension_position = output.find("<FutureExtension").unwrap();
        assert!(
            period_position < future_extension_position,
            "unknown children must follow known children: {output}"
        );
        // ContentProtection is now a typed AdaptationSet child, so it must be
        // emitted after the AdaptationSet start tag and before the
        // Representation that follows it in the RepresentationBase sequence.
        let adaptation_set_position = output.find("<AdaptationSet").unwrap();
        let content_protection_position = output.find("<ContentProtection").unwrap();
        let representation_position = output.find("<Representation").unwrap();
        assert!(
            adaptation_set_position < content_protection_position
                && content_protection_position < representation_position,
            "ContentProtection (known) must follow AdaptationSet and precede Representation: {output}"
        );
        assert!(output.contains("<cenc:pssh>AAAA</cenc:pssh>"));
        assert!(
            output.contains(r#"xsi:schemaLocation="urn:mpeg:dash:schema:mpd:2011 DASH-MPD.xsd""#)
        );
    }

    #[test]
    fn prefixed_input_roundtrips_to_unprefixed_known_elements() {
        let input = concat!(
            r#"<ns1:MPD xmlns:ns1="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S">"#,
            "<ns1:Period/>",
            "</ns1:MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);
        assert!(output.starts_with(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011""#));
        assert!(output.contains("<Period></Period>"));
        assert!(output.contains(r#"xmlns:ns1="urn:mpeg:dash:schema:mpd:2011""#));
    }

    #[test]
    fn canonical_lexical_forms_replace_input_variants() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT120S" availabilityStartTime="2026-06-10T00:00:00+00:00">"#,
            r#"<Period bitstreamSwitching="1"/>"#,
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);
        assert!(output.contains(r#"minBufferTime="PT2M""#));
        assert!(output.contains(r#"availabilityStartTime="2026-06-10T00:00:00Z""#));
        assert!(output.contains(r#"bitstreamSwitching="true""#));
    }

    #[test]
    fn live_profile_segment_template_roundtrips() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" type="dynamic" "#,
            r#"availabilityStartTime="2026-06-10T00:00:00Z" minBufferTime="PT2S">"#,
            "<Period>",
            r#"<SegmentTemplate timescale="90000" duration="180000"/>"#,
            r#"<AdaptationSet mimeType="video/mp4">"#,
            r#"<SegmentTemplate timescale="90000" startNumber="1" presentationTimeOffset="900000" "#,
            r#"media="seg-$RepresentationID$-$Number%05d$.m4s" initialization="init-$RepresentationID$.mp4" "#,
            r#"bitstreamSwitching="bs.mp4">"#,
            r#"<SegmentTimeline><S t="0" d="180000" r="24"/><S d="90000"/></SegmentTimeline>"#,
            r#"<BitstreamSwitching sourceURL="bs-element.mp4"/>"#,
            "</SegmentTemplate>",
            r#"<Representation id="v0" bandwidth="1000"/>"#,
            "</AdaptationSet>",
            "</Period>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);

        let segment_template_position = output.rfind("<SegmentTemplate").unwrap();
        let representation_position = output.find("<Representation").unwrap();
        assert!(
            segment_template_position < representation_position,
            "SegmentTemplate must precede Representation: {output}"
        );
        assert!(output.contains(r#"media="seg-$RepresentationID$-$Number%05d$.m4s""#));
        assert!(output.contains(r#"<S t="0" d="180000" r="24"></S>"#));
    }

    #[test]
    fn on_demand_profile_segment_base_roundtrips() {
        assert_roundtrip(concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period><AdaptationSet>",
            r#"<Representation id="v0" bandwidth="1000">"#,
            r#"<SegmentBase timescale="48000" indexRange="0-499" indexRangeExact="true" "#,
            r#"availabilityTimeOffset="INF" eptDelta="-1" presentationTimeOffset="100">"#,
            r#"<Initialization sourceURL="init.mp4" range="0-99"/>"#,
            r#"<RepresentationIndex sourceURL="index.sidx"/>"#,
            r#"<FailoverContent valid="false"><FCS t="0" d="48000"/><FCS t="96000"/></FailoverContent>"#,
            "</SegmentBase>",
            "</Representation>",
            "</AdaptationSet></Period>",
            "</MPD>",
        ));
    }

    #[test]
    fn segment_list_roundtrips() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period>",
            r#"<SegmentList timescale="48000" duration="96000" startNumber="1" endNumber="2">"#,
            r#"<Initialization sourceURL="init.mp4"/>"#,
            r#"<SegmentURL media="s1.mp4" mediaRange="0-499" index="i1.idx" indexRange="500-"/>"#,
            r#"<SegmentURL media="s2.mp4"/>"#,
            "</SegmentList>",
            "</Period>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);

        let initialization_position = output.find("<Initialization").unwrap();
        let segment_url_position = output.find("<SegmentURL").unwrap();
        assert!(
            initialization_position < segment_url_position,
            "Initialization must precede SegmentURL: {output}"
        );
        assert!(output.contains(r#"indexRange="500-""#));
    }

    #[test]
    fn unknown_content_inside_segment_elements_roundtrips() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            "<Period>",
            r#"<SegmentTemplate xmlns:custom="urn:example:custom" custom:hint="x" media="$Number$.m4s">"#,
            r#"<SegmentTimeline><S d="100"/></SegmentTimeline>"#,
            r#"<custom:Extra>data</custom:Extra>"#,
            "</SegmentTemplate>",
            "</Period>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let segment_template = mpd
            .periods
            .first()
            .unwrap()
            .segment_template
            .as_ref()
            .unwrap();
        assert_eq!(
            segment_template.base.base.unknown_attributes,
            vec![
                ("xmlns:custom".to_string(), "urn:example:custom".to_string()),
                ("custom:hint".to_string(), "x".to_string()),
            ]
        );
        match segment_template.base.base.unknown_children.as_slice() {
            [extra] => assert_eq!(extra.name, "custom:Extra"),
            other => panic!("unexpected unknown children: {other:?}"),
        }
    }

    #[test]
    fn unzoned_date_time_roundtrips() {
        assert_roundtrip(concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" "#,
            r#"minBufferTime="PT2S" availabilityStartTime="2011-05-10T06:16:42">"#,
            "<Period/>",
            "</MPD>",
        ));
    }

    /// Every Period- and RepresentationBase-level element added in this slice
    /// now parses back into its typed field, so the whole tree survives a
    /// roundtrip and the elements with distinct serialized tag names
    /// (`InbandEventStream`, `EmptyAdaptationSet`) are not confused with their
    /// siblings.
    #[test]
    fn period_and_base_children_roundtrip() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" "#,
            r#"xmlns:xlink="http://www.w3.org/1999/xlink" profiles="p" minBufferTime="PT2S">"#,
            "<Period>",
            r#"<EventStream schemeIdUri="urn:evt" value="v" timescale="1000" "#,
            r#"presentationTimeOffset="500" xlink:href="evt.xml" xlink:actuate="onLoad">"#,
            r#"<Event presentationTime="0" duration="100" id="1">payload</Event>"#,
            "</EventStream>",
            r#"<AdaptationSet mimeType="video/mp4">"#,
            r#"<InbandEventStream schemeIdUri="urn:inband"/>"#,
            r#"<Switching interval="10" type="bitstream"/>"#,
            r#"<RandomAccess interval="5" type="gradual" minBufferTime="PT1S" bandwidth="700"/>"#,
            r#"<GroupLabel id="2">grp</GroupLabel>"#,
            r#"<Label id="1" lang="en">lbl</Label>"#,
            r#"<ProducerReferenceTime id="1" wallClockTime="2026-06-10T00:00:00Z" presentationTime="0">"#,
            r#"<UTCTiming schemeIdUri="urn:mpeg:dash:utc:http-xsdate:2014"/>"#,
            "</ProducerReferenceTime>",
            r#"<ContentPopularityRate source="content"><PR popularityRate="50" r="-1"/></ContentPopularityRate>"#,
            r#"<Resync type="1" dImax="INF" dImin="0.5" marker="true"/>"#,
            r#"<ContentComponent id="1" lang="en" contentType="video"><Role schemeIdUri="urn:role" value="main"/></ContentComponent>"#,
            r#"<Representation id="v0" bandwidth="1000">"#,
            r#"<ExtendedBandwidth vbr="true"><ModelPair bufferTime="PT1S" bandwidth="500"/></ExtendedBandwidth>"#,
            r#"<SubRepresentation level="0" bandwidth="800"/>"#,
            "</Representation>",
            "</AdaptationSet>",
            r#"<Subset contains="0 1" id="s1"/>"#,
            r#"<EmptyAdaptationSet id="9"/>"#,
            "<GroupLabel>periodgrp</GroupLabel>",
            r#"<Preselection id="p1" preselectionComponents="v0 a0" order="time-ordered"/>"#,
            "</Period>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);

        assert!(
            output.contains(r#"<InbandEventStream schemeIdUri="urn:inband">"#),
            "InbandEventStream must keep its own tag name: {output}"
        );
        assert!(
            output.contains(r#"<EmptyAdaptationSet id="9">"#),
            "EmptyAdaptationSet must keep its own tag name: {output}"
        );
        assert!(
            output.contains(r#"xlink:href="evt.xml""#)
                && output.contains(r#"xlink:actuate="onLoad""#),
            "EventStream xlink attributes must roundtrip: {output}"
        );
        assert!(output.contains(r#"type="gradual""#), "{output}");
        assert!(output.contains(r#"dImax="INF""#), "{output}");
        assert!(output.contains(r#"order="time-ordered""#), "{output}");
    }

    /// `xs:double` and `xs:float` attributes carrying `INF` must serialize to
    /// the XSD lexical form `INF`, not Rust's `Display` form `inf`, so the
    /// value parses back instead of being rejected as `InvalidValue`.
    #[test]
    fn infinite_double_and_float_attributes_roundtrip() {
        let input = concat!(
            r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" profiles="p" minBufferTime="PT2S">"#,
            r#"<BaseURL availabilityTimeOffset="INF">https://cdn.example/</BaseURL>"#,
            r#"<PatchLocation ttl="INF">patch.mpp</PatchLocation>"#,
            r#"<ServiceDescription id="0"><PlaybackRate min="-INF" max="INF"/></ServiceDescription>"#,
            "<Period/>",
            "</MPD>",
        );
        let mpd = assert_roundtrip(input);
        let output = serialize(&mpd);
        assert!(
            output.contains(r#"availabilityTimeOffset="INF""#),
            "{output}"
        );
        assert!(output.contains(r#"ttl="INF""#), "{output}");
        assert!(output.contains(r#"min="-INF""#), "{output}");
        assert!(output.contains(r#"max="INF""#), "{output}");
        assert!(
            !output.contains("inf"),
            "must not emit Display form: {output}"
        );
    }

    fn minimal_hand_built_mpd() -> Mpd {
        let mut mpd = Mpd::new(
            "urn:mpeg:dash:profile:isoff-on-demand:2011",
            "PT2S".parse::<XsDuration>().unwrap(),
        );
        mpd.periods.push(Period::new());
        mpd
    }

    /// 公開フィールド経由で混入した整形式でない名前は、再パースできない
    /// XML を黙って生成せず `InvalidValue` で拒否する。
    #[test]
    fn unknown_attribute_name_that_is_not_an_xml_name_is_rejected() {
        let mut mpd = minimal_hand_built_mpd();
        mpd.unknown_attributes
            .push(("a b".to_string(), "v".to_string()));
        let error = write_mpd(&mpd, Vec::new()).unwrap_err();
        match error.kind {
            ErrorKind::InvalidValue { value, .. } => assert_eq!(value, "a b"),
            other => panic!("unexpected error kind: {other:?}"),
        }
    }

    #[test]
    fn unknown_element_and_nested_attribute_names_are_validated() {
        let mut mpd = minimal_hand_built_mpd();
        mpd.unknown_children.push(Element::new("1bad"));
        assert!(matches!(
            write_mpd(&mpd, Vec::new()).unwrap_err().kind,
            ErrorKind::InvalidValue { .. }
        ));

        let mut mpd = minimal_hand_built_mpd();
        let mut child = Element::new("x:child");
        child
            .attributes
            .push(("bad=name".to_string(), "v".to_string()));
        let mut parent = Element::new("x:parent");
        parent.children.push(Node::Element(child));
        mpd.unknown_children.push(parent);
        assert!(matches!(
            write_mpd(&mpd, Vec::new()).unwrap_err().kind,
            ErrorKind::InvalidValue { .. }
        ));
    }

    /// 手組みの深い未知ツリーによるスタック溢れ（abort）を防ぐ。上限は
    /// de 側の `MAX_UNKNOWN_ELEMENT_DEPTH` と共有する。
    #[test]
    fn unknown_tree_deeper_than_the_shared_limit_is_rejected() {
        let mut element = Element::new("x:leaf");
        for _ in 0..MAX_UNKNOWN_ELEMENT_DEPTH {
            let mut parent = Element::new("x:nest");
            parent.children.push(Node::Element(element));
            element = parent;
        }
        let mut mpd = minimal_hand_built_mpd();
        mpd.unknown_children.push(element);
        let error = write_mpd(&mpd, Vec::new()).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Xml(_)));
    }
}
