<script setup lang="ts">
import { onMounted } from "vue";
import ConnectionBar from "./components/ConnectionBar.vue";
import ProtocolPanel from "./components/ProtocolPanel.vue";
import ReceivePanel from "./components/ReceivePanel.vue";
import SendPanel from "./components/SendPanel.vue";
import { useConnStore } from "./stores/conn";
import { useRxStore } from "./stores/rx";
import { useTxStore } from "./stores/tx";

const conn = useConnStore();
const rx = useRxStore();
const tx = useTxStore();

onMounted(() => {
  conn.setupListeners();
  conn.refreshPorts();
  rx.setup();
  // 关闭时清理定时器
  window.addEventListener("beforeunload", () => tx.stopAll());
});
</script>

<template>
  <div class="app-root">
    <ConnectionBar />
    <ProtocolPanel />
    <main class="content">
      <div class="workbench">
        <div class="panel left">
          <ReceivePanel />
        </div>
        <div class="panel right">
          <SendPanel />
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
</style>
