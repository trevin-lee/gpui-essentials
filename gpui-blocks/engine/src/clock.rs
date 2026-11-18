//! GPUI's clock: std's `Instant`, but in a browser, which has none, web-time's.

#[cfg(not(target_family = "wasm"))]
pub(crate) use std::time::Instant;
#[cfg(target_family = "wasm")]
pub(crate) use web_time::Instant;
