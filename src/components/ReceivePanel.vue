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
  border: 1px solid rgba(0, 0, 0, 0.12);
  background: #fff;
  border-radius: 6px;
  padding: 3px 10px;
  font-size: 12px;
  color: #48484a;
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
  border: 1px solid rgba(0, 0, 0, 0.12);
  border-radius: 6px;
  padding: 3px 6px;
  font-size: 12px;
  background: #fff;
}
.stats {
  font-size: 12px;
  color: #6e6e73;
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
  border-bottom: 1px solid rgba(0, 0, 0, 0.03);
}
.row.rx {
  color: #1d1d1f;
}
.row.tx {
  color: #0a84ff;
  background: rgba(10, 132, 255, 0.04);
}
.ts {
  color: #98989d;
  font-size: 11.5px;
  min-width: 104px;
  flex-shrink: 0;
}
.dir {
  color: #98989d;
  flex-shrink: 0;
}
.hex {
  color: #1d1d1f;
}
.sp {
  white-space: pre-wrap;
}
</style>
