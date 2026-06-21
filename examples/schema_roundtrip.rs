//! Parses an MPD, edits a known attribute, and serializes it back.
//!
//! The focus is unknown-node preservation: the input carries a `cenc:pssh`
//! DRM payload, which is not part of the DASH schema. The crate keeps such
//! unknown elements verbatim, so editing an unrelated attribute and writing
//! the document back leaves the `cenc:pssh` untouched.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p examples --example schema_roundtrip
//! ```

use std::error::Error;

use mpd_schema::Mpd;

// `cenc:pssh` lives in the cenc namespace, declared here as `xmlns:cenc`. The
// crate has no typed field for it, so it is held as an unknown child of
// ContentProtection and re-serialized as-is.
const INPUT: &str = r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
     xmlns:cenc="urn:mpeg:cenc:2013"
     profiles="urn:mpeg:dash:profile:isoff-on-demand:2011"
     minBufferTime="PT2S">
  <Period>
    <AdaptationSet mimeType="video/mp4">
      <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" value="cenc">
        <cenc:pssh>AAAANHBzc2gBAAAA</cenc:pssh>
      </ContentProtection>
      <Representation id="v0" bandwidth="1000000"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

fn main() -> Result<(), Box<dyn Error>> {
    let mut mpd = Mpd::from_str(INPUT)?;

    // Edit a typed attribute the crate fully understands.
    mpd.id = Some("edited-manifest".to_string());

    let output = mpd.to_string();

    println!("--- serialized output ---\n{}\n", mpd.to_string_pretty()?);

    let id_written = output.contains(r#"id="edited-manifest""#);
    let pssh_preserved = output.contains("<cenc:pssh>AAAANHBzc2gBAAAA</cenc:pssh>");

    println!("id attribute written back: {id_written}");
    println!("cenc:pssh preserved verbatim: {pssh_preserved}");

    Ok(())
}
