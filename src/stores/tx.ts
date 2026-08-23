// 发送 store：发送区、历史、定时发送、快捷条
import { defineStore } from "pinia";
import { ref } from "vue";
import { api } from "../api";
import { useConnStore } from "./conn";
import { useRxStore } from "./rx";
import { useProtocolStore } from "./protocol";
import { hexToBytes, escapeToBytes } from "../utils/bytes";

const MAX_DIRECT_SEND_BYTES = 4 * 1024 * 1024;

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
  const fileSending = ref(false);
  const fileProgress = ref(0);
  const currentFileName = ref("");

  const feedback = ref("");
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let fileCancelRequested = false;
  let queueTail: Promise<void> = Promise.resolve();

  /** 所有发送共用同一把异步锁，保证文件块、定时和手动数据不会交叉。 */
  async function withSendLock<T>(job: () => Promise<T>): Promise<T> {
    const previous = queueTail;
    let release!: () => void;
    queueTail = new Promise<void>((resolve) => {
      release = resolve;
    });
    await previous;
    try {
      return await job();
    } finally {
      release();
    }
  }

  function notify(message: string) {
    feedback.value = message;
    if (feedbackTimer) clearTimeout(feedbackTimer);
    feedbackTimer = setTimeout(() => (feedback.value = ""), 4500);
  }

  function encodeText(text: string): Uint8Array {
    let raw: Uint8Array;
    if (sendHexMode.value) {
      const parsed = hexToBytes(text);
      // 非法 HEX 不得因为开启“追加换行”而退化成只发送换行。
      if (parsed === null) return new Uint8Array(0);
      raw = parsed;
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
    if (bytes.length > MAX_DIRECT_SEND_BYTES) {
      notify("单次发送不能超过 4 MiB，请改用文件分块发送");
      return false;
    }
    try {
      return await withSendLock(async () => {
        if (!conn.isConnected()) throw new Error("连接已断开");
        // 协议组帧在真正发送前执行；失败会明确抛错，不会静默透传。
        const proto = useProtocolStore();
        const out = proto.processTx(bytes);
        const written = await api.connSend(Array.from(out));
        // 回显、统计和日志必须与实际线上字节一致。
        rx.append(out, "tx", Date.now(), undefined, true, written);
        return true;
      });
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

  function normalizedInterval() {
    const value = Number(scheduledInterval.value);
    return Number.isFinite(value) ? Math.min(3_600_000, Math.max(10, value)) : 1000;
  }

  async function scheduledTick() {
    if (!scheduled.value) return;
    const ok = await doSend(nowBytes());
    if (!ok) {
      scheduled.value = false;
      timer = null;
      return;
    }
    if (scheduled.value) timer = setTimeout(scheduledTick, normalizedInterval());
  }

  function startSchedule() {
    if (timer) clearTimeout(timer);
    scheduledInterval.value = normalizedInterval();
    timer = setTimeout(scheduledTick, scheduledInterval.value);
  }

  /** 定时发送开关（上一轮完成后再调度，避免慢连接堆积请求） */
  function toggleScheduled() {
    scheduled.value = !scheduled.value;
    if (scheduled.value) {
      if (!useConnStore().isConnected()) {
        scheduled.value = false;
        notify("请先连接设备再开启定时发送");
        return;
      }
      if (nowBytes().length === 0) {
        scheduled.value = false;
        notify("请先输入有效的定时发送内容");
        return;
      }
      startSchedule();
    } else if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  /** 定时间隔变更时重启定时器 */
  function updateInterval() {
    if (scheduled.value) {
      startSchedule();
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

  /** 分块发送文件：内存占用固定，并支持进度和取消。 */
  async function sendFile(file: File) {
    if (fileSending.value) return false;
    const conn = useConnStore();
    if (!conn.isConnected()) {
      notify("未连接，无法发送文件");
      return false;
    }
    if (file.size === 0) {
      notify("文件为空，未发送");
      return false;
    }
    const CHUNK_SIZE = 64 * 1024;
    fileSending.value = true;
    fileCancelRequested = false;
    fileProgress.value = 0;
    currentFileName.value = file.name;
    try {
      const completed = await withSendLock(async () => {
        for (let offset = 0; offset < file.size; offset += CHUNK_SIZE) {
          if (fileCancelRequested) return false;
          if (!conn.isConnected()) throw new Error("连接已断开");
          const end = Math.min(file.size, offset + CHUNK_SIZE);
          const bytes = new Uint8Array(await file.slice(offset, end).arrayBuffer());
          // 文件定义为原始字节传输；整次文件发送持有发送锁，块之间不会插入其他数据。
          const written = await api.connSend(Array.from(bytes));
          useRxStore().append(bytes, "tx", Date.now(), undefined, true, written);
          fileProgress.value = Math.round((end / file.size) * 100);
        }
        return true;
      });
      if (!completed) {
        notify("文件发送已取消");
        return false;
      }
      notify(`文件发送完成：${file.name}`);
      return true;
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      notify(`文件发送失败：${message}`);
      return false;
    } finally {
      fileSending.value = false;
      fileCancelRequested = false;
    }
  }

  function cancelFile() {
    fileCancelRequested = true;
  }

  function stopAll() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    fileCancelRequested = true;
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
    fileSending,
    fileProgress,
    currentFileName,
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
    cancelFile,
    stopAll,
    nowBytes,
  };
});
