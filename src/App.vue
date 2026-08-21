<script setup lang="ts">
import { ref, onMounted } from "vue";
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

const conn = useConnStore();
const rx = useRxStore();
const tx = useTxStore();
const graph = useGraphStore();

const view = ref<"debug" | "graph">("debug");

onMounted(() => {
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
  font-family: -apple-system, "PingFang SC", "Microsoft YaHei", "Segoe UI",
    sans-serif;
  font-size: 15px;
  color: #1d1d1f;
  background-color: #f5f5f7;
  -webkit-font-smoothing: antialiased;
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
</style>

<style scoped>
.app-root {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: linear-gradient(180deg, #eef0f4 0%, #e4e7ed 100%);
}
.view-tabs {
  display: flex;
  gap: 4px;
  padding: 8px 16px 0;
}
.view-tab {
  border: none;
  background: transparent;
  padding: 6px 18px;
  font-size: 13px;
  color: #6e6e73;
  cursor: pointer;
  border-radius: 8px 8px 0 0;
  border: 1px solid transparent;
  border-bottom: none;
}
.view-tab.active {
  background: rgba(255, 255, 255, 0.72);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  color: #1d1d1f;
  font-weight: 600;
  border-color: rgba(0, 0, 0, 0.06);
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
  background: rgba(255, 255, 255, 0.72);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border-radius: 14px;
  border: 1px solid rgba(0, 0, 0, 0.06);
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
