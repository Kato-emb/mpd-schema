# mpd-schema

Bidirectional conversion between MPEG-DASH MPD documents and Rust structs.

`mpd-schema` parses MPD (Media Presentation Description) documents as defined
by ISO/IEC 23009-1 (5th edition) into typed structs and serializes them back
to XML.

## Scope

This crate works strictly at the document level. It is the layer you use to
read, edit, and write the manifest itself — what to do with the manifest is
left to the caller. Out of scope by design:

- **Resolution** — segment URL derivation from templates, timeline expansion.
  Template strings such as `SegmentTemplate@media` are kept as plain
  `String`s.
- **Transport** — HTTP fetching, following `Location`, resolving `xlink`
  references.

## Features

- Typed structs for every complex type in the 5th edition schema
  (`DASH-MPD.xsd`; the Patch document type is not covered).
- Unknown elements and attributes — vendor extensions, DRM payloads such as
  `cenc:pssh` or `mspr:pro` — are preserved verbatim and written back on
  serialization.
- Strict parsing: required attributes are required, and values that do not
  conform to their lexical form are rejected with an error carrying the
  document path of the failure.
- Two dependencies (`quick-xml`, `chrono`), no proc-macros, no codegen.
- Builds for `wasm32-unknown-unknown`.

## Usage

Parsing, editing, and serializing:

```rust
use mpd_schema::Mpd;

let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011"
    profiles="urn:mpeg:dash:profile:isoff-on-demand:2011"
    minBufferTime="PT2S"><Period/></MPD>"#;

let mut mpd = Mpd::from_str(xml)?;
assert_eq!(mpd.periods.len(), 1);

mpd.id = Some("manifest".to_string());
let output = mpd.to_string();
assert!(output.contains(r#"id="manifest""#));
```

Building a document from scratch — required attributes are taken by each
struct's `new`, everything else starts empty and is set through public
fields:

```rust
use mpd_schema::{Mpd, Period};

let mut mpd = Mpd::new(
    "urn:mpeg:dash:profile:isoff-on-demand:2011",
    "PT2S".parse()?,
);
mpd.periods.push(Period::new());
let xml = mpd.to_string();
assert!(xml.starts_with(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011""#));
```

`Mpd::from_slice` / `Mpd::from_reader` and `Mpd::write_to` are available for
byte and `io` based input/output.

Input must be UTF-8; other encodings are rejected.

## Minimum supported Rust version

Rust 1.85 (required by the 2024 edition).

## Development

The test suite validates against the official [DASHSchema] files and sample
MPDs, which are not redistributable and therefore not part of this
repository or the published crate. Fetch them once per checkout before
running tests:

```sh
./scripts/fetch-fixtures.sh
cargo test
```

[DASHSchema]: https://github.com/MPEGGroup/DASHSchema

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
