<script setup lang="ts">
import { ref } from "vue";
import { useAiControlStore, type AiTimelineEntry } from "../stores/aiControl";

const ai = useAiControlStore();
const token = ref("");
const tokenVisible = ref(false);
const tokenMessage = ref("");
const actionError = ref("");
const toggling = ref(false);

const statusLabel: Record<string, string> = {
  success: "成功",
  failed: "失败",
  waiting: "等待确认",
  running: "操作中",
  connected: "已连接",
};

function formatTime(timestamp: number) {
  if (!timestamp) return "--:--:--";
  return new Date(timestamp).toLocaleTimeString([], { hour12: false });
}

function timelineLabel(entry: AiTimelineEntry) {
  return `${entry.operation} · ${statusLabel[entry.status] ?? entry.stage}`;
}

async function changeMode(event: Event) {
  const mode = (event.target as HTMLSelectElement).value as typeof ai.permissionMode;
  try {
    await ai.setPermissionMode(mode);
    actionError.value = "";
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : String(error);
  }
}

async function toggleMcp() {
  toggling.value = true;
  try {
    await ai.setEnabled(!ai.enabled);
    actionError.value = "";
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : String(error);
  } finally {
    toggling.value = false;
  }
}

async function approve(actionId: string) {
  try {
    await ai.approve(actionId);
    actionError.value = "";
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : String(error);
  }
}

async function deny(actionId: string) {
  try {
    await ai.deny(actionId);
    actionError.value = "";
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : String(error);
  }
}

async function revealToken() {
  try {
    token.value = await ai.showToken();
    tokenVisible.value = true;
    tokenMessage.value = "Token 仅在本窗口显示，不会进入操作日志";
  } catch (error) {
    tokenMessage.value = error instanceof Error ? error.message : String(error);
  }
}

async function copyToken() {
  if (!token.value) return;
  try {
    await navigator.clipboard.writeText(token.value);
    tokenMessage.value = "Token 已复制";
  } catch (error) {
    tokenMessage.value = error instanceof Error ? error.message : "复制失败";
  }
}

async function resetToken() {
  try {
    await ai.resetToken();
    token.value = "";
    tokenVisible.value = false;
    tokenMessage.value = "Token 已重置，请重新点击显示";
  } catch (error) {
    tokenMessage.value = error instanceof Error ? error.message : String(error);
  }
}

</script>

<template>
  <section class="ai-control" aria-label="MCP 与 AI 控制设置">
    <div class="ai-head">
      <div class="ai-toolbar">
        <div class="ai-heading">
          <span class="ai-title">MCP 与 AI 控制</span>
          <span class="ai-subtitle">后台操作记录与审批</span>
        </div>
        <span class="ai-dot" :class="{ on: ai.enabled && ai.connected }"></span>
        <span class="ai-status">{{ ai.connected ? "已连接" : ai.enabled ? "已启用，等待连接" : "未启用" }}</span>
        <button
          class="ai-btn enable-btn"
          :class="{ active: ai.enabled }"
          :disabled="toggling"
          :aria-pressed="ai.enabled"
          @click="toggleMcp"
        >
          {{ toggling ? "处理中…" : ai.enabled ? "停用 MCP" : "启用 MCP" }}
        </button>
        <span v-if="ai.endpoint" class="ai-endpoint" :title="ai.endpoint">{{ ai.endpoint }}</span>
      </div>
      <div class="ai-permission-row">
        <label class="mode-label" for="ai-permission-mode">AI 操作权限</label>
        <select
          id="ai-permission-mode"
          class="mode-select"
          :value="ai.permissionMode"
          aria-label="AI 权限模式"
          @change="changeMode"
        >
          <option value="observe">观察</option>
          <option value="ask">询问</option>
          <option value="full">完全控制</option>
        </select>
        <span class="permission-hint">控制 AI 是否可以执行写操作</span>
      </div>
      <details class="ai-more">
        <summary>详情</summary>
        <div class="ai-more-body">
          <div class="endpoint-row">
            <span>Endpoint</span>
            <code>{{ ai.endpoint || "桌面应用启动后可用" }}</code>
          </div>
          <div class="token-actions">
            <button class="ai-btn" :disabled="!ai.enabled" @click="revealToken">{{ tokenVisible ? "刷新显示 Token" : "显示 Token" }}</button>
            <button class="ai-btn" :disabled="!token || !ai.enabled" @click="copyToken">复制</button>
            <button class="ai-btn danger" :disabled="!ai.enabled" @click="resetToken">重置 Token</button>
          </div>
          <div v-if="tokenVisible" class="token-value"><code>{{ token }}</code></div>
          <div v-if="tokenMessage" class="ai-hint">{{ tokenMessage }}</div>
        </div>
      </details>
    </div>

    <div v-if="ai.pendingApprovals.length" class="approval-list" aria-live="assertive">
      <article v-for="approval in ai.pendingApprovals" :key="approval.action_id" class="approval-card">
        <div class="approval-copy">
          <strong>需要确认：{{ approval.summary }}</strong>
          <span>{{ approval.operation }} · {{ approval.parameter_summary }}</span>
          <code>{{ approval.action_id }}</code>
        </div>
        <div class="approval-actions">
          <button class="ai-btn approve" @click="approve(approval.action_id)">允许一次</button>
          <button class="ai-btn danger" @click="deny(approval.action_id)">拒绝</button>
        </div>
      </article>
    </div>

    <div class="timeline-head">
      <span>后台操作日志</span>
      <span v-if="actionError || ai.lastError" class="ai-error">{{ actionError || ai.lastError }}</span>
    </div>
    <div v-if="ai.recentTimeline.length" class="timeline" aria-label="AI 操作时间线">
      <div v-for="entry in ai.recentTimeline.slice(-6).reverse()" :key="`${entry.timestampMs}-${entry.actionId ?? entry.operation}`" class="timeline-row" :class="entry.status">
        <span class="timeline-time">{{ formatTime(entry.timestampMs) }}</span>
        <span class="timeline-label">{{ timelineLabel(entry) }}</span>
        <code v-if="entry.actionId">{{ entry.actionId }}</code>
        <span class="timeline-summary">{{ entry.summary }}</span>
      </div>
    </div>
    <div v-else class="timeline-empty">暂无 AI 调用记录</div>
  </section>
</template>

<style scoped>
.ai-control {
  flex: 0 0 auto;
  margin: 0;
  padding: 14px;
  background: var(--panel-bg);
  border: 1px solid var(--panel-border);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-size: 13px;
}
.ai-head, .timeline-head, .endpoint-row, .token-actions, .approval-card {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ai-head { display: grid; gap: 10px; }
.ai-toolbar { display: flex; align-items: center; gap: 8px; min-width: 0; }
.ai-heading { display: grid; gap: 2px; min-width: 132px; }
.ai-title { color: var(--text-primary); font-weight: 700; }
.ai-subtitle { color: var(--text-tertiary); font-size: 11px; }
.ai-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-tertiary); }
.ai-dot.on { background: var(--success); }
.ai-status { color: var(--text-primary); white-space: nowrap; }
.ai-endpoint { min-width: 0; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-tertiary); font-family: ui-monospace, monospace; }
.ai-permission-row { display: flex; align-items: center; gap: 8px; min-width: 0; padding-top: 1px; }
.mode-label { color: var(--text-tertiary); white-space: nowrap; flex-shrink: 0; }
.mode-select { min-height: 24px; padding: 2px 24px 2px 7px; font-size: 12px; }
.permission-hint { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-tertiary); font-size: 11px; }
.ai-more { position: relative; }
.ai-more summary { cursor: pointer; color: var(--accent); list-style: none; }
.ai-more summary::-webkit-details-marker { display: none; }
.ai-more-body { position: absolute; z-index: 40; right: 0; top: calc(100% + 6px); width: 390px; padding: 10px; background: var(--panel-bg); border: 1px solid var(--panel-border); border-radius: var(--radius-md); box-shadow: 0 8px 24px rgba(0,0,0,.18); }
.endpoint-row { align-items: flex-start; }
.endpoint-row span { color: var(--text-tertiary); flex: 0 0 62px; }
.endpoint-row code, .token-value code { overflow-wrap: anywhere; color: var(--text-primary); }
.token-actions { margin-top: 8px; }
.ai-btn { min-height: 24px; padding: 3px 8px; border: 1px solid var(--btn-border); border-radius: var(--radius-sm); background: var(--btn-bg); color: var(--text-secondary); cursor: pointer; font-size: 11px; }
.ai-btn:hover { background: var(--btn-hover); color: var(--text-primary); }
.ai-btn:disabled { opacity: .45; cursor: not-allowed; }
.enable-btn { color: var(--accent); }
.enable-btn.active { color: var(--danger); }
.ai-btn.danger { color: var(--danger); }
.ai-btn.approve { color: var(--success); }
.token-value { margin-top: 8px; padding: 6px; background: var(--edit-bg); border-radius: var(--radius-sm); }
.ai-hint { margin-top: 5px; color: var(--text-tertiary); }
.approval-list { display: grid; gap: 6px; margin-top: 7px; }
.approval-card { align-items: stretch; justify-content: space-between; padding: 7px 8px; background: var(--warning-soft); border: 1px solid var(--warning); border-radius: var(--radius-sm); }
.approval-copy { min-width: 0; display: grid; gap: 2px; }
.approval-copy strong { color: var(--text-primary); }
.approval-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.approval-copy code { color: var(--text-tertiary); }
.approval-actions { display: flex; align-items: center; gap: 5px; flex-shrink: 0; }
.timeline-head { margin-top: 7px; color: var(--text-tertiary); }
.ai-error { margin-left: auto; color: var(--danger); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.timeline { display: grid; gap: 4px; margin-top: 6px; max-height: 220px; overflow-y: auto; }
.timeline-row { display: flex; align-items: center; gap: 7px; min-width: 0; line-height: 18px; }
.timeline-time { color: var(--text-tertiary); font-variant-numeric: tabular-nums; }
.timeline-label { color: var(--text-primary); white-space: nowrap; }
.timeline-row code { color: var(--text-tertiary); max-width: 145px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.timeline-summary { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-tertiary); }
.timeline-row.success .timeline-label { color: var(--success); }
.timeline-row.failed .timeline-label, .timeline-row.waiting .timeline-label { color: var(--warning); }
.timeline-empty { margin-top: 4px; color: var(--text-tertiary); }
@media (max-width: 800px) {
  .ai-endpoint, .timeline-summary { display: none; }
  .approval-card { align-items: flex-start; flex-direction: column; }
  .ai-toolbar { flex-wrap: wrap; }
  .ai-permission-row { flex-wrap: wrap; }
  .permission-hint { flex-basis: 100%; }
  .ai-more-body { right: -8px; width: min(390px, calc(100vw - 32px)); }
}
</style>
