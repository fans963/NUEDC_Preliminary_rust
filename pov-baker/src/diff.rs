//! 帧序列 → 多页 Animation keyframe 编码 + postcard 序列化。
//!
//! 核心函数 `bake_animation` 接受 N 圈 × M 帧的数据，
//! 输出只包含变化角度的稀疏 keyframe 格式。

use alloc::vec::Vec;
use pov_core::serial::{Animation, Keyframe, Page};
use pov_core::Frame;

/// 将多圈帧数据烘焙为 postcard 编码的 `Animation`。
pub fn bake_animation(pages: &[Vec<Frame>]) -> Vec<u8> {
    let pages: Vec<Page> = pages.iter().map(|frames| frames_to_page(frames)).collect();
    let anim = Animation { pages };
    postcard::to_allocvec(&anim).expect("Animation serialization failed")
}

/// 单圈帧序列 → Page（仅记录变化的帧）。
pub fn frames_to_page(frames: &[Frame]) -> Page {
    let mut keyframes = Vec::new();
    let mut prev: Frame = [0u16; 16];
    for (tick, &frame) in frames.iter().enumerate() {
        if frame != prev {
            keyframes.push(Keyframe {
                angle_tick: tick as u16,
                frame,
            });
            prev = frame;
        }
    }
    Page { keyframes }
}

/// 单圈烘焙 — 兼容 build.rs。
pub fn bake_frames(frames: &[Frame]) -> Vec<u8> {
    let page = frames_to_page(frames);
    let anim = Animation {
        pages: alloc::vec![page],
    };
    postcard::to_allocvec(&anim).expect("Animation serialization failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_frames(n: usize) -> Vec<Frame> {
        let mut frames = Vec::with_capacity(n);
        let mut prev = [0u16; 16];
        for i in 0..n {
            let mut f = prev;
            if i % 3 == 0 {
                f[7] ^= 1u16 << 7;
            }
            frames.push(f);
            prev = f;
        }
        frames
    }

    #[test]
    fn single_page_backward_compat() {
        let frames = fake_frames(64);
        let bytes = bake_frames(&frames);
        let anim: Animation = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(anim.pages.len(), 1);
    }

    #[test]
    fn multi_page_does_not_mix() {
        let p0 = fake_frames(16);
        let p1 = fake_frames(16);
        let bytes = bake_animation(&[p0, p1]);
        let anim: Animation = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(anim.pages.len(), 2);
    }

    #[test]
    fn keyframe_reconstruction_full() {
        let frames = fake_frames(64);
        let bytes = bake_frames(&frames);
        let anim: Animation = postcard::from_bytes(&bytes).unwrap();
        let page = &anim.pages[0];
        for angle in 0..64 {
            let expected = frames[angle];
            let idx = page
                .keyframes
                .partition_point(|k| (k.angle_tick as usize) <= angle);
            let actual = if idx == 0 { [0; 16] } else { page.keyframes[idx - 1].frame };
            assert_eq!(expected, actual, "mismatch at angle {}", angle);
        }
        assert!(bytes.len() < 64 * 32, "keyframe compression");
    }
}
