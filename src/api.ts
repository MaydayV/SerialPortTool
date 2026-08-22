// 与 Rust 后端交互的 API 封装
import { invoke } from "@tauri-apps/api/core";

export interface PortInfo {
  name: string;
  desc: string;
  port_type: string;
  vid: number | null;
  pid: number | null;
  serial: string | null;
}

export interface SerialConfig {
  port: string;
  baudrate: number;
  data_bits: number;
  parity: string; // none/odd/even
  stop_bits: number; // 1 / 2
  flow_control: string; // none/software/hardware
  rts: boolean;
  dtr: boolean;
  auto_reconnect: boolean;
}

export interface TcpUdpConfig {
  protocol: string; // tcp/udp
  mode: string; // client/server
  target: string; // host:port
  port: number;
  auto_reconnect: boolean;
  reconnect_interval: number;
}

export type ConnConfig =
  | { type: "Serial"; config: SerialConfig }
  | { type: "TcpUdp"; config: TcpUdpConfig };

export interface RxPayload {
  data: number[];
  ts: number;
}

export interface StatusPayload {
  status: string; // connected/closed/lose/connecting
  msg: string;
}

export const api = {
  listPorts: () => invoke<PortInfo[]>("list_ports"),
  connOpen: (cfg: ConnConfig) => invoke<void>("conn_open", { cfg }),
  connClose: () => invoke<void>("conn_close"),
  connSend: (data: number[]) => invoke<number>("conn_send", { data }),
  connIsConnected: () => invoke<boolean>("conn_is_connected"),
  appendLogFile: (path: string, line: string) =>
    invoke<void>("append_log_file", { path, line }),
};
