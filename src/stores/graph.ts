// 波形 store：曲线数据缓冲、帧解析、ECharts 数据管理
import { defineStore } from "pinia";
import { ref, computed, shallowRef, markRaw, watch } from "vue";
import {
  parseAsciiFrame,
  parseBinaryFrames,
} from "../utils/curve";

export interface CurveSeries {
  name: string;
  xs: number[];
  ys: number[];
  color: string;
}

function emptySeriesRecord(): Record<string, CurveSeries> {
  return Object.create(null) as Record<string, CurveSeries>;
}

const MAX_POINTS = 20000; // 每条曲线最大点数
const PRUNE_POINTS = 1024;
const MAX_SERIES = 32;
const MAX_SERIES_NAME = 128;
const MAX_PARSER_STREAMS = 128;
const BUILTIN_COLORS = [
  "#0a84ff",
  "#34c759",
  "#ff9500",
  "#ff3b30",
  "#af52de",
  "#5ac8fa",
  "#ffcc00",
  "#ff2d55",
  "#64d2ff",
  "#30d158",
  "#bf5af2",
  "#ffd60a",
  "#ac8e68",
  "#ff375f",
  "#0bd318",
  "#5e5ce6",
  "#ff9f0a",
  "#ff6482",
  "#66d4cf",
  "#e5e5ea",
];

export const useGraphStore = defineStore("graph", () => {
  const enabled = ref(false); // 用户明确开启后持续解析，切页不会丢数据
  const viewActive = ref(false);
  const protocol = ref<"ascii" | "binary">("ascii");
  const headerHex = ref("AA CC EE BB");
  const xRange = ref(100); // 可视 x 范围
  const autoScroll = ref(true);
  const paused = ref(false);
  const series = shallowRef<Record<string, CurveSeries>>(emptySeriesRecord());
  const order = shallowRef<string[]>([]); // 曲线显示顺序
  const frameCount = ref(0);
  const revision = ref(0);
  const maxX = ref(Number.NEGATIVE_INFINITY);

  const autoXByStream = new Map<string, number>(); // ASCII 省略 x 时按来源独立计数
  const binaryBuffers = new Map<string, Uint8Array>();
  const textBuffers = new Map<string, string>();
  const textDecoders = new Map<string, TextDecoder>();
  const parserStreamOrder = new Map<string, true>();

  const seriesList = computed(() => {
    // 数据缓冲本身不参与深层响应式追踪，仅由轻量版本号驱动视图刷新。
    revision.value;
    return order.value
      .filter((n) => series.value[n])
      .map((n) => series.value[n]);
  });

  function getHeader(): Uint8Array | null {
    const cleaned = headerHex.value.replace(/0x/gi, "").replace(/[,\s]+/g, "");
    if (!cleaned || cleaned.length % 2 !== 0 || !/^[0-9a-fA-F]+$/.test(cleaned)) {
      return null;
    }
    const bytes = new Uint8Array(cleaned.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }

  function ensureSeries(name: string): CurveSeries | null {
    const safeName = name.trim().slice(0, MAX_SERIES_NAME);
    if (!safeName) return null;
    if (!Object.prototype.hasOwnProperty.call(series.value, safeName)) {
      if (order.value.length >= MAX_SERIES) return null;
      const color = BUILTIN_COLORS[order.value.length % BUILTIN_COLORS.length];
      series.value[safeName] = markRaw({ name: safeName, xs: [], ys: [], color });
      order.value = [...order.value, safeName];
    }
    return series.value[safeName];
  }

  function recomputeMaxX() {
    let next = Number.NEGATIVE_INFINITY;
    for (const name of order.value) {
      const current = series.value[name];
      if (!current) continue;
      for (const x of current.xs) if (x > next) next = x;
    }
    maxX.value = next;
  }

  function pushPoint(pt: { name: string; x: number; y: number }): boolean {
    const s = ensureSeries(pt.name);
    if (!s || !Number.isFinite(pt.x) || !Number.isFinite(pt.y)) return false;
    s.xs.push(pt.x);
    s.ys.push(pt.y);
    if (pt.x > maxX.value) maxX.value = pt.x;
    // 批量裁剪，避免达到上限后每个点都触发 O(n) 头部移动。
    if (s.xs.length > MAX_POINTS + PRUNE_POINTS) {
      const remove = s.xs.length - MAX_POINTS;
      let removedCurrentMax = false;
      for (let i = 0; i < remove; i++) {
        if (s.xs[i] === maxX.value) removedCurrentMax = true;
      }
      s.xs.splice(0, remove);
      s.ys.splice(0, remove);
      if (removedCurrentMax) recomputeMaxX();
    }
    return true;
  }

  /** 修改曲线颜色 */
  function setSeriesColor(name: string, color: string) {
    if (series.value[name]) {
      series.value[name].color = color;
      revision.value += 1;
    }
  }

  function setViewActive(active: boolean) {
    viewActive.value = active;
  }

  function touchParserStream(key: string) {
    if (!parserStreamOrder.has(key) && parserStreamOrder.size >= MAX_PARSER_STREAMS) {
      const oldest = parserStreamOrder.keys().next().value as string | undefined;
      if (oldest !== undefined) {
        parserStreamOrder.delete(oldest);
        binaryBuffers.delete(oldest);
        textBuffers.delete(oldest);
        textDecoders.delete(oldest);
        autoXByStream.delete(oldest);
      }
    }
    parserStreamOrder.delete(key);
    parserStreamOrder.set(key, true);
  }

  /** 接收数据 → 解析曲线帧 */
  function processData(data: Uint8Array, streamKey = "default") {
    if (!enabled.value || paused.value) return;
    touchParserStream(streamKey);
    let appended = 0;
    if (protocol.value === "ascii") {
      // 按行解析（攒缓冲直到 \n）
      let decoder = textDecoders.get(streamKey);
      if (!decoder) {
        decoder = new TextDecoder("utf-8");
        textDecoders.set(streamKey, decoder);
      }
      const s = decoder.decode(data, { stream: true });
      let textBuf = (textBuffers.get(streamKey) ?? "") + s;
      let idx: number;
      while ((idx = textBuf.indexOf("\n")) >= 0) {
        const line = textBuf.slice(0, idx).replace(/\r$/, "");
        textBuf = textBuf.slice(idx + 1);
        if (!line) continue;
        const autoX = autoXByStream.get(streamKey) ?? 0;
        const pt = parseAsciiFrame(line, autoX);
        if (pt) {
          if (line.startsWith("$") && line.split(",").length === 2) {
            autoXByStream.set(streamKey, autoX + 1);
          }
          const name = streamKey === "default" ? pt.name : `${streamKey} · ${pt.name}`;
          if (pushPoint({ ...pt, name })) {
            frameCount.value += 1;
            appended += 1;
          }
        }
      }
      if (textBuf.length > 4096) textBuf = textBuf.slice(-4096); // 防膨胀
      if (textBuf) textBuffers.set(streamKey, textBuf);
      else textBuffers.delete(streamKey);
    } else {
      const header = getHeader();
      if (!header) {
        binaryBuffers.delete(streamKey);
        return;
      }
      const binaryBuf = binaryBuffers.get(streamKey) ?? new Uint8Array(0);
      const combined = new Uint8Array(binaryBuf.length + data.length);
      combined.set(binaryBuf);
      combined.set(data, binaryBuf.length);
      const { points, rest } = parseBinaryFrames(combined, header);
      if (rest.length) binaryBuffers.set(streamKey, rest);
      else binaryBuffers.delete(streamKey);
      for (const pt of points) {
        const name = streamKey === "default" ? pt.name : `${streamKey} · ${pt.name}`;
        if (pushPoint({ ...pt, name })) {
          frameCount.value += 1;
          appended += 1;
        }
      }
    }
    if (appended > 0) revision.value += 1;
  }

  function clear() {
    series.value = emptySeriesRecord();
    order.value = [];
    frameCount.value = 0;
    maxX.value = Number.NEGATIVE_INFINITY;
    revision.value += 1;
    autoXByStream.clear();
    binaryBuffers.clear();
    textBuffers.clear();
    textDecoders.clear();
    parserStreamOrder.clear();
  }

  function addDemoData() {
    const start = Number.isFinite(maxX.value) ? maxX.value + 1 : 0;
    for (let i = 0; i < 240; i++) {
      const x = start + i;
      pushPoint({ name: "温度", x, y: 24 + Math.sin(i / 18) * 2.4 });
      pushPoint({ name: "湿度", x, y: 58 + Math.cos(i / 24) * 5 });
    }
    frameCount.value += 480;
    revision.value += 1;
  }

  function resetParserBuffers() {
    binaryBuffers.clear();
    textBuffers.clear();
    textDecoders.clear();
    autoXByStream.clear();
    parserStreamOrder.clear();
  }

  watch([protocol, headerHex], resetParserBuffers);

  /** 导出 CSV（全部曲线） */
  function exportCsv(): string {
    const names = order.value.filter((n) => series.value[n]);
    if (names.length === 0) return "";
    const rows: string[] = ["series,x,y"];
    for (const name of names) {
      const s = series.value[name];
      // 防止设备提供的曲线名在电子表格中触发公式执行。
      const spreadsheetSafeName = /^[=+\-@\t\r]/.test(name) ? `'${name}` : name;
      const escapedName = `"${spreadsheetSafeName.replace(/"/g, '""')}"`;
      for (let i = 0; i < s.xs.length; i++) {
        rows.push(`${escapedName},${s.xs[i]},${s.ys[i]}`);
      }
    }
    return rows.join("\n");
  }

  return {
    enabled,
    viewActive,
    protocol,
    headerHex,
    xRange,
    autoScroll,
    paused,
    seriesList,
    frameCount,
    revision,
    maxX,
    processData,
    clear,
    exportCsv,
    setSeriesColor,
    setViewActive,
    addDemoData,
  };
});
