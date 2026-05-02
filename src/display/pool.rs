//! # AnimationPool — 多页动画存储 + O(log n) 帧查找
//!
//! 动画数据使用固定大小的 `heapless::Vec` 存储在内部 DRAM，
//! 不依赖全局堆分配器（堆只给 Embassy 内部使用）。

use pov_core::serial::{Animation, Keyframe};

pub const ANGLE_STEPS: usize = 1024;

// ── 容量限制 ───────────────────────────────────────────────────
//
// 单页 keyframe 数：典型 3D 模型一圈约 100-300 变化角度。
// 动画面数：静态对象 ~1 页，动态 3D 动画 ~10-20 页。
//
// 总内存：1024 keyframe × 34 bytes ≈ 34 KB。
// 在 32 KB 堆 + 内部 SRAM 下刚好。

/// 单页最大 keyframe 数。
const MAX_KEYFRAMES_PER_PAGE: usize = 512;

/// 最大页数。
const MAX_PAGES: usize = 8;

/// 总 keyframe 上限（所有页加起来）。
const MAX_TOTAL_KEYFRAMES: usize = 1024;

// ── Flat 存储 ───────────────────────────────────────────────────
//
// 不堆分配。所有 keyframe 在一个连续数组中，页通过偏移量索引。

/// Flat keyframe 存储。
static mut KEYFRAME_STORE: [Keyframe; MAX_TOTAL_KEYFRAMES] = [Keyframe { angle_tick: 0, frame: [0; 16] }; MAX_TOTAL_KEYFRAMES];

/// 每页的 keyframe 范围 `(start, end)`，`end` 是独占边界。
static mut PAGE_RANGES: [(u16, u16); MAX_PAGES] = [(0, 0); MAX_PAGES];

/// 实际页数。
static mut PAGE_COUNT: usize = 0;

/// 转数计数器。
static mut REV_COUNTER: u64 = 0;

/// 是否有已加载的动画。
static mut ANIMATION_LOADED: bool = false;

/// 动画上传次数。
static mut ANIM_COUNT: u32 = 0;

// ── 写入 ────────────────────────────────────────────────────

/// 替换动画数据。
pub fn set_animation(anim: Animation) -> Result<(), &'static str> {
    let page_count = anim.pages.len();
    if page_count == 0 || page_count > MAX_PAGES {
        return Err("pages out of range");
    }

    unsafe {
        let mut total_kf = 0u16;

        for (pi, page) in anim.pages.iter().enumerate() {
            let kf_count = page.keyframes.len();
            if kf_count > MAX_KEYFRAMES_PER_PAGE {
                return Err("too many keyframes in page");
            }
            if total_kf as usize + kf_count > MAX_TOTAL_KEYFRAMES {
                return Err("total keyframes exceeded");
            }

            // 复制 keyframe 到 flat 存储
            for (ki, kf) in page.keyframes.iter().enumerate() {
                KEYFRAME_STORE[total_kf as usize + ki] = *kf;
            }

            PAGE_RANGES[pi] = (total_kf, total_kf + kf_count as u16);
            total_kf += kf_count as u16;
        }

        PAGE_COUNT = page_count;
        ANIMATION_LOADED = true;
        REV_COUNTER = 0;
        ANIM_COUNT = ANIM_COUNT.wrapping_add(1);
    }

    Ok(())
}

// ── 读取（显示轮询热路径） ─────────────────────────────────

/// 读取（转数, 角度）对应的帧。
#[inline]
pub fn get_frame(rev: u64, angle_idx: usize) -> [u8; 32] {
    unsafe {
        let pc = PAGE_COUNT;
        if pc == 0 {
            return [0u8; 32];
        }

        let page_idx = (rev as usize) % pc;
        let (start, end) = PAGE_RANGES[page_idx];
        let count = end - start;

        if count == 0 {
            return [0u8; 32];
        }

        // binary search within this page's range
        let slice = core::slice::from_raw_parts(
            core::ptr::addr_of!(KEYFRAME_STORE) as *const Keyframe,
            MAX_TOTAL_KEYFRAMES,
        );
        let page_slice = &slice[start as usize..end as usize];
        let idx = page_slice.partition_point(|k| (k.angle_tick as usize) <= angle_idx);

        if idx == 0 {
            return [0u8; 32];
        }

        frame_to_bytes(&slice[idx - 1].frame)
    }
}

/// 转数计数器递增。
#[inline]
pub fn increment_rev() {
    unsafe { REV_COUNTER = REV_COUNTER.wrapping_add(1); }
}

#[inline]
pub fn rev_count() -> u64 {
    unsafe { REV_COUNTER }
}

// ── 状态查询 ───────────────────────────────────────────────

#[inline]
pub fn has_animation() -> bool {
    unsafe { ANIMATION_LOADED }
}

pub fn animation_count() -> u32 {
    unsafe { ANIM_COUNT }
}

pub fn page_count() -> usize {
    unsafe { PAGE_COUNT }
}

// ── 辅助 ────────────────────────────────────────────────────

#[inline]
fn frame_to_bytes(frame: &[u16; 16]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, &row) in frame.iter().enumerate() {
        let le = row.to_le_bytes();
        out[i * 2] = le[0];
        out[i * 2 + 1] = le[1];
    }
    out
}
