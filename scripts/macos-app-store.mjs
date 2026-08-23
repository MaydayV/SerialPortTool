import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = path.join(repoRoot, "src-tauri");
const args = new Set(process.argv.slice(2));
const prepareOnly = args.has("--prepare-only");
const upload = args.has("--upload");

function fail(message) {
  console.error(`ERROR ${message}`);
  process.exit(1);
}

function requireEnv(name) {
  const value = process.env[name]?.trim();
  if (!value) fail(`缺少环境变量 ${name}`);
  return value;
}

function run(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: options.capture ? "pipe" : "inherit",
    env: options.env ?? process.env,
    input: options.input,
  });
  if (result.error) fail(`${command} 无法执行：${result.error.message}`);
  if (result.status !== 0) {
    if (options.capture) {
      process.stderr.write(result.stdout ?? "");
      process.stderr.write(result.stderr ?? "");
    }
    fail(`${command} 执行失败（退出码 ${result.status}）`);
  }
  return options.combine
    ? `${result.stdout ?? ""}${result.stderr ?? ""}`
    : (result.stdout ?? "");
}

if (process.platform !== "darwin") fail("Mac App Store 包只能在 macOS 上构建");

const tauriConfig = JSON.parse(fs.readFileSync(path.join(tauriDir, "tauri.conf.json"), "utf8"));
const identifier = tauriConfig.identifier;
const version = tauriConfig.version;
const teamId = requireEnv("APPLE_TEAM_ID");
const profilePath = path.resolve(requireEnv("APPLE_PROVISIONING_PROFILE"));
const buildNumber = requireEnv("APP_STORE_BUILD_NUMBER");
const target = process.env.APP_STORE_TARGET?.trim() || "universal-apple-darwin";

if (!/^[A-Z0-9]{10}$/.test(teamId)) fail("APPLE_TEAM_ID 应为 10 位大写字母或数字");
if (!/^\d+(\.\d+){0,2}$/.test(buildNumber)) {
  fail("APP_STORE_BUILD_NUMBER 只能包含 1 到 3 段数字，例如 12 或 1.2.3");
}
if (!fs.existsSync(profilePath)) fail(`找不到 provisioning profile：${profilePath}`);

const profilePlist = run("security", ["cms", "-D", "-i", profilePath], { capture: true });
const profileAppId = run(
  "plutil",
  ["-extract", "Entitlements.application-identifier", "raw", "-o", "-", "-"],
  { capture: true, input: profilePlist }
).trim();
const profileTeamId = run(
  "plutil",
  ["-extract", "TeamIdentifier.0", "raw", "-o", "-", "-"],
  { capture: true, input: profilePlist }
).trim();
const appIdSuffix = `.${identifier}`;
if (!profileAppId.endsWith(appIdSuffix)) {
  fail(`provisioning profile 的 App ID 为 ${profileAppId}，必须匹配 ${identifier}`);
}
if (profileTeamId !== teamId) {
  fail(`provisioning profile 的 Team ID 为 ${profileTeamId}，预期 ${teamId}`);
}
const appIdPrefix = profileAppId.slice(0, -appIdSuffix.length);
if (!/^[A-Z0-9]{10}$/.test(appIdPrefix)) fail("provisioning profile 的 App ID Prefix 无效");

const entitlementsTemplate = fs.readFileSync(
  path.join(tauriDir, "macos/AppStore.entitlements.template.plist"),
  "utf8"
);
const generatedEntitlements = entitlementsTemplate
  .replaceAll("__APPLE_TEAM_ID__", teamId)
  .replaceAll("__APPLE_APP_ID_PREFIX__", appIdPrefix);
const entitlementsPath = path.join(tauriDir, "macos/AppStore.entitlements.generated.plist");
fs.writeFileSync(entitlementsPath, generatedEntitlements);
run("plutil", ["-lint", entitlementsPath]);

const generatedConfig = {
  bundle: {
    macOS: {
      bundleVersion: buildNumber,
      hardenedRuntime: true,
      entitlements: "./macos/AppStore.entitlements.generated.plist",
      files: { "embedded.provisionprofile": profilePath },
    },
  },
};
const generatedConfigPath = path.join(tauriDir, "tauri.appstore.conf.json");
fs.writeFileSync(generatedConfigPath, `${JSON.stringify(generatedConfig, null, 2)}\n`);

console.log(`已生成 App Store 配置：${path.relative(repoRoot, generatedConfigPath)}`);
if (prepareOnly) process.exit(0);

const appSigningIdentity = requireEnv("APPLE_APP_SIGNING_IDENTITY");
const installerSigningIdentity = requireEnv("APPLE_INSTALLER_SIGNING_IDENTITY");
const codeSigningIdentities = run("security", ["find-identity", "-v", "-p", "codesigning"], {
  capture: true,
});
if (!codeSigningIdentities.includes(appSigningIdentity)) {
  fail(`钥匙串中没有应用签名证书：${appSigningIdentity}`);
}
const allIdentities = run("security", ["find-identity", "-v"], { capture: true });
if (!allIdentities.includes(installerSigningIdentity)) {
  fail(`钥匙串中没有安装包签名证书：${installerSigningIdentity}`);
}

if (target === "universal-apple-darwin") {
  const rustup = spawnSync("rustup", ["--version"], { stdio: "ignore" });
  if (rustup.status !== 0) {
    fail("通用包需要 rustup；请安装 rustup，或设置 APP_STORE_TARGET=aarch64-apple-darwin 构建仅 Apple Silicon 版本");
  }
  run("rustup", ["target", "add", "aarch64-apple-darwin", "x86_64-apple-darwin"]);
}

run(
  "npm",
  [
    "run",
    "tauri",
    "--",
    "build",
    "--bundles",
    "app",
    "--target",
    target,
    "--config",
    generatedConfigPath,
  ],
  { env: { ...process.env, APPLE_SIGNING_IDENTITY: appSigningIdentity } }
);

const appPath = path.join(
  tauriDir,
  "target",
  target,
  "release",
  "bundle",
  "macos",
  `${tauriConfig.productName}.app`
);
if (!fs.existsSync(appPath)) fail(`未找到构建产物：${appPath}`);

run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", appPath]);
const signedEntitlements = run("codesign", ["-d", "--entitlements", ":-", appPath], {
  capture: true,
  combine: true,
});
if (!signedEntitlements.includes("com.apple.security.app-sandbox")) {
  fail("签名后的应用缺少 App Sandbox entitlement");
}

const outputDir = path.join(repoRoot, "artifacts", "app-store");
fs.mkdirSync(outputDir, { recursive: true });
const packagePath = path.join(
  outputDir,
  `SerialPortTool_${version}_${buildNumber}_${target}.pkg`
);
run("xcrun", [
  "productbuild",
  "--sign",
  installerSigningIdentity,
  "--component",
  appPath,
  "/Applications",
  packagePath,
]);
run("pkgutil", ["--check-signature", packagePath]);

const apiKeyId = process.env.APP_STORE_CONNECT_API_KEY_ID?.trim();
const apiIssuer = process.env.APP_STORE_CONNECT_API_ISSUER?.trim();
if (apiKeyId && apiIssuer) {
  run("xcrun", [
    "altool",
    upload ? "--upload-app" : "--validate-app",
    "--type",
    "macos",
    "--file",
    packagePath,
    "--apiKey",
    apiKeyId,
    "--apiIssuer",
    apiIssuer,
  ]);
  console.log(upload ? "已上传到 App Store Connect" : "App Store Connect 在线验证通过");
} else if (upload) {
  fail("上传需要 APP_STORE_CONNECT_API_KEY_ID 和 APP_STORE_CONNECT_API_ISSUER");
} else {
  console.log("未配置 App Store Connect API，已跳过在线验证；PKG 已完成本地签名检查。");
}

console.log(`Mac App Store 包：${packagePath}`);
