use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::Frame;

/// 一圈动画 — 只记录角度发生变化时的完整帧。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Page {
    /// 按 `angle_tick` 递增排序的 keyframe 列表。
    /// 相同 `angle_tick` 不应重复。
    pub keyframes: Vec<Keyframe>,
}

/// 某个角度的完整 16×16 帧。
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Keyframe {
    /// 角度索引 (0 .. 1023)
    pub angle_tick: u16,
    /// 16 行 × u16 = 32 bytes 的单色帧
    pub frame: Frame,
}

/// 完整动画 — 包含多圈（页），每圈内容可不同。
///
/// 播放：第 N 圈用 `pages[N % pages.len()]`。
/// 单页动画 `pages.len() == 1` 等价于循环播放同一圈。
#[derive(Serialize, Deserialize, Debug)]
pub struct Animation {
    pub pages: Vec<Page>,
}
