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

#[allow(
    dead_code,
    reason = "ADR-0007 の内部継ぎ目。Phase 5 の公開 API が de/ser を呼ぶまで、lib ビルドでは未使用になる"
)]
mod backend;
#[allow(
    dead_code,
    reason = "Phase 5 の公開 API（Mpd::from_slice 等）から呼ばれるまで、lib ビルドでは未使用になる"
)]
mod de;
pub mod error;
#[allow(
    dead_code,
    reason = "ADR-0007 の内部継ぎ目。Phase 5 の公開 API が de/ser を呼ぶまで、lib ビルドでは未使用になる"
)]
mod event;
pub mod model;
#[allow(
    dead_code,
    reason = "Phase 5 の公開 API（Mpd::write_to 等）から呼ばれるまで、lib ビルドでは未使用になる"
)]
mod ser;
