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

const conn = useConnStore();
const rx = useRxStore();
const tx = useTxStore();
const graph = useGraphStore();

const view = ref<"debug" | "graph">("debug");
const theme = ref<"light" | "dark">("light");

// 主题切换
function toggleTheme() {
  theme.value = theme.value === "light" ? "dark" : "light";
}
watch(theme, (t) => {
  document.documentElement.dataset.theme = t;
});
function initTheme() {
  document.documentElement.dataset.theme = theme.value;
}

onMounted(() => {
  loadConfig(theme);
  initTheme();
  initPersistence(theme);
  conn.setupListeners();
  conn.refreshPorts();
  rx.setup();
  // 曲线数据：从 rx 原始字节流解析（波形自己按曲线协议解析）
  listen<{ data: number[] }>("rx-data", (e) => {
    graph.processData(new Uint8Array(e.payload.data));
  });
  window.addEventListener("beforeunload", () => tx.stopAll());
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
      <button class="theme-btn" @click="toggleTheme" :title="theme === 'light' ? '切换到深色' : '切换到浅色'">
        {{ theme === "light" ? "深色模式" : "浅色模式" }}
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
</style>
