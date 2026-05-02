/// A 16×16 monochrome frame: 16 rows × 16 bits = 32 bytes.
///
/// Bit `j` in `self[y]` corresponds to pixel at column `j`, row `y`.
/// `LSB = column 0`, `MSB = column 15`.
pub type Frame = [u16; 16];

/// Raw bytes view of a frame (suitable for DMA).
pub type FrameBytes = [u8; 32];

/// Physical geometry of the LED matrix.
///
/// The matrix is a 16×16 grid standing vertically and rotating around
/// its center Z-axis. Each LED position maps to a 3D coordinate at
/// a given rotation angle.
#[derive(Debug, Clone, Copy)]
pub struct MatrixGeometry {
    /// Number of columns (horizontal).
    pub cols: usize,
    /// Number of rows (vertical).
    pub rows: usize,
    /// Pitch between adjacent LED centers in millimeters.
    pub pitch_mm: f32,
}

impl MatrixGeometry {
    /// Standard 16×16 matrix with 4mm pitch.
    pub const STANDARD: Self = Self {
        cols: 16,
        rows: 16,
        pitch_mm: 4.0,
    };

    /// Create a new geometry description.
    pub const fn new(cols: usize, rows: usize, pitch_mm: f32) -> Self {
        Self {
            cols,
            rows,
            pitch_mm,
        }
    }

    /// Total number of LEDs.
    pub const fn led_count(&self) -> usize {
        self.cols * self.rows
    }

    /// Width of the active area in mm.
    pub fn width_mm(&self) -> f32 {
        (self.cols - 1) as f32 * self.pitch_mm
    }

    /// Height of the active area in mm.
    pub fn height_mm(&self) -> f32 {
        (self.rows - 1) as f32 * self.pitch_mm
    }

    /// Compute the 3D position of LED (col, row) at rotation angle θ (radians).
    ///
    /// Returns (x, y, z) in millimeters.
    /// The matrix rotates around the Z axis. col is the horizontal (radial)
    /// direction, row is the vertical direction.
    #[inline]
    pub fn led_position(&self, col: usize, row: usize, theta_rad: f32) -> (f32, f32, f32) {
        let cx = (col as f32 - (self.cols - 1) as f32 * 0.5) * self.pitch_mm;
        let cy = (row as f32 - (self.rows - 1) as f32 * 0.5) * self.pitch_mm;
        let c = libm::cosf(theta_rad);
        let s = libm::sinf(theta_rad);
        (cx * c, cy, cx * s)
    }

    /// Compute the world-space position of every LED at angle θ.
    /// Returns a flat array of (x, y, z) tuples, row-major order.
    pub fn all_led_positions(&self, theta_rad: f32) -> [(f32, f32, f32); 256] {
        let mut positions = [(0.0f32, 0.0f32, 0.0f32); 256];
        let mut idx = 0;
        for row in 0..self.rows {
            for col in 0..self.cols {
                positions[idx] = self.led_position(col, row, theta_rad);
                idx += 1;
            }
        }
        positions
    }
}

/// Convert a Frame to its raw byte representation (little-endian).
#[inline]
pub fn frame_to_bytes(frame: &Frame) -> FrameBytes {
    let mut bytes = [0u8; 32];
    for (i, row) in frame.iter().enumerate() {
        let le = row.to_le_bytes();
        bytes[i * 2] = le[0];
        bytes[i * 2 + 1] = le[1];
    }
    bytes
}

/// Convert raw bytes back into a Frame.
#[inline]
pub fn bytes_to_frame(bytes: &FrameBytes) -> Frame {
    let mut frame = [0u16; 16];
    for i in 0..16 {
        frame[i] = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
    }
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let f: Frame = [
            0xDEAD, 0xBEEF, 0xCAFE, 0xABCD, 0x1234, 0x5678, 0x9ABC, 0xDEF0, 0x1111, 0x2222,
            0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888,
        ];
        let bytes = frame_to_bytes(&f);
        let back = bytes_to_frame(&bytes);
        assert_eq!(f, back);
    }

    #[test]
    fn geometry_standard_size() {
        let g = MatrixGeometry::STANDARD;
        assert_eq!(g.led_count(), 256);
        assert!((g.width_mm() - 60.0).abs() < 1e-6);
        assert!((g.height_mm() - 60.0).abs() < 1e-6);
    }

    #[test]
    fn led_position_center_neighbors() {
        let g = MatrixGeometry::STANDARD;
        // LEDs (7,7) and (8,8) straddle the center
        // (7,7): cx=-2, cy=-2 → X=-2, Y=-2, Z=0
        let (x7, y7, z7) = g.led_position(7, 7, 0.0);
        assert!((x7 - (-2.0)).abs() < 1e-5, "x7 should be -2, got {}", x7);
        assert!((y7 - (-2.0)).abs() < 1e-5, "y7 should be -2, got {}", y7);
        assert!(z7.abs() < 1e-5, "z7 should be ~0, got {}", z7);

        // (8,8): cx=2, cy=2 → X=2, Y=2, Z=0
        let (x8, y8, z8) = g.led_position(8, 8, 0.0);
        assert!((x8 - 2.0).abs() < 1e-5, "x8 should be 2, got {}", x8);
        assert!((y8 - 2.0).abs() < 1e-5, "y8 should be 2, got {}", y8);
        assert!(z8.abs() < 1e-5, "z8 should be ~0, got {}", z8);
    }

    #[test]
    fn led_position_bottom_left_at_90deg() {
        let g = MatrixGeometry::STANDARD;
        let (x, y, z) = g.led_position(0, 0, core::f32::consts::FRAC_PI_2);
        assert!(x.abs() < 1e-4, "x should be ~0, got {}", x);
        assert!((y - (-30.0)).abs() < 1e-4, "y should be -30, got {}", y);
        assert!((z - (-30.0)).abs() < 1e-4, "z should be -30, got {}", z);
    }

    #[test]
    fn led_position_top_right_at_0deg() {
        let g = MatrixGeometry::STANDARD;
        // Top-right LED (15,15) at 0°: cos=1, sin=0
        // In local frame: cx = +30, cy = +30
        // After rotation: X = +30, Y = +30, Z = 0
        let (x, y, z) = g.led_position(15, 15, 0.0);
        assert!((x - 30.0).abs() < 1e-6, "x should be 30, got {}", x);
        assert!((y - 30.0).abs() < 1e-6, "y should be 30, got {}", y);
        assert!(z.abs() < 1e-6, "z should be 0, got {}", z);
    }
}
