# SerialPortTool MCP 工具契约

> Task 1 契约基线：MCP `2026-07-28` modern 语义。

本文定义 SerialPortTool 暴露给 MCP adapter 的稳定工具名称、参数边界、结构化结果、错误码和权限语义。它是 **MCP 工具契约**，不代表 HTTP transport、OAuth Authorization 或连接控制已经实现。

## 范围与现代语义

- SerialPortTool 只定义 Tools；本任务不实现 Streamable HTTP、stdio、`server/discover`、认证或连接控制。
- 现代请求使用逐请求 metadata：协议版本、client identity 和 client capabilities 由 transport/adapter 放入请求 `_meta`；`initialize` 不是现代请求的必需握手。
- HTTP adapter（后续任务）还必须校验 `MCP-Protocol-Version` header 与请求 metadata 一致，并按规范处理 Origin、取消和错误；本文件不实现这些行为。
- 工具执行失败是 MCP Tool Result：`isError: true`。它们不是 JSON-RPC protocol error；JSON-RPC error 只用于协议层、未知方法或无效 JSON-RPC 消息。

## 工具清单

| 名称 | 类型 | 作用 | 默认权限 | 语义 annotations |
| --- | --- | --- | --- | --- |
| `list_ports` | 只读 | 列出当前串口和 USB 元信息 | observe | read-only, idempotent |
| `get_state` | 只读 | 读取连接、协议、收发和 AI 控制状态 | observe | read-only, idempotent |
| `read_received` | 只读 | 按 cursor 有界读取接收数据 | observe | read-only |
| `wait_for_data` | 只读 | 在有界超时内等待接收数据 | observe | read-only |
| `get_connection_profiles` | 只读 | 列出保存的连接配置 | observe | read-only, idempotent |
| `configure_connection` | 控制 | 校验并选择串口/TCP/UDP 参数 | ask | idempotent |
| `connect` | 控制 | 打开当前配置的连接 | ask | 非 idempotent |
| `disconnect` | 控制 | 关闭活动连接 | ask | idempotent |
| `send_data` | 控制 | 发送 text/hex/escape/base64 数据 | ask | 非 idempotent |
| `clear_received` | 控制 | 清空接收显示和缓冲 | ask | destructive, idempotent |
| `select_protocol` | 控制 | 选择已有协议模板 | ask | idempotent |

Rust 的 `tool_definitions()` 返回上述 11 个定义。每个定义都包含非空的 `name`、`title`、`description`、`inputSchema`、`outputSchema` 和 `annotations`。`annotations` 使用 MCP 标准语义字段 `readOnlyHint`、`destructiveHint`、`idempotentHint`。schema 使用 JSON Schema object，并关闭未声明字段（`additionalProperties: false`）。

## 参数契约

### `send_data`

```json
{"encoding":"text|hex|escape|base64", "data":"..."}
```

- `encoding` 必填且只能是 `text`、`hex`、`escape`、`base64`。
- 单次**解码后的**数据最多 4 MiB（`MAX_SEND_BYTES`）。因此 hex/base64 的编码字符串可能比 4 MiB 更长，但最终写入连接的字节数绝不超过 4 MiB。
- `text` 按 UTF-8 字节发送。
- `hex` 接受十六进制字节对，可含 ASCII 空白；奇数位或非十六进制字符拒绝。
- `escape` 支持 `\\n`、`\\r`、`\\t`、`\\0`、`\\\\` 和 `\\xHH`；未知或不完整转义拒绝。
- `base64` 使用标准 Base64（允许空白，要求正确 padding）。

### `read_received`

```json
{"cursor":0, "limit":4096}
```

`cursor` 是接收 ring buffer 的单调位置；`limit` 必须为 1 到 4 MiB（`MAX_READ_BYTES`）。具体记录格式和过期 cursor 行为由后续控制服务定义，本契约不写死连接状态。

### `wait_for_data`

```json
{"timeout_ms":5000, "max_bytes":4096}
```

`timeout_ms` 必须为 1 到 120000；`max_bytes` 必须为 1 到 4 MiB。超限请求返回 `invalid_params`，等待过程中到达上限返回 `timeout` 或实际数据，具体由执行层决定。

### `configure_connection`

```json
{"kind":"serial", "target":"/dev/cu.usbserial", "baud_rate":115200}
```

- `target` 必须非空、去除首尾空白后仍非空，不得含 NUL，长度最多 4096 字节。
- `serial` 必须提供 1 到 10,000,000 的 `baud_rate`，不能带网络 `port`。
- `tcp`/`udp` 必须提供 1 到 65535 的 `port`，不能带 `baud_rate`。
- 缺失或明显非法的端口/波特率都返回 `invalid_params`。

其他工具的输入结构由 `schema.rs` 中的 `inputSchema` 固定；空参数工具使用空 object schema，不允许未声明字段。

## 结构化结果

成功的结构化结果使用 MCP Tool Result 的两个并行表示：

```json
{
  "content":[{"type":"text","text":"发送完成\n{\"action_id\":\"act_...\",...}"}],
  "structuredContent": {
    "action_id":"act_...",
    "summary":"发送完成",
    "result":{"bytes_sent":3}
  },
  "isError":false
}
```

- 所有写操作（`configure_connection`、`connect`、`disconnect`、`send_data`、`clear_received`、`select_protocol`）的 structured result 必须包含 `action_id`、面向用户的 `summary` 和 `result`（实际结果）。`action_id` 由后续控制服务生成，本任务只定义其类型。
- `content` 是兼容 TextContent 的 JSON 序列化表示，不能替代 `structuredContent`。
- 稳定结构化返回在工具定义中提供 `outputSchema`；schema 描述 structured content，而不是 transport envelope 的 HTTP 响应。

## 错误模型

工具执行层使用以下稳定错误码：

- `invalid_params`：参数缺失、类型/范围/编码校验失败。
- `not_connected`：需要活动连接但当前未连接。
- `busy`：连接或发送队列正在执行互斥操作。
- `approval_required`：当前权限模式需要用户确认。
- `permission_denied`：用户或权限策略拒绝操作。
- `timeout`：等待或控制操作超时。
- `transport_error`：底层串口/TCP/UDP 传输失败。

这些错误通过 `ToolResult { isError: true, structuredContent: { error: ... } }` 返回，并同时写入 TextContent JSON；不应转换为 JSON-RPC error。错误可附带 `action_id` 和结构化 `details`。

## 权限模式

- `observe`：只允许只读工具；控制工具返回 `permission_denied`。
- `ask`：默认模式；控制工具在执行前返回 `approval_required`，等待明确用户确认后才能继续。
- `full`：允许契约范围内的控制工具，但仍受参数、大小、连接状态和互斥队列限制。

本任务只定义可序列化权限状态，不实现确认 UI、token、OAuth 或权限持久化。后续实现必须让 UI 展示 AI 操作、来源和 action ID，并让 UI 与 MCP 共享同一控制服务。

## 明确未实现

本文件和 `src-tauri/src/mcp/` 目前不提供：

- HTTP Server 或其他 MCP transport；
- OAuth Authorization、Protected Resource Metadata 或本地 token；
- `server/discover`、请求 metadata 校验和协议版本协商；
- 串口/TCP/UDP 的打开、关闭、发送、接收缓冲、审批 UI 或连接控制。

这些是后续任务的实现责任；本 Task 只冻结可测试、可复用的契约和校验类型。
