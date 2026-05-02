//! 固化动画 — 编译时生成的 postcard 编码多页动画。
//!
//! build.rs 使用 `pov_baker::pipeline::frames_to_postcard()` 生成，
//! 启动时通过 `AnimationPool::set_animation()` 加载。

use pov_core::serial::Animation;
use crate::display::pool;

/// 固化动画数据（编译时由 build.rs 生成）
pub static EMBEDDED_ANIM: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/embedded_anim.postcard"
));

/// 加载固化动画到 pool。
pub fn load_embedded() {
    let anim: Animation = match postcard::from_bytes(EMBEDDED_ANIM) {
        Ok(a) => a,
        Err(e) => {
            defmt::warn!("Failed to parse embedded animation: {:?}", defmt::Debug2Format(&e));
            return;
        }
    };

    match pool::set_animation(anim) {
        Ok(()) => defmt::info!("Embedded animation loaded ({} pages)", pool::page_count()),
        Err(e) => defmt::warn!("Failed to load embedded animation: {}", e),
    }
}
