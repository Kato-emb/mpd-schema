//! Resolves an MPD into concrete segment URLs and prints them.
//!
//! For every Representation the resolver yields the initialization segment and
//! the media segments; this example prints the init URL and the first
//! [`MAX_SEGMENTS`] media segments of each, with their timing and number.
//!
//! Run against your own manifest by passing the file and its absolute URL
//! (the resolver needs an absolute base to resolve relative `BaseURL`s):
//!
//! ```sh
//! cargo run -p examples --example resolve_segments -- manifest.mpd https://cdn.example.com/manifest.mpd
//! ```
//!
//! With no arguments it resolves a small embedded manifest so the example runs
//! out of the box:
//!
//! ```sh
//! cargo run -p examples --example resolve_segments
//! ```

use std::error::Error;

use mpd_resolve::{ResolvedSegment, Resolver};
use mpd_schema::Mpd;

const MAX_SEGMENTS: usize = 10;

// A static $Number$ manifest, used when no file argument is given.
const EMBEDDED_MPD: &str = r#"<?xml version="1.0"?>
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
const EMBEDDED_URL: &str = "https://cdn.example.com/manifest.mpd";

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let (bytes, base_url) = match arguments.as_slice() {
        [] => {
            println!("(no arguments given; resolving the embedded sample manifest)\n");
            (EMBEDDED_MPD.as_bytes().to_vec(), EMBEDDED_URL.to_string())
        }
        [path, url] => (std::fs::read(path)?, url.clone()),
        _ => {
            eprintln!("usage: resolve_segments [<manifest-file> <absolute-manifest-url>]");
            std::process::exit(2);
        }
    };

    let mpd = Mpd::from_slice(&bytes)?;
    let resolver = Resolver::new(&mpd, &base_url)?;

    for handle in resolver.representations() {
        let mime = handle.mime_type.as_deref().unwrap_or("?");
        let period = handle.period_id.as_deref().unwrap_or("-");
        println!(
            "Representation {} ({mime}, {} bps) [period {period}]",
            handle.id, handle.bandwidth
        );

        match resolver.initialization(&handle)? {
            Some(init) => println!("  init: {}", describe(&init)),
            None => println!("  init: (none)"),
        }

        // `segments` can fail on unsupported addressing (e.g. $SubNumber$);
        // report it per Representation rather than aborting the whole run.
        let segments = match resolver.segments(&handle) {
            Ok(segments) => segments,
            Err(error) => {
                println!("  segments: error: {error}");
                continue;
            }
        };

        let mut count = 0;
        for segment in segments.take(MAX_SEGMENTS) {
            println!("  {}", describe(&segment));
            count += 1;
        }
        if count == MAX_SEGMENTS {
            println!("  ... (showing first {MAX_SEGMENTS})");
        }
    }

    Ok(())
}

fn describe(segment: &ResolvedSegment) -> String {
    let primary = segment.urls[0].url.as_str();
    let extra = match segment.urls.len() {
        1 => String::new(),
        n => format!("  (+{} more candidate(s))", n - 1),
    };

    let prefix = match (segment.number, segment.time) {
        (Some(number), Some(time)) => {
            format!(
                "#{number} @{}-{} (ts={}): ",
                time.start, time.duration, time.timescale
            )
        }
        (Some(number), None) => format!("#{number}: "),
        (None, Some(time)) => {
            format!(
                "@{}-{} (ts={}): ",
                time.start, time.duration, time.timescale
            )
        }
        (None, None) => String::new(),
    };

    format!("{prefix}{primary}{extra}")
}
