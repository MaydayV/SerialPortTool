// 波形 store 回归：显式开关、切页持续采集、批量裁剪、示例数据和 CSV
const esbuild = require("esbuild");
const os = require("os");
const path = require("path");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serialporttool-graph-store-bundle.cjs");
  await esbuild.build({
    entryPoints: ["src/stores/graph.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
    external: ["vue", "pinia"],
  });
  process.env.NODE_PATH = path.join(__dirname, "..", "node_modules");
  require("module").Module._initPaths();
  const { createPinia, setActivePinia } = require("pinia");
  const { useGraphStore } = require(bundlePath);
  setActivePinia(createPinia());
  const store = useGraphStore();
  const enc = new TextEncoder();

  let pass = 0;
  let fail = 0;
  const check = (name, condition, detail = "") => {
    if (condition) {
      pass += 1;
      console.log(`PASS ${name}`);
    } else {
      fail += 1;
      console.log(`FAIL ${name}${detail ? ` — ${detail}` : ""}`);
    }
  };

  store.processData(enc.encode("$temp,1,23.5\n"));
  check("disabled graph does not parse", store.frameCount === 0);

  store.enabled = true;
  store.processData(enc.encode("$temp,1,23.5\n"));
  check("enabled graph parses while hidden", store.frameCount === 1);
  store.setViewActive(true);
  check(
    "CSV exports x and y values",
    store.exportCsv().includes('"temp",1,23.5')
  );

  store.clear();
  store.processData(enc.encode("$peer,1,2\n"), "client-a");
  store.processData(enc.encode("$peer,2,3\n"), "client-b");
  check("peer curve streams stay distinguishable", store.seriesList.length === 2);

  store.clear();
  store.processData(enc.encode("$auto,10\n"), "client-a");
  store.processData(enc.encode("$auto,20\n"), "client-b");
  check(
    "automatic x counters are isolated per peer",
    store.seriesList.length === 2 && store.seriesList.every((series) => series.xs[0] === 0)
  );

  store.clear();
  const lines = [];
  for (let i = 0; i < 22_000; i++) lines.push(`$load,${i},${i % 100}\n`);
  store.processData(enc.encode(lines.join("")));
  const kept = store.seriesList[0]?.xs.length ?? 0;
  check(
    "large graph buffer prunes in batches",
    kept >= 20_000 && kept <= 21_024,
    `kept=${kept}`
  );

  store.clear();
  store.addDemoData();
  check(
    "demo data creates two visible series",
    store.frameCount === 480 && store.seriesList.length === 2
  );

  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
