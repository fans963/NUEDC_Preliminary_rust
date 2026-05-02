//! # AS5047P 磁编码器 SPI 驱动
//!
//! ## 两层架构
//!
//! - **`As5047p<SPI>`** — 泛型驱动层，与具体 HAL 无关，可移植任何 MCU
//! - **本例** — 在 `main.rs` 中用 `embedded-hal-bus` + `esp-hal` 创建 SPI
//!
//! ## SPI 配置要求
//!
//! | 参数 | 值 |
//! |------|-----|
//! | Mode | 3 (CPOL=1, CPHA=1) |
//! | 频率 | ≤ 10 MHz (推荐 1 MHz) |
//! | 数据 | 16-bit big-endian，MSB first |
//! | CS   | 低电平有效，硬件或 GPIO 控制均可 |
//!
//! ## 用法
//!
//! ```ignore
//! let spi = Spi::new(peripherals.SPI2, 1_000_000u32.Hz(), SpiMode::Mode3)?
//!     .with_sck(peripherals.GPIO6)
//!     .with_mosi(peripherals.GPIO7)
//!     .with_miso(peripherals.GPIO2);
//! let cs = Output::new(peripherals.GPIO10, Level::High);
//! let spi_dev = ExclusiveDevice::new(spi, cs)?;
//! let mut encoder = As5047p::new(spi_dev);
//! ```

use core::f32::consts::PI;
use embedded_hal::spi::SpiDevice;
use uom::si::angle::radian;
use uom::si::angular_velocity::radian_per_second;
use uom::si::f32::{Angle, AngularVelocity, Time};
use uom::si::time::second;

// ── AS5047P 协议常量 ────────────────────────────────────────────

const READ_FLAG: u16 = 1 << 14;
const PARITY_FLAG: u16 = 1 << 15;
const DATA_MASK_14BIT: u16 = 0x3fff;
const ERROR_FLAG: u16 = 1 << 14;

const REG_ANGLECOM: u16 = 0x3fff;
const COUNTS_PER_REV: f32 = 16384.0;

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum As5047pError<SpiE> {
    Spi(SpiE),
    BadParity,
    SensorError,
}

// ── Driver ───────────────────────────────────────────────────────────────────

/// AS5047P 14-bit 磁编码器驱动。
///
/// 泛型于 `SPI`：任意实现了 `embedded_hal::spi::SpiDevice<u8>` 的总线。
/// ESP32 上通过 `Spi::new()` + `ExclusiveDevice` 创建。
pub struct As5047p<SPI> {
    spi: SPI,
    parity_check: bool,
}

impl<SPI> As5047p<SPI> {
    /// 使用已配置好的 SPI device 构造驱动。
    ///
    /// SPI 需外部已配置为 Mode 3, 1 MHz。
    pub fn new(spi: SPI) -> Self {
        Self {
            spi,
            parity_check: true,
        }
    }

    /// 禁用偶校验检查（干扰小的短走线可关闭以省 ~0.5µs）。
    pub fn with_parity_check(mut self, enabled: bool) -> Self {
        self.parity_check = enabled;
        self
    }

    /// 释放 SPI device 所有权，归还底层总线。
    pub fn release(self) -> SPI {
        self.spi
    }

    /// 14-bit 原始值 → 角度（弧度）。
    #[inline]
    pub fn raw_to_angle(raw: u16) -> Angle {
        Angle::new::<radian>((raw & DATA_MASK_14BIT) as f32 * (2.0 * PI / COUNTS_PER_REV))
    }

    /// 14-bit 原始值 → LUT 索引 (0..1023)。
    #[inline]
    pub fn raw_to_lut_index(raw: u16, lut_steps: usize) -> usize {
        (raw as usize) * lut_steps / COUNTS_PER_REV as usize
    }
}

impl<SPI, SpiE> As5047p<SPI>
where
    SPI: SpiDevice<u8, Error = SpiE>,
{
    /// 读原始 14-bit 角度值 (0-16383)。
    #[inline]
    pub fn read_angle_raw(&mut self) -> Result<u16, As5047pError<SpiE>> {
        self.read_register(REG_ANGLECOM)
    }

    /// 读角度（含 `uom::si::Angle` 单位）。
    pub fn read_angle(&mut self) -> Result<Angle, As5047pError<SpiE>> {
        let raw = self.read_angle_raw()?;
        Ok(Self::raw_to_angle(raw))
    }

    /// 读角度（弧度）。
    pub fn read_angle_rad(&mut self) -> Result<f32, As5047pError<SpiE>> {
        Ok(self.read_angle()?.get::<radian>())
    }

    /// 读角度（度）。
    pub fn read_angle_deg(&mut self) -> Result<f32, As5047pError<SpiE>> {
        let raw = self.read_angle_raw()?;
        Ok((raw as f32) * (360.0 / COUNTS_PER_REV))
    }

    /// 读角度 + 直接转为 LUT 索引（0..1023）。
    ///
    /// 这是轮询循环中最常用的：一次 SPI 事务得到可用的 LUT 索引。
    #[inline]
    pub fn read_lut_index(&mut self, lut_steps: usize) -> Result<usize, As5047pError<SpiE>> {
        let raw = self.read_angle_raw()?;
        Ok(Self::raw_to_lut_index(raw, lut_steps))
    }

    /// 读取任意寄存器（14-bit 地址）。
    ///
    /// 每次读：发送命令 → 接收回复（两次 16-bit SPI 事务）。
    pub fn read_register(&mut self, reg_addr: u16) -> Result<u16, As5047pError<SpiE>> {
        let reg = reg_addr & DATA_MASK_14BIT;
        let read_cmd = add_even_parity(READ_FLAG | reg);

        // 发送读命令 (dummy 回应的同时接收部分数据)
        let _ = self.transfer16(read_cmd)?;
        // 发送 dummy 0 接收真正的角度数据
        let reply = self.transfer16(add_even_parity(0))?;

        if self.parity_check && !has_even_parity(reply) {
            return Err(As5047pError::BadParity);
        }
        if (reply & ERROR_FLAG) != 0 {
            return Err(As5047pError::SensorError);
        }
        Ok(reply & DATA_MASK_14BIT)
    }

    /// 16-bit SPI 读写。
    #[inline]
    fn transfer16(&mut self, word: u16) -> Result<u16, As5047pError<SpiE>> {
        let mut buf = word.to_be_bytes();
        self.spi
            .transfer_in_place(&mut buf)
            .map_err(As5047pError::Spi)?;
        Ok(u16::from_be_bytes(buf))
    }
}

// ── Velocity estimator ─────────────────────────────────────────────────────

/// 跨周期防环绕的角速度估计器。
///
/// 基于两次连续采样的角度差计算角速度，
/// 自动处理 14-bit 计数器的回绕 (0 ↔ 16383)。
pub struct As5047pVelocityEstimator {
    prev_raw: Option<u16>,
}

impl As5047pVelocityEstimator {
    pub const fn new() -> Self {
        Self { prev_raw: None }
    }

    pub fn reset(&mut self) {
        self.prev_raw = None;
    }

    /// 更新并返回角速度（弧度/秒）。
    pub fn update_rad_s(&mut self, raw_angle: u16, dt_seconds: f32) -> f32 {
        if dt_seconds <= 0.0 {
            return 0.0;
        }
        let Some(prev) = self.prev_raw else {
            self.prev_raw = Some(raw_angle);
            return 0.0;
        };
        let mut delta = raw_angle as i32 - prev as i32;
        // 防回绕：如果差值超过半圈，说明跨过了 0 点
        if delta > (COUNTS_PER_REV as i32 / 2) {
            delta -= COUNTS_PER_REV as i32;
        } else if delta < -(COUNTS_PER_REV as i32 / 2) {
            delta += COUNTS_PER_REV as i32;
        }
        self.prev_raw = Some(raw_angle);
        (delta as f32) * (2.0 * PI / COUNTS_PER_REV) / dt_seconds
    }

    /// 更新并返回角速度（含 `uom` 单位）。
    pub fn update(&mut self, raw_angle: u16, dt: Time) -> AngularVelocity {
        let speed = self.update_rad_s(raw_angle, dt.get::<second>());
        AngularVelocity::new::<radian_per_second>(speed)
    }
}

impl Default for As5047pVelocityEstimator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Parity helpers ─────────────────────────────────────────────────────────

/// 为 15-bit 字添加偶校验位 (bit 15)。
#[inline]
fn add_even_parity(word_15bit: u16) -> u16 {
    let low15 = word_15bit & !PARITY_FLAG;
    let parity_even = (low15.count_ones() & 1) == 0;
    if parity_even {
        low15
    } else {
        low15 | PARITY_FLAG
    }
}

/// 检查 16-bit 字是否含偶校验。
#[inline]
fn has_even_parity(word_16bit: u16) -> bool {
    (word_16bit.count_ones() & 1) == 0
}
