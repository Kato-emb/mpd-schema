//! The resolver, its Representation handles, and the segment sequence.

use mpd_schema::Mpd;
use mpd_schema::model::{
    AdaptationSet, Period, Representation, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, Url as SchemaUrl, XsDuration,
};

use crate::base_url::{parse_manifest_base, resolve_level};
use crate::error::{Error, ErrorKind, Result};
use crate::segment::{ByteRange, CandidateUrl, ResolvedSegment, SegmentTime};
use crate::template::{Values, expand};

/// A semantic resolver over a borrowed, parsed [`Mpd`].
///
/// The resolver owns nothing but the manifest's absolute base URL; it borrows
/// the MPD for as long as it lives. Build one with [`Resolver::new`], list the
/// Representations with [`Resolver::representations`], then pull a
/// Representation's media segments with [`Resolver::segments`] and its
/// initialization segment with [`Resolver::initialization`].
#[derive(Debug)]
#[non_exhaustive]
pub struct Resolver<'a> {
    mpd: &'a Mpd,
    base: url::Url,
}

impl<'a> Resolver<'a> {
    /// Creates a resolver for `mpd`, resolving relative URLs against the
    /// absolute manifest URL `base`.
    ///
    /// # Errors
    ///
    /// Returns [`ErrorKind::InvalidBaseUrl`] when `base` is not an absolute URL.
    pub fn new(mpd: &'a Mpd, base: &str) -> Result<Self> {
        Ok(Self {
            mpd,
            base: parse_manifest_base(base)?,
        })
    }

    /// Lists every Representation across every Period, in document order.
    ///
    /// Each handle carries selection metadata and an opaque location into the
    /// MPD tree; pass it back to [`Resolver::segments`] or
    /// [`Resolver::initialization`].
    pub fn representations(&self) -> Vec<RepresentationHandle> {
        let mut handles = Vec::new();
        for (period_index, period) in self.mpd.periods.iter().enumerate() {
            for (adaptation_set_index, adaptation_set) in period.adaptation_sets.iter().enumerate()
            {
                for (representation_index, representation) in
                    adaptation_set.representations.iter().enumerate()
                {
                    let mime_type = representation
                        .base
                        .mime_type
                        .clone()
                        .or_else(|| adaptation_set.base.mime_type.clone());
                    handles.push(RepresentationHandle {
                        period_index,
                        adaptation_set_index,
                        representation_index,
                        period_id: period.id.clone(),
                        id: representation.id.clone(),
                        bandwidth: representation.bandwidth,
                        mime_type,
                    });
                }
            }
        }
        handles
    }

    /// Resolves the media segments of the Representation named by `handle`.
    ///
    /// The returned [`Segments`] is an iterator in 1:1 correspondence with the
    /// real media segments. For a duration-less (open) Period it is infinite;
    /// bound it with [`Iterator::take`] or the caller's own stop condition.
    ///
    /// # Errors
    ///
    /// Returns an error when the segment information is missing, internally
    /// inconsistent, or references an unsupported or malformed template.
    pub fn segments(&self, handle: &RepresentationHandle) -> Result<Segments> {
        let location = self.locate(handle)?;
        let bases = self.candidate_urls(&location)?;
        build_segments(&location, bases, self.mpd)
    }

    /// Resolves the initialization segment of the Representation named by
    /// `handle`, or `None` when the addressing declares no initialization.
    ///
    /// # Errors
    ///
    /// Returns an error under the same conditions as [`Resolver::segments`].
    pub fn initialization(&self, handle: &RepresentationHandle) -> Result<Option<ResolvedSegment>> {
        let location = self.locate(handle)?;
        let bases = self.candidate_urls(&location)?;
        build_initialization(&location, &bases)
    }

    fn locate(&self, handle: &RepresentationHandle) -> Result<Located<'a>> {
        let period = self
            .mpd
            .periods
            .get(handle.period_index)
            .ok_or_else(|| stale_handle(handle))?;
        let adaptation_set = period
            .adaptation_sets
            .get(handle.adaptation_set_index)
            .ok_or_else(|| stale_handle(handle))?;
        let representation = adaptation_set
            .representations
            .get(handle.representation_index)
            .ok_or_else(|| stale_handle(handle))?;
        Ok(Located {
            period,
            adaptation_set,
            representation,
            path: handle.path(),
        })
    }

    fn candidate_urls(&self, location: &Located<'a>) -> Result<Vec<CandidateUrl>> {
        let root = vec![CandidateUrl::new(self.base.clone(), None)];
        let after_mpd = resolve_level(&root, &self.mpd.base_urls, &location.path)?;
        let after_period = resolve_level(&after_mpd, &location.period.base_urls, &location.path)?;
        let after_adaptation_set = resolve_level(
            &after_period,
            &location.adaptation_set.base_urls,
            &location.path,
        )?;
        resolve_level(
            &after_adaptation_set,
            &location.representation.base_urls,
            &location.path,
        )
    }
}

fn stale_handle(handle: &RepresentationHandle) -> Error {
    Error::new(
        handle.path(),
        ErrorKind::InconsistentSegmentInfo {
            reason: "handle does not point into this MPD".to_string(),
        },
    )
}

/// An opaque handle to one Representation, with metadata for selection.
///
/// Obtain handles from [`Resolver::representations`]. The location fields are
/// private; a handle is only meaningful for the MPD it was listed from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RepresentationHandle {
    period_index: usize,
    adaptation_set_index: usize,
    representation_index: usize,
    /// The enclosing `Period@id`, when present.
    pub period_id: Option<String>,
    /// The `Representation@id`.
    pub id: String,
    /// The `Representation@bandwidth`.
    pub bandwidth: u32,
    /// The effective `mimeType` (the Representation's, else the `AdaptationSet`'s).
    pub mime_type: Option<String>,
}

impl RepresentationHandle {
    fn path(&self) -> String {
        format!(
            "Period[{}] > AdaptationSet[{}] > Representation[{}]",
            self.period_index, self.adaptation_set_index, self.representation_index
        )
    }
}

struct Located<'a> {
    period: &'a Period,
    adaptation_set: &'a AdaptationSet,
    representation: &'a Representation,
    path: String,
}

/// The resolved media segments of one Representation.
///
/// Iterates [`ResolvedSegment`]s in segment order. The iteration is infallible:
/// the template and base URLs are validated when the sequence is built, so each
/// step only substitutes numbers and joins URLs.
#[derive(Debug)]
#[non_exhaustive]
pub struct Segments {
    plan: Plan,
}

#[derive(Debug)]
enum Plan {
    Finite(std::vec::IntoIter<ResolvedSegment>),
    Template(Box<TemplateGenerator>),
}

impl Iterator for Segments {
    type Item = ResolvedSegment;

    fn next(&mut self) -> Option<ResolvedSegment> {
        match &mut self.plan {
            Plan::Finite(iter) => iter.next(),
            Plan::Template(generator) => generator.next(),
        }
    }
}

#[derive(Debug)]
struct TemplateGenerator {
    bases: Vec<CandidateUrl>,
    media: String,
    representation_id: String,
    bandwidth: u32,
    timescale: u32,
    path: String,
    timing: Timing,
}

impl TemplateGenerator {
    fn next(&mut self) -> Option<ResolvedSegment> {
        let timing = self.timing.next()?;
        let values = Values {
            representation_id: &self.representation_id,
            bandwidth: self.bandwidth,
            number: Some(timing.number),
            time: Some(timing.start),
            sub_number: None,
        };
        // Validated at build time with representative values, so a failure here
        // cannot occur for a different number; end the sequence defensively
        // rather than panic.
        let relative = expand(&self.media, &values, &self.path).ok()?;
        let urls = join_all(&self.bases, &relative)?;
        let mut segment = ResolvedSegment::new(urls);
        segment.time = Some(SegmentTime::new(
            timing.start,
            timing.duration,
            self.timescale,
        ));
        segment.number = Some(timing.number);
        Some(segment)
    }
}

fn join_all(bases: &[CandidateUrl], relative: &str) -> Option<Vec<CandidateUrl>> {
    let mut urls = Vec::with_capacity(bases.len());
    for base in bases {
        let joined = base.url.join(relative).ok()?;
        urls.push(CandidateUrl::new(joined, base.service_location.clone()));
    }
    Some(urls)
}

#[derive(Debug, Clone, Copy)]
struct SegmentTiming {
    number: u64,
    start: u64,
    duration: u64,
}

#[derive(Debug)]
enum Timing {
    /// `$Number$` with a fixed segment duration; `count` is `None` for an open
    /// Period.
    Duration {
        index: u64,
        count: Option<u64>,
        duration: u64,
        start_number: u64,
        presentation_time_offset: u64,
    },
    /// A `SegmentTimeline` flattened into runs of equal-duration segments.
    Timeline {
        runs: Vec<TimelineRun>,
        run_index: usize,
        offset: u64,
    },
}

#[derive(Debug, Clone, Copy)]
struct TimelineRun {
    start_time: u64,
    duration: u64,
    number_start: u64,
    /// The number of segments in this run, or `None` for an infinite tail.
    repeat: Option<u64>,
}

impl Timing {
    fn next(&mut self) -> Option<SegmentTiming> {
        match self {
            Timing::Duration {
                index,
                count,
                duration,
                start_number,
                presentation_time_offset,
            } => {
                if let Some(count) = count {
                    if *index >= *count {
                        return None;
                    }
                }
                let offset = index.checked_mul(*duration)?;
                let start = presentation_time_offset.checked_add(offset)?;
                let number = start_number.checked_add(*index)?;
                *index = index.checked_add(1)?;
                Some(SegmentTiming {
                    number,
                    start,
                    duration: *duration,
                })
            }
            Timing::Timeline {
                runs,
                run_index,
                offset,
            } => loop {
                let run = runs.get(*run_index)?;
                if let Some(repeat) = run.repeat {
                    if *offset >= repeat {
                        *run_index = run_index.checked_add(1)?;
                        *offset = 0;
                        continue;
                    }
                }
                let elapsed = offset.checked_mul(run.duration)?;
                let start = run.start_time.checked_add(elapsed)?;
                let number = run.number_start.checked_add(*offset)?;
                *offset = offset.checked_add(1)?;
                return Some(SegmentTiming {
                    number,
                    start,
                    duration: run.duration,
                });
            },
        }
    }
}

fn build_segments(location: &Located<'_>, bases: Vec<CandidateUrl>, mpd: &Mpd) -> Result<Segments> {
    if let Some(template) = effective_template(location) {
        return build_template_segments(location, bases, mpd, &template);
    }
    if let Some(list) = effective_list(location) {
        return build_list_segments(location, &bases, &list);
    }
    // SegmentBase or a bare BaseURL: a single segment that is the resource
    // itself. `bases` always carries the manifest URL, so require an actual
    // SegmentBase or a BaseURL element below the MPD root to avoid mistaking
    // the manifest for media.
    let has_explicit_base_url = !location.period.base_urls.is_empty()
        || !location.adaptation_set.base_urls.is_empty()
        || !location.representation.base_urls.is_empty();
    if effective_base(location).is_some() || has_explicit_base_url {
        let segment = ResolvedSegment::new(bases);
        return Ok(Segments {
            plan: Plan::Finite(vec![segment].into_iter()),
        });
    }
    Err(Error::new(
        location.path.clone(),
        ErrorKind::MissingAddressing,
    ))
}

fn build_template_segments(
    location: &Located<'_>,
    bases: Vec<CandidateUrl>,
    mpd: &Mpd,
    template: &EffectiveTemplate<'_>,
) -> Result<Segments> {
    let media = template
        .media
        .ok_or_else(|| Error::new(location.path.clone(), ErrorKind::MissingAddressing))?;

    // Validate the template grammar and base joins once, with representative
    // values, so per-segment iteration can be infallible.
    let probe = Values {
        representation_id: &location.representation.id,
        bandwidth: location.representation.bandwidth,
        number: Some(template.start_number),
        time: Some(template.presentation_time_offset),
        sub_number: None,
    };
    let probe_relative = expand(media, &probe, &location.path)?;
    if join_all(&bases, &probe_relative).is_none() {
        return Err(Error::new(
            location.path.clone(),
            ErrorKind::InvalidBaseUrl {
                value: probe_relative,
            },
        ));
    }

    let timing = if let Some(timeline) = template.segment_timeline {
        Timing::Timeline {
            runs: timeline_runs(timeline, template, location, mpd)?,
            run_index: 0,
            offset: 0,
        }
    } else if let Some(duration) = template.duration {
        if duration == 0 {
            return Err(Error::new(
                location.path.clone(),
                ErrorKind::InconsistentSegmentInfo {
                    reason: "segment duration is zero".to_string(),
                },
            ));
        }
        Timing::Duration {
            index: 0,
            count: duration_segment_count(location, mpd, template.timescale, duration)?,
            duration,
            start_number: template.start_number,
            presentation_time_offset: template.presentation_time_offset,
        }
    } else {
        return Err(Error::new(
            location.path.clone(),
            ErrorKind::InconsistentSegmentInfo {
                reason: "SegmentTemplate has neither a duration nor a SegmentTimeline".to_string(),
            },
        ));
    };

    Ok(Segments {
        plan: Plan::Template(Box::new(TemplateGenerator {
            bases,
            media: media.to_string(),
            representation_id: location.representation.id.clone(),
            bandwidth: location.representation.bandwidth,
            timescale: template.timescale,
            path: location.path.clone(),
            timing,
        })),
    })
}

fn timeline_runs(
    timeline: &SegmentTimeline,
    template: &EffectiveTemplate<'_>,
    location: &Located<'_>,
    mpd: &Mpd,
) -> Result<Vec<TimelineRun>> {
    let overflow = || Error::new(location.path.clone(), ErrorKind::Overflow);
    let period_end = period_end_ticks(location, mpd, template.timescale)?;
    let mut runs = Vec::new();
    let mut current_time = template.presentation_time_offset;
    let mut current_number = template.start_number;

    for (index, segment) in timeline.segments.iter().enumerate() {
        if let Some(start) = segment.t {
            current_time = start;
        }
        let duration = segment.d;
        let repeat_attribute = segment.r.unwrap_or(0);
        let repeat = if repeat_attribute >= 0 {
            let extra = u64::try_from(repeat_attribute).map_err(|_| overflow())?;
            Some(extra.checked_add(1).ok_or_else(overflow)?)
        } else {
            // `r = -1`: repeat to the next entry's start, else to the period
            // end, else forever.
            repeat_until(
                timeline
                    .segments
                    .get(index.checked_add(1).ok_or_else(overflow)?),
                current_time,
                duration,
                period_end,
                location,
            )?
        };
        runs.push(TimelineRun {
            start_time: current_time,
            duration,
            number_start: current_number,
            repeat,
        });
        match repeat {
            Some(count) => {
                let span = count.checked_mul(duration).ok_or_else(overflow)?;
                current_time = current_time.checked_add(span).ok_or_else(overflow)?;
                current_number = current_number.checked_add(count).ok_or_else(overflow)?;
            }
            None => break,
        }
    }
    Ok(runs)
}

fn repeat_until(
    next: Option<&S>,
    current_time: u64,
    duration: u64,
    period_end: Option<u64>,
    location: &Located<'_>,
) -> Result<Option<u64>> {
    let overflow = || Error::new(location.path.clone(), ErrorKind::Overflow);
    if duration == 0 {
        return Err(Error::new(
            location.path.clone(),
            ErrorKind::InconsistentSegmentInfo {
                reason: "SegmentTimeline entry has zero duration".to_string(),
            },
        ));
    }
    let boundary = match next.and_then(|entry| entry.t) {
        Some(next_start) => Some(next_start),
        None => period_end,
    };
    match boundary {
        Some(boundary) => {
            let span = boundary.checked_sub(current_time).ok_or_else(overflow)?;
            Some(checked_ceil_div(span, duration).ok_or_else(overflow)).transpose()
        }
        None => Ok(None),
    }
}

fn duration_segment_count(
    location: &Located<'_>,
    mpd: &Mpd,
    timescale: u32,
    duration: u64,
) -> Result<Option<u64>> {
    let overflow = || Error::new(location.path.clone(), ErrorKind::Overflow);
    // `period_end_ticks` returns the period's *length* in ticks, so the count
    // spans the whole period from zero regardless of `presentationTimeOffset`.
    match period_end_ticks(location, mpd, timescale)? {
        Some(span) => Ok(Some(checked_ceil_div(span, duration).ok_or_else(overflow)?)),
        None => Ok(None),
    }
}

fn period_end_ticks(location: &Located<'_>, mpd: &Mpd, timescale: u32) -> Result<Option<u64>> {
    let overflow = || Error::new(location.path.clone(), ErrorKind::Overflow);
    if let Some(duration) = location.period.duration {
        return Ok(Some(
            duration_to_ticks(duration, timescale).ok_or_else(overflow)?,
        ));
    }
    // Fall back to the media presentation duration for a single-period static
    // MPD; otherwise the period is open.
    if mpd.periods.len() == 1 {
        if let Some(total) = mpd.media_presentation_duration {
            return Ok(Some(
                duration_to_ticks(total, timescale).ok_or_else(overflow)?,
            ));
        }
    }
    Ok(None)
}

fn duration_to_ticks(duration: XsDuration, timescale: u32) -> Option<u64> {
    if duration.negative || duration.months != 0 {
        return None;
    }
    let timescale = u128::from(timescale);
    let whole = u128::from(duration.seconds).checked_mul(timescale)?;
    let fraction = u128::from(duration.nanoseconds)
        .checked_mul(timescale)?
        .checked_div(1_000_000_000)?;
    u64::try_from(whole.checked_add(fraction)?).ok()
}

fn checked_ceil_div(numerator: u64, denominator: u64) -> Option<u64> {
    let adjusted = numerator.checked_add(denominator.checked_sub(1)?)?;
    adjusted.checked_div(denominator)
}

fn build_list_segments(
    location: &Located<'_>,
    bases: &[CandidateUrl],
    list: &EffectiveList<'_>,
) -> Result<Segments> {
    let mut segments = Vec::new();
    for (index, segment_url) in list.segment_urls.iter().enumerate() {
        let index_u64 = u64::try_from(index)
            .map_err(|_| Error::new(location.path.clone(), ErrorKind::Overflow))?;
        let number = list
            .start_number
            .checked_add(index_u64)
            .ok_or_else(|| Error::new(location.path.clone(), ErrorKind::Overflow))?;
        let urls = match &segment_url.media {
            Some(media) => join_all(bases, media).ok_or_else(|| {
                Error::new(
                    location.path.clone(),
                    ErrorKind::InvalidBaseUrl {
                        value: media.clone(),
                    },
                )
            })?,
            None => bases.to_vec(),
        };
        let mut segment = ResolvedSegment::new(urls);
        segment.number = Some(number);
        if let Some(range) = &segment_url.media_range {
            segment.byte_range = Some(ByteRange::parse(range, &location.path)?);
        }
        if let Some(duration) = list.duration {
            let start = index_u64
                .checked_mul(duration)
                .ok_or_else(|| Error::new(location.path.clone(), ErrorKind::Overflow))?;
            segment.time = Some(SegmentTime::new(start, duration, list.timescale));
        }
        segments.push(segment);
    }
    Ok(Segments {
        plan: Plan::Finite(segments.into_iter()),
    })
}

fn build_initialization(
    location: &Located<'_>,
    bases: &[CandidateUrl],
) -> Result<Option<ResolvedSegment>> {
    if let Some(template) = effective_template(location) {
        if let Some(initialization) = template.initialization {
            let values = Values {
                representation_id: &location.representation.id,
                bandwidth: location.representation.bandwidth,
                number: None,
                time: None,
                sub_number: None,
            };
            let relative = expand(initialization, &values, &location.path)?;
            let urls = join_all(bases, &relative).ok_or_else(|| {
                Error::new(
                    location.path.clone(),
                    ErrorKind::InvalidBaseUrl { value: relative },
                )
            })?;
            return Ok(Some(ResolvedSegment::new(urls)));
        }
        // Fall through to the URL-child initialization on the embedded base.
        return initialization_from_url_child(location, bases, template.initialization_child);
    }
    if let Some(list) = effective_list(location) {
        return initialization_from_url_child(location, bases, list.initialization_child);
    }
    if let Some(base) = effective_base(location) {
        return initialization_from_url_child(location, bases, base.initialization.as_ref());
    }
    Ok(None)
}

fn initialization_from_url_child(
    location: &Located<'_>,
    bases: &[CandidateUrl],
    initialization: Option<&SchemaUrl>,
) -> Result<Option<ResolvedSegment>> {
    let Some(initialization) = initialization else {
        return Ok(None);
    };
    let urls = match &initialization.source_url {
        Some(source_url) => join_all(bases, source_url).ok_or_else(|| {
            Error::new(
                location.path.clone(),
                ErrorKind::InvalidBaseUrl {
                    value: source_url.clone(),
                },
            )
        })?,
        None => bases.to_vec(),
    };
    let mut segment = ResolvedSegment::new(urls);
    if let Some(range) = &initialization.range {
        segment.byte_range = Some(ByteRange::parse(range, &location.path)?);
    }
    Ok(Some(segment))
}

struct EffectiveTemplate<'a> {
    timescale: u32,
    presentation_time_offset: u64,
    duration: Option<u64>,
    start_number: u64,
    segment_timeline: Option<&'a SegmentTimeline>,
    media: Option<&'a str>,
    initialization: Option<&'a str>,
    initialization_child: Option<&'a SchemaUrl>,
}

fn effective_template<'a>(location: &Located<'a>) -> Option<EffectiveTemplate<'a>> {
    let chain: Vec<&'a SegmentTemplate> = [
        location.period.segment_template.as_ref(),
        location.adaptation_set.segment_template.as_ref(),
        location.representation.segment_template.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if chain.is_empty() {
        return None;
    }
    Some(EffectiveTemplate {
        timescale: pick(&chain, |template| template.base.base.timescale).unwrap_or(1),
        presentation_time_offset: pick(&chain, |template| {
            template.base.base.presentation_time_offset
        })
        .unwrap_or(0),
        duration: pick(&chain, |template| template.base.duration.map(u64::from)),
        start_number: pick(&chain, |template| template.base.start_number.map(u64::from))
            .unwrap_or(1),
        segment_timeline: pick(&chain, |template| template.base.segment_timeline.as_ref()),
        media: pick(&chain, |template| template.media.as_deref()),
        initialization: pick(&chain, |template| template.initialization.as_deref()),
        initialization_child: pick(&chain, |template| {
            template.base.base.initialization.as_ref()
        }),
    })
}

struct EffectiveList<'a> {
    timescale: u32,
    duration: Option<u64>,
    start_number: u64,
    segment_urls: &'a [mpd_schema::model::SegmentUrl],
    initialization_child: Option<&'a SchemaUrl>,
}

fn effective_list<'a>(location: &Located<'a>) -> Option<EffectiveList<'a>> {
    let chain: Vec<&'a SegmentList> = [
        location.period.segment_list.as_ref(),
        location.adaptation_set.segment_list.as_ref(),
        location.representation.segment_list.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if chain.is_empty() {
        return None;
    }
    let segment_urls = pick(&chain, |list| {
        if list.segment_urls.is_empty() {
            None
        } else {
            Some(list.segment_urls.as_slice())
        }
    })
    .unwrap_or(&[]);
    Some(EffectiveList {
        timescale: pick(&chain, |list| list.base.base.timescale).unwrap_or(1),
        duration: pick(&chain, |list| list.base.duration.map(u64::from)),
        start_number: pick(&chain, |list| list.base.start_number.map(u64::from)).unwrap_or(1),
        segment_urls,
        initialization_child: pick(&chain, |list| list.base.base.initialization.as_ref()),
    })
}

fn effective_base<'a>(location: &Located<'a>) -> Option<&'a SegmentBase> {
    location
        .representation
        .segment_base
        .as_ref()
        .or(location.adaptation_set.segment_base.as_ref())
        .or(location.period.segment_base.as_ref())
}

fn pick<'a, S, T>(chain: &[&'a S], accessor: impl Fn(&'a S) -> Option<T>) -> Option<T> {
    let mut result = None;
    for item in chain {
        if let Some(value) = accessor(*item) {
            result = Some(value);
        }
    }
    result
}
