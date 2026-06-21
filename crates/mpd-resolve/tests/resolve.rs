//! End-to-end resolution tests driven by small inline MPDs.
//!
//! Every test exercises the public API by composing `mpd-schema` parsing with
//! `mpd-resolve` resolution, the same seam a caller uses.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "テストでは失敗箇所を即座に特定するため unwrap / panic / 添字を許容する"
)]

use mpd_resolve::{ErrorKind, ResolvedSegment, Resolver};
use mpd_schema::Mpd;

fn urls(segment: &ResolvedSegment) -> Vec<String> {
    segment
        .urls
        .iter()
        .map(|candidate| candidate.url.as_str().to_string())
        .collect()
}

fn primary(segment: &ResolvedSegment) -> String {
    segment.urls[0].url.as_str().to_string()
}

#[test]
fn number_template_resolves_count_urls_time_and_number() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="urn:mpeg:dash:profile:isoff-live:2011"
     minBufferTime="PT2S" type="static" mediaPresentationDuration="PT30S">
  <BaseURL>https://cdn.example.com/base/</BaseURL>
  <Period id="p0">
    <AdaptationSet mimeType="video/mp4">
      <SegmentTemplate media="$RepresentationID$/seg-$Number%05d$.m4s"
                       initialization="$RepresentationID$/init.mp4"
                       timescale="1000" duration="10000" startNumber="1"/>
      <Representation id="v0" bandwidth="1000000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/manifest.mpd").unwrap();

    let handles = resolver.representations();
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].id, "v0");
    assert_eq!(handles[0].bandwidth, 1_000_000);
    assert_eq!(handles[0].mime_type.as_deref(), Some("video/mp4"));
    assert_eq!(handles[0].period_id.as_deref(), Some("p0"));

    let init = resolver.initialization(&handles[0]).unwrap().unwrap();
    assert_eq!(primary(&init), "https://cdn.example.com/base/v0/init.mp4");
    assert!(init.number.is_none() && init.time.is_none());

    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();
    assert_eq!(segments.len(), 3); // ceil(30000 / 10000)
    assert_eq!(
        primary(&segments[0]),
        "https://cdn.example.com/base/v0/seg-00001.m4s"
    );
    assert_eq!(
        primary(&segments[2]),
        "https://cdn.example.com/base/v0/seg-00003.m4s"
    );
    assert_eq!(segments[0].number, Some(1));
    assert_eq!(segments[2].number, Some(3));
    let time = segments[1].time.unwrap();
    assert_eq!(
        (time.start, time.duration, time.timescale),
        (10_000, 10_000, 1_000)
    );
}

#[test]
fn relative_base_url_resolves_per_rfc_3986() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT1S">
  <BaseURL>../media/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate media="$RepresentationID$.m4s" duration="1000" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/a/b/manifest.mpd").unwrap();
    let handles = resolver.representations();
    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();
    assert_eq!(primary(&segments[0]), "https://example.com/a/media/v0.m4s");
}

#[test]
fn segment_timeline_expands_repeats_and_gaps() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period duration="PT4S">
    <AdaptationSet>
      <SegmentTemplate media="$RepresentationID$-$Time$.m4s" timescale="1000">
        <SegmentTimeline>
          <S t="0" d="1000" r="2"/>
          <S d="1000"/>
        </SegmentTimeline>
      </SegmentTemplate>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();

    let times: Vec<u64> = segments.iter().map(|s| s.time.unwrap().start).collect();
    assert_eq!(times, vec![0, 1_000, 2_000, 3_000]);
    let numbers: Vec<u64> = segments.iter().map(|s| s.number.unwrap()).collect();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
    assert_eq!(primary(&segments[3]), "https://cdn.example.com/v0-3000.m4s");
}

#[test]
fn segment_list_yields_explicit_urls_with_ranges() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static">
  <BaseURL>https://cdn.example.com/v/</BaseURL>
  <Period>
    <AdaptationSet>
      <Representation id="v0" bandwidth="1">
        <SegmentList duration="1000" timescale="1000" startNumber="1">
          <Initialization sourceURL="init.mp4"/>
          <SegmentURL media="seg1.m4s"/>
          <SegmentURL media="seg2.m4s" mediaRange="0-499"/>
        </SegmentList>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();

    let init = resolver.initialization(&handles[0]).unwrap().unwrap();
    assert_eq!(primary(&init), "https://cdn.example.com/v/init.mp4");

    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();
    assert_eq!(segments.len(), 2);
    assert_eq!(primary(&segments[0]), "https://cdn.example.com/v/seg1.m4s");
    assert_eq!(segments[0].number, Some(1));
    assert_eq!(segments[1].number, Some(2));
    let range = segments[1].byte_range.unwrap();
    assert_eq!((range.start, range.end), (0, Some(499)));
    assert_eq!(segments[1].time.unwrap().start, 1_000);
}

#[test]
fn segment_base_is_a_single_segment_at_the_base_url() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static">
  <Period>
    <AdaptationSet>
      <Representation id="v0" bandwidth="1">
        <BaseURL>video.mp4</BaseURL>
        <SegmentBase indexRange="0-99">
          <Initialization range="100-599"/>
        </SegmentBase>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/dir/m.mpd").unwrap();
    let handles = resolver.representations();

    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(primary(&segments[0]), "https://example.com/dir/video.mp4");
    assert!(segments[0].number.is_none());

    let init = resolver.initialization(&handles[0]).unwrap().unwrap();
    assert_eq!(primary(&init), "https://example.com/dir/video.mp4");
    assert_eq!(init.byte_range.unwrap().start, 100);
}

#[test]
fn multiple_base_urls_become_ordered_candidates_with_service_location() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT1S">
  <Period>
    <AdaptationSet>
      <BaseURL serviceLocation="cdn-a">https://a.example.com/</BaseURL>
      <BaseURL serviceLocation="cdn-b">https://b.example.com/</BaseURL>
      <SegmentTemplate media="$RepresentationID$.m4s" duration="1000" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();

    assert_eq!(
        urls(&segments[0]),
        vec![
            "https://a.example.com/v0.m4s".to_string(),
            "https://b.example.com/v0.m4s".to_string(),
        ]
    );
    assert_eq!(
        segments[0].urls[0].service_location.as_deref(),
        Some("cdn-a")
    );
    assert_eq!(
        segments[0].urls[1].service_location.as_deref(),
        Some("cdn-b")
    );
}

#[test]
fn open_period_yields_an_infinite_sequence() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="dynamic"
     availabilityStartTime="2026-01-01T00:00:00Z">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate media="$Number$.m4s" duration="1000" timescale="1000" startNumber="1"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().take(1000).collect();
    assert_eq!(segments.len(), 1000);
    assert_eq!(primary(&segments[999]), "https://cdn.example.com/1000.m4s");
}

#[test]
fn multi_period_enumerates_every_period() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period id="main" duration="PT1S">
    <AdaptationSet>
      <SegmentTemplate media="m/$Number$.m4s" duration="1000" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
  <Period id="ad" duration="PT1S">
    <AdaptationSet>
      <SegmentTemplate media="ad/$Number$.m4s" duration="1000" timescale="1000"/>
      <Representation id="a0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    assert_eq!(handles.len(), 2);
    assert_eq!(handles[0].period_id.as_deref(), Some("main"));
    assert_eq!(handles[1].period_id.as_deref(), Some("ad"));

    let ad: Vec<_> = resolver.segments(&handles[1]).unwrap().collect();
    assert_eq!(primary(&ad[0]), "https://cdn.example.com/ad/1.m4s");
}

#[test]
fn unknown_template_identifier_is_an_error() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT1S">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate media="$Frame$.m4s" duration="1000" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let error = resolver.segments(&handles[0]).unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::UnknownTemplateIdentifier { identifier } if identifier == "Frame"
    ));
}

#[test]
fn time_identifier_in_initialization_is_unsupported() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT1S">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate media="$Number$.m4s" initialization="init-$Time$.mp4"
                       duration="1000" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let error = resolver.initialization(&handles[0]).unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::UnsupportedAddressing { .. }
    ));
}

#[test]
fn template_without_duration_or_timeline_is_inconsistent() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT1S">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate media="$Number$.m4s" timescale="1000"/>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let error = resolver.segments(&handles[0]).unwrap_err();
    assert!(matches!(
        error.kind,
        ErrorKind::InconsistentSegmentInfo { .. }
    ));
}

#[test]
fn no_addressing_is_an_error() {
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static">
  <Period>
    <AdaptationSet>
      <Representation id="v0" bandwidth="1"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();
    let error = resolver.segments(&handles[0]).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::MissingAddressing));
    assert!(error.path.contains("Representation[0]"));
}

#[test]
fn segment_inheritance_merges_attributes_down_levels() {
    // timescale/initialization on the AdaptationSet, duration/media on the
    // Representation: the effective template merges both.
    const XML: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     profiles="p" minBufferTime="PT2S" type="static" mediaPresentationDuration="PT2S">
  <BaseURL>https://cdn.example.com/</BaseURL>
  <Period>
    <AdaptationSet>
      <SegmentTemplate timescale="1000" initialization="$RepresentationID$/i.mp4"/>
      <Representation id="v0" bandwidth="1">
        <SegmentTemplate media="$RepresentationID$/$Number$.m4s" duration="1000"/>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;

    let mpd = Mpd::from_slice(XML.as_bytes()).unwrap();
    let resolver = Resolver::new(&mpd, "https://example.com/m.mpd").unwrap();
    let handles = resolver.representations();

    let init = resolver.initialization(&handles[0]).unwrap().unwrap();
    assert_eq!(primary(&init), "https://cdn.example.com/v0/i.mp4");

    let segments: Vec<_> = resolver.segments(&handles[0]).unwrap().collect();
    assert_eq!(segments.len(), 2); // ceil(2000 / 1000)
    assert_eq!(primary(&segments[0]), "https://cdn.example.com/v0/1.m4s");
    assert_eq!(segments[0].time.unwrap().timescale, 1_000);
}
