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

export interface RxEntry {
  id: number;
  ts: number; // ms
  dir: "rx" | "tx"; // 收发方向
  hex: string;
  text: string;
  spans: ColorSpan[]; // 预解析的 ANSI 分段
  raw: Uint8Array;
}

// 显示上限：虚拟滚动 + 截断（防内存膨胀）
const MAX_ENTRIES = 20000;

export const useRxStore = defineStore("rx", () => {
  const entries = ref<RxEntry[]>([]);
  const encoding = ref("UTF-8");
  const rxHexMode = ref(false); // 接收显示 hex
  const showTimestamp = ref(false);
  const autoScroll = ref(true);
  const paused = ref(false);
  const rxBytes = ref(0);
  const txBytes = ref(0);
  const rxCount = ref(0);
  const txCount = ref(0);
  const saveLog = ref(false);
  const logPath = ref("");

  let seq = 0;
  let logWriter: (line: string) => void = () => {};

  const totalEntries = computed(() => entries.value.length);
  const lastEntry = computed(() =>
    entries.value.length ? entries.value[entries.value.length - 1] : null
  );

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
      spans: parseAnsi(text),
      raw: data,
    };
  }

  /** 追加一条（内部：先解码再推入） */
  function append(data: Uint8Array, dir: "rx" | "tx", ts = Date.now()) {
    if (paused.value && dir === "rx") return;
    const entry = makeEntry(data, dir, ts);
    entries.value.push(entry);
    if (entries.value.length > MAX_ENTRIES) {
      entries.value.splice(0, entries.value.length - MAX_ENTRIES);
    }
    if (dir === "rx") {
      rxBytes.value += data.length;
      rxCount.value += 1;
    } else {
      txBytes.value += data.length;
      txCount.value += 1;
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
  }

  function togglePause() {
    paused.value = !paused.value;
  }

  /** 接收事件监听（由 App 初始化时调用一次） */
  async function setup() {
    await listen<{ data: number[]; ts: number }>("rx-data", (e) => {
      append(new Uint8Array(e.payload.data), "rx", e.payload.ts);
    });
  }

  return {
    entries,
    encoding,
    rxHexMode,
    showTimestamp,
    autoScroll,
    paused,
    rxBytes,
    txBytes,
    rxCount,
    txCount,
    saveLog,
    logPath,
    totalEntries,
    lastEntry,
    append,
    clear,
    togglePause,
    setup,
    setLogWriter,
    formatBytes,
  };
});
