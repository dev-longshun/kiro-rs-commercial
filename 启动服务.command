#!/bin/bash
cd "$(dirname "$0")"

# 杀掉之前可能残留的进程
lsof -ti:8990 | xargs kill -9 2>/dev/null

# 自动切换环境变量到本地服务（写入 profile 让新终端也生效）
echo "use_local > /dev/null" > ~/.claude_profile

echo "正在启动 kiro-rs 服务..."
echo "已自动切换 Claude Code 到本地模式"
echo "  URL: http://127.0.0.1:8990"
echo "  API Key: sk-kiro-rs-qazWSXedcRFV123456"
echo "管理面板: http://127.0.0.1:8990/admin"
echo "按 Ctrl+C 停止服务"
echo "---"

./target/release/kiro-rs
