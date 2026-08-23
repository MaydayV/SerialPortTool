// 协议 store：模板管理、RX 解帧、TX 组帧开关
import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  DEFAULT_TEMPLATES,
  canInferFrameBoundary,
  extractFrames,
  MAX_FRAME_LENGTH,
  normalizeFrameTemplate,
  packFrame,
  type FrameTemplate,
} from "../utils/protocol";

const RX_CHUNK_SIZE = 64 * 1024;
const MAX_TEMPLATES = 100;
const MAX_RX_STREAMS = 128;

export const useProtocolStore = defineStore("protocol", () => {
  const templates = ref<FrameTemplate[]>([...DEFAULT_TEMPLATES]);
  const activeName = ref("透传");
  const rxEnabled = ref(false); // 接收解帧
  const txEnabled = ref(false); // 发送组帧
  // TCP Server/UDP 的不同来源必须分别累计，不能把多个客户端的半帧拼到一起。
  const rxBuffers = new Map<string, Uint8Array>();

  // ===== 解帧统计 =====
  const frameCount = ref(0); // 解出的完整帧
  const frameErrorCount = ref(0); // 丢弃的坏帧（CRC 失败/长度非法）
  const frameTrashCount = ref(0); // 无法对齐的杂散字节

  const active = computed(
    () =>
      templates.value.find((t) => t.name === activeName.value) ??
      templates.value[0]
  );
  const canDecodeActive = computed(() => canInferFrameBoundary(active.value));

  function clearBuffers() {
    for (const buffer of rxBuffers.values()) frameTrashCount.value += buffer.length;
    rxBuffers.clear();
  }

  function select(name: string) {
    if (!templates.value.some((template) => template.name === name)) return false;
    activeName.value = name;
    clearBuffers(); // 切换协议清空缓冲
    return true;
  }

  function addTemplate(t: FrameTemplate) {
    const normalized = normalizeFrameTemplate(t);
    if (
      !normalized ||
      templates.value.length >= MAX_TEMPLATES ||
      templates.value.some((item) => item.name === normalized.name)
    ) return false;
    templates.value.push(normalized);
    activeName.value = normalized.name;
    clearBuffers();
    return true;
  }

  /** 用经过校验的模板集合替换当前模板库（用于持久化恢复）。 */
  function replaceTemplates(raw: unknown[]): boolean {
    const normalized: FrameTemplate[] = [];
    const names = new Set<string>();
    for (const item of raw.slice(0, MAX_TEMPLATES)) {
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
    clearBuffers();
    return true;
  }

  function removeTemplate(name: string) {
    if (templates.value.length <= 1) return;
    templates.value = templates.value.filter((t) => t.name !== name);
    if (activeName.value === name) {
      activeName.value = templates.value[0].name;
      clearBuffers();
    }
  }

  function updateTemplate(name: string, patch: Partial<FrameTemplate>) {
    const idx = templates.value.findIndex((t) => t.name === name);
    if (idx >= 0) {
      const normalized = normalizeFrameTemplate({ ...templates.value[idx], ...patch });
      if (!normalized) return false;
      if (
        normalized.name !== name &&
        templates.value.some((template, index) => index !== idx && template.name === normalized.name)
      ) return false;
      templates.value[idx] = normalized;
      if (activeName.value === name) activeName.value = normalized.name;
      clearBuffers();
      return true;
    }
    return false;
  }

  /**
   * RX 数据解帧：返回 [解出的帧数组, 是否启用了解帧]
   * 未启用或透传模板时直接返回原数据
   */
  function processRx(
    data: Uint8Array,
    streamKey = "default"
  ): { frames: Uint8Array[]; enabled: boolean } {
    if (!rxEnabled.value || active.value.checksum === "none" && !active.value.header && !active.value.tail && !active.value.length.enabled) {
      if (!rxEnabled.value && rxBuffers.size) clearBuffers();
      return { frames: [data], enabled: false };
    }
    if (!canDecodeActive.value) {
      // 无边界模板只能用于 TX；RX 安全退回透传，绝不猜测并丢弃半帧。
      frameTrashCount.value += rxBuffers.get(streamKey)?.length ?? 0;
      rxBuffers.delete(streamKey);
      return { frames: [data], enabled: false };
    }
    const frames: Uint8Array[] = [];
    let errors = 0;
    let trash = 0;
    const canInferFrameBoundary = active.value.length.enabled || active.value.tail.length > 0;
    const chunkSize = canInferFrameBoundary ? RX_CHUNK_SIZE : data.length;
    for (let offset = 0; offset < data.length; offset += chunkSize) {
      const incoming = data.subarray(offset, Math.min(offset + chunkSize, data.length));
      const currentBuffer = rxBuffers.get(streamKey) ?? new Uint8Array(0);
      const keepPrevious = Math.max(0, MAX_FRAME_LENGTH - incoming.length);
      const previous =
        keepPrevious === 0
          ? new Uint8Array(0)
          : currentBuffer.length > keepPrevious
          ? currentBuffer.slice(-keepPrevious)
          : currentBuffer;
      const dropped = currentBuffer.length - previous.length;
      const combined = new Uint8Array(previous.length + incoming.length);
      combined.set(previous);
      combined.set(incoming, previous.length);
      const result = extractFrames(combined, active.value);
      const overflow = Math.max(0, result.rest.length - MAX_FRAME_LENGTH);
      const nextBuffer = overflow ? result.rest.slice(overflow) : result.rest;
      if (nextBuffer.length) {
        if (!rxBuffers.has(streamKey) && rxBuffers.size >= MAX_RX_STREAMS) {
          const oldestKey = rxBuffers.keys().next().value as string | undefined;
          if (oldestKey !== undefined) {
            trash += rxBuffers.get(oldestKey)?.length ?? 0;
            rxBuffers.delete(oldestKey);
          }
        }
        rxBuffers.delete(streamKey);
        rxBuffers.set(streamKey, nextBuffer);
      }
      else rxBuffers.delete(streamKey);
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
    clearBuffers();
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
      if (
        parsed &&
        typeof parsed === "object" &&
        !Array.isArray(parsed) &&
        "version" in parsed &&
        parsed.version !== 1
      ) throw new Error("不支持的模板版本");
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
      for (const raw of list.slice(0, MAX_TEMPLATES)) {
        const t = normalizeFrameTemplate(raw);
        if (!t) {
          rejected++;
          continue;
        }
        const idx = templates.value.findIndex((x) => x.name === t.name);
        if (idx >= 0) {
          templates.value[idx] = t;
          replaced++;
        } else if (templates.value.length >= MAX_TEMPLATES) {
          rejected++;
          continue;
        } else {
          templates.value.push(t);
          added++;
        }
        changed = true;
      }
      if (list.length > MAX_TEMPLATES) rejected += list.length - MAX_TEMPLATES;
      if (changed) clearBuffers();
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
    canDecodeActive,
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
