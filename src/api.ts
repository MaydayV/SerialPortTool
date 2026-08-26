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
  local_port: number;
  auto_reconnect: boolean;
  reconnect_interval: number;
}

export type ConnConfig =
  | { type: "Serial"; config: SerialConfig }
  | { type: "TcpUdp"; config: TcpUdpConfig };

export interface RxPayload {
  data: number[];
  ts: number;
  peer?: string;
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
  connClearReceived: () => invoke<void>("conn_clear_received"),
  selectOutputFile: (kind: "log" | "templates" | "curve") =>
    invoke<string | null>("select_output_file", { kind }),
  writeUserFile: (path: string, text: string, truncate: boolean) =>
    invoke<void>("write_user_file", { path, text, truncate }),
  appendLogFile: (path: string, line: string) =>
    invoke<void>("append_log_file", { path, line }),
  flushLogFiles: () => invoke<void>("flush_log_files"),
};
