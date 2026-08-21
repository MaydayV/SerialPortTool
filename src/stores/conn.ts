// 连接状态 store：统一管理连接配置、状态、收发数据
import { defineStore } from "pinia";
import { ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, type PortInfo, type SerialConfig, type TcpUdpConfig } from "../api";

export const useConnStore = defineStore("conn", () => {
  const ports = ref<PortInfo[]>([]);
  const connType = ref<"serial" | "tcpudp">("serial");
  const status = ref<"closed" | "connecting" | "connected" | "lose">("closed");
  const statusMsg = ref("");
  const lastError = ref("");

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
    const name = profileName.value.trim();
    if (!name) return;
    profiles.value = [
      {
        name,
        connType: connType.value,
        serial: JSON.parse(JSON.stringify(serial.value)),
        tcpudp: JSON.parse(JSON.stringify(tcpudp.value)),
      },
      ...profiles.value.filter((p) => p.name !== name),
    ];
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

  async function refreshPorts() {
    try {
      ports.value = await api.listPorts();
      // 默认选中第一个可用端口
      if (serial.value.port === "" && ports.value.length > 0) {
        serial.value.port = ports.value[0].name;
      }
      return true;
    } catch (e) {
      lastError.value = String(e);
      return false;
    }
  }

  function setupListeners() {
    listen<{ ports: string[]; added: string[]; removed: string[] }>(
      "ports-changed",
      () => {
        refreshPorts();
      }
    );
    listen<{ status: string; msg: string }>("conn-status", (e) => {
      status.value = e.payload.status as typeof status.value;
      statusMsg.value = e.payload.msg;
    });
  }

  async function open() {
    lastError.value = "";
    try {
      if (connType.value === "serial") {
        await api.connOpen({ type: "Serial", config: { ...serial.value } });
      } else {
        await api.connOpen({ type: "TcpUdp", config: { ...tcpudp.value } });
      }
    } catch (e) {
      lastError.value = String(e);
      status.value = "closed";
    }
  }

  async function close() {
    try {
      await api.connClose();
      status.value = "closed";
      statusMsg.value = "";
    } catch (e) {
      lastError.value = String(e);
    }
  }

  async function toggle() {
    if (status.value === "connected" || status.value === "connecting") {
      await close();
    } else {
      await open();
    }
  }

  const isConnected = () =>
    status.value === "connected" || status.value === "connecting";

  return {
    ports,
    connType,
    status,
    statusMsg,
    lastError,
    serial,
    tcpudp,
    profiles,
    profileName,
    saveProfile,
    applyProfile,
    removeProfile,
    refreshPorts,
    setupListeners,
    open,
    close,
    toggle,
    isConnected,
  };
});
