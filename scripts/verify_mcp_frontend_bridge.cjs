// MCP 前端 bridge 契约回归检查。
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const read = (name) => fs.readFileSync(path.join(root, name), "utf8");
const bridge = read("src/mcpFrontendBridge.ts");
const app = read("src/App.vue");
const api = read("src/api.ts");
const schema = read("src-tauri/src/mcp/schema.rs");
const control = read("src-tauri/src/control/mod.rs");
const tools = read("src-tauri/src/mcp/tools.rs");

const checks = [
  ["bridge handles protocol state", /protocol\.get_state/.test(bridge)],
  ["bridge handles protocol selection", /protocol\.select/.test(bridge) && /store\.select\(protocolId\)/.test(bridge)],
  ["bridge handles graph state/data/clear", /graph\.get_state/.test(bridge) && /graph\.get_data/.test(bridge) && /graph\.clear/.test(bridge)],
  ["bridge reads existing Pinia stores", /useProtocolStore\(\)/.test(bridge) && /useGraphStore\(\)/.test(bridge)],
  ["template and series bounds are explicit", /MAX_TEMPLATES\s*=\s*100/.test(bridge) && /MAX_SERIES\s*=\s*32/.test(bridge)],
  ["graph point and byte bounds are explicit", /MAX_POINTS\s*=\s*20_000/.test(bridge) && /MAX_BYTES\s*=\s*1024 \* 1024/.test(bridge)],
  ["graph response reports bounded byte count", /byteCount/.test(bridge) && /max_bytes/.test(bridge)],
  ["bridge validates request IDs", /request\.request_id/.test(bridge) && /request_id 无效/.test(bridge)],
  ["bridge sends typed response to Rust", /api\.mcpFrontendBridgeResponse\(response\)/.test(bridge) && /mcp_frontend_bridge_response/.test(api)],
  ["App installs and tears down bridge", /setupMcpFrontendBridge\(\)/.test(app) && /teardownMcpBridge/.test(app)],
  ["Rust owns correlation timeout and cleanup", /FRONTEND_BRIDGE_TIMEOUT/.test(control) && /remove\(&request_id\)/.test(control)],
  ["new protocol and graph tools are exposed", /get_protocol_templates/.test(schema) && /get_graph_data/.test(schema) && /clear_graph/.test(tools)],
];

let failures = 0;
for (const [name, passed] of checks) {
  console.log(`${passed ? "PASS" : "FAIL"} ${name}`);
  if (!passed) failures += 1;
}
console.log(`\n${checks.length - failures} passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
