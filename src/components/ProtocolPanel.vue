<script setup lang="ts">
import { ref, computed } from "vue";
import { useProtocolStore } from "../stores/protocol";
import { CHECKSUM_ALGOS } from "../utils/crc";
import type { FrameTemplate } from "../utils/protocol";

const store = useProtocolStore();
const editing = ref(false);
const newName = ref("");

// 编辑草稿（复制当前模板）
const draft = ref<FrameTemplate>({ ...store.active });

function startEdit() {
  draft.value = JSON.parse(JSON.stringify(store.active));
  editing.value = true;
}

function saveEdit() {
  if (!draft.value.name.trim()) return;
  // 若改名且与原模板不同：新增；否则更新
  if (draft.value.name !== store.activeName) {
    store.addTemplate({ ...draft.value });
  } else {
    store.updateTemplate(store.activeName, { ...draft.value });
  }
  editing.value = false;
}

function cancelEdit() {
  editing.value = false;
}

function addNew() {
  const name = newName.value.trim();
  if (!name) return;
  store.addTemplate({
    name,
    header: "",
    tail: "",
    length: {
      enabled: false,
      offset: 2,
      bytes: 1,
      endian: "little",
      includeSelf: true,
    },
    checksum: "none",
    checksumRange: "all",
    checksumPosition: "tail",
    description: "",
  });
  newName.value = "";
}

const isPassThrough = computed(
  () =>
    !store.active.header &&
    !store.active.tail &&
    !store.active.length.enabled &&
    store.active.checksum === "none"
);
</script>

<template>
  <div class="proto-bar">
    <div class="row1">
      <span class="label">协议</span>
      <select v-model="store.activeName" class="tpl-sel" @change="editing = false">
        <option v-for="t in store.templates" :key="t.name" :value="t.name">
          {{ t.name }}
        </option>
      </select>
      <label class="switch">
        <input type="checkbox" v-model="store.rxEnabled" :disabled="isPassThrough" />
        <span>解帧</span>
      </label>
      <label class="switch">
        <input type="checkbox" v-model="store.txEnabled" :disabled="isPassThrough" />
        <span>组帧</span>
      </label>
      <span v-if="store.active.description" class="desc">
        {{ store.active.description }}
      </span>
      <div class="spacer"></div>
      <input
        v-model="newName"
        class="new-name"
        placeholder="新模板名"
        @keyup.enter="addNew"
      />
      <button class="mini" @click="addNew">＋新建</button>
      <button class="mini" @click="startEdit" :disabled="isPassThrough">编辑</button>
      <button
        class="mini danger"
        @click="store.removeTemplate(store.activeName)"
        :disabled="store.templates.length <= 1"
      >
        删除
      </button>
    </div>

    <!-- 编辑表单 -->
    <div v-if="editing" class="edit-form">
      <div class="field">
        <label>名称</label>
        <input v-model="draft.name" />
      </div>
      <div class="field">
        <label>帧头 (hex)</label>
        <input v-model="draft.header" placeholder="AA 55" />
      </div>
      <div class="field">
        <label>帧尾 (hex)</label>
        <input v-model="draft.tail" placeholder="0D 0A" />
      </div>
      <div class="field">
        <label>
          <input type="checkbox" v-model="draft.length.enabled" /> 长度字段
        </label>
        <template v-if="draft.length.enabled">
          <input v-model.number="draft.length.offset" type="number" class="num" title="偏移" />
          <select v-model.number="draft.length.bytes">
            <option :value="1">1B</option>
            <option :value="2">2B</option>
            <option :value="4">4B</option>
          </select>
          <select v-model="draft.length.endian">
            <option value="little">小端</option>
            <option value="big">大端</option>
          </select>
          <label class="inline">
            <input type="checkbox" v-model="draft.length.includeSelf" /> 含自身
          </label>
        </template>
      </div>
      <div class="field">
        <label>校验</label>
        <select v-model="draft.checksum">
          <option v-for="a in CHECKSUM_ALGOS" :key="a.id" :value="a.id">
            {{ a.name }}
          </option>
        </select>
        <template v-if="draft.checksum !== 'none'">
          <select v-model="draft.checksumRange">
            <option value="all">整帧</option>
            <option value="payload">仅负载</option>
          </select>
          <select v-model="draft.checksumPosition">
            <option value="tail">帧尾</option>
            <option value="before_tail">帧尾前</option>
          </select>
        </template>
      </div>
      <div class="field">
        <label>说明</label>
        <input v-model="draft.description" />
      </div>
      <div class="actions">
        <button class="mini primary" @click="saveEdit">保存</button>
        <button class="mini" @click="cancelEdit">取消</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.proto-bar {
  padding: 6px 16px;
  background: var(--bar-bg);
  border-bottom: 1px solid var(--seg-bg);
  font-size: 13px;
}
.row1 {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.label {
  font-weight: 600;
  color: var(--text-secondary);
}
.tpl-sel {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12.5px;
  background: var(--control-bg);
  min-width: 160px;
}
.switch {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12.5px;
  color: var(--text-secondary);
  cursor: pointer;
}
.switch input {
  accent-color: #0a84ff;
}
.desc {
  color: var(--text-tertiary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 300px;
}
.spacer {
  flex: 1;
}
.new-name {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12.5px;
  width: 110px;
}
.mini {
  border: 1px solid var(--control-border);
  background: var(--control-bg);
  border-radius: 6px;
  padding: 4px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
}
.mini:hover {
  border-color: #0a84ff;
  color: #0a84ff;
}
.mini.primary {
  background: #0a84ff;
  color: #fff;
  border-color: #0a84ff;
}
.mini.danger:hover {
  border-color: #ff3b30;
  color: #ff3b30;
}
.mini:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.edit-form {
  margin-top: 8px;
  padding: 12px;
  background: var(--edit-bg);
  border-radius: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 720px;
}
.field {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.field label {
  min-width: 70px;
  font-size: 12.5px;
  color: var(--text-secondary);
}
.field input:not([type="checkbox"]),
.field select {
  border: 1px solid var(--control-border);
  border-radius: 6px;
  padding: 4px 8px;
  font-size: 12.5px;
  background: var(--control-bg);
}
.field .num {
  width: 56px;
}
.inline {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: auto !important;
  font-size: 12px !important;
}
.actions {
  display: flex;
  gap: 8px;
}

/* 玻璃质感（继承全局控件体系） */
.tool-btn, .opt, .mini, .action-btn, .theme-btn {
  background: var(--btn-glass-bg);
  border: 1px solid var(--btn-glass-border);
  box-shadow: var(--btn-glass-highlight), var(--btn-glass-shadow);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease,
    color 0.15s ease, box-shadow 0.15s ease, transform 0.1s ease;
}
.tool-btn:hover, .opt:hover, .mini:hover, .action-btn:hover, .theme-btn:hover {
  background: var(--btn-glass-hover);
  color: var(--text-primary);
  border-color: var(--field-border-hover);
}
.tool-btn:active, .opt:active, .mini:active, .action-btn:active, .theme-btn:active {
  transform: translateY(0.5px);
}
.tool-btn.active, .opt.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
  box-shadow: 0 2px 8px rgba(10, 132, 255, 0.35);
}
.tool-btn.danger:hover, .mini.danger:hover, .action-btn.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
}
.tool-btn:disabled, .mini:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 输入类控件统一玻璃 */
.ctl, .enc-sel, .history-sel, .custom-input, .interval-input,
.new-name, .field input, .field select, .range-input, .header-input,
.tpl-sel, .target-input, .port-select {
  background: var(--field-bg);
  border: 1px solid var(--field-border);
  box-shadow: var(--field-inner-shadow);
  border-radius: var(--radius-md);
  color: var(--text-primary);
  outline: none;
  transition: border-color 0.15s ease, box-shadow 0.15s ease;
}
.ctl:hover, .enc-sel:hover, .history-sel:hover, .custom-input:hover,
.interval-input:hover, .new-name:hover, .field input:hover, .field select:hover,
.range-input:hover, .header-input:hover, .tpl-sel:hover, .target-input:hover,
.port-select:hover {
  border-color: var(--field-border-hover);
}
.ctl:focus, .enc-sel:focus, .history-sel:focus, .custom-input:focus,
.interval-input:focus, .new-name:focus, .field input:focus, .field select:focus,
.range-input:focus, .header-input:focus, .tpl-sel:focus, .target-input:focus,
.port-select:focus {
  border-color: var(--accent);
  box-shadow: var(--field-inner-shadow), var(--field-focus-ring);
}


.toggle-btn, .send-btn {
  background: var(--btn-primary-bg);
  border: none;
  box-shadow: var(--btn-primary-shadow);
  color: #fff;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: filter 0.15s ease, transform 0.1s ease, box-shadow 0.15s ease;
}
.toggle-btn:hover, .send-btn:hover {
  filter: brightness(1.08);
}
.toggle-btn:active, .send-btn:active {
  transform: translateY(0.5px);
  box-shadow: inset 0 2px 4px rgba(0, 0, 0, 0.18);
}
.toggle-btn:disabled, .send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
  box-shadow: none;
}
.toggle-btn.open {
  background: var(--btn-danger-bg);
  box-shadow: var(--btn-danger-shadow);
}
.toggle-btn.open:hover {
  filter: brightness(1.08);
}


/* ===== 技术审美覆盖：实心纯色 ===== */
.conn-bar, .proto-bar {
  background: var(--bar-bg);
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
  border-bottom: 1px solid var(--panel-border);
}
.conn-bar {
  border-bottom: 1px solid var(--panel-border);
}
.proto-bar {
  border-bottom: 1px solid var(--panel-border);
}

/* 面板内工具栏 */
.toolbar {
  border-bottom: 1px solid var(--panel-border);
}

/* 分段控件：实心 */
.seg {
  background: var(--seg-bg);
  border-radius: var(--radius-md);
  padding: 2px;
}
.seg button {
  border: none;
  background: transparent;
  border-radius: var(--radius-sm);
  color: var(--text-secondary);
}
.seg button.active {
  background: var(--seg-active-bg);
  color: var(--text-primary);
  box-shadow: none;
  border: 1px solid var(--panel-border);
}

/* 次级按钮 */
.tool-btn, .opt, .mini, .action-btn, .theme-btn {
  background: var(--btn-bg);
  border: 1px solid var(--btn-border);
  color: var(--text-secondary);
  border-radius: var(--radius-md);
  box-shadow: none;
  transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
}
.tool-btn:hover, .opt:hover, .mini:hover, .action-btn:hover, .theme-btn:hover {
  background: var(--btn-hover);
  color: var(--text-primary);
  border-color: var(--field-border-hover);
}
.tool-btn:active, .opt:active, .mini:active, .action-btn:active {
  background: var(--btn-active);
}
.tool-btn.active, .opt.active {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.tool-btn.danger:hover, .mini.danger:hover, .action-btn.danger:hover {
  border-color: var(--danger);
  color: var(--danger);
  background: var(--btn-bg);
}
.tool-btn:disabled, .mini:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 主按钮 */
.toggle-btn, .send-btn {
  background: var(--btn-primary-bg);
  border: none;
  box-shadow: none;
  color: #fff;
  border-radius: var(--radius-md);
  transition: background 0.12s ease;
}
.toggle-btn:hover, .send-btn:hover {
  background: var(--btn-primary-hover);
}
.toggle-btn.open {
  background: var(--btn-danger-bg);
}
.toggle-btn.open:hover {
  background: var(--btn-danger-hover);
}
.toggle-btn:disabled, .send-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* 输入类控件 */
.ctl, .enc-sel, .history-sel, .custom-input, .interval-input,
.new-name, .field input, .field select, .range-input, .header-input,
.tpl-sel, .target-input, .port-select {
  background: var(--field-bg);
  border: 1px solid var(--field-border);
  border-radius: var(--radius-md);
  box-shadow: none;
  color: var(--text-primary);
  transition: border-color 0.12s ease, box-shadow 0.12s ease;
}
.ctl:hover, .enc-sel:hover, .history-sel:hover, .custom-input:hover,
.interval-input:hover, .new-name:hover, .field input:hover, .field select:hover,
.range-input:hover, .header-input:hover, .tpl-sel:hover, .target-input:hover,
.port-select:hover {
  border-color: var(--field-border-hover);
}
.ctl:focus, .enc-sel:focus, .history-sel:focus, .custom-input:focus,
.interval-input:focus, .new-name:focus, .field input:focus, .field select:focus,
.range-input:focus, .header-input:focus, .tpl-sel:focus, .target-input:focus,
.port-select:focus {
  border-color: var(--accent);
  box-shadow: var(--field-focus-ring);
}

/* 发送区文本域 */
.send-area {
  background: var(--field-bg);
  border: none;
  color: var(--text-primary);
}

/* 接收区行 */
.row.rx {
  color: var(--text-primary);
}
.row.tx {
  color: var(--accent);
  background: var(--row-tx-bg);
}
.ts, .dir {
  color: var(--text-tertiary);
}
.hex {
  color: var(--text-primary);
}
.stats {
  color: var(--text-secondary);
}

/* 编辑表单 */
.edit-form {
  background: var(--edit-bg);
  border-radius: var(--radius-md);
}

/* 波形图 chart 区域 */
.chart {
  background: var(--panel-bg);
}

/* 协议面板 label */
.label {
  color: var(--text-primary);
}
.desc {
  color: var(--text-tertiary);
}
.title {
  color: var(--text-primary);
}

</style>
