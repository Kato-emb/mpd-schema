//! Handwritten model structs mirroring `DASH-MPD.xsd`.

pub mod element;
pub mod mpd;
pub mod types;

pub use element::{Element, Node};
pub use mpd::{
    AdaptationSet, ContentType, MPD_NAMESPACE, Mpd, Period, PresentationType, Representation,
    RepresentationBase, VideoScan,
};
pub use types::{ConditionalUint, FrameRate, Ratio, XsDateTime, XsDuration};
