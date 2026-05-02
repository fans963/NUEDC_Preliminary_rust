#!/usr/bin/env bash
# 构建并运行本机前端测试服务器（Rust 版本）
#
# 用法:
#   ./run-frontend.sh [端口]
#
# workspace .cargo/config.toml 设了 riscv 目标，
# 这里用 --target 显式覆盖为本机架构。

set -euo pipefail

PORT="${1:-8000}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TOOL_DIR="$SCRIPT_DIR/tools/serve-frontend"
HOST_TARGET="$(rustc -vV | grep '^host:' | cut -d' ' -f2)"

echo "==> 编译 serve-frontend (target=$HOST_TARGET)..."
cargo build --release \
    --manifest-path "$TOOL_DIR/Cargo.toml" \
    --target "$HOST_TARGET"

echo "==> 启动 http://localhost:$PORT"
echo "    Ctrl+C 停止"
"$TOOL_DIR/target/$HOST_TARGET/release/serve-frontend" "$PORT"
