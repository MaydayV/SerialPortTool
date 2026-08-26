<script setup lang="ts">
import {
  ref,
  onMounted,
  onBeforeUnmount,
  watch,
  defineAsyncComponent,
  nextTick,
} from "vue";
import ConnectionBar from "./components/ConnectionBar.vue";
import ProtocolPanel from "./components/ProtocolPanel.vue";
import AiControlPanel from "./components/AiControlPanel.vue";
import ReceivePanel from "./components/ReceivePanel.vue";
import SendPanel from "./components/SendPanel.vue";
const loadGraphPanel = () => import("./components/GraphPanel.vue");
const GraphPanel = defineAsyncComponent(loadGraphPanel);
import { useConnStore } from "./stores/conn";
import { useRxStore } from "./stores/rx";
import { useTxStore } from "./stores/tx";
import { useGraphStore } from "./stores/graph";
import { useAiControlStore } from "./stores/aiControl";
import { clearConfig, loadConfig, initPersistence } from "./stores/persist";
import { formatTime } from "./utils/bytes";
import { saveTextFile } from "./utils/save";
import { api } from "./api";
import { isTauri } from "@tauri-apps/api/core";
import { setupMcpFrontendBridge } from "./mcpFrontendBridge";

const conn = useConnStore();
const rx = useRxStore();
const tx = useTxStore();
const graph = useGraphStore();
const ai = useAiControlStore();
let teardownMcpBridge: (() => void) | null = null;

const view = ref<"debug" | "graph">("debug");
const theme = ref<"light" | "dark" | "system">("light");

// 设置面板
const showSettings = ref(false);
const settingsSection = ref<"general" | "mcp">("general");
const settingsPanel = ref<HTMLDivElement | null>(null);
let settingsReturnFocus: HTMLElement | null = null;

const splitRatio = ref(0.72);
const workbenchEl = ref<HTMLDivElement | null>(null);
let resizing = false;

/** 解析生效主题（system → 跟随系统） */
function effectiveTheme(): "light" | "dark" {
  if (theme.value !== "system") return theme.value;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

// 主题切换（浅色 ↔ 深色；system 时优先切到深色）
watch(theme, () => {
  document.documentElement.dataset.theme = effectiveTheme();
});
watch(
  view,
  (next) => graph.setViewActive(next === "graph"),
  { immediate: true }
);
watch([() => rx.saveLog, () => rx.logPath], () => {
  // 关闭日志或切换路径时及时落盘旧缓冲。
  if (rx.saveLog && !rx.logPath.trim()) rx.saveLog = false;
  void api.flushLogFiles().catch(() => {});
});
// 窗口标题同步连接状态
const statusTitle: Record<string, string> = {
  connected: "● 已连接",
  connecting: "◐ 连接中",
  lose: "○ 已断开",
  closed: "",
};
watch(
  () => conn.status,
  (s) => {
    document.title = statusTitle[s] ? `${statusTitle[s]} - 串口助手 SerialPortTool` : "串口助手 SerialPortTool";
  },
  { immediate: true }
);
let sysMedia: MediaQueryList | null = null;
function onSystemThemeChange() {
  if (theme.value === "system") {
    document.documentElement.dataset.theme = effectiveTheme();
  }
}
function initTheme() {
  document.documentElement.dataset.theme = effectiveTheme();
  if (!sysMedia) {
    sysMedia = window.matchMedia("(prefers-color-scheme: dark)");
    sysMedia.addEventListener("change", onSystemThemeChange);
  }
}

function openSettings() {
  settingsReturnFocus = document.activeElement as HTMLElement | null;
  showSettings.value = true;
  void nextTick(() => settingsPanel.value?.focus());
}

function closeSettings() {
  showSettings.value = false;
  void nextTick(() => settingsReturnFocus?.focus());
}

function onSettingsKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    closeSettings();
    return;
  }
  if (e.key !== "Tab" || !settingsPanel.value) return;
  const focusable = Array.from(
    settingsPanel.value.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )
  );
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (
    e.shiftKey &&
    (document.activeElement === first || document.activeElement === settingsPanel.value)
  ) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

function beginResize(e: PointerEvent) {
  resizing = true;
  (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  updateSplit(e.clientX);
}

function updateSplit(clientX: number) {
  const rect = workbenchEl.value?.getBoundingClientRect();
  if (!rect || rect.width <= 0) return;
  splitRatio.value = Math.min(0.78, Math.max(0.55, (clientX - rect.left) / rect.width));
}

function onSplitterMove(e: PointerEvent) {
  if (resizing) updateSplit(e.clientX);
}

function endResize() {
  resizing = false;
}

function onSplitterKeydown(e: KeyboardEvent) {
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  e.preventDefault();
  const delta = e.key === "ArrowLeft" ? -0.02 : 0.02;
  splitRatio.value = Math.min(0.78, Math.max(0.55, splitRatio.value + delta));
}

/** 重置全部配置并刷新 */
function resetAll() {
  if (confirm("确定恢复默认设置？所有配置（连接参数、模板、历史）将被清除。")) {
    clearConfig();
    location.reload();
  }
}

/** 导出接收区日志 */
async function exportLog() {
  const lines = rx.entries.map((e) => {
    const t = rx.showTimestamp ? `[${formatTime(e.ts)}] ` : "";
    const dir = e.dir === "rx" ? "<=" : "=>";
    const source = e.peer ? `[${e.peer}] ` : "";
    const body = rx.dualMode
      ? `${rx.getEntryHex(e)} | ${rx.getEntryAscii(e)}`
      : rx.rxHexMode
        ? rx.getEntryHex(e)
        : rx.asciiMode
          ? rx.getEntryAscii(e)
          : rx.getEntryText(e);
    return `${dir} ${t}${source}${body}`;
  });
  if (!lines.length) {
    alert("接收区为空，无日志可导出");
    return;
  }
  try {
    await saveTextFile(
      "log",
      "\ufeff" + lines.join("\n"),
      `serialporttool-log-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.log`,
      "text/plain;charset=utf-8"
    );
  } catch (error) {
    alert(`导出失败：${error instanceof Error ? error.message : String(error)}`);
  }
}

async function chooseLogPath() {
  if (!isTauri()) {
    alert("持续写入日志需要在桌面应用中使用");
    return;
  }
  rx.saveLog = false;
  await api.flushLogFiles().catch(() => {});
  try {
    const path = await api.selectOutputFile("log");
    if (path) {
      rx.logPath = path;
      rx.logError = "";
    }
  } catch (error) {
    rx.logError = error instanceof Error ? error.message : String(error);
  }
}

function clearReceived() {
  if (isTauri()) {
    void api.connClearReceived().catch(() => rx.clear());
  } else {
    rx.clear();
  }
}

// 全局快捷键（非输入框焦点时生效）
function onGlobalKeydown(e: KeyboardEvent) {
  if (showSettings.value) {
    if (e.key === "Escape") {
      e.preventDefault();
      closeSettings();
    }
    return;
  }
  // 输入框/文本域/select 内不拦截（保留原生行为）
  const target = e.target as HTMLElement | null;
  const tag = target?.tagName;
  if (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    tag === "BUTTON" ||
    tag === "A" ||
    target?.isContentEditable
  ) return;
  if (!e.ctrlKey && !e.metaKey && !e.altKey) {
    switch (e.key.toLowerCase()) {
      case "f5":
        // 刷新串口列表
        e.preventDefault();
        conn.refreshPorts();
        break;
      case "escape":
        // 清空接收区
        e.preventDefault();
        clearReceived();
        break;
      case "h":
        // 切换 HEX 显示，并确保与 ASCII 互斥
        rx.rxHexMode = !rx.rxHexMode;
        rx.asciiMode = false;
        rx.dualMode = false;
        break;
      case "t":
        // 切换时间戳
        rx.showTimestamp = !rx.showTimestamp;
        break;
      case "p":
        // 暂停/继续接收
        rx.togglePause();
        break;
      case " ":
        // 空格开关连接
        e.preventDefault();
        conn.toggle();
        break;
    }
  }
}

// 主题切换
onMounted(() => {
  loadConfig(theme);
  initTheme();
  initPersistence(theme);
  rx.startRateTimer();
  if (isTauri()) {
    void ai.setupListeners().catch((error) => {
      ai.lastError = `MCP 监听初始化失败：${String(error)}`;
    });
    void setupMcpFrontendBridge().then((teardown) => {
      teardownMcpBridge = teardown;
    });
    void conn.setupListeners().catch((error) => {
      conn.lastError = `事件监听初始化失败：${String(error)}`;
    });
    void conn.refreshPorts();
    void rx.setup().catch((error) => {
      rx.logError = `接收监听初始化失败：${String(error)}`;
    });
  }
  const MAX_LOG_QUEUE = 4096;
  const MAX_LOG_QUEUE_CHARS = 8 * 1024 * 1024;
  const LOG_BATCH_SIZE = 64;
  let logQueue: { path: string; line: string }[] = [];
  let logQueueHead = 0;
  let queuedLogChars = 0;
  let inFlightLogChars = 0;
  let logWriting = false;

  async function flushLogQueue() {
    if (logWriting) return;
    logWriting = true;
    try {
      while (logQueueHead < logQueue.length && rx.saveLog) {
        const first = logQueue[logQueueHead];
        const batch: string[] = [];
        while (
          batch.length < LOG_BATCH_SIZE &&
          logQueueHead < logQueue.length &&
          logQueue[logQueueHead].path === first.path
        ) {
          const item = logQueue[logQueueHead++];
          queuedLogChars -= item.line.length;
          batch.push(item.line);
        }
        const batchText = batch.join("");
        inFlightLogChars += batchText.length;
        try {
          await api.appendLogFile(first.path, batchText);
        } finally {
          inFlightLogChars -= batchText.length;
        }
        if (logQueueHead > 1024 && logQueueHead * 2 > logQueue.length) {
          logQueue = logQueue.slice(logQueueHead);
          logQueueHead = 0;
        }
      }
      if (!rx.saveLog) {
        logQueue = [];
        logQueueHead = 0;
        queuedLogChars = 0;
        inFlightLogChars = 0;
        await api.flushLogFiles();
      }
    } catch (error) {
      logQueue = [];
      logQueueHead = 0;
      queuedLogChars = 0;
      inFlightLogChars = 0;
      rx.logError = error instanceof Error ? error.message : String(error);
      rx.saveLog = false;
    } finally {
      logWriting = false;
      if (logQueueHead < logQueue.length && rx.saveLog) void flushLogQueue();
    }
  }

  rx.setLogWriter((line) => {
    const path = rx.logPath.trim();
    if (!rx.saveLog || !path) return;
    if (line.length > MAX_LOG_QUEUE_CHARS) {
      rx.logError = "单条日志超过大小上限，已丢弃";
      return;
    }
    if (inFlightLogChars + line.length > MAX_LOG_QUEUE_CHARS) {
      rx.logError = "日志写入速度不足，已丢弃新日志";
      return;
    }
    while (
      logQueue.length - logQueueHead >= MAX_LOG_QUEUE ||
      queuedLogChars + inFlightLogChars + line.length > MAX_LOG_QUEUE_CHARS
    ) {
      const dropped = logQueue[logQueueHead++];
      if (!dropped) break;
      queuedLogChars -= dropped.line.length;
      rx.logError = "日志写入速度不足，已丢弃部分旧日志";
    }
    logQueue.push({ path, line });
    queuedLogChars += line.length;
    void flushLogQueue();
  });
  window.addEventListener("keydown", onGlobalKeydown);
  window.addEventListener("pointermove", onSplitterMove);
  window.addEventListener("pointerup", endResize);
  window.addEventListener("beforeunload", onBeforeUnload);
});

function onBeforeUnload() {
  tx.stopAll();
  rx.stopRateTimer();
  void api.flushLogFiles().catch(() => {});
}

function renameProfile(name: string) {
  const next = prompt("新的收藏名称", name);
  if (next === null || next.trim() === name) return;
  if (!conn.renameProfile(name, next)) alert("重命名失败：名称为空或已经存在");
}

function removeProfile(name: string) {
  if (confirm(`确定删除连接收藏「${name}」？`)) conn.removeProfile(name);
}

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onGlobalKeydown);
  window.removeEventListener("pointermove", onSplitterMove);
  window.removeEventListener("pointerup", endResize);
  window.removeEventListener("beforeunload", onBeforeUnload);
  sysMedia?.removeEventListener("change", onSystemThemeChange);
  graph.setViewActive(false);
  teardownMcpBridge?.();
  teardownMcpBridge = null;
  conn.teardownListeners();
  rx.teardown();
  ai.teardown();
  onBeforeUnload();
});
</script>

<template>
  <div class="app-root">
    <ConnectionBar />
    <ProtocolPanel />
    <nav class="view-tabs" role="tablist" aria-label="工作区视图">
      <button
        class="view-tab"
        :class="{ active: view === 'debug' }"
        role="tab"
        :aria-selected="view === 'debug'"
        @click="view = 'debug'"
      >
        收发
      </button>
      <button
        class="view-tab"
        :class="{ active: view === 'graph' }"
        role="tab"
        :aria-selected="view === 'graph'"
        @pointerenter="loadGraphPanel"
        @focus="loadGraphPanel"
        @click="view = 'graph'"
      >
        波形
      </button>
      <div class="tab-spacer"></div>
      <span class="kbd-hint" title="全局快捷键：空格=开关连接 · H=HEX · T=时间戳 · P=暂停 · Esc=清空 · F5=刷新串口">
        空格 连接 · H HEX · P 暂停 · Esc 清空
      </span>
      <button class="theme-btn settings-entry" @click="openSettings" title="设置">
        设置
        <span v-if="ai.pendingApprovals.length" class="settings-badge" aria-label="有待处理的 MCP 审批">
          {{ ai.pendingApprovals.length }}
        </span>
      </button>
    </nav>
    <main class="content">
      <div
        v-if="view === 'debug'"
        ref="workbenchEl"
        class="workbench debug-workbench"
        :style="{
          gridTemplateColumns: `${splitRatio * 100}fr 6px ${(1 - splitRatio) * 100}fr`,
        }"
      >
        <div class="panel left">
          <ReceivePanel />
        </div>
        <button
          class="splitter"
          role="separator"
          aria-label="调整收发区域宽度"
          aria-orientation="vertical"
          :aria-valuenow="Math.round(splitRatio * 100)"
          :aria-valuemin="55"
          :aria-valuemax="78"
          @pointerdown="beginResize"
          @keydown="onSplitterKeydown"
        ></button>
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

    <!-- 设置面板 -->
    <div
      v-if="showSettings"
      class="settings-overlay"
      @click.self="closeSettings"
      @keydown="onSettingsKeydown"
    >
      <div
        ref="settingsPanel"
        class="settings-panel"
        :class="{ 'mcp-settings-panel': settingsSection === 'mcp' }"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        tabindex="-1"
      >
        <div class="settings-head">
          <span id="settings-title" class="settings-title">设置</span>
          <button class="mini-btn" @click="closeSettings">✕ 关闭</button>
        </div>
        <div class="settings-tabs" role="tablist" aria-label="设置分类">
          <button
            class="settings-tab"
            :class="{ active: settingsSection === 'general' }"
            role="tab"
            :aria-selected="settingsSection === 'general'"
            @click="settingsSection = 'general'"
          >
            常规
          </button>
          <button
            class="settings-tab"
            :class="{ active: settingsSection === 'mcp' }"
            role="tab"
            :aria-selected="settingsSection === 'mcp'"
            @click="settingsSection = 'mcp'"
          >
            MCP 与 AI
            <span v-if="ai.pendingApprovals.length" class="settings-tab-badge">{{ ai.pendingApprovals.length }}</span>
          </button>
        </div>
        <div v-if="settingsSection === 'general'" class="settings-content">
        <div class="setting-row">
          <span class="setting-label">主题</span>
          <div class="seg">
            <button
              :class="{ active: theme === 'light' }"
              :aria-pressed="theme === 'light'"
              @click="theme = 'light'"
            >
              浅色
            </button>
            <button
              :class="{ active: theme === 'dark' }"
              :aria-pressed="theme === 'dark'"
              @click="theme = 'dark'"
            >
              深色
            </button>
            <button
              :class="{ active: theme === 'system' }"
              :aria-pressed="theme === 'system'"
              @click="theme = 'system'"
            >
              跟随系统
            </button>
          </div>
        </div>
        <div class="setting-row">
          <span class="setting-label">接收字号</span>
          <input
            v-model.number="rx.fontSize"
            type="range"
            min="10"
            max="20"
            step="0.5"
            class="range-slider"
          />
          <span class="setting-val">{{ rx.fontSize }}px</span>
        </div>
        <div class="setting-row col">
          <span class="setting-label">持续日志</span>
          <label class="log-toggle">
            <input type="checkbox" v-model="rx.saveLog" :disabled="!rx.logPath.trim()" />
            写入文件
          </label>
          <div class="log-path-row">
            <input
              :value="rx.logPath"
              class="log-path-input"
              readonly
              placeholder="尚未选择日志文件"
              title="日志文件必须通过系统保存对话框选择"
            />
            <button class="mini-btn" @click="chooseLogPath">选择文件…</button>
          </div>
          <span v-if="rx.logError" class="log-error">{{ rx.logError }}</span>
          <span class="settings-hint">记录实际线上原始 HEX；每次启动需重新选择文件并明确开启</span>
        </div>
        <div class="setting-row">
          <span class="setting-label">数据</span>
          <div class="setting-actions">
            <button class="mini-btn" @click="exportLog">导出接收日志</button>
            <button class="mini-btn danger" @click="resetAll">恢复默认设置</button>
          </div>
        </div>
        <div v-if="conn.profiles.length" class="setting-row col">
          <span class="setting-label">连接收藏</span>
          <div class="profile-list">
            <div v-for="p in conn.profiles" :key="p.name" class="profile-item">
              <span class="profile-name">{{ p.name }}</span>
              <span class="profile-detail">
                {{ p.connType === "serial" ? `串口 ${p.serial.port} @ ${p.serial.baudrate}` : `${p.tcpudp.protocol.toUpperCase()} ${p.tcpudp.mode} ${p.tcpudp.mode === "client" ? p.tcpudp.target : ":" + p.tcpudp.port}` }}
              </span>
              <button class="mini-btn danger" @click="removeProfile(p.name)">删除</button>
              <button class="mini-btn" @click="renameProfile(p.name)">重命名</button>
            </div>
          </div>
        </div>
        </div>
        <div v-else class="settings-content mcp-settings-content">
          <AiControlPanel />
        </div>
        <div class="settings-foot">
          设置自动保存 · 快捷键：空格 连接 · H HEX · T 时间戳 · P 暂停 · Esc
          清空 · F5 刷新串口
        </div>
      </div>
    </div>
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
  --text-tertiary: #6b7280;
  --control-bg: #ffffff;
  --control-border: #d4d6da;
  --control-text: #1f2328;
  --accent: #0969da;
  --accent-hover: #0757b5;
  --accent-soft: rgba(9, 105, 218, 0.12);
  --danger: #cf222e;
  --danger-hover: #a40e26;
  --success: #1a7f37;
  --row-tx-bg: #f0f6ff;
  --row-border: #f0f1f3;
  --seg-bg: #e9ebee;
  --seg-active-bg: #ffffff;
  --bar-bg: #ffffff;
  --edit-bg: #f7f8fa;
  --chart-grid: #e2e4e8;
  --warning: #9a5b00;
  --warning-soft: #fff4dd;

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
  --btn-primary-bg: #0969da;
  --btn-primary-hover: #0757b5;
  --btn-danger-bg: #cf222e;
  --btn-danger-hover: #a40e26;
  --btn-warning-bg: #9a6700;
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
  --text-tertiary: #929292;
  --control-bg: #333333;
  --control-border: #4a4a4a;
  --control-text: #e0e0e0;
  --accent: #58a6ff;
  --accent-hover: #79c0ff;
  --accent-soft: rgba(88, 166, 255, 0.18);
  --danger: #ff7b72;
  --danger-hover: #ffa198;
  --success: #56d364;
  --row-tx-bg: #1d2b3d;
  --row-border: #2d2d2d;
  --seg-bg: #2d2d2d;
  --seg-active-bg: #3c3c3c;
  --bar-bg: #252526;
  --edit-bg: #2a2a2b;
  --chart-grid: #444444;
  --warning: #ffb340;
  --warning-soft: #3a2b16;

  --field-bg: #333333;
  --field-border: #4a4a4a;
  --field-border-hover: #6e6e6e;
  --field-inner-shadow: none;
  --field-focus-ring: 0 0 0 2px rgba(55, 148, 255, 0.4);
  --btn-bg: #3c3c3c;
  --btn-border: #4a4a4a;
  --btn-hover: #454545;
  --btn-active: #505050;
  --btn-primary-bg: #1f6feb;
  --btn-primary-hover: #388bfd;
  --btn-danger-bg: #da3633;
  --btn-danger-hover: #b62324;
  --btn-warning-bg: #9a6700;
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
button,
select,
input:not([type="checkbox"]):not([type="radio"]):not([type="range"]):not([type="color"]) {
  min-height: 28px;
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
textarea:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: var(--field-focus-ring);
}
button:focus-visible,
[role="separator"]:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
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
  white-space: nowrap;
  flex-shrink: 0;
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
  background: var(--btn-primary-bg);
  border-color: var(--btn-primary-bg);
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

/* 折叠更多菜单 */
.more-wrap {
  position: relative;
  display: inline-flex;
}
.more-menu {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 60;
  min-width: 140px;
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: var(--radius-md);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.18);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.more-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 10px;
  font-size: 12.5px;
  color: var(--text-primary);
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  text-align: left;
  width: 100%;
}
.more-item:hover {
  background: var(--btn-hover);
}
.more-item:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.more-item.on {
  color: var(--accent);
  font-weight: 600;
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
.kbd-hint {
  font-size: 11.5px;
  color: var(--text-tertiary);
  margin-right: 10px;
  user-select: none;
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
.debug-workbench {
  display: grid;
  gap: 0;
}
.panel {
  background: var(--panel-bg);
  border-radius: var(--radius-md);
  border: 1px solid var(--panel-border);
  overflow: hidden;
  min-height: 0;
}
.panel.left {
  flex: 11;
  min-width: 0;
}
.panel.right {
  flex: 4;
  min-width: 0;
}
.panel.full {
  flex: 1;
}
.splitter {
  width: 6px;
  min-width: 6px;
  margin: 0;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: col-resize;
  position: relative;
}
.splitter::after {
  content: "";
  position: absolute;
  inset: 8px 2px;
  border-radius: 2px;
  background: var(--panel-border);
  transition: background 0.12s ease;
}
.splitter:hover::after,
.splitter:focus-visible::after {
  background: var(--accent);
}

/* ===== 设置面板 ===== */
.settings-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.32);
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
}
.settings-panel {
  width: 460px;
  max-width: calc(100vw - 48px);
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: 10px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.22);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: calc(100vh - 32px);
  overflow-y: auto;
  outline: none;
}
.settings-panel.mcp-settings-panel {
  width: 680px;
}
.settings-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.settings-title {
  font-size: 15px;
  font-weight: 700;
}
.settings-tabs {
  display: flex;
  gap: 4px;
  border-bottom: 1px solid var(--panel-border);
}
.settings-tab {
  position: relative;
  border: 0;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  padding: 6px 10px 8px;
  font-size: 13px;
  cursor: pointer;
}
.settings-tab:hover,
.settings-tab.active {
  color: var(--accent);
}
.settings-tab.active {
  border-bottom-color: var(--accent);
}
.settings-tab-badge,
.settings-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 999px;
  background: var(--danger);
  color: #fff;
  font-size: 10px;
  font-weight: 700;
  line-height: 1;
}
.settings-tab-badge {
  margin-left: 4px;
  vertical-align: 1px;
}
.settings-entry {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.settings-content {
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.mcp-settings-content {
  margin: -4px -8px 0;
}
.mini-btn {
  border: 1px solid var(--btn-border);
  background: var(--btn-bg);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  padding: 4px 12px;
  font-size: 12.5px;
  line-height: 18px;
  min-height: 28px;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
}
.mini-btn:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
}
.mini-btn.danger {
  border-color: var(--danger);
  color: var(--danger);
  background: transparent;
}
.mini-btn.danger:hover {
  background: var(--danger);
  color: #fff;
}
.setting-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.setting-label {
  width: 76px;
  flex-shrink: 0;
  font-size: 13px;
  color: var(--text-secondary);
}
.range-slider {
  flex: 1;
  accent-color: var(--accent);
}
.setting-val {
  width: 44px;
  font-size: 12px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
.setting-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}
.setting-row.col {
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
}
.log-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.log-path-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.log-path-input {
  min-width: 0;
  flex: 1;
  box-sizing: border-box;
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 6px 8px;
  background: var(--control-bg);
  color: var(--text-primary);
  font-size: 12px;
}
.log-path-input[readonly] {
  color: var(--text-secondary);
  cursor: default;
}
.log-error {
  color: var(--danger);
  font-size: 12px;
}
.settings-hint {
  color: var(--text-tertiary);
  font-size: 11px;
}
.profile-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 140px;
  overflow-y: auto;
}
.profile-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border: 1px solid var(--panel-border);
  border-radius: var(--radius-sm);
}
.profile-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
  min-width: 80px;
}
.profile-detail {
  flex: 1;
  font-size: 11.5px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.settings-foot {
  border-top: 1px solid var(--panel-border);
  padding-top: 10px;
  font-size: 11px;
  color: var(--text-tertiary);
}

@media (max-width: 1080px) {
  .kbd-hint {
    display: none;
  }
  .content {
    padding: 8px;
  }
}

@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    scroll-behavior: auto !important;
    transition-duration: 0.01ms !important;
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
  }
}
</style>
