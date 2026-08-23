<script setup lang="ts">
import { computed, ref } from "vue";
import { useConnStore } from "../stores/conn";

const store = useConnStore();
const showAdvanced = ref(false);
const showProfiles = ref(false);
const configLocked = computed(
  () => store.status !== "closed" || store.operationPending
);
const closing = computed(
  () => store.operationPending && store.statusMsg.includes("关闭")
);

const baudrates = [
  110, 300, 600, 1200, 2400, 4800, 9600, 14400, 19200, 28800, 31250, 38400,
  56000, 57600, 74880, 115200, 128000, 230400, 250000, 256000, 460800,
  500000, 576000, 921600, 1000000, 1500000, 2000000, 3000000, 4000000,
];

const statusColor = computed(() => {
  switch (store.status) {
    case "connected":
      return "var(--success)";
    case "connecting":
      return "var(--warning)";
    case "lose":
      return "var(--danger)";
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

</script>

<template>
  <div class="conn-bar">
    <div class="conn-main">
      <div class="seg" aria-label="连接类型">
        <button
          :class="{ active: store.connType === 'serial' }"
          :aria-pressed="store.connType === 'serial'"
          :disabled="configLocked"
          @click="store.connType = 'serial'"
        >串口</button>
        <button
          :class="{ active: store.connType === 'tcpudp' }"
          :aria-pressed="store.connType === 'tcpudp'"
          :disabled="configLocked"
          @click="store.connType = 'tcpudp'"
        >TCP/UDP</button>
      </div>

      <template v-if="store.connType === 'serial'">
        <input
          v-model="store.serial.port"
          class="ctl port-select"
          list="port-list"
          placeholder="选择或输入串口"
          aria-label="串口设备"
          maxlength="1024"
          :disabled="configLocked"
        />
        <datalist id="port-list">
          <option v-for="p in store.ports" :key="p.name" :value="p.name">{{ p.desc || "" }}</option>
        </datalist>
        <button class="opt" title="刷新串口列表" @click="store.refreshPorts()">刷新</button>
        <select
          v-model.number="store.serial.baudrate"
          class="ctl baud-select"
          aria-label="波特率"
          title="选择常用波特率；自定义数值可在“参数”中输入"
          :disabled="configLocked"
        >
          <option
            v-if="!baudrates.includes(store.serial.baudrate)"
            :value="store.serial.baudrate"
          >
            {{ store.serial.baudrate }}（自定义）
          </option>
          <option v-for="b in baudrates" :key="b" :value="b">{{ b }}</option>
        </select>
      </template>

      <template v-else>
        <select v-model="store.tcpudp.protocol" class="ctl short" aria-label="网络协议" :disabled="configLocked">
          <option value="tcp">TCP</option>
          <option value="udp">UDP</option>
        </select>
        <select v-model="store.tcpudp.mode" class="ctl short" aria-label="连接模式" :disabled="configLocked">
          <option value="client">客户端</option>
          <option value="server">服务端</option>
        </select>
        <input
          v-if="store.tcpudp.mode === 'client'"
          v-model="store.tcpudp.target"
          class="ctl target-input"
          placeholder="目标地址 host:port"
          aria-label="目标地址"
          maxlength="2048"
          list="tcp-port-list"
          :disabled="configLocked"
        />
        <input
          v-else
          v-model.number="store.tcpudp.port"
          class="ctl short"
          placeholder="监听端口"
          aria-label="监听端口"
          type="number"
          min="0"
          max="65535"
          list="listen-port-list"
          :disabled="configLocked"
        />
        <datalist id="tcp-port-list">
          <option value="127.0.0.1:2345" />
          <option value="127.0.0.1:8080" />
          <option value="192.168.1.100:8080" />
        </datalist>
        <datalist id="listen-port-list">
          <option value="8080" />
          <option value="8081" />
          <option value="2345" />
          <option value="9000" />
        </datalist>
      </template>

      <button
        class="opt"
        :class="{ active: showAdvanced }"
        :aria-expanded="showAdvanced"
        @click="showAdvanced = !showAdvanced"
      >参数</button>
      <button
        class="opt"
        :class="{ active: showProfiles }"
        :aria-expanded="showProfiles"
        @click="showProfiles = !showProfiles"
      >收藏</button>

      <div class="main-spacer"></div>
      <div class="status-area" role="status" aria-live="polite">
        <span class="dot" :style="{ background: statusColor }"></span>
        <span class="status-text" :style="{ color: statusColor }">{{ statusText }}</span>
      </div>
      <button
        class="toggle-btn"
        :class="{ open: store.status === 'connected' || store.status === 'lose', connecting: store.status === 'connecting' }"
        :disabled="closing"
        @click="store.toggle()"
      >{{ store.status === "connected" || store.status === "lose" ? "关闭" : closing ? "关闭中..." : store.status === "connecting" ? "取消连接" : "打开" }}</button>
    </div>

    <div v-if="showAdvanced" class="conn-details" aria-label="高级连接参数">
      <template v-if="store.connType === 'serial'">
        <label>自定义波特率
          <input
            v-model.number="store.serial.baudrate"
            class="ctl baud-input"
            type="number"
            min="1"
            max="10000000"
            :disabled="configLocked"
          />
        </label>
        <label>数据位
          <select v-model.number="store.serial.data_bits" class="ctl short" :disabled="configLocked">
            <option v-for="bits in [5, 6, 7, 8]" :key="bits" :value="bits">{{ bits }}</option>
          </select>
        </label>
        <label>校验
          <select v-model="store.serial.parity" class="ctl short" :disabled="configLocked">
            <option value="none">None</option><option value="odd">Odd</option><option value="even">Even</option>
          </select>
        </label>
        <label>停止位
          <select v-model.number="store.serial.stop_bits" class="ctl short" :disabled="configLocked">
            <option :value="1">1</option><option :value="2">2</option>
          </select>
        </label>
        <label>流控
          <select v-model="store.serial.flow_control" class="ctl flow-select" :disabled="configLocked">
            <option value="none">无流控</option><option value="software">XON/XOFF</option><option value="hardware">RTS/CTS</option>
          </select>
        </label>
        <label class="chk"><input type="checkbox" v-model="store.serial.auto_reconnect" :disabled="configLocked" /> 自动重连</label>
        <label class="chk"><input type="checkbox" v-model="store.serial.rts" :disabled="configLocked" /> RTS</label>
        <label class="chk"><input type="checkbox" v-model="store.serial.dtr" :disabled="configLocked" /> DTR</label>
      </template>
      <template v-else-if="store.tcpudp.protocol === 'tcp' && store.tcpudp.mode === 'client'">
        <label class="chk"><input type="checkbox" v-model="store.tcpudp.auto_reconnect" :disabled="configLocked" /> 自动重连</label>
        <label class="tcp-interval">重连间隔
          <input v-model.number="store.tcpudp.reconnect_interval" class="ctl reconnect-input" type="number" min="0.1" step="0.1" :disabled="configLocked" /> 秒
        </label>
      </template>
      <template v-else-if="store.tcpudp.protocol === 'udp' && store.tcpudp.mode === 'client'">
        <label class="tcp-interval">本地端口
          <input v-model.number="store.tcpudp.local_port" class="ctl reconnect-input" type="number" min="0" max="65535" :disabled="configLocked" />
          <span>0=自动</span>
        </label>
      </template>
      <span v-else class="detail-hint">当前模式没有额外连接参数</span>
    </div>

    <div v-if="showProfiles" class="conn-details profile-box">
      <select
        class="ctl profile-sel"
        :value="''"
        aria-label="应用连接收藏"
        :disabled="configLocked"
        @change="(e: Event) => { const el = e.target as HTMLSelectElement; if (el.value) { store.applyProfile(el.value); el.value = ''; } }"
      >
        <option value="" disabled>选择连接收藏</option>
        <option v-for="p in store.profiles" :key="p.name" :value="p.name">{{ p.name }}</option>
      </select>
      <input v-model="store.profileName" class="ctl profile-input" maxlength="100" placeholder="新收藏名称" @keyup.enter="store.saveProfile()" />
      <button class="opt" :disabled="!store.profileName.trim()" @click="store.saveProfile()">保存当前配置</button>
    </div>

    <div v-if="store.lastError" class="error-msg" role="alert">
      <span>{{ store.lastError }}</span>
      <details v-if="store.lastErrorDetail && store.lastErrorDetail !== store.lastError">
        <summary>详情</summary>
        <code>{{ store.lastErrorDetail }}</code>
      </details>
    </div>
    <div v-if="store.statusMsg" class="status-msg">{{ store.statusMsg }}</div>
  </div>
</template>

<style scoped>
.conn-bar {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 16px;
  background: var(--bar-bg);
  border-bottom: 1px solid var(--panel-border);
}
.conn-main,
.conn-details {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
}
.conn-main {
  min-width: 0;
}
.main-spacer {
  flex: 1;
  min-width: 8px;
}
.conn-details {
  flex-wrap: wrap;
  padding-top: 6px;
  border-top: 1px solid var(--panel-border);
}
.conn-details > label:not(.chk) {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--text-secondary);
  font-size: 12px;
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
  border-color: var(--accent);
}
.ctl.short {
  max-width: 90px;
}
.baud-select {
  width: 128px;
  max-width: 128px;
}
.baud-input {
  width: 108px;
  max-width: 108px;
}
.chk {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  white-space: nowrap;
}
.chk input {
  accent-color: var(--accent);
}
.opt {
  border: 1px solid var(--btn-border);
  background: var(--btn-bg);
  border-radius: var(--radius-md);
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
}
.opt:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
}
.opt.active {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent);
}
.port-select {
  min-width: 180px;
}
.tcp-interval {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.reconnect-input {
  width: 62px;
  max-width: 62px;
  padding: 4px 6px;
}
.profile-box {
  justify-content: flex-start;
}
.profile-sel {
  max-width: 130px;
}
.profile-input {
  max-width: 100px;
}
.target-input {
  min-width: 180px;
}

.status-area {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-left: 4px;
  white-space: nowrap;
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
  background: var(--btn-primary-bg);
  color: #fff;
  cursor: pointer;
  transition: background 0.15s ease;
}
.toggle-btn:hover {
  background: var(--btn-primary-hover);
}
.toggle-btn.open {
  background: var(--btn-danger-bg);
}
.toggle-btn.connecting {
  background: var(--btn-warning-bg);
}
.toggle-btn.connecting:hover {
  filter: brightness(0.92);
}
.toggle-btn.open:hover {
  background: var(--btn-danger-hover);
}

.error-msg {
  color: var(--danger);
  font-size: 12px;
  width: 100%;
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.error-msg details {
  color: var(--text-secondary);
}
.error-msg summary {
  cursor: pointer;
}
.error-msg code {
  display: block;
  margin-top: 4px;
  max-width: min(760px, calc(100vw - 48px));
  overflow-wrap: anywhere;
}
.status-msg {
  color: var(--text-secondary);
  font-size: 12px;
  width: 100%;
}
.flow-select {
  min-width: 105px;
}
.detail-hint {
  color: var(--text-tertiary);
  font-size: 12px;
}

@media (max-width: 980px) {
  .conn-bar {
    padding-inline: 10px;
  }
  .port-select,
  .target-input {
    min-width: 150px;
  }
  .status-text {
    display: none;
  }
  .toggle-btn {
    padding-inline: 16px;
  }
}
</style>
