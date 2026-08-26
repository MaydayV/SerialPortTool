# SerialPortTool MCP 使用指南

SerialPortTool 可以作为本机 MCP Server，让 AI Agent 调用正在运行的桌面应用；所有连接、收发、协议和波形状态仍由真实 GUI 展示。

## 1. 启动方式

启动桌面应用后，进入「设置 → MCP 与 AI」，显式点击「启用 MCP」后，应用才会启动仅监听 loopback 的 Streamable HTTP MCP endpoint：

```text
http://127.0.0.1:<随机端口>/mcp
```

端口和随机 pairing token 会显示在设置页的「MCP 与 AI」页面中。停用 MCP 后 HTTP 服务和 macOS/Linux Unix socket 都会关闭；token 只有用户显式点击显示/复制时才会返回，不会写入普通事件、操作时间线或日志。

默认权限模式是 `ask`：

- `observe`：只允许只读工具，禁止 MCP 写操作。
- `ask`：写操作在同一个 MCP 请求内等待 GUI 用户允许或拒绝；超时不会执行。
- `full`：写操作直接执行，适合用户明确授权的本机自动化场景。

## 2. MCP 工具

只读工具：

- `list_ports`
- `get_state`
- `read_received`
- `wait_for_data`
- `get_connection_profiles`
- `get_protocol_templates`
- `get_protocol_state`
- `get_frame_statistics`
- `get_graph_state`
- `get_graph_data`

写工具：

- `configure_connection`
- `connect`
- `disconnect`
- `send_data`
- `clear_received`
- `select_protocol`
- `clear_graph`

所有工具结果同时提供 MCP `content` 文本和 `structuredContent`；工具执行失败使用 `isError: true`，不会把业务错误伪装成 JSON-RPC 协议错误。

## 3. HTTP 客户端要求

请求必须满足：

- `POST /mcp`
- `Authorization: Bearer <pairing-token>`
- `Content-Type: application/json`
- `Accept: application/json` 或 `text/event-stream`
- 请求 body 的 JSON-RPC `_meta` 必须包含协议版本、clientInfo 和 clientCapabilities。
- 当前支持的 MCP 协议版本：`2026-07-28`。

示例 discovery 请求：

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "server/discover",
  "_meta": {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": {
      "name": "my-agent",
      "version": "1.0"
    },
    "io.modelcontextprotocol/clientCapabilities": {}
  }
}
```

## 4. stdio 代理

需要由 MCP Client 启动本地进程时，可使用 `serialporttool-mcp`。代理只负责 MCP stdio 和 GUI 本地 IPC 转发，不复制工具业务逻辑：

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin serialporttool-mcp
```

代理从 stdin 按行读取 JSON-RPC，并只向 stdout 输出 JSON-RPC；诊断信息全部写入 stderr。没有显式配置时，代理不会自动启动 GUI：

```bash
serialporttool-mcp --socket /tmp/serialporttool-mcp.sock
```

只有用户明确传入 `--start-gui <executable>` 或设置 `SERIALPORTTOOL_MCP_GUI_COMMAND` 时，代理才会启动 GUI。

macOS/Linux 的 GUI 与代理使用 Unix domain socket，默认路径为：

```text
/tmp/serialporttool-mcp.sock
```

socket 权限为 `0600`，IPC frame 使用大端 u32 长度前缀，并限制最大帧长；慢客户端不会无限阻塞 GUI 关闭。

## 5. 安全和数据边界

- HTTP MCP Server 仅绑定 `127.0.0.1`，默认不能被局域网访问。
- HTTP 请求必须通过 pairing token 认证，并校验 loopback Origin 策略。
- GUI 审批由 Rust `AppControlService` 作为唯一权威，前端只展示状态投影。
- 审批卡片只展示操作摘要和参数摘要，不展示完整 payload。
- 接收数据、发送数据、模板和波形结果均有数量/字节上限。
- 协议模板和波形状态通过 typed frontend bridge 读取现有 Pinia store，不维护第二份副本。
- 本机连接目标、串口数据和波形数据不会上传到开发者服务。

## 6. 当前限制

- 模板和波形导出尚未作为 MCP 工具开放，避免 MCP 任意写入本地路径。
- 当前已实现并验证 macOS/Linux Unix socket；Windows Named Pipe 尚未实现，Windows 分支会返回明确的 unsupported 错误，不会退化成 TCP 假装支持。
- macOS `.app` 和 DMG 已验证包含 GUI 主程序与 `serialporttool-mcp` sidecar；Windows Named Pipe 暂不处理。

## English summary

SerialPortTool exposes an opt-in loopback-only Streamable HTTP MCP server from the running GUI. Enable it explicitly in Settings → MCP & AI. The default permission mode is `ask`; write operations wait for explicit approval in the settings page. The stdio proxy forwards JSON-RPC over a bounded local IPC framing layer and never writes diagnostics to stdout. macOS/Linux Unix sockets and the macOS packaged sidecar are implemented and verified; Windows Named Pipe is intentionally deferred.
