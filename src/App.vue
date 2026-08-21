<script setup lang="ts">
import ConnectionBar from "./components/ConnectionBar.vue";
</script>

<template>
  <div class="app-root">
    <ConnectionBar />
    <main class="content">
      <div class="placeholder">
        <h1>串口助手 SerialAid</h1>
        <p>M1 连接层已完成：串口 + TCP/UDP 收发链路</p>
        <p class="hint">下一步：收发面板（M2）</p>
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
  overflow: hidden;
  padding: 24px;
}
.placeholder {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  background: rgba(255, 255, 255, 0.6);
  border-radius: 16px;
  border: 1px solid rgba(0, 0, 0, 0.06);
}
.placeholder h1 {
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 8px;
}
.placeholder p {
  color: #6e6e73;
  margin: 4px 0;
}
.placeholder .hint {
  margin-top: 16px;
  font-size: 13px;
  color: #0a84ff;
}
</style>
