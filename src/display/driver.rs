//! # POV Display — 轮询模式驱动
//!
//! 轮询模式：没有 ISR，没有 IRAM，没有中断。
//! `display_task` 每次轮询编码器后调用 `render_angle()` 输出对应的帧。

use crate::display::pool;

/// POV 显示驱动器。
pub struct PovDisplay<W> {
    writer: W,
    encoder_steps: usize,
}

impl<W> PovDisplay<W> {
    pub fn new(writer: W, encoder_steps: usize) -> Self {
        Self { writer, encoder_steps }
    }

    pub fn set_steps(&mut self, steps: usize) {
        self.encoder_steps = steps;
    }

    /// 暴露 writer 引用给调用者直接写入帧数据。
    pub fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<W> PovDisplay<W>
where
    W: FrameWriter,
{
    /// 根据编码器角度输出对应帧（使用当前转数）。
    #[inline]
    pub fn render_angle(&mut self, angle_idx: usize) {
        if !pool::has_animation() {
            return;
        }

        let mask = self.encoder_steps - 1;
        let rev = pool::rev_count();
        let frame = pool::get_frame(rev, angle_idx & mask);
        let _ = self.writer.write_frame_bytes(&frame);
    }

    /// 清屏。
    pub fn clear(&mut self) {
        let blank = [0u8; 32];
        let _ = self.writer.write_frame_bytes(&blank);
    }
}

/// 32 字节帧写入器。
///
/// 实现者应为**非阻塞 DMA 触发**：
/// - 写入 SPI 数据寄存器
/// - 硬件自动移位输出
/// - 函数立即返回，不等传输完成
pub trait FrameWriter {
    type Error;
    fn write_frame_bytes(&mut self, data: &[u8; 32]) -> Result<(), Self::Error>;
}

/// 空写入器 — 什么也不做，用于编译期占位。
pub struct NullWriter;

impl FrameWriter for NullWriter {
    type Error = ();
    fn write_frame_bytes(&mut self, _data: &[u8; 32]) -> Result<(), ()> {
        Ok(())
    }
}

