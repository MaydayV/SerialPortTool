// CRC 算法验证（内联实现，与 src/utils/crc.ts 同逻辑）
function makeCrc16Table(poly) {
  const table = new Uint16Array(256);
  for (let i = 0; i < 256; i++) {
    let crc = i;
    for (let j = 0; j < 8; j++) crc = crc & 1 ? (crc >> 1) ^ poly : crc >> 1;
    table[i] = crc & 0xffff;
  }
  return table;
}
const TABLE_IBM = makeCrc16Table(0xa001);
function crc16Reflected(data, table, init) {
  let crc = init & 0xffff;
  for (let i = 0; i < data.length; i++) {
    crc = ((crc >> 8) ^ table[(crc ^ data[i]) & 0xff]) & 0xffff;
  }
  return crc;
}
function crc16Msb(data, poly, init) {
  let crc = init & 0xffff;
  for (let i = 0; i < data.length; i++) {
    crc ^= data[i] << 8;
    for (let j = 0; j < 8; j++) crc = crc & 0x8000 ? ((crc << 1) ^ poly) & 0xffff : (crc << 1) & 0xffff;
  }
  return crc & 0xffff;
}
const crc16IBM = (d) => crc16Reflected(d, TABLE_IBM, 0x0000);
const crc16Modbus = (d) => crc16Reflected(d, TABLE_IBM, 0xffff);
const crc16CCITT = (d) => crc16Msb(d, 0x1021, 0xffff);
const CRC32_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();
function crc32(data) {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) crc = CRC32_TABLE[(crc ^ data[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}
function hex(s) {
  return new Uint8Array(s.match(/[0-9a-fA-F]{2}/g).map((b) => parseInt(b, 16)));
}
const tests = [
  ["31 32 33 34 35 36 37 38 39", "crc16_ibm", 0xbb3d],
  ["31 32 33 34 35 36 37 38 39", "crc16_modbus", 0x4b37],
  ["31 32 33 34 35 36 37 38 39", "crc16_ccitt", 0x29b1],
  ["31 32 33 34 35 36 37 38 39", "crc32", 0xcbf43926],
];
let pass = 0;
for (const [dataHex, algo, expected] of tests) {
  const data = hex(dataHex);
  const got =
    algo === "crc16_ibm" ? crc16IBM(data)
    : algo === "crc16_modbus" ? crc16Modbus(data)
    : algo === "crc16_ccitt" ? crc16CCITT(data)
    : crc32(data);
  const ok = got === expected;
  console.log(`${algo}: got=0x${got.toString(16)} expected=0x${expected.toString(16)} ${ok ? "PASS" : "FAIL"}`);
  if (ok) pass++;
}
const d = new Uint8Array([0x01, 0x02, 0x03, 0xfc]);
let s = 0; for (const b of d) s = (s + b) & 0xff;
let x = 0; for (const b of d) x ^= b;
console.log(`sum8: ${s} (expect 2)`);
console.log(`xor8: ${x} (expect ${0xfc ^ 3 ^ 2 ^ 1})`);
console.log(`${pass}/${tests.length} standard vectors passed`);
