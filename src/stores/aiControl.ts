import { computed, ref } from "vue";
import { defineStore } from "pinia";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { api, type PendingApproval, type PermissionMode } from "../api";

export type ActionStage =
  | "started"
  | "approval_required"
  | "approved"
  | "finished"
  | "failed"
  | "denied"
  | "timed_out";

export interface ControlActionEvent {
  action_id: string;
  origin: "ui" | "mcp" | "system" | string;
  operation: string;
  stage: ActionStage;
  summary: string;
  timestamp_ms?: number;
}

export interface McpActivityEvent {
  stage: "connected" | "started" | "finished" | "failed" | string;
  operation: string;
  summary: string;
  action_id?: string;
  timestamp_ms?: number;
}

export interface AiTimelineEntry {
  actionId?: string;
  operation: string;
  stage: ActionStage | "connected";
  status: "success" | "failed" | "waiting" | "running" | "connected";
  summary: string;
  origin: string;
  timestampMs: number;
}

const MAX_TIMELINE = 100;
const MAX_PENDING = 32;

export const useAiControlStore = defineStore("aiControl", () => {
  const enabled = ref(false);
  const connected = ref(false);
  const endpoint = ref("");
  const permissionMode = ref<PermissionMode>("ask");
  const pendingApprovals = ref<PendingApproval[]>([]);
  const recentTimeline = ref<AiTimelineEntry[]>([]);
  const lastError = ref("");
  const lastActivityAt = ref(0);

  let listenersReady = false;
  let listenerSetup: Promise<void> | null = null;
  let unlisten: UnlistenFn[] = [];

  const pendingApproval = computed(() => pendingApprovals.value[0] ?? null);

  function trimTimeline() {
    if (recentTimeline.value.length > MAX_TIMELINE) {
      recentTimeline.value = recentTimeline.value.slice(-MAX_TIMELINE);
    }
  }

  function addTimeline(entry: AiTimelineEntry) {
    recentTimeline.value.push(entry);
    trimTimeline();
  }

  function removePending(actionId: string) {
    pendingApprovals.value = pendingApprovals.value.filter(
      (approval) => approval.action_id !== actionId
    );
  }

  function onControlAction(event: ControlActionEvent) {
    if (!event || typeof event.action_id !== "string") return;
    const stage = event.stage;
    const status: AiTimelineEntry["status"] =
      stage === "approval_required"
        ? "waiting"
        : stage === "started" || stage === "approved"
          ? "running"
          : stage === "finished"
            ? "success"
            : "failed";
    addTimeline({
      actionId: event.action_id,
      operation: event.operation,
      stage,
      status,
      summary: event.summary,
      origin: event.origin,
      timestampMs: event.timestamp_ms ?? Date.now(),
    });
    if (["denied", "timed_out", "finished", "failed"].includes(stage)) {
      removePending(event.action_id);
    }
  }

  function onApprovalRequired(event: PendingApproval) {
    if (!event || typeof event.action_id !== "string") return;
    pendingApprovals.value = [
      event,
      ...pendingApprovals.value.filter(
        (approval) => approval.action_id !== event.action_id
      ),
    ].slice(0, MAX_PENDING);
  }

  function onMcpActivity(event: McpActivityEvent) {
    if (!event || typeof event.operation !== "string") return;
    enabled.value = true;
    connected.value = true;
    lastActivityAt.value = event.timestamp_ms ?? Date.now();
    if (event.stage === "connected") {
      addTimeline({
        actionId: event.action_id,
        operation: event.operation,
        stage: "connected",
        status: "connected",
        summary: event.summary,
        origin: "mcp",
        timestampMs: lastActivityAt.value,
      });
    }
  }

  async function setupListeners() {
    if (listenersReady) return;
    if (listenerSetup) return listenerSetup;
    listenerSetup = (async () => {
      try {
        endpoint.value = await api.mcpEndpoint();
        enabled.value = true;
        permissionMode.value = await api.getPermissionMode();
        pendingApprovals.value = (await api.listPendingApprovals()).slice(
          0,
          MAX_PENDING
        );
        const action = await listen<ControlActionEvent>(
          "control-action",
          (event) => onControlAction(event.payload)
        );
        const approval = await listen<PendingApproval>(
          "approval-required",
          (event) => onApprovalRequired(event.payload)
        );
        const activity = await listen<McpActivityEvent>(
          "mcp-activity",
          (event) => onMcpActivity(event.payload)
        );
        unlisten = [action, approval, activity];
        listenersReady = true;
      } catch (error) {
        lastError.value = error instanceof Error ? error.message : String(error);
        enabled.value = false;
        unlisten.forEach((stop) => stop());
        unlisten = [];
        throw error;
      }
    })();
    try {
      await listenerSetup;
    } finally {
      listenerSetup = null;
    }
  }

  async function setPermissionMode(mode: PermissionMode) {
    await api.setPermissionMode(mode);
    permissionMode.value = mode;
  }

  async function approve(actionId: string) {
    if (!pendingApprovals.value.some((approval) => approval.action_id === actionId)) {
      throw new Error("审批不存在或已结束");
    }
    await api.approveMcpAction(actionId);
  }

  async function deny(actionId: string) {
    if (!pendingApprovals.value.some((approval) => approval.action_id === actionId)) {
      throw new Error("审批不存在或已结束");
    }
    await api.denyMcpAction(actionId);
  }

  async function showToken() {
    // This method is only called by an explicit user click in the panel.
    return api.mcpToken();
  }

  async function resetToken() {
    await api.resetMcpToken();
  }

  function teardown() {
    unlisten.forEach((stop) => stop());
    unlisten = [];
    listenersReady = false;
    listenerSetup = null;
  }

  return {
    enabled,
    connected,
    endpoint,
    permissionMode,
    pendingApproval,
    pendingApprovals,
    recentTimeline,
    lastError,
    lastActivityAt,
    setupListeners,
    setPermissionMode,
    approve,
    deny,
    showToken,
    resetToken,
    teardown,
  };
});
