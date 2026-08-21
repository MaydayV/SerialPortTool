// 波形 store：曲线数据缓冲、帧解析、ECharts 数据管理
import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  parseAsciiFrame,
  parseBinaryFrames,
  DEFAULT_HEADER,
} from "../utils/curve";

export interface CurveSeries {
  name: string;
  xs: number[];
  ys: number[];
  color: string;
}

const MAX_POINTS = 20000; // 每条曲线最大点数
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
  const enabled = ref(true); // 解析开关
  const protocol = ref<"ascii" | "binary">("ascii");
  const headerHex = ref("AA CC EE BB");
  const xRange = ref(100); // 可视 x 范围
  const autoScroll = ref(true);
  const paused = ref(false);
  const series = ref<Record<string, CurveSeries>>({});
  const order = ref<string[]>([]); // 曲线显示顺序
  const frameCount = ref(0);

  let autoX = 0; // ASCII 省略 x 时的自动序号
  let binaryBuf = new Uint8Array(0); // 二进制解析缓冲
  let textBuf = ""; // ASCII 行缓冲

  const seriesList = computed(() =>
    order.value
      .filter((n) => series.value[n])
      .map((n) => series.value[n])
  );

  function getHeader(): Uint8Array {
    try {
      const cleaned = headerHex.value.replace(/0x/gi, "").replace(/[,\s]+/g, "");
      const bytes = new Uint8Array(cleaned.length / 2);
      for (let i = 0; i < bytes.length; i++)
        bytes[i] = parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
      return bytes;
    } catch {
      return DEFAULT_HEADER;
    }
  }

  function ensureSeries(name: string): CurveSeries {
    if (!series.value[name]) {
      const color = BUILTIN_COLORS[order.value.length % BUILTIN_COLORS.length];
      series.value[name] = { name, xs: [], ys: [], color };
      order.value.push(name);
    }
    return series.value[name];
  }

  function pushPoint(pt: { name: string; x: number; y: number }) {
    const s = ensureSeries(pt.name);
    s.xs.push(pt.x);
    s.ys.push(pt.y);
    // 超限裁剪
    if (s.xs.length > MAX_POINTS) {
      s.xs.splice(0, s.xs.length - MAX_POINTS);
      s.ys.splice(0, s.ys.length - MAX_POINTS);
    }
  }

  /** 修改曲线颜色 */
  function setSeriesColor(name: string, color: string) {
    if (series.value[name]) series.value[name].color = color;
  }

  /** 接收数据 → 解析曲线帧 */
  function processData(data: Uint8Array) {
    if (!enabled.value || paused.value) return;
    if (protocol.value === "ascii") {
      // 按行解析（攒缓冲直到 \n）
      let s = new TextDecoder("utf-8").decode(data);
      textBuf += s;
      let idx: number;
      while ((idx = textBuf.indexOf("\n")) >= 0) {
        const line = textBuf.slice(0, idx).replace(/\r$/, "");
        textBuf = textBuf.slice(idx + 1);
        if (!line) continue;
        const pt = parseAsciiFrame(line, autoX);
        if (pt) {
          if (line.startsWith("$") && line.split(",").length === 2) autoX += 1;
          pushPoint(pt);
          frameCount.value += 1;
        }
      }
      if (textBuf.length > 4096) textBuf = textBuf.slice(-4096); // 防膨胀
    } else {
      const combined = new Uint8Array(binaryBuf.length + data.length);
      combined.set(binaryBuf);
      combined.set(data, binaryBuf.length);
      const { points, rest } = parseBinaryFrames(combined, getHeader());
      binaryBuf = rest;
      for (const pt of points) {
        pushPoint(pt);
        frameCount.value += 1;
      }
    }
  }

  function clear() {
    series.value = {};
    order.value = [];
    frameCount.value = 0;
    autoX = 0;
    binaryBuf = new Uint8Array(0);
    textBuf = "";
  }

  /** 导出 CSV（全部曲线） */
  function exportCsv(): string {
    const names = order.value.filter((n) => series.value[n]);
    if (names.length === 0) return "";
    // 用 x 对齐（简单方案：逐点导出）
    const rows: string[] = [["x", ...names].join(",")];
    const maxLen = Math.max(...names.map((n) => series.value[n].xs.length));
    for (let i = 0; i < maxLen; i++) {
      const row: string[] = [];
      for (const n of names) {
        const s = series.value[n];
        row.push(s.xs[i] !== undefined ? String(s.xs[i]) : "");
      }
      rows.push(row.join(","));
    }
    return rows.join("\n");
  }

  return {
    enabled,
    protocol,
    headerHex,
    xRange,
    autoScroll,
    paused,
    seriesList,
    frameCount,
    processData,
    clear,
    exportCsv,
    setSeriesColor,
  };
});
