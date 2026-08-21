// 协议 store：模板管理、RX 解帧、TX 组帧开关
import { defineStore } from "pinia";
import { ref, computed } from "vue";
import {
  DEFAULT_TEMPLATES,
  extractFrames,
  packFrame,
  type FrameTemplate,
} from "../utils/protocol";

export const useProtocolStore = defineStore("protocol", () => {
  const templates = ref<FrameTemplate[]>([...DEFAULT_TEMPLATES]);
  const activeName = ref("透传");
  const rxEnabled = ref(false); // 接收解帧
  const txEnabled = ref(false); // 发送组帧
  const rxBuffer = ref<Uint8Array>(new Uint8Array(0)); // 解帧累积缓冲

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
    templates.value.push(t);
    activeName.value = t.name;
    rxBuffer.value = new Uint8Array(0);
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
      templates.value[idx] = { ...templates.value[idx], ...patch };
      rxBuffer.value = new Uint8Array(0);
    }
  }

  /**
   * RX 数据解帧：返回 [解出的帧数组, 是否启用了解帧]
   * 未启用或透传模板时直接返回原数据
   */
  function processRx(data: Uint8Array): { frames: Uint8Array[]; enabled: boolean } {
    if (!rxEnabled.value || active.value.checksum === "none" && !active.value.header && !active.value.tail && !active.value.length.enabled) {
      return { frames: [data], enabled: false };
    }
    const combined = new Uint8Array(rxBuffer.value.length + data.length);
    combined.set(rxBuffer.value);
    combined.set(data, rxBuffer.value.length);
    const { frames, rest } = extractFrames(combined, active.value);
    rxBuffer.value = rest;
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

  return {
    templates,
    activeName,
    rxEnabled,
    txEnabled,
    active,
    select,
    addTemplate,
    removeTemplate,
    updateTemplate,
    processRx,
    processTx,
    resetBuffer,
  };
});
