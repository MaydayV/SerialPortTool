# SerialPortTool 隐私政策 / Privacy Policy

生效日期 / Effective date: 2026-08-23

## 中文

串口助手 SerialPortTool 是一款本地运行的串口与 TCP/UDP 调试工具。

- 开发者不会通过本应用收集、存储、出售或共享个人数据，也未集成广告、分析、跟踪或第三方账号系统。
- 串口数据、协议模板、连接配置和波形数据默认仅在用户设备上处理。连接配置与界面设置保存在应用的本地容器中。
- 当用户主动配置 TCP/UDP 地址并连接时，用户选择发送的数据会传输到该目标地址；开发者不会接收这些内容。
- 应用仅在用户主动选择文件后读取发送文件、导入模板或写入导出/日志文件。用户可在应用设置中清除本地配置，并可删除自己导出的文件。
- 应用请求本地网络权限仅用于连接用户指定的 TCP/UDP 设备或监听用户启用的服务端端口；请求串口、USB 和蓝牙串口权限仅用于设备通信。
- MCP Server 仅监听本机 loopback；stdio 代理仅通过本机受权限保护的 Unix socket 连接运行中的 GUI。AI Agent 触发的写操作默认需要用户在 GUI 中确认。

如对隐私或数据处理有疑问，请通过 [项目支持页面](https://github.com/MaydayV/SerialPortTool/issues) 联系开发者。

## English

SerialPortTool is a local serial-port and TCP/UDP debugging utility.

- The developer does not collect, retain, sell, or share personal data through the app. The app contains no advertising, analytics, tracking, or third-party account system.
- Serial data, protocol templates, connection profiles, and graph data are processed locally by default. Connection profiles and interface settings are stored in the app's local container.
- When a user explicitly configures and connects to a TCP/UDP endpoint, data selected by the user is transmitted to that endpoint. The developer does not receive this content.
- The app reads or writes files only after the user selects a file for sending, importing, exporting, or logging. Users can clear local settings in the app and delete exported files at any time.
- Local-network access is used only to connect to user-specified TCP/UDP devices or listen on a server port enabled by the user. Serial, USB, and Bluetooth serial access is used only for device communication.
- The MCP server listens only on the local loopback interface. The stdio proxy uses a local permission-protected Unix socket to reach the running GUI, and write actions require GUI approval by default.

For privacy or data-handling questions, contact the developer through the [project support page](https://github.com/MaydayV/SerialPortTool/issues).
