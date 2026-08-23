// TX store 回归：统一发送队列、文件原子分块、组帧回显与错误阻断。
const esbuild = require("esbuild");
const os = require("os");
const path = require("path");
const Module = require("module");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serial-aid-tx-store-bundle.cjs");
  await esbuild.build({
    entryPoints: ["scripts/tx_test_entry.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
    external: ["vue", "pinia", "@tauri-apps/api/core", "@tauri-apps/api/event"],
  });
  process.env.NODE_PATH = path.join(__dirname, "..", "node_modules");
  Module._initPaths();
  let active = 0;
  let maxActive = 0;
  const starts = [];
  const invocations = [];
  const originalLoad = Module._load;
  Module._load = function (request, parent, isMain) {
    if (request === "@tauri-apps/api/core") {
      return {
        invoke: async (command, args) => {
          if (command !== "conn_send") return undefined;
          active += 1;
          maxActive = Math.max(maxActive, active);
          starts.push(args.data[0]);
          invocations.push(args.data);
          await new Promise((resolve) => setTimeout(resolve, 4));
          active -= 1;
          return args.data.length;
        },
      };
    }
    if (request === "@tauri-apps/api/event") {
      return { listen: async () => () => {} };
    }
    return originalLoad.call(this, request, parent, isMain);
  };
  const { createPinia, setActivePinia } = require("pinia");
  const bundle = require(bundlePath);
  setActivePinia(createPinia());
  const tx = bundle.useTxStore();
  const conn = bundle.useConnStore();
  const rx = bundle.useRxStore();
  const protocol = bundle.useProtocolStore();
  conn.status = "connected";

  let pass = 0;
  let fail = 0;
  const check = (name, condition, detail = "") => {
    console.log(`${condition ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
    condition ? pass++ : fail++;
  };

  await Promise.all([tx.sendHistory("A"), tx.sendHistory("B")]);
  check("concurrent sends never overlap", maxActive === 1, `maxActive=${maxActive}`);

  tx.sendHexMode = true;
  tx.appendNewline = true;
  const beforeBadHex = invocations.length;
  const badHex = await tx.sendHistory("GG");
  check(
    "invalid HEX cannot degrade into a newline-only send",
    !badHex && invocations.length === beforeBadHex
  );
  tx.sendHexMode = false;
  tx.appendNewline = false;

  const beforeHugeRaw = invocations.length;
  const hugeRaw = await tx.sendHistory("A".repeat(4 * 1024 * 1024 + 1));
  check(
    "oversize raw command is blocked before IPC",
    !hugeRaw && invocations.length === beforeHugeRaw
  );

  conn.status = "closed";
  tx.sendText = "periodic";
  tx.toggleScheduled();
  check("schedule cannot start while disconnected", tx.scheduled === false);
  conn.status = "connected";

  starts.length = 0;
  const content = new Uint8Array(130 * 1024);
  content.fill(0xf0);
  const fakeFile = {
    name: "firmware.bin",
    size: content.length,
    slice(start, end) {
      return new Blob([content.slice(start, end)]);
    },
  };
  const filePromise = tx.sendFile(fakeFile);
  await new Promise((resolve) => setTimeout(resolve, 1));
  const commandPromise = tx.sendHistory("Z");
  const [fileDone, commandDone] = await Promise.all([filePromise, commandPromise]);
  check("file send and queued command both complete", fileDone && commandDone);
  check(
    "no command is inserted between file chunks",
    starts.join(",") === "240,240,240,90",
    starts.join(",")
  );

  protocol.select("SUM 校验帧");
  protocol.txEnabled = true;
  const before = invocations.length;
  const rejected = await tx.sendHistory("X".repeat(300));
  check("oversize protocol frame is blocked before invoke", !rejected && invocations.length === before);

  const framed = await tx.sendHistory("OK");
  const echo = rx.entries[rx.entries.length - 1]?.raw;
  check(
    "TX echo records actual framed wire bytes",
    framed && echo && echo[0] === 0xaa && echo[1] === 0x55 && echo.length > 2
  );
  rx.stopRateTimer();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
