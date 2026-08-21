<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, nextTick } from "vue";
import * as echarts from "echarts";
import { useGraphStore } from "../stores/graph";

const store = useGraphStore();
const chartEl = ref<HTMLDivElement | null>(null);
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
  });
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
      const allX = store.seriesList.flatMap((s) => s.xs);
      if (allX.length) {
        const maxX = Math.max(...allX);
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
  { deep: true }
);

onMounted(async () => {
  await nextTick();
  initChart();
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
        placeholder="帧头 hex"
      />
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
        :class="{ active: store.paused }"
        @click="store.paused = !store.paused"
      >
        {{ store.paused ? "▶ 继续" : "⏸ 暂停" }}
      </button>
      <span class="stats">帧 {{ store.frameCount }}</span>
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
.range-input {
  width: 64px;
}
.stats {
  font-size: 12px;
  color: var(--text-secondary);
}
.spacer {
  flex: 1;
}
.chart {
  flex: 1;
  min-height: 0;
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
