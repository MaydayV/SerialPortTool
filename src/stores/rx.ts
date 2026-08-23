// 接收数据 store：RX 缓冲、渲染数据、统计、日志
import { defineStore } from "pinia";
import { ref, computed, markRaw, shallowReactive, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  bytesToHex,
  decodeText,
  createAnsiParserState,
  parseAnsiChunk,
  formatTime,
  formatBytes,
  type ColorSpan,
} from "../utils/bytes";
import { bytesToAscii } from "../utils/bytes";
import { useProtocolStore } from "./protocol";
import { useGraphStore } from "./graph";

export interface RxEntry {
  id: number;
  ts: number; // ms
  dir: "rx" | "tx"; // 收发方向
  raw: Uint8Array;
  peer?: string;
  decodedEncoding?: string;
  decodedText?: string;
  decodedSpans?: ColorSpan[];
}

interface EntryDisplayCache {
  hex?: string;
  ascii?: string;
}

// 显示上限：虚拟滚动 + 截断（防内存膨胀）
const MAX_ENTRIES = 20000;
const PRUNE_BATCH = 512;
// 原始字节留 24 MiB，给文本/HEX/ANSI 缓存和 Vue/ECharts 留出明确余量。
const MAX_STORED_BYTES = 24 * 1024 * 1024;
const TARGET_STORED_BYTES = 20 * 1024 * 1024;
const MAX_DISPLAY_CACHE_CHARS = 8 * 1024 * 1024;
const MAX_DECODE_STREAMS = 128;

export const useRxStore = defineStore("rx", () => {
  const entries = ref<RxEntry[]>([]);
  const encoding = ref("UTF-8");
  const rxHexMode = ref(false); // 接收显示 hex
  const asciiMode = ref(false); // 接收显示 ascii（字节映射，与 hex 互斥）
  const dualMode = ref(false); // HEX + ASCII 双栏
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
  let storedBytes = 0;
  let displayCache = new Map<number, EntryDisplayCache>();
  let displayCacheChars = 0;
  let logWriter: (line: string) => void = () => {};
  let listenerReady = false;
  let listenerSetup: Promise<void> | null = null;
  let unlistenRx: UnlistenFn | null = null;
  const textStreams = new Map<string, TextDecoder | null>();
  const ansiStreams = new Map<string, ReturnType<typeof createAnsiParserState>>();
  const decodeStreamOrder = new Map<string, true>();
  const filteredCache = shallowReactive<RxEntry[]>([]);

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
  /** 过滤后的条目；过滤条件变化时全量构建，后续接收只增量匹配新条目。 */
  const filteredEntries = computed(() => {
    const kw = filterText.value.trim().toLowerCase();
    if (!kw) return entries.value;
    return filteredCache;
  });
  /** 过滤后总行数（用于虚拟滚动高度） */
  const filteredCount = computed(() => filteredEntries.value.length);

  function rawContainsHex(data: Uint8Array, query: string): boolean {
    const cleaned = query.replace(/0x/gi, "").replace(/[\s,]+/g, "");
    if (!cleaned || cleaned.length % 2 !== 0 || !/^[0-9a-f]+$/i.test(cleaned)) {
      return false;
    }
    const needle = new Uint8Array(cleaned.length / 2);
    for (let i = 0; i < needle.length; i++) {
      needle[i] = Number.parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
    }
    outer: for (let i = 0; i <= data.length - needle.length; i++) {
      for (let j = 0; j < needle.length; j++) {
        if (data[i + j] !== needle[j]) continue outer;
      }
      return true;
    }
    return false;
  }

  function cacheFor(entry: RxEntry): EntryDisplayCache {
    const current = displayCache.get(entry.id);
    if (current) {
      displayCache.delete(entry.id);
      displayCache.set(entry.id, current);
      return current;
    }
    const next: EntryDisplayCache = {};
    displayCache.set(entry.id, next);
    return next;
  }

  function cacheString(
    entry: RxEntry,
    kind: "hex" | "ascii",
    value: string
  ): string {
    const cache = cacheFor(entry);
    displayCacheChars -= cache[kind]?.length ?? 0;
    cache[kind] = value;
    displayCacheChars += value.length;
    while (displayCacheChars > MAX_DISPLAY_CACHE_CHARS && displayCache.size) {
      const oldestId = displayCache.keys().next().value as number | undefined;
      if (oldestId === undefined) break;
      const oldest = displayCache.get(oldestId)!;
      displayCacheChars -= (oldest.hex?.length ?? 0) + (oldest.ascii?.length ?? 0);
      displayCache.delete(oldestId);
    }
    return value;
  }

  function streamKey(entry: Pick<RxEntry, "dir" | "peer">): string {
    return `${entry.dir}:${entry.peer ?? "default"}`;
  }

  function decoderLabel(): string {
    return encoding.value === "UTF-8" ? "utf-8" : encoding.value.toLowerCase();
  }

  function touchDecodeStream(key: string) {
    if (!decodeStreamOrder.has(key) && decodeStreamOrder.size >= MAX_DECODE_STREAMS) {
      const oldest = decodeStreamOrder.keys().next().value as string | undefined;
      if (oldest !== undefined) {
        decodeStreamOrder.delete(oldest);
        textStreams.delete(oldest);
        ansiStreams.delete(oldest);
      }
    }
    decodeStreamOrder.delete(key);
    decodeStreamOrder.set(key, true);
  }

  function decodeStream(entry: RxEntry): string {
    const key = streamKey(entry);
    touchDecodeStream(key);
    if (encoding.value === "ASCII") return decodeText(entry.raw, "ASCII");
    if (!textStreams.has(key)) {
      try {
        textStreams.set(key, new TextDecoder(decoderLabel()));
      } catch {
        textStreams.set(key, new TextDecoder("utf-8", { fatal: false }));
      }
    }
    return textStreams.get(key)!.decode(entry.raw, { stream: true });
  }

  function cacheStreamingDisplay(entry: RxEntry) {
    const text = decodeStream(entry);
    const key = streamKey(entry);
    let ansi = ansiStreams.get(key);
    if (!ansi) {
      ansi = createAnsiParserState();
      ansiStreams.set(key, ansi);
    }
    entry.decodedEncoding = encoding.value;
    entry.decodedText = text;
    entry.decodedSpans = parseAnsiChunk(text, ansi);
  }

  function rebuildDisplayCaches() {
    displayCache = new Map<number, EntryDisplayCache>();
    displayCacheChars = 0;
    textStreams.clear();
    ansiStreams.clear();
    decodeStreamOrder.clear();
    for (const entry of entries.value) cacheStreamingDisplay(entry);
  }

  function getEntryText(entry: RxEntry): string {
    if (entry.decodedEncoding !== encoding.value || entry.decodedText === undefined) {
      rebuildDisplayCaches();
    }
    return entry.decodedText ?? "";
  }

  function getEntryHex(entry: RxEntry): string {
    const cache = cacheFor(entry);
    return cache.hex ?? cacheString(entry, "hex", bytesToHex(entry.raw));
  }

  function getEntryAscii(entry: RxEntry): string {
    const cache = cacheFor(entry);
    return cache.ascii ?? cacheString(entry, "ascii", bytesToAscii(entry.raw));
  }

  function getEntrySpans(entry: RxEntry): ColorSpan[] {
    if (entry.decodedEncoding !== encoding.value || !entry.decodedSpans) {
      rebuildDisplayCaches();
    }
    return entry.decodedSpans ?? [{ text: getEntryText(entry) }];
  }

  function makeEntry(
    data: Uint8Array,
    dir: "rx" | "tx",
    ts: number,
    peer?: string
  ): RxEntry {
    return markRaw({ id: ++seq, ts, dir, raw: markRaw(data), peer });
  }

  function matchesFilter(entry: RxEntry, keyword = filterText.value.trim().toLowerCase()) {
    return !!keyword && (
      getEntryText(entry).toLowerCase().includes(keyword) || rawContainsHex(entry.raw, keyword)
    );
  }

  function rebuildFilter() {
    const keyword = filterText.value.trim().toLowerCase();
    const matches = keyword
      ? entries.value.filter((entry) => matchesFilter(entry, keyword))
      : [];
    filteredCache.splice(0, filteredCache.length, ...matches);
  }

  function pruneEntries() {
    const overCount = entries.value.length > MAX_ENTRIES + PRUNE_BATCH;
    const overBytes = storedBytes > MAX_STORED_BYTES;
    if (!overCount && !overBytes) return;

    let removeCount = overCount
      ? Math.max(0, entries.value.length - MAX_ENTRIES)
      : 0;
    let removedBytes = 0;
    for (let i = 0; i < removeCount; i++) {
      removedBytes += entries.value[i].raw.byteLength;
    }
    while (
      removeCount < entries.value.length &&
      storedBytes - removedBytes > TARGET_STORED_BYTES
    ) {
      removedBytes += entries.value[removeCount].raw.byteLength;
      removeCount += 1;
    }
    if (removeCount > 0) {
      // 只做一次头部移动；格式缓存采用有界 LRU。
      entries.value.splice(0, removeCount);
      storedBytes -= removedBytes;
      for (const id of [...displayCache.keys()]) {
        if (id < (entries.value[0]?.id ?? Number.POSITIVE_INFINITY)) {
          const cached = displayCache.get(id)!;
          displayCacheChars -= (cached.hex?.length ?? 0) + (cached.ascii?.length ?? 0);
          displayCache.delete(id);
        }
      }
      if (filterText.value.trim()) {
        const firstId = entries.value[0]?.id ?? Number.POSITIVE_INFINITY;
        const retained = filteredCache.filter((entry) => entry.id >= firstId);
        filteredCache.splice(0, filteredCache.length, ...retained);
      }
    }
  }

  function recordWire(
    data: Uint8Array,
    dir: "rx" | "tx",
    ts = Date.now(),
    peer?: string,
    byteCount = data.length
  ) {
    if (dir === "rx") {
      rxBytes.value += byteCount;
      rxCount.value += 1;
      rxWindowBytes += byteCount;
    } else {
      txBytes.value += byteCount;
      txCount.value += 1;
      txWindowBytes += byteCount;
    }
    if (saveLog.value) logWriter(logWireLine(data, dir, ts, peer));
  }

  /** 追加显示记录；trackWire=false 用于协议解出的帧，避免重复统计/日志。 */
  function append(
    data: Uint8Array,
    dir: "rx" | "tx",
    ts = Date.now(),
    peer?: string,
    trackWire = true,
    wireByteCount = data.length
  ) {
    // 暂停=缓冲模式：数据继续接收，仅停止自动滚动（由面板控制）
    const entry = makeEntry(data, dir, ts, peer);
    cacheStreamingDisplay(entry);
    entries.value.push(entry);
    storedBytes += data.byteLength;
    pruneEntries();
    if (trackWire) recordWire(data, dir, ts, peer, wireByteCount);
    if (filterText.value.trim() && matchesFilter(entry)) {
      filteredCache.push(entry);
    }
  }

  function logWireLine(
    data: Uint8Array,
    dir: "rx" | "tx",
    ts: number,
    peer?: string
  ): string {
    const head = dir === "rx" ? "<= " : "=> ";
    const source = peer ? `[${peer}] ` : "";
    // 持续日志始终记录无损原始 HEX，不受当前显示模式或协议解帧影响。
    return `${head}[${formatTime(ts)}] ${source}${bytesToHex(data)}\n`;
  }

  /** 设置日志写入器（由主组件注入 fs 实现） */
  function setLogWriter(fn: (line: string) => void) {
    logWriter = fn;
  }

  function clear() {
    entries.value = [];
    storedBytes = 0;
    displayCache = new Map<number, EntryDisplayCache>();
    displayCacheChars = 0;
    textStreams.clear();
    ansiStreams.clear();
    decodeStreamOrder.clear();
    filteredCache.splice(0, filteredCache.length);
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
    if (listenerReady) return;
    if (listenerSetup) return listenerSetup;
    listenerSetup = (async () => {
      const unlisten = await listen<{ data: number[]; ts: number; peer?: string }>(
        "rx-data",
        (e) => {
          const raw = new Uint8Array(e.payload.data);
          const peer = typeof e.payload.peer === "string" ? e.payload.peer : undefined;
          const key = peer ?? "default";
          // 波形与接收显示共享同一条事件管线，避免重复反序列化和监听。
          useGraphStore().processData(raw, key);
          const proto = useProtocolStore();
          const { frames, enabled } = proto.processRx(raw, key);
          if (!enabled) {
            append(raw, "rx", e.payload.ts, peer);
          } else {
            recordWire(raw, "rx", e.payload.ts, peer);
            for (const frame of frames) append(frame, "rx", e.payload.ts, peer, false);
          }
        }
      );
      unlistenRx = unlisten;
      listenerReady = true;
    })();
    try {
      await listenerSetup;
    } finally {
      listenerSetup = null;
    }
  }

  function teardown() {
    unlistenRx?.();
    unlistenRx = null;
    listenerReady = false;
  }

  watch(encoding, () => {
    rebuildDisplayCaches();
    rebuildFilter();
  });
  watch(filterText, rebuildFilter);

  return {
    entries,
    encoding,
    rxHexMode,
    asciiMode,
    dualMode,
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
    teardown,
    setLogWriter,
    startRateTimer,
    stopRateTimer,
    formatBytes,
    getEntryText,
    getEntryHex,
    getEntryAscii,
    getEntrySpans,
  };
});
