// 会话持久化：自动保存/恢复所有 store 配置到 localStorage
import { watch } from "vue";
import { useConnStore } from "./conn";
import { useRxStore } from "./rx";
import { useTxStore } from "./tx";
import { useProtocolStore } from "./protocol";
import { useGraphStore } from "./graph";
import type { SerialConfig, TcpUdpConfig } from "../api";

const KEY = "serialaid.config.v1";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFinitePositive(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

const ENCODINGS = ["UTF-8", "ASCII", "GBK", "GB2312", "GB18030", "UTF-16"];

function normalizeSerial(raw: unknown, fallback: SerialConfig): SerialConfig {
  if (!isRecord(raw)) return { ...fallback };
  return {
    port: typeof raw.port === "string" ? raw.port.slice(0, 1024) : fallback.port,
    baudrate:
      Number.isSafeInteger(raw.baudrate) && (raw.baudrate as number) > 0 && (raw.baudrate as number) <= 10_000_000
        ? raw.baudrate as number
        : fallback.baudrate,
    data_bits: [5, 6, 7, 8].includes(raw.data_bits as number)
      ? raw.data_bits as number
      : fallback.data_bits,
    parity: ["none", "odd", "even"].includes(raw.parity as string)
      ? raw.parity as string
      : fallback.parity,
    stop_bits: raw.stop_bits === 1 || raw.stop_bits === 2 ? raw.stop_bits : fallback.stop_bits,
    flow_control: ["none", "software", "hardware"].includes(raw.flow_control as string)
      ? raw.flow_control as string
      : fallback.flow_control,
    rts: typeof raw.rts === "boolean" ? raw.rts : fallback.rts,
    dtr: typeof raw.dtr === "boolean" ? raw.dtr : fallback.dtr,
    auto_reconnect:
      typeof raw.auto_reconnect === "boolean" ? raw.auto_reconnect : fallback.auto_reconnect,
  };
}

function normalizeTcpUdp(raw: unknown, fallback: TcpUdpConfig): TcpUdpConfig {
  if (!isRecord(raw)) return { ...fallback };
  return {
    protocol: raw.protocol === "udp" ? "udp" : raw.protocol === "tcp" ? "tcp" : fallback.protocol,
    mode: raw.mode === "server" ? "server" : raw.mode === "client" ? "client" : fallback.mode,
    target: typeof raw.target === "string" ? raw.target.slice(0, 2048) : fallback.target,
    port:
      Number.isInteger(raw.port) && (raw.port as number) >= 0 && (raw.port as number) <= 65535
        ? raw.port as number
        : fallback.port,
    local_port:
      Number.isInteger(raw.local_port) && (raw.local_port as number) >= 0 && (raw.local_port as number) <= 65535
        ? raw.local_port as number
        : fallback.local_port,
    auto_reconnect:
      typeof raw.auto_reconnect === "boolean" ? raw.auto_reconnect : fallback.auto_reconnect,
    reconnect_interval:
      isFinitePositive(raw.reconnect_interval)
        ? Math.min(raw.reconnect_interval, 3600)
        : fallback.reconnect_interval,
  };
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
    dualMode: boolean;
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
      dualMode: rx.dualMode,
      showLineNo: rx.showLineNo,
      showTimestamp: rx.showTimestamp,
      fontSize: rx.fontSize,
      // 沙盒文件授权只在本次会话有效，不持久化路径或自动写入状态。
      saveLog: false,
      logPath: "",
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
  try {
    localStorage.setItem(KEY, JSON.stringify(data));
  } catch (error) {
    console.warn("config save failed", error);
  }
}

export function loadConfig(themeRef: { value: "light" | "dark" | "system" }) {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return;
    const d = JSON.parse(raw) as Persisted;
    const conn = useConnStore();
    const rx = useRxStore();
    const tx = useTxStore();
    const proto = useProtocolStore();
    const graph = useGraphStore();

    conn.connType = d.connType === "tcpudp" ? "tcpudp" : "serial";
    Object.assign(conn.serial, normalizeSerial(d.serial, conn.serial));
    Object.assign(conn.tcpudp, normalizeTcpUdp(d.tcpudp, conn.tcpudp));
    if (Array.isArray(d.profiles)) {
      const names = new Set<string>();
      conn.profiles = d.profiles.slice(0, 100).flatMap((profile) => {
        if (!isRecord(profile) || typeof profile.name !== "string") return [];
        const name = profile.name.trim().slice(0, 100);
        if (!name || names.has(name)) return [];
        names.add(name);
        return [{
          name,
          connType: profile.connType === "tcpudp" ? "tcpudp" as const : "serial" as const,
          serial: normalizeSerial(profile.serial, conn.serial),
          tcpudp: normalizeTcpUdp(profile.tcpudp, conn.tcpudp),
        }];
      });
    }
    if (ENCODINGS.includes(d.rx?.encoding)) rx.encoding = d.rx.encoding;
    if (typeof d.rx?.rxHexMode === "boolean") rx.rxHexMode = d.rx.rxHexMode;
    if (typeof d.rx?.asciiMode === "boolean") rx.asciiMode = d.rx.asciiMode;
    if (typeof d.rx?.dualMode === "boolean") rx.dualMode = d.rx.dualMode;
    if (rx.dualMode) {
      rx.rxHexMode = false;
      rx.asciiMode = false;
    } else if (rx.rxHexMode && rx.asciiMode) {
      rx.asciiMode = false;
    }
    if (typeof d.rx?.showLineNo === "boolean") rx.showLineNo = d.rx.showLineNo;
    if (typeof d.rx?.showTimestamp === "boolean") rx.showTimestamp = d.rx.showTimestamp;
    if (typeof d.rx?.fontSize === "number" && Number.isFinite(d.rx.fontSize)) {
      rx.fontSize = Math.min(20, Math.max(10, d.rx.fontSize));
    }
    // App Sandbox 的用户文件授权不跨启动恢复，必须重新通过系统面板选择。
    rx.saveLog = false;
    rx.logPath = "";
    if (typeof d.tx?.sendHexMode === "boolean") tx.sendHexMode = d.tx.sendHexMode;
    if (typeof d.tx?.appendNewline === "boolean") tx.appendNewline = d.tx.appendNewline;
    if (typeof d.tx?.useCRLF === "boolean") tx.useCRLF = d.tx.useCRLF;
    if (typeof d.tx?.escapeMode === "boolean") tx.escapeMode = d.tx.escapeMode;
    if (tx.sendHexMode && tx.escapeMode) tx.escapeMode = false;
    if (typeof d.tx?.scheduledInterval === "number" && Number.isFinite(d.tx.scheduledInterval)) {
      tx.scheduledInterval = Math.min(3_600_000, Math.max(10, d.tx.scheduledInterval));
    }
    tx.history = Array.isArray(d.tx?.history)
      ? d.tx.history.filter((x): x is string => typeof x === "string").map((x) => x.slice(0, 65_536)).slice(0, 20)
      : tx.history;
    if (Array.isArray(d.tx?.customItems)) {
      const ids = new Set<number>();
      tx.customItems = d.tx.customItems.slice(0, 100).flatMap((item) => {
        if (!item || !Number.isSafeInteger(item.id) || item.id <= 0 || ids.has(item.id) || typeof item.text !== "string") return [];
        ids.add(item.id);
        return [{ id: item.id, text: item.text.slice(0, 65_536) }];
      });
    }
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
      if (typeof d.graph.headerHex === "string") graph.headerHex = d.graph.headerHex.slice(0, 4096);
      if (typeof d.graph.xRange === "number" && Number.isFinite(d.graph.xRange) && d.graph.xRange > 0) graph.xRange = Math.min(d.graph.xRange, 1_000_000_000);
      if (typeof d.graph.autoScroll === "boolean") graph.autoScroll = d.graph.autoScroll;
      if (typeof d.graph.enabled === "boolean") graph.enabled = d.graph.enabled;
      if (typeof d.graph.paused === "boolean") graph.paused = d.graph.paused;
    }
    if (d.theme === "light" || d.theme === "dark" || d.theme === "system") {
      themeRef.value = d.theme;
    }
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
      () => rx.dualMode,
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
