// 字节工具回归：HEX、Unicode 转义、流式 ANSI。
const esbuild = require("esbuild");
const os = require("os");
const path = require("path");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serial-aid-bytes-bundle.cjs");
  await esbuild.build({
    entryPoints: ["src/utils/bytes.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
  });
  const utils = require(bundlePath);
  let pass = 0;
  let fail = 0;
  const check = (name, condition) => {
    console.log(`${condition ? "PASS" : "FAIL"} ${name}`);
    condition ? pass++ : fail++;
  };
  check("hex roundtrip", utils.bytesToHex(utils.hexToBytes("0xAA, 55")) === "AA 55");
  check(
    "escape mode keeps unicode as UTF-8",
    [...utils.escapeToBytes("中\\n")].join(",") === "228,184,173,10"
  );
  const state = utils.createAnsiParserState();
  const first = utils.parseAnsiChunk("\u001b[3", state);
  const second = utils.parseAnsiChunk("1mERR", state);
  check(
    "ANSI sequence may span chunks",
    first[0].text === "" && second.some((span) => span.text === "ERR" && span.fg === "#ff3b30")
  );
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
