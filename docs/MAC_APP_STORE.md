# Mac App Store 发布指南

工程已具备 App Store 专用沙盒、隐私清单、原生文件授权、签名配置生成、PKG 打包、在线验证与上传流程。证书、描述文件和 App Store Connect 记录必须由 Apple Developer 账号创建，不能提交进 Git。

## 账号侧一次性准备

1. 加入 Apple Developer Program。
2. 在 Certificates, Identifiers & Profiles 创建显式 Mac App ID，Bundle ID 必须为 `com.maydayv.serialporttool`。
3. 创建并安装应用签名证书（Apple Distribution 或 Mac App Distribution）。
4. 创建并安装 Mac Installer Distribution 证书。
5. 创建类型为 Mac App Store Connect 的 provisioning profile，绑定上述 App ID 和应用签名证书。
6. 在 App Store Connect 创建 macOS App 记录，Bundle ID 与工程保持一致。
7. 在 Users and Access → Integrations 创建具有 Developer 权限的 API Key，并仅下载一次 `.p8` 私钥。
8. 按 [商店元数据](app-store/metadata.zh-Hans.md) 填写描述、隐私、审核备注和截图。版权中的权利人名称应在提交前按账号的真实法定主体确认。

## 本地构建

确保钥匙串中已安装应用和安装包两类证书，然后设置以下环境变量：

```bash
export APPLE_TEAM_ID="你的10位TeamID"
export APPLE_PROVISIONING_PROFILE="/绝对路径/SerialPortTool_AppStore.provisionprofile"
export APPLE_APP_SIGNING_IDENTITY="Apple Distribution: 证书名称 (TEAMID)"
export APPLE_INSTALLER_SIGNING_IDENTITY="Mac Installer Distribution: 证书名称 (TEAMID)"
export APP_STORE_BUILD_NUMBER="1"
```

默认生成同时支持 Apple Silicon 与 Intel 的 Universal 包，因此需要 rustup 和两个 macOS Rust target。只发布 Apple Silicon 时可额外设置：

```bash
export APP_STORE_TARGET="aarch64-apple-darwin"
```

仅本地构建和签名检查：

```bash
npm run appstore:check
npm run appstore:build
```

输出位于 `artifacts/app-store/`。脚本会从 provisioning profile 读取并验证 Team ID、App ID Prefix 与 Bundle ID，检查证书，生成不含凭据的临时配置，应用 entitlements，验证 `.app` 签名，再使用 `productbuild` 生成并验证 `.pkg`。旧账号的 App ID Prefix 即使与 Team ID 不同也会被正确处理。

## 在线验证和上传

将 API 私钥保存为 `AuthKey_<KEY_ID>.p8`，并放到当前目录的 `private_keys/`、`~/.private_keys/` 或 `~/.appstoreconnect/private_keys/`，然后设置：

```bash
export APP_STORE_CONNECT_API_KEY_ID="Key ID"
export APP_STORE_CONNECT_API_ISSUER="Issuer ID"
```

`npm run appstore:build` 会执行 App Store Connect 在线验证；确认无误后执行：

```bash
npm run appstore:upload
```

上传后需等待 App Store Connect 处理构建，再选择该构建并提交审核。Mac App Store 构建不走 Developer ID 公证；项目原有站外 DMG 发布仍需使用 Developer ID 签名与公证，两条发布链路不要混用证书。

## GitHub Actions

手动运行“Mac App Store 构建与上传”工作流。仓库 Secrets 需要配置：

- `APPLE_TEAM_ID`
- `APPLE_APP_SIGNING_IDENTITY`
- `APPLE_INSTALLER_SIGNING_IDENTITY`
- `APPLE_APP_CERTIFICATE_BASE64`
- `APPLE_APP_CERTIFICATE_PASSWORD`
- `APPLE_INSTALLER_CERTIFICATE_BASE64`
- `APPLE_INSTALLER_CERTIFICATE_PASSWORD`
- `APPLE_PROVISIONING_PROFILE_BASE64`
- `APP_STORE_CONNECT_API_KEY_ID`
- `APP_STORE_CONNECT_API_ISSUER`
- `APP_STORE_CONNECT_API_KEY_BASE64`

证书 `.p12`、`.provisionprofile` 和 API `.p8` 文件均需先做单行 Base64 编码。工作流默认只验证；只有手动勾选“上传到 App Store Connect”才会上传。

## 提交前检查

- 三处版本一致，并为每次上传设置未使用过的 `APP_STORE_BUILD_NUMBER`。
- App Store Connect 中的 Bundle ID 为 `com.maydayv.serialporttool`。
- 截图使用 `docs/app-store/screenshots/zh-Hans/` 中的 1440×900、无透明通道 PNG。
- 隐私政策 URL 已公开访问，支持 URL 包含可用联系方式。
- App 隐私回答为“不收集数据”，与 [隐私政策](../PRIVACY.md) 和 `PrivacyInfo.xcprivacy` 一致。
- 使用真实串口、TCP 客户端/服务端、UDP 客户端/服务端和系统保存面板完成沙盒包实机测试。
- 审核备注包含无硬件演示入口，避免审核人员因无法连接设备而误判功能不完整。
