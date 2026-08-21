<script setup lang="ts">
import { ref, onMounted, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import ConnectionBar from "./components/ConnectionBar.vue";
import ProtocolPanel from "./components/ProtocolPanel.vue";
import ReceivePanel from "./components/ReceivePanel.vue";
import SendPanel from "./components/SendPanel.vue";
import GraphPanel from "./components/GraphPanel.vue";
import { useConnStore } from "./stores/conn";
import { useRxStore } from "./stores/rx";
import { useTxStore } from "./stores/tx";
import { useGraphStore } from "./stores/graph";
import { loadConfig, initPersistence } from "./stores/persist";
import { formatTime } from "./utils/bytes";

const conn = useConnStore();
const rx = useRxStore();
const tx = useTxStore();
const graph = useGraphStore();

const view = ref<"debug" | "graph">("debug");
const theme = ref<"light" | "dark" | "system">("light");

// 设置面板
const showSettings = ref(false);

/** 解析生效主题（system → 跟随系统） */
function effectiveTheme(): "light" | "dark" {
  if (theme.value !== "system") return theme.value;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

// 主题切换（浅色 ↔ 深色；system 时优先切到深色）
watch(theme, () => {
  document.documentElement.dataset.theme = effectiveTheme();
});
// 窗口标题同步连接状态
const statusTitle: Record<string, string> = {
  connected: "● 已连接",
  connecting: "◐ 连接中",
  lose: "○ 已断开",
  closed: "",
};
watch(
  () => conn.status,
  (s) => {
    document.title = statusTitle[s] ? `${statusTitle[s]} - 串口助手 SerialAid` : "串口助手 SerialAid";
  }
);
let sysMedia: MediaQueryList | null = null;
function initTheme() {
  document.documentElement.dataset.theme = effectiveTheme();
  if (!sysMedia) {
    sysMedia = window.matchMedia("(prefers-color-scheme: dark)");
    sysMedia.addEventListener("change", () => {
      if (theme.value === "system") {
        document.documentElement.dataset.theme = effectiveTheme();
      }
    });
  }
}

/** 重置全部配置并刷新 */
function resetAll() {
  if (confirm("确定恢复默认设置？所有配置（连接参数、模板、历史）将被清除。")) {
    localStorage.removeItem("serialaid.config.v1");
    localStorage.removeItem("serialaid.ui.v1");
    location.reload();
  }
}

/** 导出接收区日志 */
function exportLog() {
  const lines = rx.entries.map((e) => {
    const t = rx.showTimestamp ? `[${formatTime(e.ts)}] ` : "";
    const dir = e.dir === "rx" ? "<=" : "=>";
    const body = rx.rxHexMode ? e.hex : e.text;
    return `${dir} ${t}${body}`;
  });
  if (!lines.length) {
    alert("接收区为空，无日志可导出");
    return;
  }
  const blob = new Blob(["\ufeff" + lines.join("\n")], {
    type: "text/plain;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `serialaid-log-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.log`;
  a.click();
  URL.revokeObjectURL(url);
}

// 全局快捷键（非输入框焦点时生效）
function onGlobalKeydown(e: KeyboardEvent) {
  // 输入框/文本域/select 内不拦截（保留原生行为）
  const tag = (e.target as HTMLElement)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  if (!e.ctrlKey && !e.metaKey && !e.altKey) {
    switch (e.key.toLowerCase()) {
      case "f5":
        // 刷新串口列表
        e.preventDefault();
        conn.refreshPorts();
        break;
      case "escape":
        // 清空接收区
        rx.clear();
        break;
      case "h":
        // 切换 HEX 显示
        rx.rxHexMode = !rx.rxHexMode;
        break;
      case "t":
        // 切换时间戳
        rx.showTimestamp = !rx.showTimestamp;
        break;
      case "p":
        // 暂停/继续接收
        rx.togglePause();
        break;
      case " ":
        // 空格开关连接
        e.preventDefault();
        conn.toggle();
        break;
    }
  }
}

// 主题切换
onMounted(() => {
  loadConfig(theme);
  initTheme();
  initPersistence(theme);
  conn.setupListeners();
  conn.refreshPorts();
  rx.setup();
  window.addEventListener("keydown", onGlobalKeydown);
  // 曲线数据：从 rx 原始字节流解析（波形自己按曲线协议解析）
  listen<{ data: number[] }>("rx-data", (e) => {
    graph.processData(new Uint8Array(e.payload.data));
  });
  window.addEventListener("beforeunload", () => {
    tx.stopAll();
    rx.stopRateTimer();
  });
});
</script>

<template>
  <div class="app-root">
    <ConnectionBar />
    <ProtocolPanel />
    <nav class="view-tabs">
      <button
        class="view-tab"
        :class="{ active: view === 'debug' }"
        @click="view = 'debug'"
      >
        收发
      </button>
      <button
        class="view-tab"
        :class="{ active: view === 'graph' }"
        @click="view = 'graph'"
      >
        波形
      </button>
      <div class="tab-spacer"></div>
      <span class="kbd-hint" title="全局快捷键：空格=开关连接 · H=HEX · T=时间戳 · P=暂停 · Esc=清空 · F5=刷新串口">
        空格 连接 · H HEX · P 暂停 · Esc 清空
      </span>
      <button class="theme-btn" @click="showSettings = !showSettings" title="设置">
        设置
      </button>
    </nav>
    <main class="content">
      <div v-if="view === 'debug'" class="workbench">
        <div class="panel left">
          <ReceivePanel />
        </div>
        <div class="panel right">
          <SendPanel />
        </div>
      </div>
      <div v-else class="workbench">
        <div class="panel full">
          <GraphPanel />
        </div>
      </div>
    </main>

    <!-- 设置面板 -->
    <div v-if="showSettings" class="settings-overlay" @click.self="showSettings = false">
      <div class="settings-panel">
        <div class="settings-head">
          <span class="settings-title">设置</span>
          <button class="mini-btn" @click="showSettings = false">✕ 关闭</button>
        </div>
        <div class="setting-row">
          <span class="setting-label">主题</span>
          <div class="seg">
            <button
              :class="{ active: theme === 'light' }"
              @click="theme = 'light'"
            >
              浅色
            </button>
            <button
              :class="{ active: theme === 'dark' }"
              @click="theme = 'dark'"
            >
              深色
            </button>
            <button
              :class="{ active: theme === 'system' }"
              @click="theme = 'system'"
            >
              跟随系统
            </button>
          </div>
        </div>
        <div class="setting-row">
          <span class="setting-label">接收字号</span>
          <input
            v-model.number="rx.fontSize"
            type="range"
            min="10"
            max="20"
            step="0.5"
            class="range-slider"
          />
          <span class="setting-val">{{ rx.fontSize }}px</span>
        </div>
        <div class="setting-row">
          <span class="setting-label">数据</span>
          <div class="setting-actions">
            <button class="mini-btn" @click="exportLog">导出接收日志</button>
            <button class="mini-btn danger" @click="resetAll">恢复默认设置</button>
          </div>
        </div>
        <div class="settings-foot">
          设置自动保存 · 快捷键：空格 连接 · H HEX · T 时间戳 · P 暂停 · Esc
          清空 · F5 刷新串口
        </div>
      </div>
    </div>
  </div>
</template>

<style>
:root {
  /* 技术审美 · 浅色（实心纯色，无透明/模糊） */
  --bg: #f2f3f5;
  --panel-bg: #ffffff;
  --panel-border: #e2e4e8;
  --text-primary: #1f2328;
  --text-secondary: #5c6370;
  --text-tertiary: #9aa0a8;
  --control-bg: #ffffff;
  --control-border: #d4d6da;
  --control-text: #1f2328;
  --accent: #0a84ff;
  --accent-hover: #006fd6;
  --accent-soft: rgba(10, 132, 255, 0.12);
  --danger: #e5484d;
  --danger-hover: #cc3d42;
  --row-tx-bg: #f0f6ff;
  --row-border: #f0f1f3;
  --seg-bg: #e9ebee;
  --seg-active-bg: #ffffff;
  --bar-bg: #ffffff;
  --edit-bg: #f7f8fa;

  /* 控件 */
  --field-bg: #ffffff;
  --field-border: #d4d6da;
  --field-border-hover: #b0b4ba;
  --field-inner-shadow: none;
  --field-focus-ring: 0 0 0 2px rgba(10, 132, 255, 0.35);
  --btn-bg: #f5f6f8;
  --btn-border: #d4d6da;
  --btn-hover: #e9ebee;
  --btn-active: #dcdfe4;
  --btn-primary-bg: #0a84ff;
  --btn-primary-hover: #006fd6;
  --btn-danger-bg: #e5484d;
  --btn-danger-hover: #cc3d42;
  --radius-md: 6px;
  --radius-sm: 4px;

  font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI",
    sans-serif;
  font-size: 15px;
  color: var(--text-primary);
  background-color: var(--bg);
  -webkit-font-smoothing: antialiased;
}

:root[data-theme="dark"] {
  /* 技术审美 · 深色（VS Code 风） */
  --bg: #1e1e1e;
  --panel-bg: #252526;
  --panel-border: #3c3c3c;
  --text-primary: #e0e0e0;
  --text-secondary: #a0a0a0;
  --text-tertiary: #6e6e6e;
  --control-bg: #333333;
  --control-border: #4a4a4a;
  --control-text: #e0e0e0;
  --accent: #3794ff;
  --accent-hover: #4ba0ff;
  --accent-soft: rgba(55, 148, 255, 0.18);
  --danger: #f14c4c;
  --danger-hover: #d64141;
  --row-tx-bg: #1d2b3d;
  --row-border: #2d2d2d;
  --seg-bg: #2d2d2d;
  --seg-active-bg: #3c3c3c;
  --bar-bg: #252526;
  --edit-bg: #2a2a2b;

  --field-bg: #333333;
  --field-border: #4a4a4a;
  --field-border-hover: #6e6e6e;
  --field-inner-shadow: none;
  --field-focus-ring: 0 0 0 2px rgba(55, 148, 255, 0.4);
  --btn-bg: #3c3c3c;
  --btn-border: #4a4a4a;
  --btn-hover: #454545;
  --btn-active: #505050;
  --btn-primary-bg: #3794ff;
  --btn-primary-hover: #4ba0ff;
  --btn-danger-bg: #f14c4c;
  --btn-danger-hover: #d64141;
}

* {
  box-sizing: border-box;
}

html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  overflow: hidden;
}

input,
select,
textarea,
button {
  font-family: inherit;
}

/* ===== 控件基础 ===== */

/* 输入框 / 下拉 / 文本域：实心、清晰边框、focus 蓝色描边 */
input[type="text"],
input[type="number"],
input:not([type]),
select,
textarea {
  background: var(--field-bg);
  border: 1px solid var(--field-border);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  outline: none;
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
input[type="text"]:hover,
input[type="number"]:hover,
input:not([type]):hover,
select:hover,
textarea:hover {
  border-color: var(--field-border-hover);
}
input[type="text"]:focus,
input[type="number"]:focus,
input:not([type]):focus,
select:focus,
textarea:focus {
  border-color: var(--accent);
  box-shadow: var(--field-focus-ring);
}
select:focus-visible,
input:focus-visible,
textarea:focus-visible,
button:focus-visible {
  outline: none;
}

/* select 自定义箭头 */
select {
  appearance: none;
  -webkit-appearance: none;
  background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'><path d='M1 1l4 4 4-4' stroke='%238e8e93' stroke-width='1.6' fill='none' stroke-linecap='round' stroke-linejoin='round'/></svg>");
  background-repeat: no-repeat;
  background-position: right 9px center;
  padding-right: 26px !important;
  cursor: pointer;
}
select option,
select optgroup {
  background: var(--control-bg);
  color: var(--text-primary);
}

::placeholder {
  color: var(--text-tertiary);
}

/* 次级按钮：实心 */
.glass-btn {
  background: var(--btn-bg);
  border: 1px solid var(--btn-border);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.12s ease, border-color 0.12s ease,
    color 0.12s ease;
}
.glass-btn:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
  border-color: var(--field-border-hover);
}
.glass-btn:active {
  background: var(--btn-active);
}
.glass-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 主按钮：纯色实心 */
.primary-btn {
  background: var(--btn-primary-bg);
  border: none;
  color: #fff;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.12s ease;
}
.primary-btn:hover {
  background: var(--btn-primary-hover);
}
.primary-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 危险主按钮 */
.danger-btn {
  background: var(--btn-danger-bg);
  border: none;
  color: #fff;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.12s ease;
}
.danger-btn:hover {
  background: var(--btn-danger-hover);
}
/* ===== 技术审美覆盖：实心纯色 ===== */
.conn-bar, .proto-bar {
  background: var(--bar-bg);
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
  border-bottom: 1px solid var(--panel-border);
}
.conn-bar {
  border-bottom: 1px solid var(--panel-border);
}
.proto-bar {
  border-bottom: 1px solid var(--panel-border);
}

/* 面板内工具栏 */
.toolbar {
  border-bottom: 1px solid var(--panel-border);
}

/* 分段控件：实心 */
.seg {
  background: var(--seg-bg);
  border-radius: var(--radius-md);
  padding: 2px;
}
.seg button {
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
}
.seg button.active {
  background: var(--seg-active-bg);
  color: var(--text-primary);
  box-shadow: none;
  border: 1px solid var(--panel-border);
}

/* 次级按钮 */
.tool-btn, .opt, .mini, .action-btn, .theme-btn {
  background: var(--btn-bg);
  border: 1px solid var(--btn-border);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  box-shadow: none;
  transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
}
.tool-btn:hover, .opt:hover, .mini:hover, .action-btn:hover, .theme-btn:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
  border-color: var(--field-border-hover);
}
.tool-btn:active, .opt:active, .mini:active, .action-btn:active {
  background: var(--btn-active);
}
.tool-btn.active, .opt.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.tool-btn.danger:hover, .mini.danger:hover, .action-btn.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
  background: var(--btn-bg);
}
.tool-btn:disabled, .mini:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 主按钮 */
.toggle-btn, .send-btn {
  background: var(--btn-primary-bg);
  border: none;
  box-shadow: none;
  color: #fff;
  border-radius: var(--radius-md);
  transition: background 0.12s ease;
}
.toggle-btn:hover, .send-btn:hover {
  background: var(--btn-primary-hover);
}
.toggle-btn.open {
  background: var(--btn-danger-bg);
}
.toggle-btn.open:hover {
  background: var(--btn-danger-hover);
}
.toggle-btn:disabled, .send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 输入类控件 */
.ctl, .enc-sel, .history-sel, .custom-input, .interval-input,
.new-name, .field input, .field select, .range-input, .header-input,
.tpl-sel, .target-input, .port-select {
  background: var(--field-bg);
  border: 1px solid var(--field-border);
  border-radius: var(--radius-md);
  box-shadow: none;
  color: var(--text-primary);
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.ctl:hover, .enc-sel:hover, .history-sel:hover, .custom-input:hover,
.interval-input:hover, .new-name:hover, .field input:hover, .field select:hover,
.range-input:hover, .header-input:hover, .tpl-sel:hover, .target-input:hover,
.port-select:hover {
  border-color: var(--field-border-hover);
}
.ctl:focus, .enc-sel:focus, .history-sel:focus, .custom-input:focus,
.interval-input:focus, .new-name:focus, .field input:focus, .field select:focus,
.range-input:focus, .header-input:focus, .tpl-sel:focus, .target-input:focus,
.port-select:focus {
  border-color: var(--accent);
  box-shadow: var(--field-focus-ring);
}

/* 发送区文本域 */
.send-area {
  background: var(--field-bg);
  border: none;
  color: var(--text-primary);
}

/* 接收区行 */
.row.rx {
  color: var(--text-primary);
}
.row.tx {
  color: var(--accent);
  background: var(--row-tx-bg);
}
.ts, .dir {
  color: var(--text-tertiary);
}
.hex {
  color: var(--text-primary);
}
.stats {
  color: var(--text-secondary);
}

/* 编辑表单 */
.edit-form {
  background: var(--edit-bg);
  border-radius: var(--radius-md);
}

/* 波形图 chart 区域 */
.chart {
  background: var(--panel-bg);
}

/* 协议面板 label */
.label {
  color: var(--text-primary);
}
.desc {
  color: var(--text-tertiary);
}
.title {
  color: var(--text-primary);
}
</style>

<style scoped>
.app-root {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--bg);
}
.view-tabs {
  display: flex;
  gap: 2px;
  padding: 8px 12px 0;
  align-items: center;
}
.tab-spacer {
  flex: 1;
}
.kbd-hint {
  font-size: 11.5px;
  color: var(--text-tertiary);
  margin-right: 10px;
  user-select: none;
}
.theme-btn {
  border: 1px solid var(--btn-border);
  background: var(--btn-bg);
  border-radius: var(--radius-md);
  padding: 4px 10px;
  font-size: 14px;
  cursor: pointer;
  color: var(--text-secondary);
}
.theme-btn:hover {
  background: var(--btn-hover);
}
.view-tab {
  border: 1px solid transparent;
  border-bottom: 2px solid transparent;
  background: transparent;
  padding: 7px 18px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: var(--radius-sm) var(--radius-sm) 0 0;
}
.view-tab:hover {
  color: var(--text-primary);
  background: var(--seg-bg);
}
.view-tab.active {
  color: var(--accent);
  font-weight: 600;
  border-bottom-color: var(--accent);
  background: transparent;
}
.content {
  flex: 1;
  min-height: 0;
  padding: 12px;
}
.workbench {
  height: 100%;
  display: flex;
  gap: 12px;
}
.panel {
  background: var(--panel-bg);
  border-radius: var(--radius-md);
  border: 1px solid var(--panel-border);
  overflow: hidden;
  min-height: 0;
}
.panel.left {
  flex: 3;
}
.panel.right {
  flex: 2;
  min-width: 320px;
}
.panel.full {
  flex: 1;
}

/* ===== 设置面板 ===== */
.settings-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.32);
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
}
.settings-panel {
  width: 460px;
  max-width: calc(100vw - 48px);
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: 10px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.22);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.settings-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.settings-title {
  font-size: 15px;
  font-weight: 700;
}
.mini-btn {
  border: 1px solid var(--btn-border);
  background: var(--btn-bg);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  padding: 4px 12px;
  font-size: 12.5px;
  cursor: pointer;
}
.mini-btn:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
}
.mini-btn.danger {
  border-color: var(--danger);
  color: var(--danger);
  background: transparent;
}
.mini-btn.danger:hover {
  background: var(--danger);
  color: #fff;
}
.setting-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.setting-label {
  width: 76px;
  flex-shrink: 0;
  font-size: 13px;
  color: var(--text-secondary);
}
.range-slider {
  flex: 1;
  accent-color: #0a84ff;
}
.setting-val {
  width: 44px;
  font-size: 12px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
.setting-actions {
  display: flex;
  gap: 8px;
}
.settings-foot {
  border-top: 1px solid var(--panel-border);
  padding-top: 10px;
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
