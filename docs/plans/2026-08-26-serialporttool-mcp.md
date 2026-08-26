# SerialPortTool MCP 功能开发计划

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** 让 AI Agent 通过 MCP 控制正在运行的 SerialPortTool，同时由 SerialPortTool 的真实桌面界面向用户展示连接、收发、协议和 AI 操作状态。

**Architecture:** SerialPortTool 作为 MCP Server，AI Agent 作为 MCP Client。MCP 接口不模拟鼠标或 DOM 操作，而是调用统一的 `AppControlService`；该服务同时驱动 Rust 连接层、接收缓冲和 Vue 界面状态，保证 AI 操作与用户手动操作走同一条业务路径。第一阶段使用仅监听 `127.0.0.1` 的 Streamable HTTP，后续增加 stdio 代理以兼容需要由 Client 启动进程的 MCP 客户端。

**Tech Stack:** Tauri 2 + Rust + Vue 3 + TypeScript + Pinia；MCP transport 使用 Streamable HTTP，MCP Server 放在 Tauri Rust 后端；测试使用 Rust 集成测试、TypeScript/Node 校验脚本和现有构建流程。

---

## 0. MCP 规范符合性结论

本计划的角色划分和“应用作为 MCP Server、Agent 作为 MCP Client”的方向符合 MCP；SerialPortTool 只提供 Tools，不提供 Resources 或 Prompts 也不违反规范，因为这些能力是可选的。[1]

实现目标必须明确采用 **MCP 2026-07-28 modern 语义**：每个请求携带协议版本、客户端身份和客户端能力 metadata；HTTP 请求同时携带匹配的 `MCP-Protocol-Version` header；Server 实现 `server/discover`。`initialize` 是 `2025-11-25` 及更早 legacy 版本的握手流程，不能写成现代 HTTP 的必需流程；如需兼容旧客户端，必须单独实现 dual-era 兼容。[2][3]

因此当前文档需要补充以下规范要求后，才能进入“规范合规实现”阶段：

- Streamable HTTP 的单一 POST endpoint、`Accept: application/json, text/event-stream`、Origin 校验、协议版本 header/body 一致性和 GET/DELETE 行为
- `tools` capability、`tools/list` 分页 / 稳定排序 / list-changed 策略、工具 `inputSchema` / `outputSchema`、`structuredContent` 和 `isError`
- 现代无 session 语义：不创建或回传 `Mcp-Session-Id`，不使用独立 GET SSE；长连接通知使用规范定义的 subscriptions 机制
- SSE 响应关闭即取消请求，并且服务端停止工作且不得再发送该请求的消息
- HTTP 认证方案的规范边界：自定义本地 Token 不是完整 MCP Authorization 实现；若声明支持 MCP HTTP Authorization，必须实现 OAuth 2.1 / Protected Resource Metadata。对于本机开发版，可以把 loopback bearer token 明确标记为本地部署保护层，而不是 OAuth 能力。[3][5]

---

## 1. 当前基线与约束

仓库：`/Users/colin/Documents/serial-aid`

正式产品名称：`SerialPortTool` / `串口助手 SerialPortTool`

当前已经存在的可复用能力：

- `src-tauri/src/lib.rs`：`list_ports`、`conn_open`、`conn_close`、`conn_send` 等 Tauri command
- `src-tauri/src/conn/mod.rs`：单活动连接 `ConnManager`，支持串口 / TCP / UDP
- `src/api.ts`：前端到 Rust 的 API 封装
- `src/stores/conn.ts`：连接配置、连接状态、端口刷新和状态事件监听
- `src/stores/tx.ts`：发送编码、协议组帧、发送锁、定时发送和文件发送
- `src/stores/rx.ts`：接收记录、原始字节、文本/HEX/ASCII 展示、统计、过滤和日志
- `src/stores/protocol.ts`：协议模板、组帧、解帧和统计

必须遵守：

1. MCP 不得通过模拟鼠标、键盘或 DOM 点击控制应用。
2. MCP 不得绕过发送锁、连接生命周期、大小限制、协议校验和权限策略。
3. UI 与 MCP 必须展示同一份真实状态，不能各自维护一套连接状态。
4. MCP Server 默认只监听本机回环地址，不允许默认暴露到局域网。
5. 高风险操作必须可配置用户确认；默认采用询问模式。
6. 旧名称清理不能破坏已有用户配置，旧 localStorage key 必须保留一次性迁移能力。
7. 第一版只实现硬件调试闭环，不一次性开放所有文件、协议、波形和自动化能力。

---

## 2. 第一版范围

### 2.1 MCP 只读工具

| 工具 | 作用 |
| --- | --- |
| `list_ports` | 获取当前串口列表及 USB 元信息 |
| `get_state` | 获取连接、协议、统计、AI 控制和 UI 状态 |
| `read_received` | 按 cursor / 条数读取接收记录 |
| `wait_for_data` | 等待满足条件的接收数据，支持超时 |
| `get_connection_profiles` | 获取已保存连接配置 |

### 2.2 MCP 控制工具

| 工具 | 作用 | 默认权限 |
| --- | --- | --- |
| `configure_connection` | 设置串口 / TCP / UDP 参数 | 询问 |
| `connect` | 打开当前连接 | 询问 |
| `disconnect` | 关闭当前连接 | 询问 |
| `send_data` | 发送 text / hex / escape / base64 数据 | 询问 |
| `clear_received` | 清空接收显示和相关缓冲 | 询问 |
| `select_protocol` | 选择已有协议模板 | 询问 |

第一版暂不开放：任意本地路径读取、任意文件发送、任意路径写文件、修改协议模板、定时发送和外部网络监听。相关能力在权限模型和审计日志稳定后再增加。

### 2.3 用户可见反馈

界面必须增加：

- 顶部 AI 控制状态：未连接 / 已连接 / 操作中 / 等待确认 / 错误
- AI 操作时间线：工具名、参数摘要、开始时间、结果、耗时、`action_id`
- 收发记录的来源标签：用户 / AI / 定时任务 / 文件
- 高风险操作确认卡片：允许一次、允许本次会话、拒绝
- MCP 连接状态、监听端口和 Token 重置入口

---

## 3. 目标数据流

```text
AI Agent
  ↓ MCP tools/call
MCP Server（Rust，loopback Streamable HTTP）
  ↓ 参数校验 / Token / 权限 / action_id
AppControlService
  ↓ 串行化写操作
ConnManager + RX Ring Buffer + Protocol Service
  ↓ 状态事件 / 收发事件 / AI 操作事件
Vue + Pinia
  ↓
SerialPortTool 可视化界面
```

所有发送必须经过统一出口：

```text
MCP send_data / UI send
  → 编码和参数校验
  → 权限确认
  → 发送队列
  → 协议处理
  → ConnManager.send
  → tx/rx 统计和日志
  → UI 与 MCP 读取同一结果
```

如果第一阶段还不能把协议编码迁移到共享服务，则 MCP 的 `send_data` 必须明确按“显式输入字节”执行，不能声称自动继承 Vue 当前的 HEX/转义/协议开关。

---

## 4. 分阶段任务

### Task 0：清理旧名称并统一为 SerialPortTool

**Objective:** 清理用户可见和技术元数据中的 `SerialAid` / `serialaid` / `serial-aid` 旧名称，同时保留已有配置的一次性迁移兼容。

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src-tauri/Cargo.toml`
- Regenerate: `src-tauri/Cargo.lock`
- Modify: `src/stores/persist.ts`
- Modify: `src/App.vue`
- Modify: `src/components/ProtocolPanel.vue`
- Modify: `src-tauri/src/lib.rs`
- Modify: `scripts/gen_icon.py`
- Modify: `scripts/verify_*.cjs` / `scripts/verify_*.mjs` 中的临时文件前缀

**Requirements:**

1. npm package 名称统一为 `serialporttool`。
2. Rust package / library 技术名称统一为 `serialporttool` / `serialporttool_lib`，由 Cargo 自动更新 lockfile。
3. 新配置 key 使用 `serialporttool.config.v1`。
4. 首次读取时兼容迁移旧 key `serialaid.config.v1`，迁移成功后删除旧 key。
5. 默认日志、模板、测试临时文件统一使用 `serialporttool-*`。
6. `com.maydayv.serialporttool`、`SerialPortTool`、`串口助手 SerialPortTool` 保持不变，不做错误替换。
7. 本地工作区目录 `/Users/colin/Documents/serial-aid` 可以暂时保留，不把目录名变更与产品身份变更混在一起。

**Verification:**

```bash
rg -n -i 'serial[- ]?aid|serialaid' . \
  --glob '!node_modules/**' \
  --glob '!src-tauri/target/**'
```

预期：只允许出现迁移旧 key 所需的兼容常量；不允许出现在用户可见名称、默认新文件名、产品描述和正式技术标识中。

```bash
npm install
npm run build
npm test
cargo check --manifest-path src-tauri/Cargo.toml
```

---

### Task 1：定义 MCP 工具契约和统一错误模型

**Objective:** 固化工具名称、参数 schema、返回值、错误码、大小限制和权限状态，避免 MCP 接口随实现漂移。

**Files:**
- Create: `docs/mcp-tool-contract.md`
- Create: `src-tauri/src/mcp/schema.rs`
- Test: `src-tauri/tests/mcp_schema.rs`

**Requirements:**

- 所有写操作返回 `action_id`。
- 错误区分：`invalid_params`、`not_connected`、`busy`、`approval_required`、`permission_denied`、`timeout`、`transport_error`。
- `send_data` 单次最大 4 MiB，与现有后端限制保持一致。
- `read_received` 使用 cursor，避免 Agent 重复读取或无限拉取。
- `wait_for_data` 必须有最大超时时间和最大返回字节数。
- 每个现代 MCP 请求携带 `_meta.io.modelcontextprotocol/protocolVersion`、`_meta.io.modelcontextprotocol/clientInfo` 和 `clientCapabilities`；HTTP 还必须携带与协议版本 metadata 一致的 `MCP-Protocol-Version` header。
- Server 必须实现 `server/discover`，并在版本不支持时返回规范定义的 `UnsupportedProtocolVersionError` 及支持版本列表。
- 每个工具声明 `inputSchema`；稳定的结构化返回声明 `outputSchema`，结果同时提供 `structuredContent` 和兼容旧客户端的 TextContent JSON 序列化。
- 工具执行失败返回 MCP Tool Result 的 `isError: true`；只有未知方法、无效 JSON-RPC 等协议层异常才返回 JSON-RPC error response。
- 返回值同时提供机器可读字段和面向用户的摘要。

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml mcp_schema
```

---

### Task 2：抽取 AppControlService 和统一写操作队列

**Objective:** 把 UI 与 MCP 共用的连接、发送、状态和事件逻辑集中到统一控制服务。

**Files:**
- Create: `src-tauri/src/control/mod.rs`
- Create: `src-tauri/src/control/state.rs`
- Create: `src-tauri/src/control/events.rs`
- Modify: `src-tauri/src/conn/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/control_service.rs`

**Requirements:**

- 暴露获取状态、打开、关闭、发送、读取接收缓冲等 typed API。
- 所有发送进入同一队列，不能与用户发送、定时发送或文件发送交叉。
- 为每次 AI 操作生成 `action_id`，发布 started / finished / failed 事件。
- 连接切换和关闭操作保持现有生命周期锁语义。
- Rust 侧维护有界 RX ring buffer，至少支持 cursor、条数和字节数限制。
- 不允许因为 MCP 客户端断开而关闭用户主动建立的连接，除非用户明确启用了“随 AI 会话关闭”。

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml control_service
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
```

---

### Task 3：实现 MCP Server 和本地 Streamable HTTP transport

**Objective:** 在 Tauri 后端内置 MCP Server，让外部 Agent 连接正在运行的 SerialPortTool。

**Files:**
- Create: `src-tauri/src/mcp/mod.rs`
- Create: `src-tauri/src/mcp/server.rs`
- Create: `src-tauri/src/mcp/transport.rs`
- Create: `src-tauri/src/mcp/auth.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/mcp_transport.rs`

**Requirements:**

- 仅绑定 `127.0.0.1`。
- 默认随机端口，避免固定端口冲突。
- 对本地开发版使用内存中的随机 bearer token，并在文档中明确这是本地部署保护层，不冒充 OAuth Authorization；若面向通用远程 MCP Client 宣称支持 HTTP Authorization，则改为实现 OAuth 2.1、RFC 9728 Protected Resource Metadata、`WWW-Authenticate` 和 discovery。
- Token 只在内存和用户明确导出时可见，不写普通日志。
- 拒绝无凭据、错误 Origin、超大请求体和非 MCP JSON-RPC 请求；Origin 存在时必须校验，非法 Origin 返回 HTTP 403。
- 实现现代 `server/discover`、`tools/list`、`tools/call`，并为每个请求处理 `_meta` 和 `MCP-Protocol-Version`。
- `tools` capability 显式声明；第一版工具列表固定时设置 `listChanged: false`，按确定性顺序返回并支持分页字段。
- 支持 POST 返回 `application/json` 或 `text/event-stream`；通知必须关联原请求。
- 对现代 Streamable HTTP 不创建或回传 `Mcp-Session-Id`，GET / DELETE endpoint 返回 405，不提供旧版独立 SSE。
- SSE 响应断开必须触发取消，取消后不得再发送该请求的响应或通知。
- 使用 Streamable HTTP，不新增已废弃的 HTTP+SSE 实现。
- 应用退出时停止 Server，并结束所有未完成请求。
- 在 UI 中显示连接地址、端口和 Token 重置操作。

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml mcp_transport
npm run build
```

使用一个 MCP Client 完成真实闭环：

```text
list_ports → get_state → configure_connection → connect
→ send_data → wait_for_data → read_received → disconnect
```

---

### Task 4：接入基础 MCP 工具

**Objective:** 实现第一版连接、发送和接收工具，并保证结果与 UI 状态一致。

**Files:**
- Create: `src-tauri/src/mcp/tools.rs`
- Modify: `src-tauri/src/control/mod.rs`
- Modify: `src/api.ts`
- Modify: `src/stores/conn.ts`
- Modify: `src/stores/rx.ts`
- Test: `src-tauri/tests/mcp_tools.rs`

**Tools:**

```text
list_ports
get_state
read_received
wait_for_data
get_connection_profiles
configure_connection
connect
disconnect
send_data
clear_received
```

**Requirements:**

- MCP 发送必须产生 UI 可见的 TX 记录和统计。
- MCP 接收必须进入与用户接收相同的 RX ring buffer。
- TCP Server / UDP 多来源数据保留 peer 信息。
- 连接失败、设备不存在、权限拒绝和超时返回稳定错误码。
- 客户端重复请求不得造成重复连接、重复发送或重复清空。
- MCP 断开后 UI 仍保持可操作。

**Verification:**

```bash
cargo test --manifest-path src-tauri/Cargo.toml mcp_tools
npm test
npm run build
```

至少覆盖：

- 串口配置参数校验
- TCP / UDP 配置参数校验
- HEX、文本、base64 输入
- 空数据和超过 4 MiB 的数据
- 未连接发送
- `wait_for_data` 超时
- MCP 与 UI 同时发送时的队列顺序

---

### Task 5：增加 AI 状态栏、操作时间线和用户确认

**Objective:** 让用户能够清楚看到 AI 是否连接、正在做什么以及每次操作的结果。

**Files:**
- Create: `src/stores/aiControl.ts`
- Create: `src/components/AiControlPanel.vue`
- Modify: `src/App.vue`
- Modify: `src/api.ts`
- Modify: `src-tauri/src/control/events.rs`
- Test: `scripts/verify_ai_control.cjs`

**Requirements:**

- 支持观察模式、询问模式、完全控制模式。
- 默认启用询问模式。
- `connect`、`disconnect`、`send_data`、`clear_received` 等写操作可进入审批流程。
- 审批请求显示目标、参数摘要和风险说明。
- 用户确认属于应用层审批：默认在同一个 `tools/call` 请求内等待用户操作，并设置明确超时；允许后返回正常 Tool Result，拒绝或超时返回 `isError: true`，不得把 `approval_required` 当成 JSON-RPC 协议错误。
- 操作时间线不展示完整敏感数据到普通日志；大数据只显示摘要和 HEX 长度。
- AI 事件与用户事件视觉上可区分，但不改变已有收发数据显示逻辑。

**Verification:**

```bash
npm run build
node scripts/verify_ai_control.cjs
```

手动验收：

1. Agent 调用 `list_ports`，界面显示 AI 已连接。
2. Agent 调用 `send_data`，界面显示待确认卡片。
3. 用户拒绝，设备不应收到数据。
4. 用户允许，收发区显示 AI 来源和实际线 bytes。
5. Agent 断开，SerialPortTool 仍可以手动操作。

---

### Task 6：协议和波形能力接入

**Objective:** 在基础 MCP 闭环稳定后，逐步暴露协议模板、解帧统计和波形数据。

**Files:**
- Modify: `src-tauri/src/control/mod.rs`
- Modify: `src-tauri/src/mcp/tools.rs`
- Modify: `src/stores/protocol.ts`
- Modify: `src/stores/graph.ts`
- Create: `docs/mcp-protocol-tools.md`
- Test: `src-tauri/tests/mcp_protocol_tools.rs`

**Requirements:**

- 明确协议模板的权威存储位置，禁止 MCP 和 Vue 各自维护副本。
- 模板修改默认需要确认，并限制数量、名称和字段范围。
- 没有明确帧边界的模板不能被 MCP 宣称为可安全解帧。
- 波形数据必须有点数、时间范围和大小上限。

---

### Task 7：stdio 代理和打包集成

**Objective:** 为需要由 MCP Client 启动本地进程的客户端提供 `serialporttool-mcp`。

**Files:**
- Create: `src-tauri/src/bin/serialporttool-mcp.rs`
- Create: `src-tauri/src/control/local_ipc.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `.github/workflows/`
- Modify: `docs/README` 或 MCP 使用指南
- Test: `src-tauri/tests/mcp_stdio.rs`

**Requirements:**

- stdio 只输出 JSON-RPC，日志全部写 stderr。
- 现代 stdio 使用 `server/discover` 和逐请求 metadata；如支持旧客户端，再增加明确隔离的 legacy `initialize` 兼容路径。
- 代理优先连接已运行的 GUI；没有 GUI 时按配置决定是否启动。
- macOS 使用 Unix socket，Windows 使用 Named Pipe，Linux 使用 Unix socket。
- 不复制业务逻辑，stdio 只是 MCP transport / local IPC 适配层。
- 发布包包含代理程序，并在三平台验证启动和退出。

---

### Task 8：安全、兼容性和发布前验收

**Objective:** 完成 MCP 功能的安全审计、跨平台构建和文档交付。

**Files:**
- Modify: `PRIVACY.md`
- Create: `docs/MCP使用指南.md`
- Modify: `README.md`
- Modify: `src-tauri/macos/AppStore.entitlements.template.plist`（仅在实际验证需要时）
- Modify: `.github/workflows/`

**Acceptance Criteria:**

- macOS、Windows、Linux 均能构建。
- `npm test`、`npm run build`、`cargo fmt --check`、`cargo check`、`cargo clippy -D warnings`、Rust 全量测试通过。
- MCP 默认不能被局域网其他设备访问。
- 未授权 Agent 不能发送数据、清空数据或修改配置。
- 应用关闭后端口释放，重复启动不会产生不可恢复的僵尸 Server。
- App Store 沙盒构建至少完成本地验证：MCP Server、串口、TCP Client、TCP Server、日志权限均不回归。
- README 和 MCP 使用指南统一使用 `SerialPortTool`，不再把 `SerialAid` 作为产品名。

---

## 5. 规范依据

- [1] [MCP Specification Overview](https://modelcontextprotocol.io/specification/2026-07-28)
- [2] [Lifecycle and protocol version negotiation](https://modelcontextprotocol.io/specification/2026-07-28/basic/lifecycle)
- [3] [Streamable HTTP transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [4] [Tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools)
- [5] [Authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)

## 6. 推荐的第一阶段验收脚本

最终应支持下面的可重复测试流程：

```text
1. 启动 SerialPortTool GUI
2. 从 MCP Client 连接本机 MCP Endpoint
3. 调用 list_ports
4. 调用 get_state
5. 调用 configure_connection
6. 调用 connect
7. 用户在 GUI 中看到“AI 控制中”和连接状态变化
8. 调用 send_data
9. 用户在 GUI 中看到 AI 发送记录
10. 设备返回数据
11. 调用 wait_for_data
12. 调用 read_received
13. 用户在 GUI 中看到同一条 RX 记录
14. 调用 disconnect
15. GUI 恢复可手动操作
```

最终报告必须包含：

- MCP 工具列表和 endpoint 配置
- 本地测试命令和真实输出
- 三平台构建状态
- 权限和安全验证结果
- 旧名称残留扫描结果
- 已知限制和后续任务

## Sources

[1] https://modelcontextprotocol.io/specification/2026-07-28
[2] https://modelcontextprotocol.io/specification/2026-07-28/basic/lifecycle
[3] https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http
[4] https://modelcontextprotocol.io/specification/2026-07-28/server/tools
[5] https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization
