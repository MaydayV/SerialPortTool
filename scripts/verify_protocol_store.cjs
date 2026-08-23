// 协议 store 验证：切换模板清空 RX 缓冲，导入归一化并拒绝坏模板
const esbuild = require("esbuild");
const os = require("os");
const path = require("path");

async function main() {
  const bundlePath = path.join(os.tmpdir(), "serial-aid-protocol-store-bundle.cjs");
  await esbuild.build({
    entryPoints: ["src/stores/protocol.ts"],
    bundle: true,
    outfile: bundlePath,
    format: "cjs",
    platform: "node",
    external: ["vue", "pinia"],
  });
  process.env.NODE_PATH = path.join(__dirname, "..", "node_modules");
  require("module").Module._initPaths();
  const { createPinia, setActivePinia } = require("pinia");
  const { useProtocolStore } = require(bundlePath);
  setActivePinia(createPinia());
  const store = useProtocolStore();
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
  const sum = store.templates.find((t) => t.name === "SUM 校验帧");
  const crcTemplate = store.templates.find((t) => t.name === "CRC16-MODBUS 帧");
  store.rxEnabled = true;
  store.select(sum.name);
  const frame = store.processTx(new Uint8Array([1, 2, 3]));
  store.processRx(frame.subarray(0, 3));
  store.select("CRC16-MODBUS 帧");
  const afterSwitch = store.processRx(frame.subarray(3));
  check("template switch uses a clean RX buffer", afterSwitch.frames.length === 0);

  const before = store.templates.length;
  const result = store.importTemplates(JSON.stringify({
    version: 1,
    templates: [
      {
        name: "Normalized",
        header: "0xaa, 55",
        tail: "0d 0a",
        length: { enabled: false },
        checksum: "sum8",
        checksumRange: "payload",
        checksumPosition: "before_tail",
      },
      { name: "Bad hex", header: "GG", checksum: "sum8" },
    ],
  }));
  const normalized = store.templates.find((t) => t.name === "Normalized");
  check("import adds only valid templates", result.added === 1 && result.rejected === 1);
  check("import fully normalizes structure", normalized && normalized.header === "AA 55" && normalized.length.bytes === 1);
  check("bad template does not enter library", store.templates.length === before + 1 && !store.templates.some((t) => t.name === "Bad hex"));

  const replacement = store.templates.find((t) => t.name === "Normalized");
  const badReplacement = store.importTemplates(JSON.stringify([
    { name: replacement.name, header: "AA0", checksum: "sum8" },
  ]));
  check("bad replacement is rejected", badReplacement.rejected === 1);
  check("bad replacement leaves old template intact", store.templates.find((t) => t.name === "Normalized").header === "AA 55");

  const persistedTemplate = { ...replacement, name: "Persisted only" };
  const replaced = store.replaceTemplates([persistedTemplate, persistedTemplate, { ...persistedTemplate, name: "Broken", header: "GG" }]);
  check("persisted templates replace defaults and deduplicate", replaced && store.templates.length === 1 && store.templates[0].name === "Persisted only");
  check("duplicate template names are rejected", store.addTemplate({ ...persistedTemplate }) === false);

  const largeTemplate = {
    name: "Large chunk",
    header: "AA",
    tail: "55",
    length: { enabled: false, offset: 0, bytes: 1, endian: "little", includeSelf: false },
    checksum: "none",
    checksumRange: "all",
    checksumPosition: "tail",
    description: "",
  };
  store.replaceTemplates([largeTemplate]);
  store.select("Large chunk");
  const largeData = new Uint8Array(128 * 1024);
  largeData[0] = 0xaa;
  largeData[largeData.length - 1] = 0x55;
  const largeResult = store.processRx(largeData);
  check("large RX chunks preserve complete frames", largeResult.frames.length === 1 && largeResult.frames[0].length === largeData.length);

  store.replaceTemplates([crcTemplate]);
  store.select("CRC16-MODBUS 帧");
  store.txEnabled = true;
  store.rxEnabled = true;
  const crcPayload = new Uint8Array(128 * 1024);
  crcPayload.fill(0x5a);
  const crcFrame = store.processTx(crcPayload);
  const crcResult = store.processRx(crcFrame);
  check("unbounded CRC frames stay whole", crcResult.frames.length === 1 && crcResult.frames[0].length === crcFrame.length);

  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail > 0 ? 1 : 0);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
