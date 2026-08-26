// Build and stage the MCP stdio proxy for Tauri externalBin packaging.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, statSync } from "node:fs";
import { join, normalize } from "node:path";
import process from "node:process";

const root = process.cwd();
const tauriRoot = join(root, "src-tauri");
const targetTriple =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  execFileSync("rustc", ["-vV"], { encoding: "utf8" })
    .match(/^host: (.+)$/m)?.[1]
    ?.trim();
if (!targetTriple) throw new Error("Unable to determine Rust target triple");

const debug = process.env.TAURI_ENV_DEBUG === "true";
const profile = debug ? "debug" : "release";
const binaryName = process.platform === "win32" ? "serialporttool-mcp.exe" : "serialporttool-mcp";
const targetDir = process.env.CARGO_TARGET_DIR
  ? normalize(process.env.CARGO_TARGET_DIR)
  : join(tauriRoot, "target");
const targetPrefix = process.env.TAURI_ENV_TARGET_TRIPLE ? join(targetTriple) : "";
const source = join(targetDir, targetPrefix, profile, binaryName);
const stagedDir = join(tauriRoot, "binaries");
const staged = join(stagedDir, `serialporttool-mcp-${targetTriple}${process.platform === "win32" ? ".exe" : ""}`);

if (!statExists(source)) {
  const args = ["build", "--manifest-path", "src-tauri/Cargo.toml", "--bin", "serialporttool-mcp"];
  if (!debug) args.splice(1, 0, "--release");
  if (process.env.TAURI_ENV_TARGET_TRIPLE) args.push("--target", targetTriple);
  console.error(`serialporttool: building ${profile} MCP sidecar for ${targetTriple}`);
  execFileSync("cargo", args, {
    cwd: root,
    stdio: "inherit",
    env: {
      ...process.env,
      // The sidecar build must not ask tauri-build to package itself again.
      TAURI_CONFIG: JSON.stringify({ bundle: { externalBin: [] } }),
    },
  });
}
if (!statExists(source)) throw new Error(`MCP sidecar was not built: ${source}`);
mkdirSync(stagedDir, { recursive: true });
copyFileSync(source, staged);
console.log(`Staged MCP sidecar: ${staged}`);

function statExists(path) {
  try {
    return statSync(path).isFile();
  } catch {
    return false;
  }
}
