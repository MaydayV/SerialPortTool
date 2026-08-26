import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type McpFrontendBridgeResponse } from "./api";
import { useGraphStore, type CurveSeries } from "./stores/graph";
import { useProtocolStore } from "./stores/protocol";
import type { FrameTemplate } from "./utils/protocol";

export interface McpFrontendBridgeRequest {
  request_id: string;
  operation: "protocol.get_state" | "protocol.select" | "graph.get_state" | "graph.get_data" | "graph.clear";
  payload: Record<string, unknown>;
}

export interface BridgeProtocolState {
  templates: FrameTemplate[];
  activeName: string;
  rxEnabled: boolean;
  txEnabled: boolean;
  frameCount: number;
  frameErrorCount: number;
  frameTrashCount: number;
  canDecodeActive: boolean;
}

export interface BridgeGraphState {
  enabled: boolean;
  protocol: "ascii" | "binary";
  headerHex: string;
  xRange: number;
  frameCount: number;
  series: Array<{
    name: string;
    pointCount: number;
    color: string;
    minX?: number;
    maxX?: number;
  }>;
}

interface BridgeGraphData {
  series: Array<{ name: string; points: Array<{ x: number; y: number }> }>;
  pointCount: number;
  byteCount: number;
  truncated: boolean;
  minX?: number;
  maxX?: number;
}

const MAX_TEMPLATES = 100;
const MAX_SERIES = 32;
const MAX_SERIES_NAME = 128;
const MAX_POINTS = 20_000;
const MAX_BYTES = 1024 * 1024;

function protocolState(): BridgeProtocolState {
  const store = useProtocolStore();
  return {
    templates: store.templates.slice(0, MAX_TEMPLATES).map((template) => ({
      ...template,
      length: { ...template.length },
    })),
    activeName: store.activeName,
    rxEnabled: store.rxEnabled,
    txEnabled: store.txEnabled,
    frameCount: store.frameCount,
    frameErrorCount: store.frameErrorCount,
    frameTrashCount: store.frameTrashCount,
    canDecodeActive: store.canDecodeActive,
  };
}

function seriesSummary(series: CurveSeries) {
  const minX = series.xs.length ? Math.min(...series.xs) : undefined;
  const maxX = series.xs.length ? Math.max(...series.xs) : undefined;
  return {
    name: series.name,
    pointCount: series.xs.length,
    color: series.color,
    ...(minX === undefined ? {} : { minX }),
    ...(maxX === undefined ? {} : { maxX }),
  };
}

function graphState(): BridgeGraphState {
  const store = useGraphStore();
  return {
    enabled: store.enabled,
    protocol: store.protocol,
    headerHex: store.headerHex,
    xRange: Number.isFinite(store.xRange) && store.xRange > 0 ? store.xRange : 100,
    frameCount: store.frameCount,
    series: store.seriesList.slice(0, MAX_SERIES).map(seriesSummary),
  };
}

function graphData(payload: Record<string, unknown>): BridgeGraphData {
  const store = useGraphStore();
  const maxPoints = Number.isSafeInteger(payload.max_points)
    ? Math.min(payload.max_points as number, MAX_POINTS)
    : 0;
  const maxBytes = Number.isSafeInteger(payload.max_bytes)
    ? Math.min(payload.max_bytes as number, MAX_BYTES)
    : 0;
  if (maxPoints < 1 || maxBytes < 1) throw new Error("max_points 和 max_bytes 必须为正数");

  const requested = payload.series;
  if (requested !== undefined && (!Array.isArray(requested) || requested.length > MAX_SERIES)) {
    throw new Error("series 数量超过限制");
  }
  const requestedNames = requested === undefined ? null : new Set(
    (requested as unknown[]).map((name) => {
      if (typeof name !== "string" || !name.trim() || name.length > MAX_SERIES_NAME) {
        throw new Error("series 名称无效");
      }
      return name;
    })
  );
  const selected = store.seriesList.filter((series) => requestedNames === null || requestedNames.has(series.name));
  const base = Math.floor(maxPoints / Math.max(1, selected.length));
  let remainder = maxPoints;
  const resultSeries = selected.map((series) => {
    const quota = Math.min(series.xs.length, base + (remainder > 0 ? 1 : 0));
    remainder -= quota;
    const start = Math.max(0, series.xs.length - quota);
    return {
      name: series.name,
      points: Array.from({ length: quota }, (_, index) => ({
        x: series.xs[start + index],
        y: series.ys[start + index],
      })),
    };
  });

  const makeResult = (series: BridgeGraphData["series"], truncated: boolean): BridgeGraphData => {
    const points = series.flatMap((item) => item.points);
    const xs = points.map((point) => point.x);
    const value: BridgeGraphData = {
      series,
      pointCount: points.length,
      byteCount: 0,
      truncated,
      ...(xs.length ? { minX: Math.min(...xs), maxX: Math.max(...xs) } : {}),
    };
    // byteCount is part of the returned JSON, so converge after including its own digits.
    for (let i = 0; i < 4; i += 1) {
      const encodedSize = new TextEncoder().encode(JSON.stringify(value)).byteLength;
      if (value.byteCount === encodedSize) break;
      value.byteCount = encodedSize;
    }
    return value;
  };

  let truncated = false;
  let value = makeResult(resultSeries, false);
  while (value.byteCount > maxBytes && value.pointCount > 0) {
    truncated = true;
    const candidate = value.series
      .map((series, index) => ({ series, index }))
      .filter(({ series }) => series.points.length > 0)
      .pop();
    if (!candidate) break;
    candidate.series.points.pop();
    value = makeResult(value.series, truncated);
  }
  if (value.byteCount > maxBytes) throw new Error("max_bytes 太小，无法返回波形数据");
  return value;
}

export async function handleMcpFrontendRequest(
  request: McpFrontendBridgeRequest
): Promise<unknown> {
  if (!request.request_id || typeof request.request_id !== "string" || request.request_id.length > 128) {
    throw new Error("request_id 无效");
  }
  switch (request.operation) {
    case "protocol.get_state":
      return protocolState();
    case "protocol.select": {
      const protocolId = request.payload.protocol_id;
      if (typeof protocolId !== "string" || !protocolId.trim() || protocolId.length > 100) {
        throw new Error("protocol_id 无效");
      }
      const store = useProtocolStore();
      if (!store.select(protocolId)) throw new Error("协议模板不存在");
      return protocolState();
    }
    case "graph.get_state":
      return graphState();
    case "graph.get_data":
      return graphData(request.payload);
    case "graph.clear":
      useGraphStore().clear();
      return graphState();
    default:
      throw new Error("不支持的前端 bridge 操作");
  }
}

let unlisten: UnlistenFn | null = null;

export async function setupMcpFrontendBridge(): Promise<() => void> {
  if (unlisten) return () => unlisten?.();
  unlisten = await listen<McpFrontendBridgeRequest>("mcp-frontend-request", async ({ payload }) => {
    let response: McpFrontendBridgeResponse;
    try {
      const result = await handleMcpFrontendRequest(payload);
      response = { request_id: payload.request_id, ok: true, result };
    } catch (error) {
      response = {
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message.slice(0, 512) : "前端 bridge 操作失败",
      };
    }
    try {
      await api.mcpFrontendBridgeResponse(response);
    } catch {
      // Rust owns timeout/error reporting; never turn a failed callback into success.
    }
  });
  return () => {
    unlisten?.();
    unlisten = null;
  };
}
