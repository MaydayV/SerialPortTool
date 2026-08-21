<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { useRxStore } from "../stores/rx";
import { formatTime } from "../utils/bytes";

const store = useRxStore();

const scrollEl = ref<HTMLDivElement | null>(null);
const viewportEl = ref<HTMLDivElement | null>(null);

// 虚拟滚动状态
const ROW_HEIGHT = 22;
const viewportH = ref(400);
const scrollTop = ref(0);
const buffer = 20; // 上下缓冲行数

const visible = computed(() => {
  const start = Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - buffer);
  const count = Math.ceil(viewportH.value / ROW_HEIGHT) + buffer * 2;
  return store.entries.slice(start, start + count).map((e, i) => ({
    entry: e,
    index: start + i,
  }));
});

const totalHeight = computed(() => store.entries.length * ROW_HEIGHT);

function onScroll() {
  scrollTop.value = scrollEl.value?.scrollTop ?? 0;
  // 靠近底部判定（自动跟随）
  const el = scrollEl.value;
  if (!el) return;
  const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  store.autoScroll = nearBottom;
}

async function scrollToBottom() {
  await nextTick();
  const el = scrollEl.value;
  if (el) el.scrollTop = el.scrollHeight;
}

// 数据变化后自动滚动
let lastLen = 0;
watch(
  () => store.entries.length,
  async () => {
    if (store.autoScroll && store.entries.length !== lastLen) {
      lastLen = store.entries.length;
      await scrollToBottom();
    }
  }
);

function resizeObserver() {
  const el = viewportEl.value;
  if (el) viewportH.value = el.clientHeight;
}

onMounted(() => {
  resizeObserver();
  const ro = new ResizeObserver(resizeObserver);
  if (viewportEl.value) ro.observe(viewportEl.value);
  (window as any)._ro = ro;
});

onBeforeUnmount(() => {
  (window as any)._ro?.disconnect();
});

function entryClass(dir: string) {
  return dir === "rx" ? "row rx" : "row tx";
}

/** 注入模拟数据（无设备时验证渲染链路） */
function demoData() {
  const enc = new TextEncoder();
  const samples = [
    "AT+OK\r\n",
    "Sensor: 23.5C, Humidity: 61%\r\n",
    "\u001b[32mOK\u001b[0m \u001b[1mBOOT\u001b[0m complete\r\n",
    "ERR: \u001b[31mtimeout\u001b[0m waiting ack\r\n",
    "data[0]=0xAA data[1]=0x55 crc=0x1F\r\n",
  ];
  for (let i = 0; i < 50; i++) {
    const s = samples[i % samples.length];
    const dir: "rx" | "tx" = i % 5 === 0 ? "tx" : "rx";
    store.append(enc.encode(s), dir, Date.now() - (50 - i) * 37);
  }
}
</script>

<template>
  <div class="receive-panel">
    <div class="toolbar">
      <div class="left">
        <button
          class="tool-btn"
          :class="{ active: store.rxHexMode }"
          @click="store.rxHexMode = !store.rxHexMode"
          title="HEX 显示"
        >
          HEX
        </button>
        <button
          class="tool-btn"
          :class="{ active: store.showTimestamp }"
          @click="store.showTimestamp = !store.showTimestamp"
          title="时间戳"
        >
          时间戳
        </button>
        <button
          class="tool-btn"
          :class="{ active: store.paused }"
          @click="store.togglePause()"
          title="暂停接收"
        >
          {{ store.paused ? "▶ 继续" : "⏸ 暂停" }}
        </button>
        <select v-model="store.encoding" class="enc-sel" title="编码">
          <option>UTF-8</option>
          <option>ASCII</option>
          <option>GBK</option>
          <option>GB2312</option>
          <option>GB18030</option>
          <option>UTF-16</option>
        </select>
      </div>
      <div class="right">
        <span class="stats">
          收 {{ store.rxCount }} 帧 · {{ store.formatBytes(store.rxBytes) }} / 发
          {{ store.txCount }} 帧 · {{ store.formatBytes(store.txBytes) }}
        </span>
        <button
          class="tool-btn danger"
          @click="store.clear()"
          title="清空接收区"
        >
          清空
        </button>
        <button class="tool-btn" @click="demoData()" title="注入模拟数据验证渲染">
          模拟
        </button>
      </div>
    </div>

    <div ref="viewportEl" class="viewport">
      <div
        ref="scrollEl"
        class="scroll"
        @scroll.passive="onScroll"
        :style="{ height: totalHeight + 'px' }"
      >
        <div
          class="virtual-inner"
          :style="{ transform: `translateY(${visible[0]?.index ?? 0 * ROW_HEIGHT}px)` }"
        >
          <div
            v-for="entry in visible"
            :key="entry.entry.id"
            :class="entryClass(entry.entry.dir)"
            :style="{ height: ROW_HEIGHT + 'px' }"
          >
            <span v-if="store.showTimestamp" class="ts">
              {{ formatTime(entry.entry.ts) }}
            </span>
            <span class="dir">
              {{ entry.entry.dir === "rx" ? "⬇" : "⬆" }}
            </span>
            <template v-if="store.rxHexMode">
              <code class="hex">{{ entry.entry.hex }}</code>
            </template>
            <template v-else>
              <span
                v-for="(sp, i) in entry.entry.spans"
                :key="i"
                class="sp"
                :style="{
                  color: sp.fg,
                  background: sp.bg,
                  fontWeight: sp.bold ? 700 : 400,
                }"
                >{{ sp.text }}</span
              >
            </template>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.receive-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 10px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.07);
}
.left,
.right {
  display: flex;
  align-items: center;
  gap: 6px;
}
.tool-btn {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 6px;
  padding: 3px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
}
.tool-btn:hover {
  border-color: #0a84ff;
  color: #0a84ff;
}
.tool-btn.active {
  background: #0a84ff;
  color: #fff;
  border-color: #0a84ff;
}
.tool-btn.danger:hover {
  border-color: #ff3b30;
  color: #ff3b30;
}
.enc-sel {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 3px 6px;
  font-size: 12px;
  background: var(--control-bg);
}
.stats {
  font-size: 12px;
  color: var(--text-secondary);
  font-variant-numeric: tabular-nums;
}
.viewport {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}
.scroll {
  overflow-y: auto;
  overflow-x: hidden;
  height: 100%;
  position: relative;
}
.virtual-inner {
  position: relative;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  font-size: 12.5px;
  line-height: 22px;
  white-space: nowrap;
  font-family: "SF Mono", Menlo, Consolas, "Courier New", monospace;
  border-bottom: 1px solid var(--edit-bg);
}
.row.rx {
  color: var(--text-primary);
}
.row.tx {
  color: #0a84ff;
  background: var(--row-tx-bg);
}
.ts {
  color: var(--text-tertiary);
  font-size: 11.5px;
  min-width: 104px;
  flex-shrink: 0;
}
.dir {
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.hex {
  color: var(--text-primary);
}
.sp {
  white-space: pre-wrap;
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
