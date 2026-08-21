<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from "vue";
import { useTxStore } from "../stores/tx";
import { useConnStore } from "../stores/conn";

const tx = useTxStore();
const conn = useConnStore();
const txMoreOpen = ref(false); // 工具栏更多菜单
const txActMoreOpen = ref(false); // 操作区更多菜单

// 点击菜单外部时关闭
function onDocClick(e: MouseEvent) {
  if (
    (txMoreOpen.value || txActMoreOpen.value) &&
    !(e.target as HTMLElement).closest?.(".more-wrap")
  ) {
    txMoreOpen.value = false;
    txActMoreOpen.value = false;
  }
}
onMounted(() => document.addEventListener("click", onDocClick));
onBeforeUnmount(() => document.removeEventListener("click", onDocClick));

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

/** 快捷条删除确认 */
function onRemoveCustom(id: number) {
  const item = tx.customItems.find((i) => i.id === id);
  if (item && confirm(`确定删除快捷项「${item.text || "(空)"}」？`)) {
    tx.removeCustomItem(id);
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
        <input
          v-if="tx.scheduled"
          v-model.number="tx.scheduledInterval"
          type="number"
          min="10"
          class="interval-input"
          @change="onScheduleChange"
        />
        <span v-if="tx.scheduled" class="unit">ms</span>
        <div class="more-wrap">
          <button
            class="opt"
            :class="{ active: txMoreOpen }"
            @click="txMoreOpen = !txMoreOpen"
            title="更多选项"
          >
            ⋯
          </button>
          <div v-if="txMoreOpen" class="more-menu">
            <button
              class="more-item"
              :class="{ on: tx.appendNewline }"
              @click="tx.appendNewline = !tx.appendNewline; txMoreOpen = false"
            >
              +换行 {{ tx.appendNewline ? "✓" : "" }}
            </button>
            <button
              class="more-item"
              :class="{ on: tx.scheduled }"
              @click="tx.toggleScheduled(); txMoreOpen = false"
            >
              定时发送 {{ tx.scheduled ? "✓" : "" }}
            </button>
          </div>
        </div>
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
        <button class="mini-btn danger" @click="onRemoveCustom(item.id)">
          ✕
        </button>
      </div>
    </div>

    <div class="actions">
      <button class="send-btn" :disabled="!conn.isConnected()" @click="onSend">
        发送
      </button>
      <div class="more-wrap">
        <button
          class="action-btn"
          :class="{ active: txActMoreOpen }"
          @click="txActMoreOpen = !txActMoreOpen"
          title="更多发送选项"
        >
          ⋯
        </button>
        <div v-if="txActMoreOpen" class="more-menu">
          <button
            class="more-item"
            :disabled="!tx.sendText.trim()"
            @click="tx.addCustomItem(tx.sendText); txActMoreOpen = false"
          >
            ＋存入快捷
          </button>
          <button
            class="more-item"
            @click="tx.addCustomItem(); txActMoreOpen = false"
          >
            ＋快捷
          </button>
          <button class="more-item" @click="pickFile(); txActMoreOpen = false">
            发送文件
          </button>
        </div>
      </div>
      <span v-if="selectedFile" class="file-name">{{ selectedFile }}</span>
      <input
        ref="fileInput"
        type="file"
        style="display: none"
        @change="onFileChange"
      />
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
  flex-wrap: wrap;
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
  gap: 6px;
  align-items: center;
  flex-wrap: wrap;
  padding: 8px 10px;
  border-top: 1px solid rgba(0, 0, 0, 0.07);
}
.send-btn {
  background: #0a84ff;
  color: #fff;
  border: none;
  border-radius: 8px;
  padding: 8px 20px;
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
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
  padding: 8px 9px;
  font-size: 12.5px;
  cursor: pointer;
  color: var(--text-secondary);
  white-space: nowrap;
  flex-shrink: 0;
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
</style>
