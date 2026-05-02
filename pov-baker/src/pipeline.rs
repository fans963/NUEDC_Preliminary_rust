//! # Pipeline — 通用烘焙管线

use alloc::vec::Vec;
use pov_core::Frame;

/// 多圈帧 → postcard Animation。
pub fn frames_to_animation(pages: &[Vec<Frame>]) -> Vec<u8> {
    crate::diff::bake_animation(pages)
}

/// 单圈帧 → postcard Animation（兼容 build.rs）。
pub fn frames_to_postcard(frames: &[Frame]) -> Vec<u8> {
    crate::diff::bake_frames(frames)
}

/// 裸 LUT 字节（每帧 32 bytes 连续排列）。
pub fn frames_to_lut_bytes(frames: &[Frame]) -> Vec<u8> {
    let mut bytes = alloc::vec![0u8; frames.len() * 32];
    for (step, frame) in frames.iter().enumerate() {
        for (row, &row_bits) in frame.iter().enumerate() {
            let offset = step * 32 + row * 2;
            bytes[offset] = row_bits as u8;
            bytes[offset + 1] = (row_bits >> 8) as u8;
        }
    }
    bytes
}

/// 降采样图像到 16×16 Frame。
pub fn downsample_to_frame(pixels: &[u8], width: u32, height: u32, threshold: u8) -> Frame {
    let mut frame = [0u16; 16];
    let bw = width / 16;
    let bh = height / 16;

    for row in 0..16 {
        for col in 0..16 {
            let cx = (col * bw + bw / 2) as usize;
            let cy = (row * bh + bh / 2) as usize;
            let idx = (cy * width as usize + cx) * 4;
            let lit = if idx + 3 < pixels.len() {
                let luma = (pixels[idx] as u32 * 299
                    + pixels[idx + 1] as u32 * 587
                    + pixels[idx + 2] as u32 * 114)
                    / 1000;
                luma > threshold as u32
            } else {
                false
            };
            if lit {
                frame[row as usize] |= 1u16 << (col as u16);
            }
        }
    }
    frame
}
