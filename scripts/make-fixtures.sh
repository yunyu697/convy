#!/usr/bin/env bash
# 生成用于冒烟测试的样例文件。仅依赖系统已安装的 ffmpeg。
set -e

DIR="${1:-./fixtures}"
mkdir -p "$DIR"

echo "==> 生成测试视频 (2s, 480p)"
ffmpeg -y -loglevel error -f lavfi -i "testsrc=size=854x480:rate=25:duration=2" \
  -f lavfi -i "sine=frequency=440:duration=2" \
  -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -shortest \
  "$DIR/sample.mp4"

echo "==> 生成测试音频 (3s)"
ffmpeg -y -loglevel error -f lavfi -i "sine=frequency=440:duration=3" \
  -c:a libmp3lame -b:a 128k "$DIR/sample.mp3"

echo "==> 生成测试图片 (1600x1200)"
ffmpeg -y -loglevel error -f lavfi -i "testsrc2=size=1600x1200:duration=1" \
  -frames:v 1 "$DIR/sample.png"

echo "==> 生成测试动图"
ffmpeg -y -loglevel error -f lavfi -i "testsrc=size=320x240:rate=10:duration=1" \
  -vf "fps=10" "$DIR/sample.gif"

echo "==> 生成测试文档"
printf '# 测试文档\n\n这是一个用于验证 Markdown 转 PDF 的样例。\n\n- 第一项\n- 第二项\n' \
  > "$DIR/sample.md"

ls -lh "$DIR"
