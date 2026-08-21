// 协议引擎验证：组帧→解帧环回 + 已知帧解析
const esbuild = require("esbuild");
const path = require("path");

async function main() {
  await esbuild.build({
    entryPoints: ["src/utils/protocol.ts"],
    bundle: true,
    outfile: "/tmp/proto_bundle.cjs",
    format: "cjs",
    platform: "node",
  });
  const { extractFrames, packFrame, DEFAULT_TEMPLATES } = require("/tmp/proto_bundle.cjs");

  let pass = 0;
  let fail = 0;
  const check = (name, cond, detail = "") => {
    if (cond) {
      pass++;
      console.log(`PASS ${name}`);
    } else {
      fail++;
      console.log(`FAIL ${name} ${detail}`);
    }
  };

  // 1. SUM 校验帧：组帧 → 解帧 环回
  const sumTpl = DEFAULT_TEMPLATES.find((t) => t.name === "SUM 校验帧");
  const payload = new Uint8Array([0x01, 0x02, 0x03, 0x10, 0x20]);
  const frame = packFrame(payload, sumTpl);
  console.log(`frame: ${Buffer.from(frame).toString("hex")}`);
  // AA 55 + len(1B) + payload(5B) + sum(1B) = 8 字节
  check("SUM frame length", frame.length === 9, `got ${frame.length}`);
  check(
    "SUM frame header",
    frame[0] === 0xaa && frame[1] === 0x55,
    Buffer.from(frame).toString("hex")
  );
  // 长度字段 = 整帧长 9 (includeSelf)
  check("SUM frame length field", frame[2] === 9, `got ${frame[2]}`);
  // 校验 = sum(header+len+payload) & 0xff = (0xaa+0x55+9+1+2+3+0x10+0x20)&0xff
  const expectCs = (0xaa + 0x55 + 9 + 1 + 2 + 3 + 0x10 + 0x20) & 0xff;
  check("SUM frame checksum", frame[8] === expectCs, `got ${frame[8]} expect ${expectCs}`);

  // 解帧（含粘包：两帧 + 杂散字节）
  const frame2 = packFrame(new Uint8Array([0xff, 0xee]), sumTpl);
  const junk = new Uint8Array([0x00, 0x11]); // 杂散
  const stream = concat(junk, frame);
  const stream2 = concat(stream, frame2);
  const { frames, rest } = extractFrames(stream2, sumTpl);
  check("extract 2 frames from stream", frames.length === 2, `got ${frames.length}`);
  check(
    "frame1 content match",
    frames.length > 0 && bytesEqual(frames[0], frame),
    frames[0] ? Buffer.from(frames[0]).toString("hex") : "empty"
  );
  check(
    "frame2 content match",
    frames.length > 1 && bytesEqual(frames[1], frame2),
    ""
  );
  check("rest empty", rest.length === 0, `got ${rest.length}`);

  // 2. CRC16-MODBUS 帧
  const crcTpl = DEFAULT_TEMPLATES.find((t) => t.name === "CRC16-MODBUS 帧");
  const p2 = new Uint8Array([0x01, 0x03, 0x00, 0x00]);
  const f2 = packFrame(p2, crcTpl);
  check("CRC frame length", f2.length === 6, `got ${f2.length}`);
  // 已知值：01 03 00 00 + CRC16-MODBUS
  // 手动验证环回
  const { frames: f2frames } = extractFrames(f2, crcTpl);
  check("CRC frame extract", f2frames.length === 1 && bytesEqual(f2frames[0], f2), "");

  // 3. 透传
  const pasTpl = DEFAULT_TEMPLATES.find((t) => t.name === "透传");
  const raw = new Uint8Array([1, 2, 3, 4, 5]);
  const { frames: pasFrames, rest: pasRest } = extractFrames(raw, pasTpl);
  check("passthrough keeps all", pasFrames.length === 1 && bytesEqual(pasFrames[0], raw), "");
  check("passthrough rest empty", pasRest.length === 0, "");

  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail > 0 ? 1 : 0);
}

function concat(a, b) {
  const out = new Uint8Array(a.length + b.length);
  out.set(a);
  out.set(b, a.length);
  return out;
}
function bytesEqual(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
