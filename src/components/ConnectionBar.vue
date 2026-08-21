<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useConnStore } from "../stores/conn";

const store = useConnStore();

const baudrates = [
  1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
  1500000, 2000000, 3000000, 4000000,
];

const statusColor = computed(() => {
  switch (store.status) {
    case "connected":
      return "#34c759";
    case "connecting":
      return "#ff9f0a";
    case "lose":
      return "#ff3b30";
    default:
      return "var(--text-tertiary)";
  }
});

const statusText = computed(() => {
  switch (store.status) {
    case "connected":
      return "已连接";
    case "connecting":
      return "连接中...";
    case "lose":
      return "连接断开";
    default:
      return "未连接";
  }
});

onMounted(() => {
  store.refreshPorts();
  store.setupListeners();
});
</script>

<template>
  <div class="conn-bar">
    <!-- 连接类型切换 -->
    <div class="seg">
      <button
        :class="{ active: store.connType === 'serial' }"
        @click="store.connType = 'serial'"
      >
        串口
      </button>
      <button
        :class="{ active: store.connType === 'tcpudp' }"
        @click="store.connType = 'tcpudp'"
      >
        TCP/UDP
      </button>
    </div>

    <!-- 串口参数 -->
    <template v-if="store.connType === 'serial'">
      <select v-model="store.serial.port" class="ctl port-select">
        <option value="" disabled>选择串口</option>
        <option v-for="p in store.ports" :key="p.name" :value="p.name">
          {{ p.name }}{{ p.desc ? " · " + p.desc : "" }}
        </option>
      </select>
      <select v-model.number="store.serial.baudrate" class="ctl">
        <option v-for="b in baudrates" :key="b" :value="b">{{ b }}</option>
      </select>
      <select v-model.number="store.serial.data_bits" class="ctl short">
        <option :value="5">5</option>
        <option :value="6">6</option>
        <option :value="7">7</option>
        <option :value="8">8</option>
      </select>
      <select v-model="store.serial.parity" class="ctl short">
        <option value="none">None</option>
        <option value="odd">Odd</option>
        <option value="even">Even</option>
      </select>
      <select v-model.number="store.serial.stop_bits" class="ctl short">
        <option :value="1">1</option>
        <option :value="2">2</option>
      </select>
      <select v-model="store.serial.flow_control" class="ctl short">
        <option value="none">无流控</option>
        <option value="software">XON/XOFF</option>
        <option value="hardware">RTS/CTS</option>
      </select>
    </template>

    <!-- TCP/UDP 参数 -->
    <template v-else>
      <select v-model="store.tcpudp.protocol" class="ctl short">
        <option value="tcp">TCP</option>
        <option value="udp">UDP</option>
      </select>
      <select v-model="store.tcpudp.mode" class="ctl short">
        <option value="client">客户端</option>
        <option value="server">服务端</option>
      </select>
      <input
        v-if="store.tcpudp.mode === 'client'"
        v-model="store.tcpudp.target"
        class="ctl target-input"
        placeholder="目标地址 host:port"
      />
      <input
        v-else
        v-model.number="store.tcpudp.port"
        class="ctl short"
        placeholder="监听端口"
      />
    </template>

    <!-- 状态 + 开关 -->
    <div class="status-area">
      <span class="dot" :style="{ background: statusColor }"></span>
      <span class="status-text" :style="{ color: statusColor }">
        {{ statusText }}
      </span>
    </div>
    <button
      class="toggle-btn"
      :class="{ open: store.status === 'connected' }"
      @click="store.toggle()"
    >
      {{ store.status === "connected" ? "关闭" : "打开" }}
    </button>

    <div v-if="store.lastError" class="error-msg">
      {{ store.lastError }}
    </div>
    <div v-if="store.statusMsg" class="status-msg">{{ store.statusMsg }}</div>
  </div>
</template>

<style scoped>
.conn-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  background: var(--bar-bg);
  border-bottom: 1px solid rgba(0, 0, 0, 0.08);
  flex-wrap: wrap;
}

.seg {
  display: flex;
  background: var(--seg-bg);
  border-radius: 8px;
  padding: 2px;
}
.seg button {
  border: none;
  background: transparent;
  padding: 5px 14px;
  border-radius: 6px;
  font-size: 13px;
  color: var(--text-secondary);
  cursor: pointer;
}
.seg button.active {
  background: var(--control-bg);
  color: var(--text-primary);
  font-weight: 600;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.1);
}

.ctl {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 13px;
  color: var(--text-primary);
  outline: none;
  max-width: 220px;
}
.ctl:focus {
  border-color: #0a84ff;
}
.ctl.short {
  max-width: 90px;
}
.port-select {
  min-width: 180px;
}
.target-input {
  min-width: 180px;
}

.status-area {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 4px;
}
.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
}
.status-text {
  font-size: 13px;
  font-weight: 500;
}

.toggle-btn {
  border: none;
  border-radius: 8px;
  padding: 7px 22px;
  font-size: 13px;
  font-weight: 600;
  background: #0a84ff;
  color: #fff;
  cursor: pointer;
  transition: all 0.15s;
}
.toggle-btn:hover {
  background: #0a7ae0;
}
.toggle-btn.open {
  background: #ff3b30;
}
.toggle-btn.open:hover {
  background: #e0352b;
}

.error-msg {
  color: #ff3b30;
  font-size: 12px;
  flex-basis: 100%;
}
.status-msg {
  color: var(--text-secondary);
  font-size: 12px;
}
</style>
