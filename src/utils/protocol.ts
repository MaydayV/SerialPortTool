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
  checksumEndian: "little" | "big";
  description: string;
}

export const MAX_FRAME_LENGTH = 4 * 1024 * 1024;
const VALID_LENGTH_BYTES = [1, 2, 4];
const MAX_MARKER_BYTES = 1024;
const MAX_TEMPLATE_NAME = 100;
const MAX_TEMPLATE_DESCRIPTION = 1000;
const DEFAULT_LENGTH: LengthField = {
  enabled: false,
  offset: 0,
  bytes: 1,
  endian: "little",
  includeSelf: false,
};

export const DEFAULT_TEMPLATES: FrameTemplate[] = [
  {
    name: "透传",
    header: "",
    tail: "",
    length: { ...DEFAULT_LENGTH },
    checksum: "none",
    checksumRange: "all",
    checksumPosition: "tail",
    checksumEndian: "little",
    description: "不解析，原始字节流",
  },
  {
    name: "CRC16-MODBUS 帧",
    header: "",
    tail: "",
    length: { ...DEFAULT_LENGTH },
    checksum: "crc16_modbus",
    checksumRange: "all",
    checksumPosition: "tail",
    checksumEndian: "little",
    description: "帧尾追加 CRC16-MODBUS 小端；无边界，仅支持发送组帧",
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
    checksumEndian: "little",
    description: "AA 55 + 长度 + 数据 + SUM 校验",
  },
];

type UnknownRecord = Record<string, unknown>;

function isRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 将常见 hex 输入规范化；非法字符/奇数位直接拒绝。 */
function normalizeHex(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const cleaned = value
    .replace(/0x/gi, "")
    .replace(/[\s,;:_-]+/g, "")
    .toUpperCase();
  if (cleaned.length % 2 !== 0 || !/^[0-9A-F]*$/.test(cleaned)) return null;
  return cleaned.match(/../g)?.join(" ") ?? "";
}

function hexToBytes(hex: string): Uint8Array | null {
  const normalized = normalizeHex(hex);
  if (normalized === null) return null;
  const cleaned = normalized.replace(/\s+/g, "");
  const out = new Uint8Array(cleaned.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function isChecksumAlgo(value: unknown): value is ChecksumAlgo {
  return CHECKSUM_ALGOS.some((a) => a.id === value);
}

/**
 * 导入/运行前统一模板结构。返回 null 表示模板不可信，调用方不得写入模板库。
 * 缺少可选字段时使用安全默认值；显式提供的错误类型不会被静默接受。
 */
export function normalizeFrameTemplate(input: unknown): FrameTemplate | null {
  if (!isRecord(input)) return null;
  if (
    typeof input.name !== "string" ||
    !input.name.trim() ||
    input.name.trim().length > MAX_TEMPLATE_NAME
  ) return null;

  const header = normalizeHex(input.header === undefined ? "" : input.header);
  const tail = normalizeHex(input.tail === undefined ? "" : input.tail);
  if (header === null || tail === null) return null;
  if (header.replace(/\s/g, "").length / 2 > MAX_MARKER_BYTES) return null;
  if (tail.replace(/\s/g, "").length / 2 > MAX_MARKER_BYTES) return null;

  const rawLength = input.length === undefined ? {} : input.length;
  if (!isRecord(rawLength)) return null;
  const length: LengthField = {
    enabled: rawLength.enabled === undefined ? DEFAULT_LENGTH.enabled : rawLength.enabled as boolean,
    offset: rawLength.offset === undefined ? DEFAULT_LENGTH.offset : rawLength.offset as number,
    bytes: rawLength.bytes === undefined ? DEFAULT_LENGTH.bytes : rawLength.bytes as number,
    endian: rawLength.endian === undefined ? DEFAULT_LENGTH.endian : rawLength.endian as "little" | "big",
    includeSelf: rawLength.includeSelf === undefined ? DEFAULT_LENGTH.includeSelf : rawLength.includeSelf as boolean,
  };
  if (
    typeof length.enabled !== "boolean" ||
    !Number.isSafeInteger(length.offset) ||
    length.offset < 0 ||
    !VALID_LENGTH_BYTES.includes(length.bytes) ||
    (length.endian !== "little" && length.endian !== "big") ||
    typeof length.includeSelf !== "boolean"
  ) {
    return null;
  }

  const headerBytes = hexToBytes(header);
  if (headerBytes === null) return null;
  if (length.enabled && (length.offset < headerBytes.length || length.offset > MAX_FRAME_LENGTH)) {
    return null;
  }

  const checksum = input.checksum === undefined ? "none" : input.checksum;
  const checksumRange = input.checksumRange === undefined ? "all" : input.checksumRange;
  const checksumPosition = input.checksumPosition === undefined ? "tail" : input.checksumPosition;
  const checksumEndian = input.checksumEndian === undefined ? "little" : input.checksumEndian;
  const description = input.description === undefined ? "" : input.description;
  if (
    !isChecksumAlgo(checksum) ||
    (checksumRange !== "all" && checksumRange !== "payload") ||
    (checksumPosition !== "tail" && checksumPosition !== "before_tail") ||
    (checksumEndian !== "little" && checksumEndian !== "big") ||
    typeof description !== "string" ||
    description.length > MAX_TEMPLATE_DESCRIPTION
  ) {
    return null;
  }

  return {
    name: input.name.trim(),
    header,
    tail,
    length,
    checksum,
    checksumRange,
    checksumPosition,
    checksumEndian,
    description,
  };
}

/** 返回模板是否包含可安全执行的完整结构。 */
export function validateFrameTemplate(input: unknown): boolean {
  return normalizeFrameTemplate(input) !== null;
}

/** 流式接收必须有长度域或帧尾，否则无法区分半帧与完整帧。 */
export function canInferFrameBoundary(input: unknown): boolean {
  const template = normalizeFrameTemplate(input);
  return !!template && (template.length.enabled || template.tail.length > 0);
}

function readLength(frame: Uint8Array, lf: LengthField): number {
  let value = 0;
  for (let i = 0; i < lf.bytes; i++) {
    const significance = lf.endian === "little" ? i : lf.bytes - 1 - i;
    value += frame[lf.offset + i] * 2 ** (8 * significance);
  }
  return value;
}

function checksumBytes(t: FrameTemplate): number {
  return CHECKSUM_ALGOS.find((a) => a.id === t.checksum)?.bytes ?? 0;
}

/** 校验帧中尾部是否位于预期位置。 */
function hasExpectedTail(frame: Uint8Array, tail: Uint8Array, csLen: number, t: FrameTemplate): boolean {
  if (tail.length === 0) return true;
  const tailStart = t.checksumPosition === "tail"
    ? frame.length - csLen - tail.length
    : frame.length - tail.length;
  return tailStart >= 0 && bytesEqual(frame.slice(tailStart, tailStart + tail.length), tail);
}

function checksumStart(frame: Uint8Array, tailLen: number, csLen: number, t: FrameTemplate): number {
  return t.checksumPosition === "tail" ? frame.length - csLen : frame.length - tailLen - csLen;
}

/** RX/TX 共用的 payload 起止位置，避免两边对范围的解释分叉。 */
function payloadBounds(
  t: FrameTemplate,
  tailLen: number,
  csStart: number
): { start: number; end: number } {
  const headerLen = hexToBytes(t.header)?.length ?? 0;
  const start = t.length.enabled ? t.length.offset + t.length.bytes : headerLen;
  const end = t.checksumPosition === "tail" ? csStart - tailLen : csStart;
  return { start, end };
}

function checksumInputForFrame(
  t: FrameTemplate,
  frame: Uint8Array,
  tailLen: number,
  csStart: number
): Uint8Array | null {
  if (csStart < 0 || csStart > frame.length) return null;
  if (t.checksumRange === "all") return frame.slice(0, csStart);
  const bounds = payloadBounds(t, tailLen, csStart);
  if (bounds.start < 0 || bounds.end < bounds.start || bounds.end > frame.length) return null;
  return frame.slice(bounds.start, bounds.end);
}

/**
 * 解帧：从输入缓冲中提取完整帧。
 * 不足一帧时保留 rest；长度/校验/尾标记非法时丢弃一个字节继续找，保证不会死循环。
 */
export function extractFrames(
  buffer: Uint8Array,
  template: FrameTemplate
): { frames: Uint8Array[]; rest: Uint8Array; errors: number; trash: number } {
  const t = normalizeFrameTemplate(template);
  if (!t) return { frames: [], rest: buffer.slice(), errors: buffer.length > 0 ? 1 : 0, trash: 0 };

  const frames: Uint8Array[] = [];
  let buf = buffer.slice();
  let errors = 0;
  let trash = 0;
  const header = hexToBytes(t.header)!;
  const tail = hexToBytes(t.tail)!;
  const csLen = checksumBytes(t);

  while (buf.length > 0) {
    if (header.length > 0) {
      const start = findSubarray(buf, header);
      if (start === -1) break;
      if (start > 0) {
        trash += start;
        buf = buf.slice(start);
      }
    }

    let frameLen = buf.length;
    if (t.length.enabled) {
      const lf = t.length;
      const lengthEnd = lf.offset + lf.bytes;
      if (buf.length < lengthEnd) break;
      const lengthValue = readLength(buf, lf);
      frameLen = lf.includeSelf ? lengthValue : lengthEnd + lengthValue;
      const minimum = lf.includeSelf ? lengthEnd + csLen + tail.length : lengthEnd + csLen + tail.length;
      if (
        !Number.isSafeInteger(frameLen) ||
        frameLen < minimum ||
        frameLen > MAX_FRAME_LENGTH
      ) {
        errors++;
        buf = resyncAfterFailure(buf, header);
        continue;
      }
    } else if (tail.length > 0) {
      const end = findSubarray(buf.slice(header.length), tail);
      if (end === -1) break;
      frameLen =
        header.length + end + tail.length + (t.checksumPosition === "tail" ? csLen : 0);
    } else if (csLen > 0) {
      // 无长度/帧尾时不能安全猜测边界：保留数据，不把半帧误判并丢弃。
      break;
    }

    if (frameLen <= 0 || frameLen > MAX_FRAME_LENGTH) {
      errors++;
      buf = resyncAfterFailure(buf, header);
      continue;
    }
    if (buf.length < frameLen) break;

    const frame = buf.slice(0, frameLen);
    if (!hasExpectedTail(frame, tail, csLen, t)) {
      errors++;
      buf = resyncAfterFailure(buf, header);
      continue;
    }

    if (csLen > 0) {
      const csStart = checksumStart(frame, tail.length, csLen, t);
      const range = checksumInputForFrame(t, frame, tail.length, csStart);
      if (!range) {
        errors++;
        buf = resyncAfterFailure(buf, header);
        continue;
      }
      const expected = checksumToBytes(
        t.checksum,
        computeChecksum(t.checksum, range),
        t.checksumEndian
      );
      const actual = frame.slice(csStart, csStart + csLen);
      if (!bytesEqual(actual, expected)) {
        errors++;
        // 没有可推断边界时整包失败即结束；有帧头时跳到下一个候选帧头。
        buf = resyncAfterFailure(buf, header);
        continue;
      }
    }

    frames.push(frame);
    buf = buf.slice(frameLen);
  }

  return { frames, rest: buf, errors, trash };
}

function resyncAfterFailure(buffer: Uint8Array, header: Uint8Array): Uint8Array {
  if (buffer.length === 0 || header.length === 0) return new Uint8Array(0);
  const next = findSubarray(buffer.slice(1), header);
  if (next >= 0) return buffer.slice(next + 1);
  const keep = Math.min(header.length - 1, buffer.length);
  return keep > 0 ? buffer.slice(-keep) : new Uint8Array(0);
}

/** 组帧：把负载数据按模板封装成完整帧；无法组帧时明确报错。 */
export function packFrame(payload: Uint8Array, template: FrameTemplate): Uint8Array {
  const t = normalizeFrameTemplate(template);
  if (!t) throw new Error("协议模板无效，已阻止发送");

  const header = hexToBytes(t.header)!;
  const tail = hexToBytes(t.tail)!;
  const lf = t.length;
  const csLen = checksumBytes(t);
  const prefixLength = lf.enabled ? lf.offset : header.length;
  const payloadStart = lf.enabled ? lf.offset + lf.bytes : header.length;
  const totalLength =
    prefixLength +
    (lf.enabled ? lf.bytes : 0) +
    payload.length +
    csLen +
    tail.length;

  if (prefixLength < header.length || payloadStart > MAX_FRAME_LENGTH) {
    throw new Error("协议字段偏移超出允许范围，已阻止发送");
  }
  if (!Number.isSafeInteger(totalLength) || totalLength > MAX_FRAME_LENGTH) {
    throw new Error("协议帧超过 4 MiB 上限，已阻止发送");
  }

  let frame = new Uint8Array(prefixLength);
  frame.set(header);
  if (lf.enabled) {
    const lengthValue = lf.includeSelf
      ? lf.offset + lf.bytes + payload.length + csLen + tail.length
      : payload.length + csLen + tail.length;
    const maxLengthValue = 2 ** (8 * lf.bytes) - 1;
    if (lengthValue < 0 || lengthValue > maxLengthValue || lengthValue > MAX_FRAME_LENGTH) {
      throw new Error(
        `负载过长：长度字段 ${lf.bytes}B 无法表示 ${lengthValue} 字节，已阻止发送`
      );
    }
    const lengthBytes = new Uint8Array(lf.bytes);
    for (let i = 0; i < lf.bytes; i++) {
      const significance = lf.endian === "little" ? i : lf.bytes - 1 - i;
      lengthBytes[i] = Math.floor(lengthValue / 2 ** (8 * significance)) & 0xff;
    }
    frame = concat(frame, lengthBytes);
  }
  frame = concat(frame, payload);

  if (csLen > 0) {
    if (t.checksumPosition === "before_tail") {
      const range = checksumInputForFrame(t, frame, 0, frame.length)!;
      frame = concat(
        frame,
        checksumToBytes(t.checksum, computeChecksum(t.checksum, range), t.checksumEndian)
      );
      frame = concat(frame, tail);
    } else {
      frame = concat(frame, tail);
      const range = checksumInputForFrame(t, frame, tail.length, frame.length)!;
      frame = concat(
        frame,
        checksumToBytes(t.checksum, computeChecksum(t.checksum, range), t.checksumEndian)
      );
    }
  } else {
    frame = concat(frame, tail);
  }
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
