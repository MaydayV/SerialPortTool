// 接收数据 store：RX 缓冲、渲染数据、统计、日志
import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { listen } from "@tauri-apps/api/event";
import {
  bytesToHex,
  decodeText,
  parseAnsi,
  formatTime,
  formatBytes,
  type ColorSpan,
} from "../utils/bytes";
import { bytesToAscii } from "../utils/bytes";
import { useProtocolStore } from "./protocol";

export interface RxEntry {
  id: number;
  ts: number; // ms
  dir: "rx" | "tx"; // 收发方向
  hex: string;
  text: string;
  ascii: string; // 原始字节 ASCII 映射（不可打印→.）
  spans: ColorSpan[]; // 预解析的 ANSI 分段
  raw: Uint8Array;
}

// 显示上限：虚拟滚动 + 截断（防内存膨胀）
const MAX_ENTRIES = 20000;
const PRUNE_BATCH = 512;

export const useRxStore = defineStore("rx", () => {
  const entries = ref<RxEntry[]>([]);
  const encoding = ref("UTF-8");
  const rxHexMode = ref(false); // 接收显示 hex
  const asciiMode = ref(false); // 接收显示 ascii（字节映射，与 hex 互斥）
  const showLineNo = ref(false); // 行号
  const showTimestamp = ref(false);
  const autoScroll = ref(true);
  const paused = ref(false);
  const rxBytes = ref(0);
  const txBytes = ref(0);
  const rxCount = ref(0);
  const txCount = ref(0);
  const saveLog = ref(false);
  const logPath = ref("");
  const logError = ref("");
  const filterText = ref(""); // 接收区关键字过滤
  const fontSize = ref(12.5); // 接收区字号（px）

  let seq = 0;
  let logWriter: (line: string) => void = () => {};

  // ===== 实时速率统计（滑动窗口 1s）=====
  const rxRate = ref(0); // B/s
  const txRate = ref(0);
  let rxWindowBytes = 0;
  let txWindowBytes = 0;
  let rateTimer: ReturnType<typeof setInterval> | null = null;

  function startRateTimer() {
    if (rateTimer) return;
    rateTimer = setInterval(() => {
      rxRate.value = rxWindowBytes;
      txRate.value = txWindowBytes;
      rxWindowBytes = 0;
      txWindowBytes = 0;
    }, 1000);
  }
  startRateTimer();

  function stopRateTimer() {
    if (rateTimer) {
      clearInterval(rateTimer);
      rateTimer = null;
    }
  }

  const totalEntries = computed(() => entries.value.length);
  const lastEntry = computed(() =>
    entries.value.length ? entries.value[entries.value.length - 1] : null
  );
  /** 过滤后的条目（关键字匹配 text 或 hex，忽略大小写） */
  const filteredEntries = computed(() => {
    const kw = filterText.value.trim().toLowerCase();
    if (!kw) return entries.value;
    return entries.value.filter(
      (e) =>
        e.text.toLowerCase().includes(kw) || e.hex.toLowerCase().includes(kw)
    );
  });
  /** 过滤后总行数（用于虚拟滚动高度） */
  const filteredCount = computed(() => filteredEntries.value.length);

  function makeEntry(
    data: Uint8Array,
    dir: "rx" | "tx",
    ts: number
  ): RxEntry {
    const text = decodeText(data, encoding.value);
    return {
      id: ++seq,
      ts,
      dir,
      hex: bytesToHex(data),
      text,
      ascii: bytesToAscii(data),
      spans: parseAnsi(text),
      raw: data,
    };
  }

  /** 追加一条（内部：先解码再推入） */
  function append(data: Uint8Array, dir: "rx" | "tx", ts = Date.now()) {
    // 暂停=缓冲模式：数据继续接收，仅停止自动滚动（由面板控制）
    const entry = makeEntry(data, dir, ts);
    entries.value.push(entry);
    if (entries.value.length > MAX_ENTRIES + PRUNE_BATCH) {
      // 批量淘汰，避免高频接收时每条数据都触发 O(n) 头部移动。
      entries.value.splice(0, entries.value.length - MAX_ENTRIES);
    }
    if (dir === "rx") {
      rxBytes.value += data.length;
      rxCount.value += 1;
      rxWindowBytes += data.length;
    } else {
      txBytes.value += data.length;
      txCount.value += 1;
      txWindowBytes += data.length;
    }
    if (saveLog.value) {
      logWriter(logLine(entry));
    }
  }

  function logLine(e: RxEntry): string {
    const head = e.dir === "rx" ? "<= " : "=> ";
    const t = showTimestamp.value ? `[${formatTime(e.ts)}] ` : "";
    const body = rxHexMode.value || e.dir === "tx" ? e.hex : e.text;
    return `${head}${t}${body}\n`;
  }

  /** 设置日志写入器（由主组件注入 fs 实现） */
  function setLogWriter(fn: (line: string) => void) {
    logWriter = fn;
  }

  function clear() {
    entries.value = [];
    seq = 0;
    rxBytes.value = 0;
    txBytes.value = 0;
    rxCount.value = 0;
    txCount.value = 0;
    rxWindowBytes = 0;
    txWindowBytes = 0;
    rxRate.value = 0;
    txRate.value = 0;
  }

  function togglePause() {
    paused.value = !paused.value;
    if (paused.value) autoScroll.value = false; // 暂停=停止跟随
  }

  /** 接收事件监听（由 App 初始化时调用一次） */
  async function setup() {
    await listen<{ data: number[]; ts: number }>("rx-data", (e) => {
      const raw = new Uint8Array(e.payload.data);
      // 协议解帧（若启用）
      const proto = useProtocolStore();
      const { frames, enabled } = proto.processRx(raw);
      if (!enabled) {
        append(raw, "rx", e.payload.ts);
      } else {
        for (const f of frames) {
          append(f, "rx", e.payload.ts);
        }
      }
    });
  }

  return {
    entries,
    encoding,
    rxHexMode,
    asciiMode,
    showLineNo,
    showTimestamp,
    autoScroll,
    paused,
    rxBytes,
    txBytes,
    rxCount,
    txCount,
    rxRate,
    txRate,
    saveLog,
    logPath,
    logError,
    filterText,
    fontSize,
    totalEntries,
    lastEntry,
    filteredEntries,
    filteredCount,
    append,
    clear,
    togglePause,
    setup,
    setLogWriter,
    stopRateTimer,
    formatBytes,
  };
});
