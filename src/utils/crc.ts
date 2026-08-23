// CRC/校验算法库（与 COMTool crc.py 对应，查表法）
// 支持: CRC16-IBM / CRC16-MODBUS / CRC16-CCITT / CRC32 / SUM / XOR

// 生成 CRC16 查表（LSB-first，用于 refin=true 的 IBM/MODBUS）
function makeCrc16Table(poly: number): Uint16Array {
  const table = new Uint16Array(256);
  for (let i = 0; i < 256; i++) {
    let crc = i;
    for (let j = 0; j < 8; j++) {
      crc = crc & 1 ? (crc >> 1) ^ poly : crc >> 1;
    }
    table[i] = crc & 0xffff;
  }
  return table;
}

const TABLE_IBM = makeCrc16Table(0xa001); // reflected 0x8005

/** CRC16 查表核心（LSB-first / refin=true） */
function crc16Reflected(
  data: Uint8Array,
  table: Uint16Array,
  init: number
): number {
  let crc = init & 0xffff;
  for (let i = 0; i < data.length; i++) {
    crc = ((crc >> 8) ^ table[(crc ^ data[i]) & 0xff]) & 0xffff;
  }
  return crc;
}

/** CRC16 逐位核心（MSB-first / refin=false，用于 CCITT-FALSE） */
function crc16Msb(data: Uint8Array, poly: number, init: number): number {
  let crc = init & 0xffff;
  for (let i = 0; i < data.length; i++) {
    crc ^= data[i] << 8;
    for (let j = 0; j < 8; j++) {
      crc = crc & 0x8000 ? ((crc << 1) ^ poly) & 0xffff : (crc << 1) & 0xffff;
    }
  }
  return crc & 0xffff;
}

// CRC-16/IBM (ARC): poly=0x8005, refin=true, init=0x0000, xorout=0
export const crc16IBM = (d: Uint8Array) => crc16Reflected(d, TABLE_IBM, 0x0000);
// CRC-16/MODBUS: poly=0x8005, refin=true, init=0xffff, xorout=0
export const crc16Modbus = (d: Uint8Array) => crc16Reflected(d, TABLE_IBM, 0xffff);
// CRC-16/CCITT-FALSE: poly=0x1021, refin=false, init=0xffff, xorout=0
export const crc16CCITT = (d: Uint8Array) => crc16Msb(d, 0x1021, 0xffff);

/** CRC32 (IEEE 802.3, 标准 zlib 算法) */
const CRC32_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    t[n] = c >>> 0;
  }
  return t;
})();

export function crc32(data: Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    crc = CRC32_TABLE[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** 校验和：所有字节求和 % 256 */
export const sum8 = (d: Uint8Array) => {
  let s = 0;
  for (let i = 0; i < d.length; i++) s = (s + d[i]) & 0xff;
  return s;
};

/** 异或校验 */
export const xor8 = (d: Uint8Array) => {
  let x = 0;
  for (let i = 0; i < d.length; i++) x ^= d[i];
  return x;
};

export type ChecksumAlgo =
  | "none"
  | "crc16_ibm"
  | "crc16_modbus"
  | "crc16_ccitt"
  | "crc32"
  | "sum8"
  | "xor8";

export const CHECKSUM_ALGOS: { id: ChecksumAlgo; name: string; bytes: number }[] = [
  { id: "none", name: "无校验", bytes: 0 },
  { id: "crc16_ibm", name: "CRC16-IBM", bytes: 2 },
  { id: "crc16_modbus", name: "CRC16-MODBUS", bytes: 2 },
  { id: "crc16_ccitt", name: "CRC16-CCITT", bytes: 2 },
  { id: "crc32", name: "CRC32", bytes: 4 },
  { id: "sum8", name: "SUM (和校验)", bytes: 1 },
  { id: "xor8", name: "XOR (异或校验)", bytes: 1 },
];

export function computeChecksum(algo: ChecksumAlgo, data: Uint8Array): number {
  switch (algo) {
    case "crc16_ibm":
      return crc16IBM(data);
    case "crc16_modbus":
      return crc16Modbus(data);
    case "crc16_ccitt":
      return crc16CCITT(data);
    case "crc32":
      return crc32(data);
    case "sum8":
      return sum8(data);
    case "xor8":
      return xor8(data);
    default:
      return 0;
  }
}

/** 校验值写入字节数组，默认小端。 */
export function checksumToBytes(
  algo: ChecksumAlgo,
  value: number,
  endian: "little" | "big" = "little"
): Uint8Array {
  const info = CHECKSUM_ALGOS.find((a) => a.id === algo)!;
  const out = new Uint8Array(info.bytes);
  for (let i = 0; i < info.bytes; i++) {
    const significance = endian === "little" ? i : out.length - 1 - i;
    out[i] = (value >>> (8 * significance)) & 0xff;
  }
  return out;
}
