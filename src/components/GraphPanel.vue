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
let renderTimer: ReturnType<typeof requestAnimationFrame> | null = null;
let dirty = false;

function initChart() {
  if (!chartEl.value) return;
  chart = echarts.init(chartEl.value);
  chart.setOption({
    grid: { left: 48, right: 24, top: 36, bottom: 32 },
    legend: { top: 6, textStyle: { fontSize: 11 } },
    tooltip: { trigger: "axis" },
    xAxis: {
      type: "value",
      name: "x",
      scale: true,
      axisLabel: { fontSize: 10 },
      splitLine: { lineStyle: { color: "rgba(0,0,0,0.06)" } },
    },
    yAxis: {
      type: "value",
      scale: true,
      axisLabel: { fontSize: 10 },
      splitLine: { lineStyle: { color: "rgba(0,0,0,0.06)" } },
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
}

/** 渲染（rAF 合并，高频数据不卡） */
function scheduleRender() {
  if (dirty) return;
  dirty = true;
  renderTimer = requestAnimationFrame(() => {
    dirty = false;
    if (!chart) return;
    const series = store.seriesList.map((s) => ({
      name: s.name,
      type: "line",
      showSymbol: false,
      data: s.xs.map((x, i) => [x, s.ys[i]]),
      lineStyle: { width: 1.5, color: s.color },
      itemStyle: { color: s.color },
    }));
    chart.setOption({ series }, { notMerge: true });
    // 自动滚动 x 轴
    if (store.autoScroll && series.length) {
      let maxX = Number.NEGATIVE_INFINITY;
      for (const series of store.seriesList) {
        for (const x of series.xs) {
          if (x > maxX) maxX = x;
        }
      }
      if (maxX !== Number.NEGATIVE_INFINITY) {
        chart.setOption({
          xAxis: { min: maxX - store.xRange, max: maxX },
        });
      }
    }
  });
}

function onResize() {
  chart?.resize();
}

// 数据变化 → 合并渲染
watch(
  () => store.seriesList,
  () => scheduleRender(),
  { deep: true, immediate: true }
);

onMounted(async () => {
  await nextTick();
  initChart();
  scheduleRender();
  window.addEventListener("resize", onResize);
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", onResize);
  if (renderTimer) cancelAnimationFrame(renderTimer);
  chart?.dispose();
  chart = null;
});

function downloadCsv() {
  const csv = store.exportCsv();
  if (!csv) return;
  const blob = new Blob(["\ufeff" + csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `curve-${Date.now()}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}
</script>

<template>
  <div class="graph-panel">
    <div class="toolbar">
      <span class="title">波形</span>
      <button
        class="tool-btn"
        :class="{ active: store.enabled }"
        @click="store.enabled = !store.enabled"
      >
        {{ store.enabled ? "解析中" : "已停止" }}
      </button>
      <select v-model="store.protocol" class="ctl">
        <option value="ascii">ASCII ($name,x,y)</option>
        <option value="binary">二进制</option>
      </select>
      <input
        v-if="store.protocol === 'binary'"
        v-model="store.headerHex"
        class="ctl header-input"
        :class="{ invalid: !headerValid }"
        :aria-invalid="!headerValid"
        placeholder="帧头 hex"
        title="请输入偶数位十六进制帧头"
      />
      <span v-if="store.protocol === 'binary' && !headerValid" class="input-error">
        帧头 HEX 无效
      </span>
      <input
        v-model.number="store.xRange"
        type="number"
        class="ctl range-input"
        title="X 可视范围"
      />
      <button
        class="tool-btn"
        :class="{ active: store.autoScroll }"
        @click="store.autoScroll = !store.autoScroll"
      >
        滚动
      </button>
      <button
        class="tool-btn"
        :class="{ active: zoomMode }"
        @click="toggleZoom"
        title="开启/关闭滚轮缩放"
      >
        缩放
      </button>
      <button
        class="tool-btn"
        :class="{ active: store.paused }"
        @click="store.paused = !store.paused"
      >
        {{ store.paused ? "▶ 继续" : "⏸ 暂停" }}
      </button>
      <span class="stats">帧 {{ store.frameCount }}</span>
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
    <div ref="chartEl" class="chart"></div>
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
  border-bottom: 1px solid rgba(0, 0, 0, 0.07);
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
.chart {
  flex: 1;
  min-height: 0;
}
</style>
