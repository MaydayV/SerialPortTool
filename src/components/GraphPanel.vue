<script setup lang="ts">
import * as echarts from "echarts/core";
import { LineChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
} from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import { ref, computed, onMounted, onBeforeUnmount, watch, nextTick } from "vue";
import { useGraphStore } from "../stores/graph";
import { hexToBytes } from "../utils/bytes";
import { saveTextFile } from "../utils/save";

echarts.use([LineChart, DataZoomComponent, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

const store = useGraphStore();
const chartEl = ref<HTMLDivElement | null>(null);
const zoomMode = ref(false); // 滚轮缩放模式
const colorTarget = ref("");
const headerValid = computed(() => {
  const bytes = hexToBytes(store.headerHex);
  return !!bytes && bytes.length > 0;
});
let chart: echarts.ECharts | null = null;
let renderTimer: number | null = null;
let dirty = false;
let lastRenderAt = 0;
let chartResizeObserver: ResizeObserver | null = null;
let themeObserver: MutationObserver | null = null;
const RENDER_INTERVAL = 1000 / 30;
const MAX_RENDER_POINTS_PER_SERIES = 4000;

function chartTheme() {
  const css = getComputedStyle(document.documentElement);
  return {
    text: css.getPropertyValue("--text-secondary").trim(),
    grid: css.getPropertyValue("--chart-grid").trim(),
  };
}

function applyChartTheme() {
  if (!chart) return;
  const colors = chartTheme();
  chart.setOption({
    legend: { textStyle: { color: colors.text, fontSize: 11 } },
    xAxis: {
      axisLine: { lineStyle: { color: colors.grid } },
      axisLabel: { color: colors.text, fontSize: 10 },
      splitLine: { lineStyle: { color: colors.grid } },
    },
    yAxis: {
      axisLine: { lineStyle: { color: colors.grid } },
      axisLabel: { color: colors.text, fontSize: 10 },
      splitLine: { lineStyle: { color: colors.grid } },
    },
  });
}

function initChart() {
  if (!chartEl.value) return;
  chart = echarts.init(chartEl.value);
  const colors = chartTheme();
  chart.setOption({
    grid: { left: 48, right: 24, top: 36, bottom: 32 },
    legend: { top: 6, textStyle: { color: colors.text, fontSize: 11 } },
    tooltip: { trigger: "axis" },
    xAxis: {
      type: "value",
      name: "x",
      scale: true,
      axisLabel: { color: colors.text, fontSize: 10 },
      splitLine: { lineStyle: { color: colors.grid } },
    },
    yAxis: {
      type: "value",
      scale: true,
      axisLabel: { color: colors.text, fontSize: 10 },
      splitLine: { lineStyle: { color: colors.grid } },
    },
    dataZoom: [
      {
        type: "inside",
        xAxisIndex: 0,
        filterMode: "none",
        disabled: true, // 默认关闭，缩放模式开启
      },
    ],
  });
}

/** 切换缩放模式（开启时暂停自动滚动，避免冲突） */
function toggleZoom() {
  zoomMode.value = !zoomMode.value;
  if (!chart) return;
  chart.setOption({
    dataZoom: [
      {
        type: "inside",
        xAxisIndex: 0,
        filterMode: "none",
        disabled: !zoomMode.value,
      },
    ],
  });
  if (zoomMode.value) store.autoScroll = false;
  scheduleRender();
}

function renderChart(timestamp: number) {
  if (timestamp - lastRenderAt < RENDER_INTERVAL) {
    renderTimer = requestAnimationFrame(renderChart);
    return;
  }
  renderTimer = null;
  dirty = false;
  lastRenderAt = timestamp;
  if (!chart) return;

  const maxX = store.maxX;
  const range = Number.isFinite(store.xRange) && store.xRange > 0 ? store.xRange : 100;
  const minVisible = store.autoScroll && Number.isFinite(maxX) ? maxX - range : null;
  const series = store.seriesList.map((s) => {
    const data: [number, number][] = [];
    const stride = Math.max(1, Math.ceil(s.xs.length / MAX_RENDER_POINTS_PER_SERIES));
    for (let i = 0; i < s.xs.length; i += stride) {
      const x = s.xs[i];
      if (minVisible === null || x >= minVisible) data.push([x, s.ys[i]]);
    }
    const last = s.xs.length - 1;
    if (last >= 0 && last % stride !== 0) {
      const x = s.xs[last];
      if (minVisible === null || x >= minVisible) data.push([x, s.ys[last]]);
    }
    return {
      name: s.name,
      type: "line" as const,
      showSymbol: false,
      sampling: "lttb" as const,
      data,
      lineStyle: { width: 1.5, color: s.color },
      itemStyle: { color: s.color },
    };
  });
  const option: echarts.EChartsCoreOption = { series };
  if (store.autoScroll && Number.isFinite(maxX) && series.length) {
    option.xAxis = { min: maxX - range, max: maxX };
  } else {
    option.xAxis = { min: null, max: null };
  }
  chart.setOption(option, { replaceMerge: ["series"], lazyUpdate: true });
}

/** 渲染（30 FPS 上限 + rAF 合并） */
function scheduleRender() {
  if (dirty) return;
  dirty = true;
  renderTimer = requestAnimationFrame(renderChart);
}

function onResize() {
  chart?.resize();
}

// 数据变化 → 合并渲染
watch(
  () => [store.revision, store.xRange, store.autoScroll],
  () => scheduleRender(),
  { immediate: true }
);

onMounted(async () => {
  await nextTick();
  initChart();
  scheduleRender();
  chartResizeObserver = new ResizeObserver(onResize);
  if (chartEl.value) chartResizeObserver.observe(chartEl.value);
  themeObserver = new MutationObserver(() => applyChartTheme());
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
});

onBeforeUnmount(() => {
  chartResizeObserver?.disconnect();
  themeObserver?.disconnect();
  if (renderTimer) cancelAnimationFrame(renderTimer);
  chart?.dispose();
  chart = null;
});

async function downloadCsv() {
  const csv = store.exportCsv();
  if (!csv) return;
  try {
    await saveTextFile(
      "curve",
      "\ufeff" + csv,
      `curve-${Date.now()}.csv`,
      "text/csv;charset=utf-8"
    );
  } catch (error) {
    alert(`导出失败：${error instanceof Error ? error.message : String(error)}`);
  }
}
</script>

<template>
  <div class="graph-panel">
    <div class="toolbar">
      <span class="title">波形</span>
      <button
        class="tool-btn"
        :class="{ active: store.enabled }"
        :aria-pressed="store.enabled"
        @click="store.enabled = !store.enabled"
      >
        {{ store.enabled ? "解析中" : "已停止" }}
      </button>
      <select v-model="store.protocol" class="ctl" aria-label="波形协议">
        <option value="ascii">ASCII ($name,x,y)</option>
        <option value="binary">二进制</option>
      </select>
      <input
        v-if="store.protocol === 'binary'"
        v-model="store.headerHex"
        class="ctl header-input"
        :class="{ invalid: !headerValid }"
        :aria-invalid="!headerValid"
        maxlength="4096"
        placeholder="帧头 hex"
        title="请输入偶数位十六进制帧头"
      />
      <span v-if="store.protocol === 'binary' && !headerValid" class="input-error">
        帧头 HEX 无效
      </span>
      <input
        v-model.number="store.xRange"
        type="number"
        min="1"
        max="1000000000"
        class="ctl range-input"
        aria-label="X 轴可视范围"
        title="X 可视范围"
      />
      <button
        class="tool-btn"
        :class="{ active: store.autoScroll }"
        :aria-pressed="store.autoScroll"
        @click="store.autoScroll = !store.autoScroll"
      >
        滚动
      </button>
      <button
        class="tool-btn"
        :class="{ active: zoomMode }"
        :aria-pressed="zoomMode"
        @click="toggleZoom"
        title="开启/关闭滚轮缩放"
      >
        缩放
      </button>
      <button
        class="tool-btn"
        :class="{ active: store.paused }"
        :aria-pressed="store.paused"
        @click="store.paused = !store.paused"
      >
        {{ store.paused ? "▶ 继续" : "⏸ 暂停" }}
      </button>
      <span class="stats">点 {{ store.frameCount }}</span>
      <template v-if="store.seriesList.length">
        <span class="sep"></span>
        <select v-model="colorTarget" class="ctl color-sel">
          <option value="" disabled>曲线颜色</option>
          <option v-for="s in store.seriesList" :key="s.name" :value="s.name">
            {{ s.name }}
          </option>
        </select>
        <input
          v-if="colorTarget"
          type="color"
          class="color-pick"
          :value="
            store.seriesList.find((s) => s.name === colorTarget)?.color
          "
          @input="
            (e: Event) => {
              const v = (e.target as HTMLInputElement).value;
              if (colorTarget) store.setSeriesColor(colorTarget, v);
            }
          "
          title="修改所选曲线颜色"
        />
      </template>
      <div class="spacer"></div>
      <button class="tool-btn" @click="downloadCsv">导出 CSV</button>
      <button class="tool-btn danger" @click="store.clear()">清空</button>
    </div>
    <div class="chart-wrap">
      <div ref="chartEl" class="chart" aria-label="实时波形图"></div>
      <div v-if="store.seriesList.length === 0" class="empty-state">
        <strong>等待波形数据</strong>
        <span>ASCII 示例：$temperature,1,23.5</span>
        <span>{{ store.enabled ? "正在等待接收数据" : "点击“已停止”开启解析，切换页面后仍会持续采集" }}</span>
        <button class="tool-btn" @click="store.addDemoData()">载入示例</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.graph-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--panel-border);
  flex-wrap: wrap;
}
.title {
  font-weight: 600;
  font-size: 13px;
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
.ctl {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 3px 6px;
  font-size: 12px;
  background: var(--control-bg);
}
.header-input {
  width: 110px;
}
.header-input.invalid {
  border-color: var(--danger);
}
.input-error {
  color: var(--danger);
  font-size: 11px;
}
.range-input {
  width: 64px;
}
.stats {
  font-size: 12px;
  color: var(--text-secondary);
}
.sep {
  width: 1px;
  height: 16px;
  background: var(--panel-border);
}
.color-sel {
  max-width: 110px;
}
.color-pick {
  width: 26px;
  height: 22px;
  padding: 1px 2px;
  border: 1px solid var(--control-border);
  border-radius: 4px;
  background: var(--control-bg);
  cursor: pointer;
}
.spacer {
  flex: 1;
}
.chart-wrap {
  flex: 1;
  min-height: 0;
  position: relative;
}
.chart {
  width: 100%;
  height: 100%;
}
.empty-state {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  font-size: 12px;
  pointer-events: none;
}
.empty-state strong {
  color: var(--text-primary);
  font-size: 14px;
}
.empty-state .tool-btn {
  margin-top: 4px;
  pointer-events: auto;
}
</style>
