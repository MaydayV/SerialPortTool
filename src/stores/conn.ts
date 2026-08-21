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

  async function refreshPorts() {
    try {
      ports.value = await api.listPorts();
      // 默认选中第一个可用端口
      if (serial.value.port === "" && ports.value.length > 0) {
        serial.value.port = ports.value[0].name;
      }
    } catch (e) {
      lastError.value = String(e);
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
    refreshPorts,
    setupListeners,
    open,
    close,
    toggle,
    isConnected,
  };
});
