// 曲线帧解析：ASCII 协议 ($name,x,y[,sum]\n) 和二进制协议 (header+name+x+y+sum)
// 与 COMTool plugins/graph_protocol.py 帧格式兼容

export interface CurvePoint {
  name: string;
  x: number;
  y: number;
}

export const DEFAULT_HEADER = new Uint8Array([0xaa, 0xcc, 0xee, 0xbb]);

/**
 * ASCII 帧解析：$name,x,y[,checksum]\n
 * 支持 2/3/4 段：$roll,2.0 / $pitch,1.0,2.0 / $temp,1.0,2.0,179
 * x 省略时由调用方用自动递增序号补
 */
export function parseAsciiFrame(
  line: string,
  autoX: number
): CurvePoint | null {
  const s = line.trim();
  if (!s.startsWith("$")) return null;
  const body = s.slice(1);
  const parts = body.split(",");
  if (parts.length < 2 || parts.length > 4) return null;
  const name = parts[0];
  if (!name) return null;
  let x: number;
  let y: number;
  if (parts.length === 2) {
    // $name,y → x 自动递增
    x = autoX;
    y = parseFloat(parts[1]);
  } else {
    x = parseFloat(parts[1]);
    y = parseFloat(parts[2]);
  }
  if (isNaN(x) || isNaN(y)) return null;
  // 校验和（可选第 4 段）：除校验段外所有字节（含 $ 前缀）和 % 256
  if (parts.length === 4) {
    const cs = parseInt(parts[3], 10);
    if (!isNaN(cs)) {
      const lastComma = s.lastIndexOf(",");
      let sum = 0;
      for (let i = 0; i < lastComma; i++) {
        sum = (sum + s.charCodeAt(i)) & 0xff;
      }
      if (sum !== cs) return null;
    }
  }
  return { name, x, y };
}

/**
 * 二进制帧解析：| header(4B) | 名长(1B) | 名(utf-8) | x(double LE) | y(double LE) | 校验和(1B) |
 * 从缓冲中提取所有完整帧
 */
export function parseBinaryFrames(
  buffer: Uint8Array,
  header: Uint8Array
): { points: CurvePoint[]; rest: Uint8Array } {
  const points: CurvePoint[] = [];
  let buf = buffer;
  const hdrLen = header.length;

  while (buf.length > hdrLen) {
    // 找帧头
    const start = findSubarray(buf, header);
    if (start === -1) break;
    if (start > 0) buf = buf.slice(start);
    if (buf.length < hdrLen + 1) break;
    const nameLen = buf[hdrLen];
    // 帧长 = header + 名长 + 名字 + 8 + 8 + 1
    const frameLen = hdrLen + 1 + nameLen + 17;
    if (buf.length < frameLen) break;
    const frame = buf.slice(0, frameLen);
    // 校验和 = 帧内前面所有字节和 % 256
    let sum = 0;
    for (let i = 0; i < frame.length - 1; i++) sum = (sum + frame[i]) & 0xff;
    if (sum === frame[frame.length - 1]) {
      const nameBytes = frame.slice(hdrLen + 1, hdrLen + 1 + nameLen);
      const name = new TextDecoder("utf-8").decode(nameBytes);
      const xOff = hdrLen + 1 + nameLen;
      const x = readF64LE(frame, xOff);
      const y = readF64LE(frame, xOff + 8);
      if (!isNaN(x) && !isNaN(y)) {
        points.push({ name, x, y });
      }
    }
    buf = buf.slice(frameLen);
  }
  return { points, rest: buf };
}

function readF64LE(buf: Uint8Array, off: number): number {
  const dv = new DataView(buf.buffer, buf.byteOffset + off, 8);
  return dv.getFloat64(0, true);
}

function findSubarray(haystack: Uint8Array, needle: Uint8Array): number {
  if (needle.length === 0) return 0;
  outer: for (let i = 0; i <= haystack.length - needle.length; i++) {
    for (let j = 0; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) continue outer;
    }
    return i;
  }
  return -1;
}
