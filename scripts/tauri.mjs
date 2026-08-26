// Keep the normal `npm run tauri <command>` interface while applying the
// sidecar-only config to bundle commands.
import { spawnSync } from "node:child_process";
import process from "node:process";

const args = process.argv.slice(2);
const env = { ...process.env };
const targetIndex = args.indexOf("--target");
const targetTriple = targetIndex >= 0 ? args[targetIndex + 1] : "";
if (targetTriple) env.SERIALPORTTOOL_TARGET = targetTriple;
if (args.includes("--debug")) env.TAURI_ENV_DEBUG = "true";
const wantsDmg = args[0] === "build" && (!args.includes("--bundles") || args.includes("dmg"));
if (wantsDmg && !env.CI) {
  env.CI = "true";
}
const tauriCommand = process.platform === "win32" ? "tauri.cmd" : "tauri";
const buildArgs = [...args];
if (wantsDmg) {
  const bundlesIndex = buildArgs.indexOf("--bundles");
  if (bundlesIndex >= 0) {
    buildArgs[bundlesIndex + 1] = "app";
  } else {
    buildArgs.push("--bundles", "app");
  }
}
const result = spawnSync(tauriCommand, buildArgs, { env, stdio: "inherit", shell: false });
if (result.error) throw result.error;
if (result.status === 0 && process.platform === "darwin" && wantsDmg) {
  const packageResult = spawnSync(process.execPath, ["scripts/package_macos_dmg.mjs"], {
    env,
    stdio: "inherit",
    shell: false,
  });
  if (packageResult.error) throw packageResult.error;
  if (packageResult.status !== 0) process.exit(packageResult.status ?? 1);
}
process.exit(result.status ?? 1);
