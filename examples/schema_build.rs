//! Builds an MPD document from scratch and prints the serialized XML.
//!
//! Demonstrates the `new(required attributes)` + public-field construction
//! convention: each struct's `new` takes only the attributes the schema makes
//! mandatory, and everything else is filled in through public fields.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p examples --example schema_build
//! ```

use std::error::Error;

use mpd_schema::{AdaptationSet, Mpd, Period, Representation, SegmentTemplate};

fn main() -> Result<(), Box<dyn Error>> {
    // `profiles` and `minBufferTime` are the only attributes the schema
    // requires on MPD, so they are the only `new` arguments.
    let mut mpd = Mpd::new("urn:mpeg:dash:profile:isoff-live:2011", "PT2S".parse()?);
    mpd.id = Some("example-manifest".to_string());
    mpd.media_presentation_duration = Some("PT30S".parse()?);

    let mut period = Period::new();
    period.id = Some("p0".to_string());

    let mut adaptation_set = AdaptationSet::new();
    adaptation_set.base.mime_type = Some("video/mp4".to_string());

    // One SegmentTemplate shared by the adaptation set: $Number$ addressing
    // with a fixed segment duration.
    let mut template = SegmentTemplate::new();
    template.media = Some("$RepresentationID$/seg-$Number%05d$.m4s".to_string());
    template.initialization = Some("$RepresentationID$/init.mp4".to_string());
    template.base.base.timescale = Some(1000);
    template.base.duration = Some(10_000);
    template.base.start_number = Some(1);
    adaptation_set.segment_template = Some(template);

    let mut representation = Representation::new("v0", 1_000_000);
    representation.base.width = Some(1280);
    representation.base.height = Some(720);
    representation.base.codecs = Some("avc1.4d401f".to_string());
    adaptation_set.representations.push(representation);

    period.adaptation_sets.push(adaptation_set);
    mpd.periods.push(period);

    println!("{}", mpd.to_string_pretty()?);
    Ok(())
}
