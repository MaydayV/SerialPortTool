// 字节/文本处理工具：hex 显示、ansi 彩色解析、编码解码

/** bytes -> "48 65 6C 6C 6F" 大写 hex */
export function bytesToHex(data: Uint8Array): string {
  const parts: string[] = new Array(data.length);
  for (let i = 0; i < data.length; i++) {
    parts[i] = data[i].toString(16).padStart(2, "0").toUpperCase();
  }
  return parts.join(" ");
}

/** hex 字符串 -> bytes，支持 "00 01" / "0001" / "0x00,0x01" */
export function hexToBytes(hex: string): Uint8Array | null {
  let cleaned = hex.replace(/0x/gi, "").replace(/[,\s]+/g, "");
  if (cleaned.length % 2 !== 0) return null;
  if (!/^[0-9a-fA-F]*$/.test(cleaned)) return null;
  const out = new Uint8Array(cleaned.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/** 可打印字符显示，不可打印显示 . */
export function bytesToAscii(data: Uint8Array): string {
  let out = "";
  for (let i = 0; i < data.length; i++) {
    const b = data[i];
    if (b >= 0x20 && b <= 0x7e) out += String.fromCharCode(b);
    else out += ".";
  }
  return out;
}

/** 按编码解码为文本 */
export function decodeText(
  data: Uint8Array,
  encoding: string
): string {
  try {
    if (encoding === "ASCII") {
      // 非可打印转义显示
      let out = "";
      for (let i = 0; i < data.length; i++) {
        const b = data[i];
        if (b === 0x0a) out += "\n";
        else if (b === 0x0d) out += "\r";
        else if (b === 0x09) out += "\t";
        else if (b >= 0x20 && b <= 0x7e) out += String.fromCharCode(b);
        else out += "\\x" + b.toString(16).padStart(2, "0");
      }
      return out;
    }
    const label = encoding === "UTF-8" ? "utf-8" : encoding.toLowerCase();
    return new TextDecoder(label).decode(data);
  } catch {
    return new TextDecoder("utf-8", { fatal: false }).decode(data);
  }
}

/** 转义字符串 -> bytes：支持 \n \r \t \xHH \0 等 */
export function escapeToBytes(input: string): Uint8Array {
  const out: number[] = [];
  let i = 0;
  while (i < input.length) {
    const c = input[i];
    if (c === "\\" && i + 1 < input.length) {
      const n = input[i + 1];
      switch (n) {
        case "n":
          out.push(0x0a);
          i += 2;
          break;
        case "r":
          out.push(0x0d);
          i += 2;
          break;
        case "t":
          out.push(0x09);
          i += 2;
          break;
        case "0":
          out.push(0x00);
          i += 2;
          break;
        case "\\":
          out.push(0x5c);
          i += 2;
          break;
        case "x": {
          const hex = input.slice(i + 2, i + 4);
          if (/^[0-9a-fA-F]{2}$/.test(hex)) {
            out.push(parseInt(hex, 16));
            i += 4;
          } else {
            out.push(0x5c);
            i += 1;
          }
          break;
        }
        default:
          out.push(0x5c);
          i += 1;
      }
    } else {
      out.push(c.charCodeAt(0) & 0xff);
      i += 1;
    }
  }
  return new Uint8Array(out);
}

export interface ColorSpan {
  text: string;
  fg?: string;
  bg?: string;
  bold?: boolean;
}

/**
 * 解析 ANSI 转义序列（\x1b[..m 颜色码）为分段
 * 支持 30-37 前景、40-47 背景、1 加粗、0 重置
 */
const ANSI_COLORS: Record<number, string> = {
  30: "#1d1d1f",
  31: "#ff3b30",
  32: "#34c759",
  33: "#ff9500",
  34: "#0a84ff",
  35: "#af52de",
  36: "#5ac8fa",
  37: "#f2f2f7",
};

export function parseAnsi(text: string): ColorSpan[] {
  if (!text.includes("\x1b")) return [{ text }];
  const spans: ColorSpan[] = [];
  const re = /\x1b\[([0-9;]*)m/g;
  let last = 0;
  let fg: string | undefined;
  let bg: string | undefined;
  let bold = false;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) {
      spans.push({ text: text.slice(last, m.index), fg, bg, bold });
    }
    const codes = m[1] ? m[1].split(";").map(Number) : [0];
    for (const code of codes) {
      if (code === 0) {
        fg = undefined;
        bg = undefined;
        bold = false;
      } else if (code === 1) {
        bold = true;
      } else if (code >= 30 && code <= 37) {
        fg = ANSI_COLORS[code];
      } else if (code >= 40 && code <= 47) {
        bg = ANSI_COLORS[code - 10];
      }
    }
    last = m.index + m[0].length;
  }
  if (last < text.length) {
    spans.push({ text: text.slice(last), fg, bg, bold });
  }
  return spans;
}

/** 时间戳格式化: HH:MM:SS.mmm */
export function formatTime(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(
    d.getMilliseconds(),
    3
  )}`;
}

/** 人类可读字节数 */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
