use alloc::vec::Vec;
use pov_core::Frame;

/// Result of rendering a 3D scene to POV frames.
#[derive(Debug, Clone)]
pub struct RenderResult {
    /// One Frame per angle step.
    pub frames: Vec<Frame>,
    /// Number of angle steps.
    pub num_steps: usize,
    /// Number of distinct LEDs lit across all frames.
    pub active_leds: usize,
}

/// Generic 3D-to-frame renderer interface.
///
/// Two families of implementors:
/// - `MockRenderer` (always available, pure math, no external deps)
/// - `bevy::BevyRenderer` (optional, needs `bevy` feature)
pub trait FrameRenderer {
    type Error: core::fmt::Debug;

    /// Render a 3D model from a file path.
    fn render_file(&self, model_path: &str) -> Result<RenderResult, Self::Error>;

    /// Render from in-memory model bytes.
    fn render_bytes(&self, model_bytes: &[u8]) -> Result<RenderResult, Self::Error>;
}

/// Shared rendering configuration.
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Angle steps per full revolution.
    pub num_steps: usize,
    /// Intermediate render resolution (before downsampling to 16×16).
    pub render_size: u32,
    /// Camera distance from origin in scene units (1 unit = 1 cm).
    pub camera_distance: f32,
    /// Orthographic projection size in scene units.
    pub ortho_size: f32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            num_steps: 1024,
            render_size: 256,
            camera_distance: 10.0,
            ortho_size: 6.0, // matches 60mm LED matrix active area
        }
    }
}
