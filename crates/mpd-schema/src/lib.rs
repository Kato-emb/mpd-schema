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
    reason = "ADR-0007 の内部継ぎ目。利用側の de/ser は Phase 4 で実装されるため、それまで lib ビルドでは未使用になる"
)]
mod backend;
pub mod error;
#[allow(
    dead_code,
    reason = "ADR-0007 の内部継ぎ目。利用側の de/ser は Phase 4 で実装されるため、それまで lib ビルドでは未使用になる"
)]
mod event;
pub mod model;
