// 协议引擎：帧模板定义、解帧（RX）、组帧（TX）
import {
  CHECKSUM_ALGOS,
  computeChecksum,
  checksumToBytes,
  type ChecksumAlgo,
} from "./crc";

export interface LengthField {
  enabled: boolean;
  offset: number; // 从帧头算起（含 header）
  bytes: number; // 1/2/4
  endian: "little" | "big";
  includeSelf: boolean; // 长度是否包含长度字段本身
}

export interface FrameTemplate {
  name: string;
  header: string; // hex，如 "AA 55"
  tail: string; // hex，可空
  length: LengthField;
  checksum: ChecksumAlgo;
  checksumRange: "all" | "payload"; // 校验覆盖范围
  checksumPosition: "tail" | "before_tail"; // 校验位置
  description: string;
}

export const DEFAULT_TEMPLATES: FrameTemplate[] = [
  {
    name: "透传",
    header: "",
    tail: "",
    length: {
      enabled: false,
      offset: 0,
      bytes: 1,
      endian: "little",
      includeSelf: false,
    },
    checksum: "none",
    checksumRange: "all",
    checksumPosition: "tail",
    description: "不解析，原始字节流",
  },
  {
    name: "CRC16-MODBUS 帧",
    header: "",
    tail: "",
    length: {
      enabled: false,
      offset: 0,
      bytes: 1,
      endian: "little",
      includeSelf: false,
    },
    checksum: "crc16_modbus",
    checksumRange: "all",
    checksumPosition: "tail",
    description: "帧尾追加 CRC16-MODBUS 小端",
  },
  {
    name: "SUM 校验帧",
    header: "AA 55",
    tail: "",
    length: {
      enabled: true,
      offset: 2,
      bytes: 1,
      endian: "little",
      includeSelf: true,
    },
    checksum: "sum8",
    checksumRange: "all",
    checksumPosition: "tail",
    description: "AA 55 + 长度 + 数据 + SUM 校验",
  },
];

function hexToBytes(hex: string): Uint8Array {
  const cleaned = hex.replace(/0x/gi, "").replace(/[,\s]+/g, "");
  if (!cleaned || cleaned.length % 2 !== 0) return new Uint8Array();
  const out = new Uint8Array(cleaned.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
  return out;
}

/** 从帧数据中提取长度字段值 */
function readLength(frame: Uint8Array, lf: LengthField): number {
  let v = 0;
  for (let i = 0; i < lf.bytes; i++) {
    const idx = lf.offset + (lf.endian === "little" ? i : lf.bytes - 1 - i);
    v = (v << 8) | (frame[idx] ?? 0);
  }
  return v;
}

/** 计算校验范围 */
function checksumRange(t: FrameTemplate, frame: Uint8Array, tailLen: number): Uint8Array {
  const csLen = CHECKSUM_ALGOS.find((a) => a.id === t.checksum)?.bytes ?? 0;
  const pos = t.checksumPosition === "tail" ? 0 : tailLen;
  const end = frame.length - pos - csLen;
  return frame.slice(0, end);
}

/**
 * 解帧：从输入缓冲中提取完整帧
 * @param buffer 累积缓冲（可变）
 * @param t 模板
 * @returns 提取出的帧数组（从 buffer 头部移除已消费字节）
 */
export function extractFrames(
  buffer: Uint8Array,
  t: FrameTemplate
): {
  frames: Uint8Array[];
  rest: Uint8Array;
  /** 坏帧丢弃次数（校验失败） */
  errors: number;
  /** 杂散字节数（帧头前丢弃） */
  trash: number;
} {
  const frames: Uint8Array[] = [];
  let buf = buffer;
  let errors = 0;
  let trash = 0;
  const header = hexToBytes(t.header);
  const tail = hexToBytes(t.tail);
  const csLen = CHECKSUM_ALGOS.find((a) => a.id === t.checksum)?.bytes ?? 0;

  while (buf.length > 0) {
    // 定位帧头
    let start = 0;
    if (header.length > 0) {
      start = findSubarray(buf, header);
      if (start === -1) break; // 未找到帧头，丢弃积累
      if (start > 0) {
        trash += start;
        buf = buf.slice(start); // 丢弃帧头前的杂散字节
      }
    }

    // 计算帧长
    let frameLen = buf.length;
    if (t.length.enabled) {
      const lf = t.length;
      // 长度字段值：includeSelf=整帧长 / 否则=长度字段后内容长
      const lenVal = readLength(buf, lf);
      const dataLen = lf.includeSelf ? lenVal - lf.offset - lf.bytes : lenVal;
      // 帧总长 = 长度字段起点(offset) + 长度字段(bytes) + 其后内容
      frameLen = lf.offset + lf.bytes + dataLen;
    } else {
      // 无长度字段：靠帧尾或校验定位
      if (tail.length > 0) {
        const end = findSubarray(buf.slice(header.length), tail);
        if (end === -1) break; // 帧尾未到，等更多数据
        frameLen = header.length + end + tail.length;
      } else if (csLen > 0) {
        // 无头无尾有校验：无法分帧，整个缓冲作为一帧
        frameLen = buf.length;
      }
    }

    if (buf.length < frameLen) break; // 数据不足，等待

    const frame = buf.slice(0, frameLen);
    // 校验
    if (t.checksum !== "none") {
      const range = checksumRange(t, frame, tail.length);
      const calc = computeChecksum(t.checksum, range);
      const csBytes = checksumToBytes(t.checksum, calc);
      const csPos = t.checksumPosition === "tail" ? frame.length - csLen : frame.length - tail.length - csLen;
      const actual = frame.slice(csPos, csPos + csLen);
      if (!bytesEqual(actual, csBytes)) {
        // 校验失败：丢弃首字节继续找
        errors++;
        buf = buf.slice(1);
        continue;
      }
    }
    frames.push(frame);
    buf = buf.slice(frameLen);
  }
  return { frames, rest: buf, errors, trash };
}

/**
 * 组帧：把负载数据按模板封装成完整帧
 */
export function packFrame(payload: Uint8Array, t: FrameTemplate): Uint8Array {
  const header = hexToBytes(t.header);
  const tail = hexToBytes(t.tail);
  const lf = t.length;
  const csAlgo = t.checksum;
  const csLen = CHECKSUM_ALGOS.find((a) => a.id === csAlgo)?.bytes ?? 0;

  // 组装：header + length + payload + checksum + tail
  let frame = new Uint8Array(0);
  frame = concat(frame, header);
  if (lf.enabled) {
    // 长度字段值 = 整帧长(includeSelf) 或 长度字段后内容长
    const lenVal = lf.includeSelf
      ? lf.offset + lf.bytes + payload.length + csLen + tail.length
      : payload.length + csLen + tail.length;
    const lenBytes = new Uint8Array(lf.bytes);
    for (let i = 0; i < lf.bytes; i++) {
      const shift = 8 * (lf.endian === "little" ? i : lf.bytes - 1 - i);
      lenBytes[i] = (lenVal >>> shift) & 0xff;
    }
    frame = concat(frame, lenBytes);
  }
  frame = concat(frame, payload);

  if (csAlgo !== "none") {
    const range = t.checksumRange === "all" ? frame : payload;
    const calc = computeChecksum(csAlgo, range);
    frame = concat(frame, checksumToBytes(csAlgo, calc));
  }
  frame = concat(frame, tail);
  return frame;
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

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
}
