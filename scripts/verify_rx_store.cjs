// RX store 回归：流式多字节解码、跨块 ANSI、来源与增量过滤。
const esbuild = require("esbuild");
const os = require("os");
const path = require("path");
const Module = require("module");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serial-aid-rx-store-bundle.cjs");
  await esbuild.build({
    entryPoints: ["src/stores/rx.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
    external: ["vue", "pinia", "@tauri-apps/api/event"],
  });
  process.env.NODE_PATH = path.join(__dirname, "..", "node_modules");
  Module._initPaths();
  const originalLoad = Module._load;
  Module._load = function (request, parent, isMain) {
    if (request === "@tauri-apps/api/event") {
      return { listen: async () => () => {} };
    }
    return originalLoad.call(this, request, parent, isMain);
  };
  const { createPinia, setActivePinia } = require("pinia");
  const { nextTick } = require("vue");
  const { useRxStore } = require(bundlePath);
  setActivePinia(createPinia());
  const store = useRxStore();
  let pass = 0;
  let fail = 0;
  const check = (name, condition, detail = "") => {
    console.log(`${condition ? "PASS" : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
    condition ? pass++ : fail++;
  };

  const chinese = new TextEncoder().encode("中");
  store.append(chinese.subarray(0, 1), "rx", 1, "peer-a");
  store.append(chinese.subarray(1), "rx", 2, "peer-a");
  check(
    "UTF-8 split across receive chunks decodes once complete",
    store.getEntryText(store.entries[0]) === "" && store.getEntryText(store.entries[1]) === "中"
  );
  check("peer metadata is retained", store.entries[1].peer === "peer-a");

  store.filterText = "中";
  await nextTick();
  check("filter uses streaming decoded text", store.filteredCount === 1);
  store.append(new TextEncoder().encode("中文"), "rx", 3, "peer-a");
  await nextTick();
  check(
    "active filter matches new entries incrementally",
    store.filteredCount === 2,
    `count=${store.filteredCount} text=${store.getEntryText(store.entries[2])}`
  );

  store.clear();
  const encoder = new TextEncoder();
  store.append(encoder.encode("\u001b[3"), "rx", 4);
  store.append(encoder.encode("1mERR"), "rx", 5);
  const spans = store.getEntrySpans(store.entries[1]);
  check(
    "ANSI style and escape sequence continue across chunks",
    spans.some((span) => span.text === "ERR" && span.fg === "#ff3b30")
  );

  store.stopRateTimer();
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
