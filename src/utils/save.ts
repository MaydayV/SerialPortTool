import { isTauri } from "@tauri-apps/api/core";
import { api } from "../api";

export type OutputKind = "log" | "templates" | "curve";

const EXPORT_CHUNK_CHARS = 1024 * 1024;

function browserDownload(text: string, fileName: string, mimeType: string) {
  const blob = new Blob([text], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
}

/**
 * 桌面端使用系统保存面板取得 App Sandbox 文件授权，并分块写入以控制 IPC 峰值。
 * 浏览器预览环境继续使用标准下载行为。
 */
export async function saveTextFile(
  kind: OutputKind,
  text: string,
  browserFileName: string,
  mimeType: string
): Promise<boolean> {
  if (!isTauri()) {
    browserDownload(text, browserFileName, mimeType);
    return true;
  }

  const path = await api.selectOutputFile(kind);
  if (!path) return false;

  if (!text.length) {
    await api.writeUserFile(path, "", true);
    return true;
  }

  let first = true;
  for (let offset = 0; offset < text.length; ) {
    let end = Math.min(text.length, offset + EXPORT_CHUNK_CHARS);
    // 不在 UTF-16 代理项中间切块，避免序列化时产生替换字符。
    if (
      end < text.length &&
      end > offset &&
      text.charCodeAt(end - 1) >= 0xd800 &&
      text.charCodeAt(end - 1) <= 0xdbff
    ) {
      end -= 1;
    }
    await api.writeUserFile(path, text.slice(offset, end), first);
    first = false;
    offset = end;
  }
  return true;
}
