// 对实际 src/utils/crc.ts 实现运行标准向量与字节序测试。
import { build } from "esbuild";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

const bundlePath = join(tmpdir(), "serialporttool-crc-bundle.cjs");
await build({
  entryPoints: ["src/utils/crc.ts"],
  bundle: true,
  outfile: bundlePath,
  format: "cjs",
  platform: "node",
});
const require = createRequire(import.meta.url);
const crc = require(bundlePath);
const input = new TextEncoder().encode("123456789");
const checks = [
  ["crc16_ibm", crc.crc16IBM(input), 0xbb3d],
  ["crc16_modbus", crc.crc16Modbus(input), 0x4b37],
  ["crc16_ccitt", crc.crc16CCITT(input), 0x29b1],
  ["crc32", crc.crc32(input), 0xcbf43926],
  ["sum8", crc.sum8(new Uint8Array([1, 2, 3, 0xfc])), 2],
  ["xor8", crc.xor8(new Uint8Array([1, 2, 3, 0xfc])), 0xfc],
];

let failed = 0;
for (const [name, actual, expected] of checks) {
  const passed = actual === expected;
  console.log(`${passed ? "PASS" : "FAIL"} ${name}`);
  if (!passed) failed += 1;
}
const little = [...crc.checksumToBytes("crc16_modbus", 0x1234, "little")];
const big = [...crc.checksumToBytes("crc16_modbus", 0x1234, "big")];
const endianPassed = little.join(",") === "52,18" && big.join(",") === "18,52";
console.log(`${endianPassed ? "PASS" : "FAIL"} checksum endian`);
if (!endianPassed) failed += 1;

console.log(`\n${checks.length + 1 - failed} passed, ${failed} failed`);
process.exit(failed ? 1 : 0);
