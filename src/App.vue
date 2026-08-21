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
        {{ theme === "light" ? "🌙" : "☀️" }}
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
  --bg-gradient-1: #eef0f4;
  --bg-gradient-2: #e4e7ed;
  --panel-bg: rgba(255, 255, 255, 0.72);
  --panel-border: rgba(0, 0, 0, 0.06);
  --text-primary: #1d1d1f;
  --text-secondary: #6e6e73;
  --text-tertiary: #98989d;
  --control-bg: #ffffff;
  --control-border: rgba(0, 0, 0, 0.12);
  --control-text: #1d1d1f;
  --accent: #0a84ff;
  --accent-hover: #0a7ae0;
  --danger: #ff3b30;
  --row-tx-bg: rgba(10, 132, 255, 0.04);
  --row-border: rgba(0, 0, 0, 0.03);
  --seg-bg: rgba(0, 0, 0, 0.06);
  --seg-active-bg: #fff;
  --bar-bg: rgba(255, 255, 255, 0.65);
  --edit-bg: rgba(0, 0, 0, 0.03);

  font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI",
    sans-serif;
  font-size: 15px;
  color: var(--text-primary);
  background-color: #f5f5f7;
  -webkit-font-smoothing: antialiased;
}

:root[data-theme="dark"] {
  --bg-gradient-1: #1c1c1e;
  --bg-gradient-2: #161618;
  --panel-bg: rgba(44, 44, 46, 0.78);
  --panel-border: rgba(255, 255, 255, 0.08);
  --text-primary: #f5f5f7;
  --text-secondary: #98989d;
  --text-tertiary: #636366;
  --control-bg: #2c2c2e;
  --control-border: rgba(255, 255, 255, 0.14);
  --control-text: #f5f5f7;
  --accent: #0a84ff;
  --accent-hover: #409cff;
  --danger: #ff453a;
  --row-tx-bg: rgba(10, 132, 255, 0.12);
  --row-border: rgba(255, 255, 255, 0.04);
  --seg-bg: rgba(255, 255, 255, 0.08);
  --seg-active-bg: #48484a;
  --bar-bg: rgba(28, 28, 30, 0.72);
  --edit-bg: rgba(255, 255, 255, 0.05);
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
</style>

<style scoped>
.app-root {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: linear-gradient(
    180deg,
    var(--bg-gradient-1) 0%,
    var(--bg-gradient-2) 100%
  );
}
.view-tabs {
  display: flex;
  gap: 4px;
  padding: 8px 16px 0;
  align-items: center;
}
.tab-spacer {
  flex: 1;
}
.theme-btn {
  border: 1px solid var(--panel-border);
  background: var(--panel-bg);
  border-radius: 8px;
  padding: 4px 10px;
  font-size: 14px;
  cursor: pointer;
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
}
.view-tab {
  border: none;
  background: transparent;
  padding: 6px 18px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
  border-radius: 8px 8px 0 0;
  border: 1px solid transparent;
  border-bottom: none;
}
.view-tab.active {
  background: var(--panel-bg);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  color: var(--text-primary);
  font-weight: 600;
  border-color: var(--panel-border);
}
.content {
  flex: 1;
  min-height: 0;
  padding: 0 12px 12px;
}
.workbench {
  height: 100%;
  display: flex;
  gap: 12px;
}
.panel {
  background: var(--panel-bg);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border-radius: 14px;
  border: 1px solid var(--panel-border);
  box-shadow: 0 4px 24px rgba(0, 0, 0, 0.06);
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
