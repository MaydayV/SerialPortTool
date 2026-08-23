// 曲线帧解析验证
const esbuild = require("esbuild");
const os = require("node:os");
const path = require("node:path");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serial-aid-curve-bundle.cjs");
  await esbuild.build({
    entryPoints: ["src/utils/curve.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
  });
  const { parseAsciiFrame, parseBinaryFrames, DEFAULT_HEADER } = require(bundlePath);

  let pass = 0;
  let fail = 0;
  const check = (name, cond, detail = "") => {
    if (cond) { pass++; console.log(`PASS ${name}`); }
    else { fail++; console.log(`FAIL ${name} ${detail}`); }
  };

  // ASCII 2 段（x 自动递增）
  const p1 = parseAsciiFrame("$roll,2.5", 100);
  check("ascii 2-seg", p1 !== null && p1.name === "roll" && p1.x === 100 && p1.y === 2.5, JSON.stringify(p1));
  // ASCII 3 段
  const p2 = parseAsciiFrame("$pitch,1.0,3.14", 0);
  check("ascii 3-seg", p2 !== null && p2.x === 1.0 && p2.y === 3.14, JSON.stringify(p2));
  // ASCII 4 段带校验
  const p3 = parseAsciiFrame("$temp,1.0,36.5,141", 0);
  // 校验: sum("$temp,1.0,36.5") & 0xff = 141
  check("ascii 4-seg cs", p3 !== null && p3.y === 36.5, JSON.stringify(p3));
  // 校验错误 → null
  const p4 = parseAsciiFrame("$temp,1.0,36.5,1", 0);
  check("ascii 4-seg bad cs", p4 === null, "");
  // 非法行
  check("ascii non-frame", parseAsciiFrame("hello", 0) === null, "");
  check("ascii empty name", parseAsciiFrame("$,1.0", 0) === null, "");
  check("ascii rejects partial numbers", parseAsciiFrame("$bad,1abc,2", 0) === null, "");
  check("ascii rejects infinity", parseAsciiFrame("$bad,Infinity,2", 0) === null, "");
  check("ascii rejects nonnumeric checksum", parseAsciiFrame("$bad,1,2,nope", 0) === null, "");

  // 二进制帧构造（与 graph_protocol.plot_pack 兼容）
  function packBinary(name, x, y) {
    const nameB = Buffer.from(name, "utf-8");
    const buf = Buffer.alloc(DEFAULT_HEADER.length + 1 + nameB.length + 17);
    buf.set(DEFAULT_HEADER, 0);
    buf[4] = nameB.length;
    buf.set(nameB, 5);
    buf.writeDoubleLE(x, 5 + nameB.length);
    buf.writeDoubleLE(y, 5 + nameB.length + 8);
    let s = 0;
    for (let i = 0; i < buf.length - 1; i++) s = (s + buf[i]) & 0xff;
    buf[buf.length - 1] = s;
    return new Uint8Array(buf);
  }

  const f1 = packBinary("data1", 1.5, -2.25);
  const f2 = packBinary("data2", 3.0, 4.0);
  // 单帧
  const r1 = parseBinaryFrames(f1, DEFAULT_HEADER);
  check("binary single frame", r1.points.length === 1, `got ${r1.points.length}`);
  check("binary values", r1.points.length === 1 && r1.points[0].name === "data1" && r1.points[0].x === 1.5 && r1.points[0].y === -2.25, JSON.stringify(r1.points));
  // 两帧粘包
  const two = new Uint8Array(f1.length + f2.length);
  two.set(f1); two.set(f2, f1.length);
  const r2 = parseBinaryFrames(two, DEFAULT_HEADER);
  check("binary two frames", r2.points.length === 2, `got ${r2.points.length}`);
  // 半帧（截断）
  const half = f1.slice(0, f1.length - 3);
  const r3 = parseBinaryFrames(half, DEFAULT_HEADER);
  check("binary half frame", r3.points.length === 0 && r3.rest.length === half.length, `points=${r3.points.length} rest=${r3.rest.length}`);
  const noise = new Uint8Array(1024 * 1024);
  noise.fill(0x42);
  const noisy = parseBinaryFrames(noise, DEFAULT_HEADER);
  check("binary noise buffer stays bounded", noisy.rest.length < DEFAULT_HEADER.length, `rest=${noisy.rest.length}`);
  const falseFrame = new Uint8Array(DEFAULT_HEADER.length + 1 + 40 + 17);
  falseFrame.set(DEFAULT_HEADER);
  falseFrame[DEFAULT_HEADER.length] = 40;
  falseFrame.set(f2, 10);
  let falseSum = 0;
  for (let i = 0; i < falseFrame.length - 1; i++) falseSum = (falseSum + falseFrame[i]) & 0xff;
  falseFrame[falseFrame.length - 1] = (falseSum + 1) & 0xff;
  const resynced = parseBinaryFrames(falseFrame, DEFAULT_HEADER);
  check(
    "bad binary checksum resynchronizes to an embedded real header",
    resynced.points.some((point) => point.name === "data2"),
    JSON.stringify(resynced.points)
  );
  const emptyHeader = parseBinaryFrames(f1, new Uint8Array(0));
  check("empty binary header is rejected safely", emptyHeader.points.length === 0 && emptyHeader.rest.length === 0);

  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((e) => { console.error(e); process.exit(1); });
