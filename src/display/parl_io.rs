//! # PARL_IO + 4×74HC595 列扫描驱动器
//!
//! 列选 (IO25, IO9) 为 one-hot，每次只选通一列。
//! 行数据 (IO23, IO24) 输出该列 16 行中哪些 LED 亮。
//! 均为高电平有效。
//!
//! ## 引脚
//!
//! | PARL_IO | IO | HC595 | 数据 |
//! |---------|----|-------|------|
//! | DATA0   | 25 | HC595#0 | 列选低字节 (bit 0-7) |
//! | DATA1   | 9  | HC595#1 | 列选高字节 (bit 8-15) |
//! | DATA2   | 23 | HC595#2 | 行数据低字节 (row 0-7) |
//! | DATA3   | 24 | HC595#3 | 行数据高字节 (row 8-15) |
//! | CLK_OUT | 5  | 所有 SRCLK | |
//! | RCLK    | 4  | 独立 GPIO 脉冲锁存 |
//!
//! 时序: 20 MHz → 一列 8 字节 DMA ≈ 1 µs → 16 列 = 16 µs。

use esp_hal::dma::DmaTxBuf;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::parl_io::ParlIoTx;
use esp_hal::Blocking;
use crate::display::driver::FrameWriter;

/// 每列 8 字节 PARL_IO 数据
const COL_BYTES: usize = 8;

/// PARL_IO + DMA 列扫描驱动器。
pub struct Hc595ParlIo<'d> {
    tx: Option<ParlIoTx<'d, Blocking>>,
    dma_buf: Option<DmaTxBuf>,
    rclk: Output<'d>,
}

impl<'d> Hc595ParlIo<'d> {
    /// `dma_buf` 需 `dma_tx_buffer!(8)` 创建。
    pub fn new(
        tx: ParlIoTx<'d, Blocking>,
        dma_buf: DmaTxBuf,
        rclk: impl esp_hal::gpio::OutputPin + 'd,
    ) -> Self {
        Self {
            tx: Some(tx),
            dma_buf: Some(dma_buf),
            rclk: Output::new(rclk, Level::Low, OutputConfig::default()),
        }
    }

    /// 扫描所有 16 列。
    fn scan_all_cols(&mut self, frame: &[u16; 16]) -> Result<(), &'static str> {
        // 帧转置: frame[col] = 该列所有行的 bitmask
        let mut col_rows = [0u16; 16];
        for col in 0..16 {
            let mut rows = 0u16;
            for row in 0..16 {
                if (frame[row] >> col) & 1 != 0 {
                    rows |= 1u16 << row;
                }
            }
            col_rows[col] = rows;
        }

        for col in 0..16 {
            self.scan_col(col, col_rows[col])?;
        }
        Ok(())
    }

    /// 选通一列 → 输出行数据 → RCLK 脉冲。
    fn scan_col(&mut self, col: usize, row_data: u16) -> Result<(), &'static str> {
        let col_sel = 1u16 << col;
        let col_lo = col_sel as u8;
        let col_hi = (col_sel >> 8) as u8;
        let row_lo = row_data as u8;
        let row_hi = (row_data >> 8) as u8;

        // 打包 8 nibble → 4 字节
        // 每个 nibble = {r_hi, r_lo, c_hi, c_lo} = {IO24, IO23, IO9, IO25}
        // PARL_IO BitPackOrder::Msb: 高半字节先发
        let mut packed = [0u8; COL_BYTES];
        for cycle in 0..8 {
            let c_lo = (col_lo >> cycle) & 1;
            let c_hi = (col_hi >> cycle) & 1;
            let r_lo = (row_lo >> cycle) & 1;
            let r_hi = (row_hi >> cycle) & 1;
            let nibble = (r_hi << 3) | (r_lo << 2) | (c_hi << 1) | c_lo;
            if cycle % 2 == 0 {
                packed[cycle / 2] = nibble << 4;  // 高半字节
            } else {
                packed[cycle / 2] |= nibble;      // 低半字节
            }
        }

        // DMA 传输
        let tx = self.tx.take().ok_or("no TX")?;
        let mut dbuf = self.dma_buf.take().ok_or("no DMA buf")?;
        dbuf.as_mut_slice()[..COL_BYTES].copy_from_slice(&packed);
        let Ok(transfer) = tx.write(COL_BYTES, dbuf) else {
            return Err("DMA write failed");
        };
        let (result, tx_back, buf_back) = transfer.wait();
        result.map_err(|_| "DMA error")?;
        self.tx = Some(tx_back);
        self.dma_buf = Some(buf_back);

        // RCLK 脉冲
        self.rclk.set_high();
        self.rclk.set_low();

        Ok(())
    }
}

impl FrameWriter for Hc595ParlIo<'_> {
    type Error = &'static str;

    fn write_frame_bytes(&mut self, data: &[u8; 32]) -> Result<(), Self::Error> {
        let mut frame: [u16; 16] = [0u16; 16];
        for i in 0..16 {
            frame[i] = u16::from_le_bytes([data[i * 2], data[i * 2 + 1]]);
        }
        self.scan_all_cols(&frame)
    }
}
