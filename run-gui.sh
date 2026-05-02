#!/usr/bin/env bash
# 构建并运行 POV Baker GUI
#
# 使用系统 nightly 工具链（非 esp 分支），避免 build-std 冲突。
# 用法:
#   ./run-gui.sh          # 编译并运行
#   ./run-gui.sh build    # 仅编译
#   ./run-gui.sh check    # 仅检查

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BAKER_DIR="$SCRIPT_DIR/pov-baker"

# 使用 nightly 工具链，独立 target 目录，跳过 workspace 的 build-std
export CARGO_TARGET_DIR="$SCRIPT_DIR/target/gui"

ACTION="${1:-run}"

case "$ACTION" in
  check)
    cargo +nightly check \
      --target x86_64-unknown-linux-gnu \
      --manifest-path "$BAKER_DIR/Cargo.toml" \
      --bin baker-gui \
      --features gui
    ;;
  build)
    cargo +nightly build --release \
      --target x86_64-unknown-linux-gnu \
      --manifest-path "$BAKER_DIR/Cargo.toml" \
      --bin baker-gui \
      --features gui
    echo "✅ 构建完成: $CARGO_TARGET_DIR/x86_64-unknown-linux-gnu/release/baker-gui"
    ;;
  run|*)
    cargo +nightly run  \
      --target x86_64-unknown-linux-gnu \
      --manifest-path "$BAKER_DIR/Cargo.toml" \
      --bin baker-gui \
      --features gui
    ;;
esac
