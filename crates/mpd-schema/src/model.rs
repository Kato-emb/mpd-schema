//! Handwritten model structs mirroring `DASH-MPD.xsd`.

pub mod descriptor;
pub mod element;
pub mod mpd;
pub mod segment;
pub mod types;

pub use descriptor::{ContentProtection, Descriptor};
pub use element::{Element, Node};
pub use mpd::{
    AdaptationSet, ContentType, MPD_NAMESPACE, Mpd, Period, PresentationType, Representation,
    RepresentationBase, VideoScan,
};
pub use segment::{
    FailoverContent, Fcs, MultipleSegmentBase, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, SegmentUrl, Url,
};
pub use types::{
    AudioSamplingRate, ConditionalUint, FrameRate, Ratio, Sap, XsDateTime, XsDuration,
};
