<script setup lang="ts">
import { ref } from "vue";
import { useTxStore } from "../stores/tx";
import { useConnStore } from "../stores/conn";

const tx = useTxStore();
const conn = useConnStore();

const fileInput = ref<HTMLInputElement | null>(null);
const selectedFile = ref("");
const selectedHistory = ref("");

async function onSend() {
  await tx.send();
}

function onKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    onSend();
    return;
  }
  // ↑ 键：调出上一条历史（无输入或仅光标移动时）
  if (e.key === "ArrowUp" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
    const hist = tx.history;
    if (!hist.length) return;
    e.preventDefault();
    const idx = hist.findIndex((h) => h === tx.sendText);
    const next = idx >= 0 ? hist[(idx + 1) % hist.length] : hist[0];
    tx.sendText = next;
  }
  // ↓ 键：回退到更早/最新记录（循环）
  if (e.key === "ArrowDown" && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
    const hist = tx.history;
    if (!hist.length) return;
    e.preventDefault();
    const idx = hist.findIndex((h) => h === tx.sendText);
    if (idx <= 0) {
      tx.sendText = "";
    } else {
      tx.sendText = hist[idx - 1];
    }
  }
}

function onHistorySelect() {
  if (selectedHistory.value) {
    tx.sendText = selectedHistory.value;
    // 选中后重置，方便再次选择同一项
    selectedHistory.value = "";
  }
}

async function onSendSelected() {
  if (!selectedHistory.value) return;
  await tx.sendHistory(selectedHistory.value);
}

function onRemoveSelected() {
  if (selectedHistory.value) {
    tx.removeHistory(selectedHistory.value);
    selectedHistory.value = "";
  }
}

function pickFile() {
  fileInput.value?.click();
}

async function onFileChange(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  selectedFile.value = file.name;
  const bytes = new Uint8Array(await file.arrayBuffer());
  tx.setFileReader(async () => bytes);
  await tx.sendFile(file.name);
  input.value = "";
}

function onScheduleChange() {
  tx.updateInterval();
}
</script>

<template>
  <div class="send-panel">
    <div class="toolbar">
      <span class="title">发送</span>
      <div class="opts">
        <button
          class="opt"
          :class="{ active: tx.sendHexMode }"
          @click="tx.sendHexMode = !tx.sendHexMode"
          title="HEX 发送"
        >
          HEX
        </button>
        <button
          class="opt"
          :class="{ active: tx.escapeMode }"
          @click="tx.escapeMode = !tx.escapeMode"
          title="转义模式 \\n \\r \\xHH"
        >
          转义
        </button>
        <button
          class="opt"
          :class="{ active: tx.appendNewline }"
          @click="tx.appendNewline = !tx.appendNewline"
          title="发送时追加换行"
        >
          +换行
        </button>
        <button
          class="opt"
          :class="{ active: tx.scheduled }"
          @click="tx.toggleScheduled()"
          title="定时发送"
        >
          定时
        </button>
        <input
          v-if="tx.scheduled"
          v-model.number="tx.scheduledInterval"
          type="number"
          min="10"
          class="interval-input"
          @change="onScheduleChange"
        />
        <span v-if="tx.scheduled" class="unit">ms</span>
      </div>
    </div>

    <textarea
      v-model="tx.sendText"
      class="send-area"
      placeholder="输入要发送的内容...（Ctrl+Enter 发送）"
      @keydown="onKeydown"
    ></textarea>

    <div class="history-row" v-if="tx.history.length">
      <select
        class="history-sel"
        v-model="selectedHistory"
        @change="onHistorySelect"
      >
        <option value="">历史记录 ({{ tx.history.length }}/20)</option>
        <option v-for="h in tx.history" :key="h" :value="h">{{ h }}</option>
      </select>
      <button
        class="mini-btn"
        title="重新发送所选记录"
        :disabled="!selectedHistory || !conn.isConnected()"
        @click="onSendSelected"
      >
        发送
      </button>
      <button
        class="mini-btn"
        title="删除所选记录"
        :disabled="!selectedHistory"
        @click="onRemoveSelected"
      >
        删除
      </button>
      <button class="mini-btn" title="清空全部历史" @click="tx.clearHistory()">
        清空
      </button>
    </div>

    <div class="custom-row" v-if="tx.customItems.length">
      <div v-for="item in tx.customItems" :key="item.id" class="custom-item">
        <input v-model="item.text" class="custom-input" placeholder="快捷内容" />
        <button class="mini-btn" @click="tx.sendCustom(item.id)">发送</button>
        <button class="mini-btn danger" @click="tx.removeCustomItem(item.id)">
          ✕
        </button>
      </div>
    </div>

    <div class="actions">
      <button class="send-btn" :disabled="!conn.isConnected()" @click="onSend">
        发送
      </button>
      <button class="action-btn" @click="tx.addCustomItem()">＋快捷</button>
      <button class="action-btn" @click="pickFile">发送文件</button>
      <input
        ref="fileInput"
        type="file"
        style="display: none"
        @change="onFileChange"
      />
      <span v-if="selectedFile" class="file-name">{{ selectedFile }}</span>
    </div>
  </div>
</template>

<style scoped>
.send-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  border-left: 1px solid rgba(0, 0, 0, 0.07);
}
.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 10px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.07);
}
.title {
  font-weight: 600;
  font-size: 13px;
}
.opts {
  display: flex;
  align-items: center;
  gap: 5px;
}
.opt {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 6px;
  padding: 2px 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
  cursor: pointer;
}
.opt.active {
  background: #0a84ff;
  color: #fff;
  border-color: #0a84ff;
}
.interval-input {
  width: 60px;
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 2px 6px;
  font-size: 11.5px;
}
.unit {
  font-size: 11px;
  color: var(--text-secondary);
}
.send-area {
  flex: 1;
  min-height: 80px;
  resize: none;
  border: none;
  outline: none;
  padding: 10px 12px;
  font-family: "SF Mono", Menlo, Consolas, monospace;
  font-size: 13px;
  background: var(--control-bg);
  color: var(--text-primary);
  line-height: 1.5;
}
.history-row {
  padding: 0 10px 6px;
  display: flex;
  gap: 4px;
  align-items: center;
}
.history-sel {
  flex: 1;
  min-width: 0;
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 3px 6px;
  font-size: 12px;
  background: var(--control-bg);
  color: var(--text-primary);
}
.custom-row {
  padding: 0 10px 6px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.custom-item {
  display: flex;
  gap: 4px;
  align-items: center;
}
.custom-input {
  flex: 1;
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 3px 8px;
  font-size: 12px;
}
.mini-btn {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 6px;
  padding: 3px 8px;
  font-size: 11.5px;
  cursor: pointer;
}
.mini-btn:hover {
  border-color: #0a84ff;
  color: #0a84ff;
}
.mini-btn.danger:hover {
  border-color: #ff3b30;
  color: #ff3b30;
}
.actions {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px 10px;
  border-top: 1px solid rgba(0, 0, 0, 0.07);
}
.send-btn {
  background: #0a84ff;
  color: #fff;
  border: none;
  border-radius: 8px;
  padding: 8px 28px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
}
.send-btn:hover {
  background: #0a7ae0;
}
.send-btn:disabled {
  background: #c7c7cc;
  cursor: not-allowed;
}
.action-btn {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 8px;
  padding: 8px 14px;
  font-size: 12.5px;
  cursor: pointer;
  color: var(--text-secondary);
}
.action-btn:hover {
  border-color: #0a84ff;
  color: #0a84ff;
}
.file-name {
  font-size: 12px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 120px;
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
