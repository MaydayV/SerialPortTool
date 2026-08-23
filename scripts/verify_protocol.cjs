// 协议引擎验证：组帧→解帧环回，覆盖校验范围/位置、长度偏移、粘包半包
const esbuild = require("esbuild");

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
      console.log(`FAIL ${name}${detail ? ` — ${detail}` : ""}`);
    }
  };
  const bytes = (...values) => new Uint8Array(values);
  const hex = (value) => Buffer.from(value).toString("hex");
  const equal = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
  const concat = (a, b) => {
    const out = new Uint8Array(a.length + b.length);
    out.set(a);
    out.set(b, a.length);
    return out;
  };
  const base = (overrides = {}) => ({
    name: "test",
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
    description: "",
    ...overrides,
  });

  // 1. SUM：整帧范围，默认模板环回；同时覆盖粘包。
  const sumTpl = DEFAULT_TEMPLATES.find((t) => t.name === "SUM 校验帧");
  const payload = bytes(0x01, 0x02, 0x03, 0x10, 0x20);
  const frame = packFrame(payload, sumTpl);
  check("SUM frame length", frame.length === 9, `got ${frame.length}`);
  check("SUM frame header", hex(frame).startsWith("aa55"), hex(frame));
  check("SUM frame length field", frame[2] === 9, `got ${frame[2]}`);
  const expectCs = (0xaa + 0x55 + 9 + 1 + 2 + 3 + 0x10 + 0x20) & 0xff;
  check("SUM frame checksum", frame[8] === expectCs, `got ${frame[8]} expect ${expectCs}`);

  const frame2 = packFrame(bytes(0xff, 0xee), sumTpl);
  const stream = concat(concat(bytes(0x00, 0x11), frame), frame2);
  const extracted = extractFrames(stream, sumTpl);
  check("extract 2 frames from sticky stream", extracted.frames.length === 2, `got ${extracted.frames.length}`);
  check("sticky frame1 content", equal(extracted.frames[0] || bytes(), frame));
  check("sticky frame2 content", equal(extracted.frames[1] || bytes(), frame2));
  check("sticky rest empty", extracted.rest.length === 0, `got ${extracted.rest.length}`);

  // 2. 半包：第一段必须留在 rest，第二段接上后才能出帧。
  const firstPart = extractFrames(frame.subarray(0, 4), sumTpl);
  check("half frame produces no frame", firstPart.frames.length === 0);
  check("half frame is retained", equal(firstPart.rest, frame.subarray(0, 4)));
  const secondPart = extractFrames(concat(firstPart.rest, frame.subarray(4)), sumTpl);
  check("half frame completes after second chunk", secondPart.frames.length === 1);
  check("half frame content", equal(secondPart.frames[0] || bytes(), frame));

  // 3. SUM payload 范围必须只计算 payload，不得把 header 带进去。
  const sumPayloadTpl = base({
    name: "SUM payload",
    header: "AA 55",
    tail: "0D 0A",
    checksum: "sum8",
    checksumRange: "payload",
    checksumPosition: "before_tail",
  });
  const sumPayloadFrame = packFrame(bytes(1, 2, 3), sumPayloadTpl);
  check("SUM payload checksum value", sumPayloadFrame[5] === 6, hex(sumPayloadFrame));
  check("SUM payload before tail layout", hex(sumPayloadFrame) === "aa55010203060d0a", hex(sumPayloadFrame));
  const sumPayloadDecoded = extractFrames(sumPayloadFrame, sumPayloadTpl);
  check("SUM payload range roundtrip", sumPayloadDecoded.frames.length === 1, hex(sumPayloadDecoded.rest));

  // 4. CRC payload 范围 + tail 位置：校验应在 tail 后，RX/TX 规则一致。
  const crcPayloadTailTpl = base({
    name: "CRC payload tail",
    header: "AA 55",
    tail: "0D 0A",
    checksum: "crc16_modbus",
    checksumRange: "payload",
    checksumPosition: "tail",
  });
  const crcPayloadTailFrame = packFrame(bytes(1, 3, 5), crcPayloadTailTpl);
  check("CRC payload tail layout", hex(crcPayloadTailFrame).startsWith("aa550103050d0a"), hex(crcPayloadTailFrame));
  const crcPayloadTailDecoded = extractFrames(crcPayloadTailFrame, crcPayloadTailTpl);
  check("CRC payload tail roundtrip", crcPayloadTailDecoded.frames.length === 1, hex(crcPayloadTailDecoded.rest));

  // 5. CRC all 范围 + before_tail：校验应在 tail 前，且 RX/TX 帧布局一致。
  const crcAllBeforeTailTpl = base({
    name: "CRC all before tail",
    header: "AA 55",
    tail: "0D 0A",
    checksum: "crc16_modbus",
    checksumRange: "all",
    checksumPosition: "before_tail",
  });
  const crcAllBeforeTailFrame = packFrame(bytes(1, 3, 5), crcAllBeforeTailTpl);
  check("CRC all before tail layout", hex(crcAllBeforeTailFrame).endsWith("0d0a"), hex(crcAllBeforeTailFrame));
  const crcAllBeforeTailDecoded = extractFrames(crcAllBeforeTailFrame, crcAllBeforeTailTpl);
  check("CRC all before tail roundtrip", crcAllBeforeTailDecoded.frames.length === 1, hex(crcAllBeforeTailDecoded.rest));

  // 6. 非零 length.offset：字段按绝对偏移写入，间隔字节安全填零，环回可解。
  const offsetTpl = base({
    name: "offset length",
    header: "AA",
    tail: "0D 0A",
    length: { enabled: true, offset: 3, bytes: 2, endian: "big", includeSelf: false },
    checksum: "sum8",
    checksumRange: "all",
    checksumPosition: "before_tail",
  });
  const offsetFrame = packFrame(bytes(0x10, 0x20), offsetTpl);
  check("non-zero length offset placement", hex(offsetFrame).startsWith("aa000000051020"), hex(offsetFrame));
  check("non-zero length offset roundtrip", extractFrames(offsetFrame, offsetTpl).frames.length === 1, hex(offsetFrame));

  // 7. 非法长度/hex 不得制造 NaN/越界帧，也不得让解帧死循环。
  const invalidHexTpl = base({ name: "invalid hex", header: "GG", checksum: "sum8" });
  let invalidRejected = false;
  try {
    packFrame(bytes(1, 2), invalidHexTpl);
  } catch {
    invalidRejected = true;
  }
  check("invalid hex blocks sending", invalidRejected);
  const invalidLengthTpl = base({
    name: "invalid length",
    header: "AA",
    length: { enabled: true, offset: 1, bytes: 1, endian: "little", includeSelf: true },
    checksum: "sum8",
  });
  const invalidLength = extractFrames(bytes(0xaa, 0x00, 0x01), invalidLengthTpl);
  check("invalid length is rejected safely", invalidLength.frames.length === 0 && invalidLength.errors > 0);

  const tooLarge = new Uint8Array(300);
  let tooLargeRejected = false;
  try {
    packFrame(tooLarge, sumTpl);
  } catch {
    tooLargeRejected = true;
  }
  check("length overflow blocks sending", tooLargeRejected);

  let totalLimitRejected = false;
  try {
    packFrame(new Uint8Array(4 * 1024 * 1024), base({ tail: "0D" }));
  } catch {
    totalLimitRejected = true;
  }
  check("total frame size limit blocks sending", totalLimitRejected);

  const unbounded = DEFAULT_TEMPLATES.find((t) => t.name === "CRC16-MODBUS 帧");
  const crcFrame = packFrame(bytes(1, 2, 3, 4), unbounded);
  const partialCrc = extractFrames(crcFrame.subarray(0, 3), unbounded);
  check(
    "unbounded checksum never discards a possible half frame",
    partialCrc.frames.length === 0 && equal(partialCrc.rest, crcFrame.subarray(0, 3))
  );

  const bigEndianTpl = base({
    name: "CRC big endian",
    tail: "0D 0A",
    checksum: "crc16_modbus",
    checksumEndian: "big",
    checksumPosition: "before_tail",
  });
  const bigEndianFrame = packFrame(bytes(1, 2, 3), bigEndianTpl);
  check("big-endian checksum roundtrip", extractFrames(bigEndianFrame, bigEndianTpl).frames.length === 1);

  // 8. 透传仍保持原行为。
  const pasTpl = DEFAULT_TEMPLATES.find((t) => t.name === "透传");
  const raw = bytes(1, 2, 3, 4, 5);
  const passthrough = extractFrames(raw, pasTpl);
  check("passthrough keeps all", passthrough.frames.length === 1 && equal(passthrough.frames[0], raw));
  check("passthrough rest empty", passthrough.rest.length === 0);

  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
