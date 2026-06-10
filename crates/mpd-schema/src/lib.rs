//! Bidirectional conversion between MPEG-DASH MPD documents and Rust structs.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::panic,
        clippy::arithmetic_side_effects,
        reason = "テストでは失敗箇所を即座に特定するため unwrap / panic を許容し、期待値計算の素朴な算術を許容する"
    )
)]

pub mod error;
pub mod model;
