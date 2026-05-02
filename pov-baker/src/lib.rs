//! # pov-baker — POV 动画烘焙库
//!
//! 两种编译目标：
//! - **Host CLI** (`default`): 含 `clap` CLI，用于命令行烘焙
//! - **Embedded** (`default-features = false`): `no_std`，可直接编译进 ESP32-C5 固件

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod diff;
pub mod pipeline;
pub mod renderer;

#[cfg(feature = "std")]
pub mod bevy;

// Re-exports for convenience
pub use pipeline::{frames_to_lut_bytes, frames_to_postcard};
pub use renderer::{FrameRenderer, RenderConfig, RenderResult};
