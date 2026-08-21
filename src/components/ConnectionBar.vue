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

/* 玻璃质感（继承全局控件体系） */
.tool-btn, .opt, .mini, .action-btn, .theme-btn {
  background: var(--btn-glass-bg);
  border: 1px solid var(--btn-glass-border);
  box-shadow: var(--btn-glass-highlight), var(--btn-glass-shadow);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease,
    color 0.15s ease, box-shadow 0.15s ease, transform 0.1s ease;
}
.tool-btn:hover, .opt:hover, .mini:hover, .action-btn:hover, .theme-btn:hover {
  background: var(--btn-glass-hover);
  color: var(--text-primary);
  border-color: var(--field-border-hover);
}
.tool-btn:active, .opt:active, .mini:active, .action-btn:active, .theme-btn:active {
  transform: translateY(0.5px);
}
.tool-btn.active, .opt.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
  box-shadow: 0 2px 8px rgba(10, 132, 255, 0.35);
}
.tool-btn.danger:hover, .mini.danger:hover, .action-btn.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
}
.tool-btn:disabled, .mini:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 输入类控件统一玻璃 */
.ctl, .enc-sel, .history-sel, .custom-input, .interval-input,
.new-name, .field input, .field select, .range-input, .header-input,
.tpl-sel, .target-input, .port-select {
  background: var(--field-bg);
  border: 1px solid var(--field-border);
  box-shadow: var(--field-inner-shadow);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  outline: none;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
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
  box-shadow: var(--field-inner-shadow), var(--field-focus-ring);
}


.toggle-btn, .send-btn {
  background: var(--btn-primary-bg);
  border: none;
  box-shadow: var(--btn-primary-shadow);
  color: #fff;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: filter 0.15s ease, transform 0.1s ease, box-shadow 0.15s ease;
}
.toggle-btn:hover, .send-btn:hover {
  filter: brightness(1.08);
}
.toggle-btn:active, .send-btn:active {
  transform: translateY(0.5px);
  box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.18);
}
.toggle-btn:disabled, .send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  box-shadow: none;
}
.toggle-btn.open {
  background: var(--btn-danger-bg);
  box-shadow: var(--btn-danger-shadow);
}
.toggle-btn.open:hover {
  filter: brightness(1.08);
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
