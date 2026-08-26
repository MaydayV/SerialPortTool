// AI 控制 UI/事件契约回归检查。
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const read = (name) => fs.readFileSync(path.join(root, name), "utf8");
const aiStore = read("src/stores/aiControl.ts");
const aiPanel = read("src/components/AiControlPanel.vue");
const api = read("src/api.ts");
const controlEvents = read("src-tauri/src/control/events.rs");
const control = read("src-tauri/src/control/mod.rs");
const tools = read("src-tauri/src/mcp/tools.rs");

const checks = [
  ["AI timeline is bounded", /MAX_TIMELINE\s*=\s*100/.test(aiStore)],
  ["pending approvals are bounded", /MAX_PENDING\s*=\s*32/.test(aiStore)],
  ["control action listener exists", /listen<ControlActionEvent>/.test(aiStore)],
  ["approval listener exists", /listen<PendingApproval>/.test(aiStore)],
  ["MCP activity listener exists", /listen<McpActivityEvent>/.test(aiStore)],
  ["approval allow and deny actions exist", /async function approve[\s\S]*async function deny/.test(aiStore)],
  ["panel renders approval actions", /允许一次/.test(aiPanel) && /拒绝/.test(aiPanel)],
  ["panel renders action IDs", /action_id|actionId/.test(aiPanel)],
  ["token is an explicit API", /mcpToken:/.test(api) && /resetMcpToken:/.test(api)],
  ["approval event hides full payload", /parameter_summary/.test(controlEvents) && /parameter_summary/.test(control)],
  ["MCP activity does not include token", /mcp-activity/.test(tools) && !/local_pairing_token/.test(tools)],
  ["denied MCP operation maps to permission denied", /error\.contains\("拒绝"\)/.test(tools)],
];

let failures = 0;
for (const [name, passed] of checks) {
  console.log(`${passed ? "PASS" : "FAIL"} ${name}`);
  if (!passed) failures += 1;
}
console.log(`\n${checks.length - failures} passed, ${failures} failed`);
process.exit(failures ? 1 : 0);
