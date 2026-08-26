// Keep the normal `npm run tauri <command>` interface while applying the
// sidecar-only config to bundle commands.
import { spawnSync } from "node:child_process";
import process from "node:process";

const args = process.argv.slice(2);
const env = { ...process.env };
if (args[0] === "build" && args.includes("--bundles") && args.includes("dmg") && !env.CI) {
  env.CI = "true";
}
const tauriCommand = process.platform === "win32" ? "tauri.cmd" : "tauri";
const result = spawnSync(tauriCommand, args, { env, stdio: "inherit", shell: false });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
