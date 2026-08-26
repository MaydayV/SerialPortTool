#!/bin/bash
set -u

APP_NAME="串口助手 SerialPortTool.app"
APP_PATH="/Applications/$APP_NAME"

printf '\nSerialPortTool macOS 安装修复工具\n'
printf '%s\n' '================================'
printf '\n'

if [ ! -d "$APP_PATH" ]; then
  echo "未找到：/Applications/$APP_NAME"
  echo "请先将 DMG 中的 App 拖入“应用程序”文件夹，然后重新双击本脚本。"
  echo
  read -r -p "按回车键退出..." _
  exit 1
fi

echo "正在移除 macOS 隔离属性..."
/usr/bin/xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true

echo "正在执行本机 ad-hoc 签名..."
if ! /usr/bin/codesign --force --deep --sign - "$APP_PATH"; then
  echo
  echo "签名失败。请确认 App 已完整复制到“应用程序”文件夹。"
  read -r -p "按回车键退出..." _
  exit 1
fi

/usr/bin/xattr -dr com.apple.quarantine "$APP_PATH" 2>/dev/null || true
echo "修复完成，正在启动 SerialPortTool。"
/usr/bin/open "$APP_PATH"
sleep 1
