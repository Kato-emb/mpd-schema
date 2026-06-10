//! Handwritten model structs mirroring `DASH-MPD.xsd`.

pub mod element;
pub mod types;

pub use element::{Element, Node};
pub use types::{ConditionalUint, FrameRate, Ratio, XsDuration};
