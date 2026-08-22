// 发送 store：发送区、历史、定时发送、快捷条
import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "../api";
import { useConnStore } from "./conn";
import { useRxStore } from "./rx";
import { useProtocolStore } from "./protocol";
import { hexToBytes, escapeToBytes } from "../utils/bytes";

export const useTxStore = defineStore("tx", () => {
  const sendText = ref("");
  const sendHexMode = ref(false);
  const appendNewline = ref(false); // 发送自动补 \r\n
  const useCRLF = ref(true); // \n -> \r\n
  const escapeMode = ref(false); // 转义模式 \xHH \n 等
  const history = ref<string[]>([]);
  const scheduled = ref(false); // 定时发送
  const scheduledInterval = ref(1000); // ms
  const sending = ref(false);

  const feedback = ref("");
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let timer: ReturnType<typeof setInterval> | null = null;

  function notify(message: string) {
    feedback.value = message;
    if (feedbackTimer) clearTimeout(feedbackTimer);
    feedbackTimer = setTimeout(() => (feedback.value = ""), 4500);
  }

  function encodeText(text: string): Uint8Array {
    let raw: Uint8Array;
    if (sendHexMode.value) {
      raw = hexToBytes(text) ?? new Uint8Array(0);
    } else if (escapeMode.value) {
      raw = escapeToBytes(text);
    } else {
      raw = new TextEncoder().encode(text);
    }
    if (!appendNewline.value) return raw;
    const nl = useCRLF.value ? new Uint8Array([0x0d, 0x0a]) : new Uint8Array([0x0a]);
    const out = new Uint8Array(raw.length + nl.length);
    out.set(raw);
    out.set(nl, raw.length);
    return out;
  }

  function nowBytes(): Uint8Array {
    return encodeText(sendText.value);
  }

  function pushHistory(t: string) {
    if (!t) return;
    history.value = [t, ...history.value.filter((h) => h !== t)].slice(0, 20);
  }

  /** 删除单条历史 */
  function removeHistory(t: string) {
    history.value = history.value.filter((h) => h !== t);
  }

  /** 清空全部历史 */
  function clearHistory() {
    history.value = [];
  }

  /** 直接发送一条历史记录（按当前 HEX/转义/换行设置解析） */
  async function sendHistory(t: string) {
    if (!t) return false;
    const bytes = encodeText(t);
    return doSend(bytes);
  }

  async function doSend(bytes: Uint8Array): Promise<boolean> {
    const conn = useConnStore();
    const rx = useRxStore();
    if (!conn.isConnected()) {
      notify("未连接，无法发送");
      return false;
    }
    if (bytes.length === 0) {
      notify("发送内容为空或 HEX 格式无效");
      return false;
    }
    try {
      // 协议组帧（若启用）
      const proto = useProtocolStore();
      const out = proto.processTx(bytes);
      await api.connSend(Array.from(out));
      // 发送回显到接收区（方向 tx，回显原始负载）
      rx.append(bytes, "tx");
      return true;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      notify(`发送失败：${message}`);
      return false;
    }
  }

  async function send() {
    if (sending.value) return;
    const bytes = nowBytes();
    if (bytes.length === 0) {
      notify("发送内容为空或 HEX 格式无效");
      return;
    }
    sending.value = true;
    try {
      const ok = await doSend(bytes);
      if (ok) {
        pushHistory(sendText.value);
      }
    } finally {
      sending.value = false;
    }
  }

  /** 定时发送开关 */
  function toggleScheduled() {
    scheduled.value = !scheduled.value;
    if (scheduled.value) {
      timer = setInterval(() => {
        doSend(nowBytes());
      }, scheduledInterval.value);
    } else if (timer) {
      clearInterval(timer);
      timer = null;
    }
  }

  /** 定时间隔变更时重启定时器 */
  function updateInterval() {
    if (scheduled.value) {
      if (timer) clearInterval(timer);
      timer = setInterval(() => {
        doSend(nowBytes());
      }, scheduledInterval.value);
    }
  }

  /** 快捷条：自定义发送项 */
  const customItems = ref<{ id: number; text: string }[]>([]);
  let itemSeq = 0;

  function addCustomItem(text = "") {
    const maxId = customItems.value.reduce(
      (max, item) => Math.max(max, Number.isFinite(item.id) ? item.id : 0),
      0
    );
    itemSeq = Math.max(itemSeq, maxId) + 1;
    customItems.value.push({ id: itemSeq, text });
  }

  function removeCustomItem(id: number) {
    customItems.value = customItems.value.filter((i) => i.id !== id);
  }

  async function sendCustom(id: number) {
    const item = customItems.value.find((i) => i.id === id);
    if (!item) return false;
    return doSend(encodeText(item.text));
  }

  /** 发送文件（读文件字节后发送）——由组件注入 readFileBytes 实现 */
  let fileReader: (path: string) => Promise<Uint8Array> = async () => new Uint8Array();
  function setFileReader(fn: (path: string) => Promise<Uint8Array>) {
    fileReader = fn;
  }

  async function sendFile(path: string) {
    try {
      const bytes = await fileReader(path);
      if (bytes.length === 0) {
        notify("文件为空，未发送");
        return false;
      }
      return doSend(bytes);
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      notify(`文件发送失败：${message}`);
      return false;
    }
  }

  function stopAll() {
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    scheduled.value = false;
  }

  return {
    sendText,
    sendHexMode,
    appendNewline,
    useCRLF,
    escapeMode,
    history,
    scheduled,
    scheduledInterval,
    sending,
    feedback,
    customItems,
    send,
    removeHistory,
    clearHistory,
    sendHistory,
    toggleScheduled,
    updateInterval,
    addCustomItem,
    removeCustomItem,
    sendCustom,
    sendFile,
    setFileReader,
    stopAll,
    nowBytes,
  };
});
