#!/usr/bin/env bash
# 构建并烧录 ESP32-C5 固件
#
# 用法:
#   ./run-firmware.sh build  # 仅编译
#   ./run-firmware.sh run    # 编译并烧录（默认）

set -euo pipefail

ACTION="${1:-run}"

# 使用 .cargo/firmware.toml 中的配置（build-std, target 等）
case "$ACTION" in
  build)
    cargo build --release --config .cargo/firmware.toml
    ;;
  run|*)
    cargo run --release --config .cargo/firmware.toml
    ;;
esac
