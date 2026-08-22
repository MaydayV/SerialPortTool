// 会话持久化：自动保存/恢复所有 store 配置到 localStorage
import { watch } from "vue";
import { useConnStore } from "./conn";
import { useRxStore } from "./rx";
import { useTxStore } from "./tx";
import { useProtocolStore } from "./protocol";
import { useGraphStore } from "./graph";

const KEY = "serialaid.config.v1";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFinitePositive(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

interface Persisted {
  connType: "serial" | "tcpudp";
  serial: object;
  tcpudp: object;
  profiles: object[];
  rx: {
    encoding: string;
    rxHexMode: boolean;
    asciiMode: boolean;
    showLineNo: boolean;
    showTimestamp: boolean;
    fontSize: number;
    saveLog: boolean;
    logPath: string;
  };
  tx: {
    sendHexMode: boolean;
    appendNewline: boolean;
    useCRLF: boolean;
    escapeMode: boolean;
    scheduledInterval: number;
    history: string[];
    customItems: { id: number; text: string }[];
  };
  proto: {
    templates: object[];
    activeName: string;
    rxEnabled: boolean;
    txEnabled: boolean;
  };
  graph: {
    protocol: "ascii" | "binary";
    headerHex: string;
    xRange: number;
    autoScroll: boolean;
    enabled: boolean;
    paused: boolean;
  };
  theme: "light" | "dark" | "system";
}

export function saveConfig(theme: string) {
  const conn = useConnStore();
  const rx = useRxStore();
  const tx = useTxStore();
  const proto = useProtocolStore();
  const graph = useGraphStore();

  const data: Persisted = {
    connType: conn.connType,
    serial: { ...conn.serial },
    tcpudp: { ...conn.tcpudp },
    profiles: JSON.parse(JSON.stringify(conn.profiles)),
    rx: {
      encoding: rx.encoding,
      rxHexMode: rx.rxHexMode,
      asciiMode: rx.asciiMode,
      showLineNo: rx.showLineNo,
      showTimestamp: rx.showTimestamp,
      fontSize: rx.fontSize,
      saveLog: rx.saveLog,
      logPath: rx.logPath,
    },
    tx: {
      sendHexMode: tx.sendHexMode,
      appendNewline: tx.appendNewline,
      useCRLF: tx.useCRLF,
      escapeMode: tx.escapeMode,
      scheduledInterval: tx.scheduledInterval,
      history: tx.history,
      customItems: tx.customItems.map((i) => ({ ...i })),
    },
    proto: {
      templates: JSON.parse(JSON.stringify(proto.templates)),
      activeName: proto.activeName,
      rxEnabled: proto.rxEnabled,
      txEnabled: proto.txEnabled,
    },
    graph: {
      protocol: graph.protocol,
      headerHex: graph.headerHex,
      xRange: graph.xRange,
      autoScroll: graph.autoScroll,
      enabled: graph.enabled,
      paused: graph.paused,
    },
    theme: theme as "light" | "dark" | "system",
  };
  localStorage.setItem(KEY, JSON.stringify(data));
}

export function loadConfig(themeRef: { value: "light" | "dark" | "system" }) {
  const raw = localStorage.getItem(KEY);
  if (!raw) return;
  try {
    const d = JSON.parse(raw) as Persisted;
    const conn = useConnStore();
    const rx = useRxStore();
    const tx = useTxStore();
    const proto = useProtocolStore();
    const graph = useGraphStore();

    conn.connType = d.connType === "tcpudp" ? "tcpudp" : "serial";
    if (isRecord(d.serial)) {
      if (typeof d.serial.port === "string") conn.serial.port = d.serial.port;
      if (isFinitePositive(d.serial.baudrate)) conn.serial.baudrate = d.serial.baudrate;
      if ([5, 6, 7, 8].includes(d.serial.data_bits as number)) conn.serial.data_bits = d.serial.data_bits as number;
      if (["none", "odd", "even"].includes(d.serial.parity as string)) conn.serial.parity = d.serial.parity as string;
      if (d.serial.stop_bits === 1 || d.serial.stop_bits === 2) conn.serial.stop_bits = d.serial.stop_bits as number;
      if (["none", "software", "hardware"].includes(d.serial.flow_control as string)) conn.serial.flow_control = d.serial.flow_control as string;
      if (typeof d.serial.rts === "boolean") conn.serial.rts = d.serial.rts;
      if (typeof d.serial.dtr === "boolean") conn.serial.dtr = d.serial.dtr;
      if (typeof d.serial.auto_reconnect === "boolean") conn.serial.auto_reconnect = d.serial.auto_reconnect;
    }
    if (isRecord(d.tcpudp)) {
      if (d.tcpudp.protocol === "tcp" || d.tcpudp.protocol === "udp") conn.tcpudp.protocol = d.tcpudp.protocol;
      if (d.tcpudp.mode === "client" || d.tcpudp.mode === "server") conn.tcpudp.mode = d.tcpudp.mode;
      if (typeof d.tcpudp.target === "string") conn.tcpudp.target = d.tcpudp.target;
      if (Number.isInteger(d.tcpudp.port) && (d.tcpudp.port as number) > 0 && (d.tcpudp.port as number) <= 65535) conn.tcpudp.port = d.tcpudp.port as number;
      if (typeof d.tcpudp.auto_reconnect === "boolean") conn.tcpudp.auto_reconnect = d.tcpudp.auto_reconnect;
      if (isFinitePositive(d.tcpudp.reconnect_interval)) conn.tcpudp.reconnect_interval = d.tcpudp.reconnect_interval;
    }
    if (Array.isArray(d.profiles)) {
      conn.profiles = d.profiles.filter((p): p is Record<string, unknown> =>
        isRecord(p) && typeof p.name === "string" && (p.connType === "serial" || p.connType === "tcpudp") && isRecord(p.serial) && isRecord(p.tcpudp)
      ) as any;
    }
    Object.assign(rx, {
      encoding: d.rx?.encoding ?? rx.encoding,
      rxHexMode: d.rx?.rxHexMode ?? rx.rxHexMode,
      asciiMode: d.rx?.asciiMode ?? rx.asciiMode,
      showLineNo: d.rx?.showLineNo ?? rx.showLineNo,
      showTimestamp: d.rx?.showTimestamp ?? rx.showTimestamp,
      fontSize: d.rx?.fontSize ?? rx.fontSize,
      saveLog: d.rx?.saveLog ?? rx.saveLog,
      logPath: typeof d.rx?.logPath === "string" ? d.rx.logPath : rx.logPath,
    });
    Object.assign(tx, {
      sendHexMode: d.tx?.sendHexMode ?? tx.sendHexMode,
      appendNewline: d.tx?.appendNewline ?? tx.appendNewline,
      useCRLF: d.tx?.useCRLF ?? tx.useCRLF,
      escapeMode: d.tx?.escapeMode ?? tx.escapeMode,
      scheduledInterval:
        typeof d.tx?.scheduledInterval === "number" && d.tx.scheduledInterval >= 10
          ? d.tx.scheduledInterval
          : tx.scheduledInterval,
      history: Array.isArray(d.tx?.history) ? d.tx.history.filter((x): x is string => typeof x === "string").slice(0, 20) : tx.history,
      customItems: Array.isArray(d.tx?.customItems)
        ? d.tx.customItems.filter((x) => x && Number.isSafeInteger(x.id) && typeof x.text === "string")
        : tx.customItems,
    });
    if (d.proto && Array.isArray(d.proto.templates)) {
      proto.replaceTemplates(d.proto.templates);
      if (typeof d.proto.activeName === "string" && proto.templates.some((t) => t.name === d.proto.activeName)) {
        proto.select(d.proto.activeName);
      }
      if (typeof d.proto.rxEnabled === "boolean") proto.rxEnabled = d.proto.rxEnabled;
      if (typeof d.proto.txEnabled === "boolean") proto.txEnabled = d.proto.txEnabled;
    }
    if (d.graph) {
      if (d.graph.protocol === "ascii" || d.graph.protocol === "binary") graph.protocol = d.graph.protocol;
      if (typeof d.graph.headerHex === "string") graph.headerHex = d.graph.headerHex;
      if (typeof d.graph.xRange === "number" && d.graph.xRange > 0) graph.xRange = d.graph.xRange;
      if (typeof d.graph.autoScroll === "boolean") graph.autoScroll = d.graph.autoScroll;
      if (typeof d.graph.enabled === "boolean") graph.enabled = d.graph.enabled;
      if (typeof d.graph.paused === "boolean") graph.paused = d.graph.paused;
    }
    if (d.theme) themeRef.value = d.theme as typeof themeRef.value;
  } catch (e) {
    console.warn("config load failed", e);
  }
}

/** 启动持久化订阅（App 初始化调用） */
export function initPersistence(themeRef: { value: "light" | "dark" | "system" }) {
  // 防抖保存
  let timer: ReturnType<typeof setTimeout> | null = null;
  const schedule = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => saveConfig(themeRef.value), 500);
  };

  const conn = useConnStore();
  const rx = useRxStore();
  const tx = useTxStore();
  const proto = useProtocolStore();
  const graph = useGraphStore();

  watch(
    [
      () => conn.connType,
      () => conn.serial,
      () => conn.tcpudp,
      () => conn.profiles,
      () => rx.encoding,
      () => rx.rxHexMode,
      () => rx.asciiMode,
      () => rx.showLineNo,
      () => rx.showTimestamp,
      () => rx.fontSize,
      () => rx.saveLog,
      () => rx.logPath,
      () => tx.sendHexMode,
      () => tx.appendNewline,
      () => tx.useCRLF,
      () => tx.escapeMode,
      () => tx.scheduledInterval,
      () => tx.history,
      () => tx.customItems,
      () => proto.templates,
      () => proto.activeName,
      () => proto.rxEnabled,
      () => proto.txEnabled,
      () => graph.protocol,
      () => graph.headerHex,
      () => graph.xRange,
      () => graph.autoScroll,
      () => graph.enabled,
      () => graph.paused,
      () => themeRef.value,
    ],
    schedule,
    { deep: true }
  );
}
