# NUEDC POV Display — 体积极速显示系统

> 一个基于 **ESP32-C5** 的 16×16 LED 矩阵旋转 POV 显示系统。
> PCB 竖直站立绕 Z 轴旋转，利用视觉暂留效应在空中呈现立体 3D 画面。

---

## Hardware

| 芯片 | ESP32-C5 (RISC-V) |
|------|-------------------|
| PSRAM | 8 MB |
| 编码器 | AS5047P (SPI + DMA) |
| 显示 | 4×74HC595 (PARL_IO + DMA) |
| 引脚 | 见 [ARCHITECTURE.md](ARCHITECTURE.md) |

---

## Quick Start

```bash
# 编译固件（含固化动画）
cargo build --release -p pov-firmware

# 烧录
probe-rs run --chip=esp32c5 target/riscv32imac-unknown-none-elf/release/pov-firmware

# 本地测试前端
python3 serve-frontend.py
```

## 烘焙动画

### 浏览器（无需任何安装）
打开 ESP32 控制面板 → 点击形状按钮 → JS 自动生成 + 上传

支持：正方体 / 球体 / 圆柱 / 圆锥（自定义步数）

### CLI（Bevy，适合 glTF 模型）

```bash
# 基本形状
cargo run -p pov-baker -- cube -o cube.bin --steps 1024
cargo run -p pov-baker -- sphere -o sphere.bin --radius 12
cargo run -p pov-baker -- cylinder -o cylinder.bin
cargo run -p pov-baker -- cone -o cone.bin

# glTF/GLB 模型
cargo run -p pov-baker -- gltf -i model.glb -o anim.bin
```

### 上传

```bash
# 浏览器拖入 .bin 文件
# 或 curl
curl -X POST http://esp32-ip/api/upload --data-binary @anim.bin
```

---

## Project Structure

```
NUEDC_Preliminary_rust/
├── .cargo/config.toml
├── Cargo.toml
├── build.rs                — Bevy 生成固化动画
├── frontend/index.html     — 控制面板 + 帧预览 + JS 形状生成
├── serve-frontend.py       — 本地测试服务器
├── run-frontend.sh         — Rust 版测试服务器
├── pov-core/               — 共享类型 (Frame, Keyframe, Animation)
├── pov-baker/              — CLI 烘焙器 (Bevy + gltf)
├── tools/serve-frontend/
└── src/                    — ESP32-C5 固件
    ├── main.rs
    ├── lib.rs
    ├── fsm/
    ├── display/
    ├── sensor/
    ├── control/
    └── web/
```

---

## Web API

| Method | Path | 说明 |
|--------|------|------|
| `GET` | `/` | 控制面板 |
| `GET` | `/api/status` | 系统状态 |
| `POST` | `/api/start` | 启动显示 |
| `POST` | `/api/stop` | 停止显示 |
| `POST` | `/api/upload` | 上传 postcard Animation |

---

## Key Decisions

1. **无中断** — 全轮询
2. **稀疏 keyframe** — 只存变化帧
3. **PARL_IO + DMA** — 16 µs/帧
4. **PSRAM 堆** — 8MB
5. **双烘焙通道** — CLI (Bevy) + 浏览器 (JS)

---

## License

MIT
