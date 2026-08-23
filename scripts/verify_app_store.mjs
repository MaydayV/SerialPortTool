import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

let passed = 0;
let failed = 0;
function check(name, condition, detail = "") {
  if (condition) {
    passed += 1;
    console.log(`PASS ${name}`);
  } else {
    failed += 1;
    console.error(`FAIL ${name}${detail ? ` — ${detail}` : ""}`);
  }
}

const requiredFiles = [
  "PRIVACY.md",
  "docs/MAC_APP_STORE.md",
  "docs/app-store/metadata.zh-Hans.md",
  "src-tauri/Info.plist",
  "src-tauri/macos/PrivacyInfo.xcprivacy",
  "src-tauri/macos/AppStore.entitlements.template.plist",
  "src-tauri/tauri.appstore.conf.example.json",
  "scripts/macos-app-store.mjs",
];
for (const file of requiredFiles) check(`required file ${file}`, exists(file));

const packageJson = JSON.parse(read("package.json"));
const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json"));
const cargoVersion = read("src-tauri/Cargo.toml").match(/^version = "([^"]+)"/m)?.[1];
check(
  "package/Cargo/Tauri versions match",
  packageJson.version === cargoVersion && cargoVersion === tauriConfig.version,
  `${packageJson.version} / ${cargoVersion} / ${tauriConfig.version}`
);
check(
  "bundle identifier is explicit reverse-DNS",
  /^[a-zA-Z][a-zA-Z0-9-]*(\.[a-zA-Z0-9-]+){2,}$/.test(tauriConfig.identifier),
  tauriConfig.identifier
);
check(
  "product name fits App Store 30-character limit",
  Array.from(tauriConfig.productName).length <= 30,
  tauriConfig.productName
);
check("App Store category configured", Boolean(tauriConfig.bundle?.category));

const infoPlist = read("src-tauri/Info.plist");
check(
  "encryption declaration configured",
  /<key>ITSAppUsesNonExemptEncryption<\/key>\s*<false\/>/.test(infoPlist)
);
check(
  "local-network purpose string configured",
  infoPlist.includes("<key>NSLocalNetworkUsageDescription</key>")
);

const privacy = read("src-tauri/macos/PrivacyInfo.xcprivacy");
check(
  "privacy manifest declares no tracking",
  /<key>NSPrivacyTracking<\/key>\s*<false\/>/.test(privacy)
);
check(
  "privacy manifest declares collected-data section",
  privacy.includes("<key>NSPrivacyCollectedDataTypes</key>")
);

const entitlements = read("src-tauri/macos/AppStore.entitlements.template.plist");
for (const key of [
  "com.apple.security.app-sandbox",
  "com.apple.application-identifier",
  "com.apple.developer.team-identifier",
  "com.apple.security.network.client",
  "com.apple.security.network.server",
  "com.apple.security.device.serial",
  "com.apple.security.device.usb",
  "com.apple.security.device.bluetooth",
  "com.apple.security.files.user-selected.read-write",
]) {
  check(`entitlement ${key}`, entitlements.includes(`<key>${key}</key>`));
}
check("entitlements Team ID remains generated", entitlements.includes("__APPLE_TEAM_ID__"));
check("entitlements App ID Prefix remains generated", entitlements.includes("__APPLE_APP_ID_PREFIX__"));
check(
  "entitlements bundle identifier matches Tauri config",
  entitlements.includes(`.${tauriConfig.identifier}</string>`)
);

const resources = tauriConfig.bundle?.resources ?? {};
check(
  "privacy manifest is bundled in Contents/Resources",
  resources["macos/PrivacyInfo.xcprivacy"] === "PrivacyInfo.xcprivacy"
);
check("localized Info.plist strings are bundled", resources["infoplist/"] === "");

const gitignore = read(".gitignore");
for (const ignored of [
  "src-tauri/macos/*.provisionprofile",
  "AuthKey_*.p8",
  "private_keys/",
  "src-tauri/tauri.appstore.conf.json",
]) {
  check(`secret ignored: ${ignored}`, gitignore.includes(ignored));
}

const iconSourcePath = path.join(repoRoot, "src-tauri/icons/app-icon-source.png");
if (fs.existsSync(iconSourcePath)) {
  const png = fs.readFileSync(iconSourcePath);
  const signature = png.subarray(0, 8).toString("hex") === "89504e470d0a1a0a";
  const width = signature && png.length >= 24 ? png.readUInt32BE(16) : 0;
  const height = signature && png.length >= 24 ? png.readUInt32BE(20) : 0;
  check("1024px App Store icon source", signature && width === 1024 && height === 1024, `${width}x${height}`);
} else {
  check("1024px App Store icon source", false, "missing app-icon-source.png");
}

const screenshotDir = path.join(repoRoot, "docs/app-store/screenshots/zh-Hans");
const screenshots = fs.existsSync(screenshotDir)
  ? fs.readdirSync(screenshotDir).filter((name) => name.endsWith(".png"))
  : [];
check("at least three Mac App Store screenshots", screenshots.length >= 3, `${screenshots.length} found`);

console.log(`\n${passed} passed, ${failed} failed`);
if (failed) process.exit(1);
