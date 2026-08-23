// 连接状态 store：统一管理连接配置、状态、收发数据
import { defineStore } from "pinia";
import { ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type PortInfo, type SerialConfig, type TcpUdpConfig } from "../api";

export const useConnStore = defineStore("conn", () => {
  const ports = ref<PortInfo[]>([]);
  const connType = ref<"serial" | "tcpudp">("serial");
  const status = ref<"closed" | "connecting" | "connected" | "lose">("closed");
  const statusMsg = ref("");
  const lastError = ref("");
  const lastErrorDetail = ref("");
  const operationPending = ref(false);

  function setError(context: string, error: unknown) {
    const raw = error instanceof Error ? error.message : String(error);
    lastErrorDetail.value = raw;
    if (raw.includes("reading 'invoke'") || raw.includes("__TAURI_INTERNALS__")) {
      lastError.value = "当前预览环境无法访问桌面连接后端";
    } else if (/permission|denied|拒绝|权限/i.test(raw)) {
      lastError.value = `${context}失败：没有设备或文件访问权限`;
    } else if (/not found|不存在|No such/i.test(raw)) {
      lastError.value = `${context}失败：设备或目标不存在`;
    } else {
      const clean = raw.replace(/^(Error|TypeError):\s*/i, "").trim();
      lastError.value = `${context}失败${clean ? `：${clean}` : ""}`;
    }
  }

  // 串口参数
  const serial = ref<SerialConfig>({
    port: "",
    baudrate: 115200,
    data_bits: 8,
    parity: "none",
    stop_bits: 1,
    flow_control: "none",
    rts: false,
    dtr: false,
    auto_reconnect: false,
  });

  // TCP/UDP 参数
  const tcpudp = ref<TcpUdpConfig>({
    protocol: "tcp",
    mode: "client",
    target: "127.0.0.1:2345",
    port: 2345,
    local_port: 0,
    auto_reconnect: false,
    reconnect_interval: 1,
  });

  // ===== 连接配置收藏 =====
  interface ConnProfile {
    name: string;
    connType: "serial" | "tcpudp";
    serial: SerialConfig;
    tcpudp: TcpUdpConfig;
  }
  const profiles = ref<ConnProfile[]>([]);
  const profileName = ref(""); // 收藏命名输入

  function saveProfile() {
    const name = profileName.value.trim().slice(0, 100);
    if (!name) return;
    profiles.value = [
      {
        name,
        connType: connType.value,
        serial: JSON.parse(JSON.stringify(serial.value)),
        tcpudp: JSON.parse(JSON.stringify(tcpudp.value)),
      },
      ...profiles.value.filter((p) => p.name !== name),
    ].slice(0, 100);
    profileName.value = "";
  }

  function applyProfile(name: string) {
    const p = profiles.value.find((x) => x.name === name);
    if (!p) return;
    connType.value = p.connType;
    Object.assign(serial.value, p.serial);
    Object.assign(tcpudp.value, p.tcpudp);
  }

  function removeProfile(name: string) {
    profiles.value = profiles.value.filter((x) => x.name !== name);
  }

  function renameProfile(name: string, nextName: string): boolean {
    const next = nextName.trim().slice(0, 100);
    if (!next || (next !== name && profiles.value.some((profile) => profile.name === next))) {
      return false;
    }
    const profile = profiles.value.find((item) => item.name === name);
    if (!profile) return false;
    profile.name = next;
    profiles.value = [...profiles.value];
    return true;
  }

  let refreshGeneration = 0;
  async function refreshPorts() {
    const generation = ++refreshGeneration;
    try {
      const nextPorts = await api.listPorts();
      if (generation !== refreshGeneration) return false;
      ports.value = nextPorts;
      // 默认选中第一个可用端口
      if (serial.value.port === "" && ports.value.length > 0) {
        serial.value.port = ports.value[0].name;
      }
      lastError.value = "";
      lastErrorDetail.value = "";
      return true;
    } catch (e) {
      setError("刷新串口", e);
      return false;
    }
  }

  let listenersReady = false;
  let listenerSetup: Promise<void> | null = null;
  let unlistenPorts: UnlistenFn | null = null;
  let unlistenStatus: UnlistenFn | null = null;
  let desiredOpen = false;
  let pendingOperations = 0;

  function beginOperation() {
    pendingOperations += 1;
    operationPending.value = true;
  }

  function endOperation() {
    pendingOperations = Math.max(0, pendingOperations - 1);
    operationPending.value = pendingOperations > 0;
  }

  /** 注册后端事件监听；组件重复挂载时也只注册一次。 */
  async function setupListeners() {
    if (listenersReady) return;
    if (listenerSetup) return listenerSetup;
    listenerSetup = (async () => {
      const portsListener = await listen<{ ports: string[]; added: string[]; removed: string[] }>(
        "ports-changed",
        () => void refreshPorts()
      );
      try {
        const statusListener = await listen<{ status: string; msg: string }>(
          "conn-status",
          (e) => {
            const next = e.payload.status;
            if (!["closed", "connecting", "connected", "lose"].includes(next)) return;
            // 已请求关闭时忽略迟到的连接中/已连接状态。
            if (!desiredOpen && (next === "connecting" || next === "connected")) return;
            if (next === "closed") desiredOpen = false;
            status.value = next as typeof status.value;
            statusMsg.value = typeof e.payload.msg === "string" ? e.payload.msg : "";
          }
        );
        unlistenPorts = portsListener;
        unlistenStatus = statusListener;
        listenersReady = true;
      } catch (error) {
        portsListener();
        throw error;
      }
    })();
    try {
      await listenerSetup;
    } finally {
      listenerSetup = null;
    }
  }

  async function open() {
    if (desiredOpen) return;
    desiredOpen = true;
    beginOperation();
    lastError.value = "";
    lastErrorDetail.value = "";
    status.value = "connecting";
    statusMsg.value = "正在打开连接...";
    try {
      if (connType.value === "serial") {
        await api.connOpen({ type: "Serial", config: { ...serial.value } });
      } else {
        await api.connOpen({ type: "TcpUdp", config: { ...tcpudp.value } });
      }
      if (!desiredOpen) await api.connClose();
    } catch (e) {
      setError("打开连接", e);
      status.value = "closed";
      statusMsg.value = "";
      desiredOpen = false;
    } finally {
      endOperation();
    }
  }

  async function close() {
    desiredOpen = false;
    beginOperation();
    status.value = "connecting";
    statusMsg.value = "正在关闭连接...";
    try {
      await api.connClose();
      status.value = "closed";
      statusMsg.value = "";
    } catch (e) {
      setError("关闭连接", e);
      status.value = "lose";
      statusMsg.value = "关闭状态未知，请重试";
    } finally {
      endOperation();
    }
  }

  async function toggle() {
    // 掉线重连阶段 desiredOpen 仍为 true，此时按钮必须能真正关闭连接。
    if (desiredOpen || status.value !== "closed") {
      await close();
    } else {
      await open();
    }
  }

  const isConnected = () => status.value === "connected" && !operationPending.value;

  function teardownListeners() {
    unlistenPorts?.();
    unlistenStatus?.();
    unlistenPorts = null;
    unlistenStatus = null;
    listenersReady = false;
  }

  return {
    ports,
    connType,
    status,
    statusMsg,
    lastError,
    lastErrorDetail,
    operationPending,
    serial,
    tcpudp,
    profiles,
    profileName,
    saveProfile,
    applyProfile,
    removeProfile,
    renameProfile,
    refreshPorts,
    setupListeners,
    teardownListeners,
    open,
    close,
    toggle,
    isConnected,
  };
});
