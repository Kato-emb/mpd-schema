//! Handwritten model structs mirroring `DASH-MPD.xsd`.

pub mod descriptor;
pub mod element;
pub mod mpd;
pub mod period_representation;
pub mod segment;
pub mod service_description;
pub mod types;

pub use descriptor::{ContentProtection, Descriptor};
pub use element::{Element, Node};
pub use mpd::{
    AdaptationSet, BaseUrl, ContentType, InitializationSet, LeapSecondInformation, MPD_NAMESPACE,
    Metrics, Mpd, PatchLocation, Period, PresentationType, ProgramInformation, Range,
    Representation, RepresentationBase, VideoScan,
};
pub use period_representation::{
    ContentComponent, ContentEncoding, ContentPopularityRate, ContentPopularitySource, Event,
    EventStream, ExtendedBandwidth, Label, ModelPair, PopularityRate, Preselection,
    PreselectionOrder, ProducerReferenceTime, ProducerReferenceTimeType, RandomAccess,
    RandomAccessType, Resync, SubRepresentation, Subset, Switching, SwitchingType,
};
pub use segment::{
    FailoverContent, Fcs, MultipleSegmentBase, S, SegmentBase, SegmentList, SegmentTemplate,
    SegmentTimeline, SegmentUrl, Url,
};
pub use service_description::{
    Latency, OperatingBandwidth, OperatingBandwidthMediaType, OperatingQuality,
    OperatingQualityMediaType, PlaybackRate, ServiceDescription, UIntPairsWithId, UIntVWithId,
};
pub use types::{
    AudioSamplingRate, ConditionalUint, FrameRate, Ratio, Sap, XsDateTime, XsDuration,
};
