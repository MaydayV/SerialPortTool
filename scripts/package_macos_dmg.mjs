// Build a small macOS DMG with the app and a visible self-sign helper.
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { tmpdir } from "node:os";

const root = process.cwd();
const debug = process.env.TAURI_ENV_DEBUG === "true" || process.argv.includes("--debug");
const targetTriple = process.env.SERIALPORTTOOL_TARGET || "";
const targetRoot = join(root, "src-tauri", "target", ...(targetTriple ? [targetTriple] : []), debug ? "debug" : "release");
const macosBundle = join(targetRoot, "bundle", "macos");
const dmgBundle = join(targetRoot, "bundle", "dmg");
const appName = "串口助手 SerialPortTool.app";
const appSource = join(macosBundle, appName);
const arch = targetTriple.includes("aarch64") || (!targetTriple && process.arch === "arm64") ? "aarch64" : "x64";
const dmgPath = join(dmgBundle, `串口助手 SerialPortTool_${process.env.SERIALPORTTOOL_VERSION || readVersion()}_${arch}.dmg`);
const fixScript = join(root, "scripts", "macos_fix_and_launch.command");
const readme = join(root, "scripts", "macos_install_readme.txt");

if (!existsSync(appSource)) throw new Error(`macOS app not found: ${appSource}`);
mkdirSync(dmgBundle, { recursive: true });
const stage = mkdtempSync(join(tmpdir(), "serialporttool-dmg-"));
const stagedApp = join(stage, appName);
cpSync(appSource, stagedApp, { recursive: true });
cpSync(fixScript, join(stage, "修复并启动 SerialPortTool.command"));
cpSync(readme, join(stage, "安装说明.txt"));
chmodSync(join(stage, "修复并启动 SerialPortTool.command"), 0o755);
symlinkSync("/Applications", join(stage, "应用程序"));

try {
  if (existsSync(dmgPath)) rmSync(dmgPath, { force: true });
  execFileSync("hdiutil", ["create", "-volname", "SerialPortTool", "-srcfolder", stage, "-format", "UDZO", "-ov", dmgPath], { stdio: "inherit" });
  console.log(`Created macOS DMG: ${dmgPath}`);
} finally {
  rmSync(stage, { recursive: true, force: true });
}

function readVersion() {
  const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  return packageJson.version;
}
