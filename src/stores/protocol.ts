// 协议 store：模板管理、RX 解帧、TX 组帧开关
import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  DEFAULT_TEMPLATES,
  extractFrames,
  MAX_FRAME_LENGTH,
  normalizeFrameTemplate,
  packFrame,
  type FrameTemplate,
} from "../utils/protocol";

const RX_CHUNK_SIZE = 64 * 1024;

export const useProtocolStore = defineStore("protocol", () => {
  const templates = ref<FrameTemplate[]>([...DEFAULT_TEMPLATES]);
  const activeName = ref("透传");
  const rxEnabled = ref(false); // 接收解帧
  const txEnabled = ref(false); // 发送组帧
  const rxBuffer = ref<Uint8Array>(new Uint8Array(0)); // 解帧累积缓冲

  // ===== 解帧统计 =====
  const frameCount = ref(0); // 解出的完整帧
  const frameErrorCount = ref(0); // 丢弃的坏帧（CRC 失败/长度非法）
  const frameTrashCount = ref(0); // 无法对齐的杂散字节

  const active = computed(
    () =>
      templates.value.find((t) => t.name === activeName.value) ??
      templates.value[0]
  );

  function select(name: string) {
    activeName.value = name;
    rxBuffer.value = new Uint8Array(0); // 切换协议清空缓冲
  }

  function addTemplate(t: FrameTemplate) {
    const normalized = normalizeFrameTemplate(t);
    if (!normalized || templates.value.some((item) => item.name === normalized.name)) return false;
    templates.value.push(normalized);
    activeName.value = normalized.name;
    rxBuffer.value = new Uint8Array(0);
    return true;
  }

  /** 用经过校验的模板集合替换当前模板库（用于持久化恢复）。 */
  function replaceTemplates(raw: unknown[]): boolean {
    const normalized: FrameTemplate[] = [];
    const names = new Set<string>();
    for (const item of raw) {
      const template = normalizeFrameTemplate(item);
      if (!template || names.has(template.name)) continue;
      names.add(template.name);
      normalized.push(template);
    }
    if (normalized.length === 0) return false;
    templates.value = normalized;
    activeName.value = normalized.some((item) => item.name === activeName.value)
      ? activeName.value
      : normalized[0].name;
    rxBuffer.value = new Uint8Array(0);
    return true;
  }

  function removeTemplate(name: string) {
    if (templates.value.length <= 1) return;
    templates.value = templates.value.filter((t) => t.name !== name);
    if (activeName.value === name) {
      activeName.value = templates.value[0].name;
      rxBuffer.value = new Uint8Array(0);
    }
  }

  function updateTemplate(name: string, patch: Partial<FrameTemplate>) {
    const idx = templates.value.findIndex((t) => t.name === name);
    if (idx >= 0) {
      const normalized = normalizeFrameTemplate({ ...templates.value[idx], ...patch });
      if (!normalized) return false;
      templates.value[idx] = normalized;
      rxBuffer.value = new Uint8Array(0);
      return true;
    }
    return false;
  }

  /**
   * RX 数据解帧：返回 [解出的帧数组, 是否启用了解帧]
   * 未启用或透传模板时直接返回原数据
   */
  function processRx(data: Uint8Array): { frames: Uint8Array[]; enabled: boolean } {
    if (!rxEnabled.value || active.value.checksum === "none" && !active.value.header && !active.value.tail && !active.value.length.enabled) {
      return { frames: [data], enabled: false };
    }
    const frames: Uint8Array[] = [];
    let errors = 0;
    let trash = 0;
    const canInferFrameBoundary = active.value.length.enabled || active.value.tail.length > 0;
    const chunkSize = canInferFrameBoundary ? RX_CHUNK_SIZE : data.length;
    for (let offset = 0; offset < data.length; offset += chunkSize) {
      const incoming = data.subarray(offset, Math.min(offset + chunkSize, data.length));
      const keepPrevious = Math.max(0, MAX_FRAME_LENGTH - incoming.length);
      const previous =
        keepPrevious === 0
          ? new Uint8Array(0)
          : rxBuffer.value.length > keepPrevious
          ? rxBuffer.value.slice(-keepPrevious)
          : rxBuffer.value;
      const dropped = rxBuffer.value.length - previous.length;
      const combined = new Uint8Array(previous.length + incoming.length);
      combined.set(previous);
      combined.set(incoming, previous.length);
      const result = extractFrames(combined, active.value);
      const overflow = Math.max(0, result.rest.length - MAX_FRAME_LENGTH);
      rxBuffer.value = overflow ? result.rest.slice(overflow) : result.rest;
      frames.push(...result.frames);
      errors += result.errors;
      trash += result.trash + dropped + overflow;
    }
    frameCount.value += frames.length;
    frameErrorCount.value += errors;
    frameTrashCount.value += trash;
    return { frames, enabled: true };
  }

  /** TX 组帧 */
  function processTx(data: Uint8Array): Uint8Array {
    if (!txEnabled.value) return data;
    return packFrame(data, active.value);
  }

  function resetBuffer() {
    rxBuffer.value = new Uint8Array(0);
  }

  /** 重置解帧统计 */
  function resetStats() {
    frameCount.value = 0;
    frameErrorCount.value = 0;
    frameTrashCount.value = 0;
  }

  /** 导出模板库 JSON */
  function exportTemplates(): string {
    return JSON.stringify(
      {
        version: 1,
        templates: templates.value.map((t) => ({
          ...t,
          length: { ...t.length },
        })),
      },
      null,
      2
    );
  }

  /** 从 JSON 导入模板（合并：同名覆盖，新增追加） */
  function importTemplates(json: string): { added: number; replaced: number; rejected: number } {
    try {
      const parsed = JSON.parse(json);
      const list: unknown[] | null = Array.isArray(parsed)
        ? parsed
        : parsed && typeof parsed === "object" && Array.isArray(parsed.templates)
          ? parsed.templates
          : null;
      if (!Array.isArray(list)) throw new Error("格式错误");
      let added = 0;
      let replaced = 0;
      let rejected = 0;
      let changed = false;
      for (const raw of list) {
        const t = normalizeFrameTemplate(raw);
        if (!t) {
          rejected++;
          continue;
        }
        const idx = templates.value.findIndex((x) => x.name === t.name);
        if (idx >= 0) {
          templates.value[idx] = t;
          replaced++;
        } else {
          templates.value.push(t);
          added++;
        }
        changed = true;
      }
      if (changed) rxBuffer.value = new Uint8Array(0);
      return { added, replaced, rejected };
    } catch {
      return { added: 0, replaced: 0, rejected: 0 };
    }
  }

  return {
    templates,
    activeName,
    rxEnabled,
    txEnabled,
    active,
    frameCount,
    frameErrorCount,
    frameTrashCount,
    select,
    addTemplate,
    replaceTemplates,
    removeTemplate,
    updateTemplate,
    processRx,
    processTx,
    resetBuffer,
    resetStats,
    exportTemplates,
    importTemplates,
  };
});
