// 会话持久化：自动保存/恢复所有 store 配置到 localStorage
import { watch } from "vue";
import { useConnStore } from "./conn";
import { useRxStore } from "./rx";
import { useTxStore } from "./tx";
import { useProtocolStore } from "./protocol";
import { useGraphStore } from "./graph";

const KEY = "serialaid.config.v1";

interface Persisted {
  connType: "serial" | "tcpudp";
  serial: object;
  tcpudp: object;
  rx: {
    encoding: string;
    rxHexMode: boolean;
    showTimestamp: boolean;
    fontSize: number;
  };
  tx: {
    sendHexMode: boolean;
    appendNewline: boolean;
    useCRLF: boolean;
    escapeMode: boolean;
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
    rx: {
      encoding: rx.encoding,
      rxHexMode: rx.rxHexMode,
      showTimestamp: rx.showTimestamp,
      fontSize: rx.fontSize,
    },
    tx: {
      sendHexMode: tx.sendHexMode,
      appendNewline: tx.appendNewline,
      useCRLF: tx.useCRLF,
      escapeMode: tx.escapeMode,
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

    conn.connType = d.connType ?? "serial";
    Object.assign(conn.serial, d.serial);
    Object.assign(conn.tcpudp, d.tcpudp);
    Object.assign(rx, d.rx);
    Object.assign(tx, d.tx);
    if (d.proto) {
      proto.templates = d.proto.templates as any;
      proto.activeName = d.proto.activeName;
      proto.rxEnabled = d.proto.rxEnabled;
      proto.txEnabled = d.proto.txEnabled;
    }
    Object.assign(graph, d.graph);
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
      () => rx.encoding,
      () => rx.rxHexMode,
      () => rx.showTimestamp,
      () => rx.fontSize,
      () => tx.sendHexMode,
      () => tx.appendNewline,
      () => tx.useCRLF,
      () => tx.escapeMode,
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
      () => themeRef.value,
    ],
    schedule,
    { deep: true }
  );
}
