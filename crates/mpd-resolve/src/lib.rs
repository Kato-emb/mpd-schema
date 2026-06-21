//! Semantic resolution of MPEG-DASH MPD documents.
//!
//! `mpd-resolve` turns a parsed [`mpd_schema::Mpd`] into concrete segment
//! locators: it resolves the `BaseURL` hierarchy against the manifest URL
//! (RFC 3986), expands segment templates (`$Number$`, `$Time$`, and friends),
//! and folds the four DASH addressing modes into one segment sequence.
//!
//! The crate is the resolution layer described in `mpd-schema`'s architecture
//! docs: it borrows a parsed MPD and performs pure computation over it, with no
//! I/O. Fetching the manifest, following `xlink:href`, and refreshing a live
//! presentation remain the caller's responsibility.
//!
//! # Limitations (v1)
//!
//! - Live availability is not computed: segment generation is clock-free, so a
//!   dynamic Period yields its full theoretical timeline (infinite for an open
//!   Period). Filtering by `availabilityStartTime` and a wall clock is future
//!   work.
//! - `$SubNumber$` is parsed and formatted but not iterated; a media template
//!   that uses it is rejected. Low-latency sub-segment addressing is out of
//!   scope.
//! - Cross-Period continuity, DVB `BaseURL` priority/weight ordering, and
//!   `xlink` resolution are out of scope; callers splice `xlink` before
//!   resolving.
//!
//! ```no_run
//! use mpd_schema::Mpd;
//! use mpd_resolve::Resolver;
//!
//! # fn run(bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
//! let mpd = Mpd::from_slice(bytes)?;
//! let resolver = Resolver::new(&mpd, "https://example.com/live/manifest.mpd")?;
//! for handle in resolver.representations() {
//!     if let Some(init) = resolver.initialization(&handle)? {
//!         // fetch init.urls[0].url ...
//!         let _ = init;
//!     }
//!     for segment in resolver.segments(&handle)?.take(5) {
//!         // fetch segment.urls[0].url ...
//!         let _ = segment;
//!     }
//! }
//! # Ok(())
//! # }
//! ```

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::panic,
        reason = "テストでは失敗箇所を即座に特定するため unwrap / panic を許容する"
    )
)]

mod base_url;
mod error;
mod resolver;
mod segment;
mod template;

pub use error::{Error, ErrorKind, Result};
pub use resolver::{RepresentationHandle, Resolver, Segments};
pub use segment::{ByteRange, CandidateUrl, ResolvedSegment, SegmentTime};
