//! Reusable building blocks for After Effects effect plug-ins written in Rust.
//!
//! Everything outside the `ae` feature is host-independent so it can be unit
//! tested on any CI runner (Windows, macOS arm64/x86_64) without After Effects.
//!
//! - [`world`]: validate world geometry (stride sign, alignment, NULL, overflow)
//!   before touching any pixel.
//! - [`pixels`]: decode/encode AE ARGB rows (8/16/32 bpc) to straight RGBA f32.
//! - [`alpha`]: premultiply helpers and a straight-alpha acceptance check.
//! - [`cleanup`]: always check layers back in on error and panic paths.
//! - [`parallel`]: bounded, deterministic row parallelism without a global pool.
//! - [`simd`]: example kernel with runtime CPU feature detection and a
//!   bit-identical scalar fallback.
//! - [`sdk`]: compare SDK integer fields whose types differ between the Windows
//!   and macOS headers.
//! - `ae` (feature): adapters for the pinned `after-effects` crate.

pub mod alpha;
pub mod cleanup;
pub mod parallel;
pub mod pixels;
pub mod sdk;
pub mod simd;
pub mod world;

#[cfg(feature = "ae")]
pub mod ae;

pub use cleanup::{with_checkouts, with_render_cleanup, CheckinHost, CheckoutTracker};
pub use world::{checked_world_access, Depth, WorldAccessError, WorldLayout};
