<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { useRxStore } from "../stores/rx";
import type { RxEntry } from "../stores/rx";
import { useConnStore } from "../stores/conn";
import { formatTime } from "../utils/bytes";

const store = useRxStore();
const filterDraft = ref(store.filterText);
let filterTimer: ReturnType<typeof setTimeout> | null = null;
watch(filterDraft, (value) => {
  if (filterTimer) clearTimeout(filterTimer);
  filterTimer = setTimeout(() => {
    store.filterText = value;
  }, 160);
});
const rxMoreOpen = ref(false);
const conn = useConnStore();
const connStatus = computed(() => conn.status);

// 点击菜单外部时关闭
function onDocClick(e: MouseEvent) {
  if (rxMoreOpen.value && !(e.target as HTMLElement).closest?.(".more-wrap")) {
    rxMoreOpen.value = false;
  }
}
onMounted(() => document.addEventListener("click", onDocClick));
onBeforeUnmount(() => document.removeEventListener("click", onDocClick));

const scrollEl = ref<HTMLDivElement | null>(null);
const viewportEl = ref<HTMLDivElement | null>(null);

// 虚拟滚动状态
const ROW_HEIGHT_BASE = 22;
const viewportH = ref(400);
const scrollTop = ref(0);
const buffer = 20; // 上下缓冲行数

const rowHeight = computed(() => ROW_HEIGHT_BASE + (store.fontSize - 12.5) * 2.2);
const visible = computed(() => {
  const list = store.filteredEntries;
  const start = Math.max(0, Math.floor(scrollTop.value / rowHeight.value) - buffer);
  const count = Math.ceil(viewportH.value / rowHeight.value) + buffer * 2;
  return list.slice(start, start + count).map((e, i) => ({
    entry: e,
    index: start + i,
  }));
});

const totalHeight = computed(() => store.filteredCount * rowHeight.value);

function onScroll() {
  scrollTop.value = scrollEl.value?.scrollTop ?? 0;
  // 靠近底部判定（自动跟随）
  const el = scrollEl.value;
  if (!el) return;
  const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  store.autoScroll = nearBottom;
  checkNearBottom();
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

/** 是否远离底部（显示"回到底部"按钮） */
const farFromBottom = ref(false);
watch(
  () => store.filterText,
  async () => {
    scrollTop.value = 0;
    await nextTick();
    if (scrollEl.value) scrollEl.value.scrollTop = 0;
    farFromBottom.value = false;
  }
);
function checkNearBottom() {
  const el = scrollEl.value;
  if (!el) return;
  const near = el.scrollHeight - el.scrollTop - el.clientHeight < 120;
  farFromBottom.value = !near && store.filteredCount > 0;
  if (near) store.autoScroll = true;
}

async function scrollToBottomNow() {
  await nextTick();
  const el = scrollEl.value;
  if (el) {
    el.scrollTop = el.scrollHeight;
    store.autoScroll = true;
    farFromBottom.value = false;
  }
}

function setView(v: "text" | "hex" | "ascii" | "dual") {
  store.rxHexMode = v === "hex";
  store.asciiMode = v === "ascii";
  store.dualMode = v === "dual";
}

function onTogglePause() {
  store.togglePause();
  if (store.paused) {
    store.autoScroll = false; // 暂停=停止跟随
  } else {
    store.autoScroll = true;
    scrollToBottomNow();
  }
}

/** 复制行内容到剪贴板 */
let copyTip: ReturnType<typeof setTimeout> | null = null;
const copyToast = ref("");
async function copyEntry(e: RxEntry) {
  const text = store.dualMode
    ? `${store.getEntryHex(e)} | ${store.getEntryAscii(e)}`
    : store.rxHexMode
      ? store.getEntryHex(e)
      : store.asciiMode
        ? store.getEntryAscii(e)
        : store.getEntryText(e);
  try {
    await navigator.clipboard.writeText(text);
    copyToast.value = `已复制 ${text.length} 字符`;
  } catch {
    copyToast.value = "复制失败";
  }
  if (copyTip) clearTimeout(copyTip);
  copyTip = setTimeout(() => (copyToast.value = ""), 1500);
}

let resizeWatcher: ResizeObserver | null = null;
onMounted(() => {
  resizeObserver();
  resizeWatcher = new ResizeObserver(resizeObserver);
  if (viewportEl.value) resizeWatcher.observe(viewportEl.value);
});

onBeforeUnmount(() => {
  resizeWatcher?.disconnect();
  resizeWatcher = null;
  if (copyTip) clearTimeout(copyTip);
  if (filterTimer) clearTimeout(filterTimer);
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
          :class="{ active: store.rxHexMode && !store.asciiMode && !store.dualMode }"
          :aria-pressed="store.rxHexMode && !store.asciiMode && !store.dualMode"
          @click="setView('hex')"
          title="HEX 显示"
        >
          HEX
        </button>
        <button
          class="tool-btn"
          :class="{ active: store.dualMode }"
          :aria-pressed="store.dualMode"
          @click="setView('dual')"
          title="HEX 与 ASCII 双栏对照"
        >
          双栏
        </button>
        <button
          class="tool-btn"
          :class="{ active: store.asciiMode }"
          :aria-pressed="store.asciiMode"
          @click="setView('ascii')"
          title="ASCII 显示（不可打印字节显示为 .）"
        >
          ASCII
        </button>
        <button
          class="tool-btn"
          :class="{ active: !store.rxHexMode && !store.asciiMode && !store.dualMode }"
          :aria-pressed="!store.rxHexMode && !store.asciiMode && !store.dualMode"
          @click="setView('text')"
          title="文本显示（按编码解码并保留 ANSI 样式）"
        >
          文本
        </button>
        <button
          class="tool-btn"
          :class="{ active: store.paused }"
          :aria-pressed="store.paused"
          @click="onTogglePause"
          title="暂停滚动（数据继续接收）"
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
        <div class="more-wrap">
          <button
            class="tool-btn"
            :class="{ active: rxMoreOpen }"
            :aria-expanded="rxMoreOpen"
            @click="rxMoreOpen = !rxMoreOpen"
            title="更多选项"
          >
            ⋯ 更多
          </button>
          <div v-if="rxMoreOpen" class="more-menu">
            <button
              class="more-item"
              :class="{ on: store.showLineNo }"
              @click="store.showLineNo = !store.showLineNo; rxMoreOpen = false"
            >
              行号 {{ store.showLineNo ? "✓" : "" }}
            </button>
            <button
              class="more-item"
              :class="{ on: store.showTimestamp }"
              @click="store.showTimestamp = !store.showTimestamp; rxMoreOpen = false"
            >
              时间戳 {{ store.showTimestamp ? "✓" : "" }}
            </button>
            <button class="more-item" @click="demoData(); rxMoreOpen = false">
              注入模拟数据（诊断）
            </button>
          </div>
        </div>
      </div>
      <div class="right">
        <input
          v-model="filterDraft"
          class="filter-input"
          placeholder="过滤关键字..."
          title="按文本或 HEX 过滤接收内容"
        />
        <span class="stats">
          收 {{ store.rxCount }} 块 · {{ store.formatBytes(store.rxBytes) }} / 发
          {{ store.txCount }} 块 · {{ store.formatBytes(store.txBytes) }}
          <span class="rates">
            ↓ {{ store.formatBytes(store.rxRate) }}/s ↑
            {{ store.formatBytes(store.txRate) }}/s
          </span>
        </span>
        <button
          class="tool-btn danger"
          @click="store.clear()"
          title="清空接收区"
        >
          清空
        </button>
      </div>
    </div>

    <div v-if="store.paused" class="pause-banner">
      ⏸ 已暂停滚动 · 数据仍在接收缓冲（点击「▶ 继续」回到最新）
    </div>
    <div v-if="store.filterText" class="filter-banner">
      过滤中：仅显示匹配 "{{ store.filterText }}" 的行（{{
        store.filteredCount
      }}
      条）
    </div>

    <div ref="viewportEl" class="viewport">
      <div
        v-if="store.filteredCount === 0"
        class="empty-state"
      >
        <template v-if="store.filterText">没有匹配 "{{ store.filterText }}" 的记录</template>
        <template v-else-if="connStatus === 'connected'">等待接收数据...</template>
        <template v-else>未连接 · 配置连接参数后点击「打开」开始调试</template>
      </div>
      <div
        ref="scrollEl"
        class="scroll"
        @scroll.passive="onScroll"
      >
        <div class="virtual-content" :style="{ height: totalHeight + 'px' }">
          <div
            class="virtual-inner"
            :style="{ transform: `translateY(${(visible[0]?.index ?? 0) * rowHeight}px)` }"
          >
          <div
            v-for="entry in visible"
            :key="entry.entry.id"
            :class="[entryClass(entry.entry.dir), { dual: store.dualMode }]"
            :style="{
              height: rowHeight + 'px',
              fontSize: store.fontSize + 'px',
              lineHeight: rowHeight + 'px',
            }"
            :title="'点击复制'"
            @click="copyEntry(entry.entry)"
          >
            <span v-if="store.showTimestamp" class="ts">
              {{ formatTime(entry.entry.ts) }}
            </span>
            <span v-if="store.showLineNo" class="lineno">
              {{ entry.index + 1 }}
            </span>
            <span class="dir">
              {{ entry.entry.dir === "rx" ? "⬇" : "⬆" }}
            </span>
            <span v-if="entry.entry.peer" class="peer">{{ entry.entry.peer }}</span>
            <template v-if="store.dualMode">
              <code class="hex dual-hex" :title="store.getEntryHex(entry.entry)">{{ store.getEntryHex(entry.entry) }}</code>
              <code class="hex ascii-view dual-ascii" :title="store.getEntryAscii(entry.entry)">{{ store.getEntryAscii(entry.entry) }}</code>
            </template>
            <template v-else-if="store.rxHexMode && !store.asciiMode">
              <code class="hex">{{ store.getEntryHex(entry.entry) }}</code>
            </template>
            <template v-else-if="store.asciiMode">
              <code class="hex ascii-view">{{ store.getEntryAscii(entry.entry) }}</code>
            </template>
            <template v-else>
              <span
                v-for="(sp, i) in store.getEntrySpans(entry.entry)"
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

    <button
      v-if="farFromBottom && store.filteredCount > 0"
      class="jump-bottom"
      @click="scrollToBottomNow"
      title="回到底部"
    >
      ⬇ 回到底部
    </button>
    <transition name="toast">
      <div v-if="copyToast" class="copy-toast">{{ copyToast }}</div>
    </transition>
  </div>
</template>

<style scoped>
.receive-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  position: relative;
}
.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--panel-border);
}
.left,
.right {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
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
  border-color: var(--accent);
  color: var(--accent);
}
.tool-btn.active {
  background: var(--btn-primary-bg);
  color: #fff;
  border-color: var(--btn-primary-bg);
}
.tool-btn.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
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
.rates {
  color: var(--accent);
  margin-left: 6px;
  font-size: 11.5px;
}
.filter-input {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 3px 8px;
  font-size: 12px;
  background: var(--control-bg);
  color: var(--text-primary);
  width: 130px;
  outline: none;
}
.filter-input:focus {
  border-color: var(--accent);
}
.pause-banner,
.filter-banner {
  padding: 3px 12px;
  font-size: 11.5px;
  border-bottom: 1px solid var(--panel-border);
  color: var(--text-secondary);
}
.pause-banner {
  background: var(--warning-soft);
  color: var(--warning);
}
.filter-banner {
  background: var(--accent-soft);
  color: var(--accent);
}
.jump-bottom {
  position: absolute;
  right: 14px;
  bottom: 14px;
  z-index: 10;
  border: 1px solid var(--btn-border);
  background: var(--btn-bg);
  color: var(--text-primary);
  border-radius: var(--radius-md);
  padding: 5px 12px;
  font-size: 12px;
  cursor: pointer;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.12);
}
.jump-bottom:hover {
  background: var(--btn-hover);
}
.copy-toast {
  position: absolute;
  left: 50%;
  bottom: 18px;
  transform: translateX(-50%);
  z-index: 20;
  background: var(--control-bg);
  color: var(--text-primary);
  border: 1px solid var(--panel-border);
  border-radius: var(--radius-md);
  padding: 4px 12px;
  font-size: 12px;
  box-shadow: 0 2px 10px rgba(0, 0, 0, 0.15);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 0.15s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
}
.viewport {
  flex: 1;
  min-height: 0;
  overflow: hidden;
  position: relative;
}
.empty-state {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  font-size: 13px;
  pointer-events: none;
}
.scroll {
  overflow-y: auto;
  overflow-x: auto;
  height: 100%;
  position: relative;
}
.virtual-content {
  position: relative;
  min-width: 100%;
  width: max-content;
}
.virtual-inner {
  position: absolute;
  inset: 0 0 auto;
  min-width: 100%;
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
  min-width: 100%;
}
.row.rx {
  color: var(--text-primary);
}
.row.tx {
  color: var(--accent);
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
.peer {
  color: var(--text-tertiary);
  flex-shrink: 0;
  padding-right: 8px;
  border-right: 1px solid var(--panel-border);
}
.row.dual {
  width: 100%;
  max-width: 100%;
  overflow: hidden;
}
.dual-hex,
.dual-ascii {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.dual-hex {
  flex: 2 1 0;
  padding-right: 12px;
  border-right: 1px solid var(--panel-border);
}
.dual-ascii {
  flex: 1 1 0;
  padding-left: 4px;
}
.lineno {
  color: var(--text-tertiary);
  font-size: 11px;
  min-width: 44px;
  flex-shrink: 0;
  text-align: right;
  user-select: none;
}
.ascii-view {
  color: var(--text-primary);
}
.hex {
  color: var(--text-primary);
}
.sp {
  white-space: inherit;
}
</style>
