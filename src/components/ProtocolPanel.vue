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
    if (!store.addTemplate({ ...draft.value })) {
      alert("模板名称已存在或内容无效");
      return;
    }
  } else {
    if (!store.updateTemplate(store.activeName, { ...draft.value })) {
      alert("模板内容无效，请检查 HEX、长度和校验配置");
      return;
    }
  }
  editing.value = false;
}

function cancelEdit() {
  editing.value = false;
}

function onTemplateChange(e: Event) {
  const name = (e.target as HTMLSelectElement).value;
  store.select(name);
  editing.value = false;
}

function addNew() {
  const name = newName.value.trim();
  if (!name) return;
  const created = store.addTemplate({
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
  if (!created) {
    alert("模板名称已存在或内容无效");
    return;
  }
  newName.value = "";
}

const isPassThrough = computed(
  () =>
    !store.active.header &&
    !store.active.tail &&
    !store.active.length.enabled &&
    store.active.checksum === "none"
);

/** 导出模板库到文件 */
function exportTemplates() {
  const blob = new Blob([store.exportTemplates()], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "serialaid-templates.json";
  a.click();
  URL.revokeObjectURL(url);
}

/** 导入模板库 */
const importInput = ref<HTMLInputElement | null>(null);
function pickImport() {
  importInput.value?.click();
}
async function onImportFile(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;
  const text = await file.text();
  const res = store.importTemplates(text);
  if (res.added || res.replaced) {
    const rejected = res.rejected ? `，拒绝 ${res.rejected} 个坏模板` : "";
    alert(`导入成功：新增 ${res.added} 个，覆盖 ${res.replaced} 个${rejected}`);
  } else if (res.rejected) {
    alert(`导入失败：拒绝 ${res.rejected} 个坏模板`);
  } else {
    alert("导入失败：文件格式不正确");
  }
  input.value = "";
}

/** 删除确认 */
function onRemove() {
  if (confirm(`确定删除模板「${store.activeName}」？`)) {
    store.removeTemplate(store.activeName);
  }
}
</script>

<template>
  <div class="proto-bar">
    <div class="row1">
      <span class="label">协议</span>
      <select :value="store.activeName" class="tpl-sel" @change="onTemplateChange">
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
      <span
        v-if="store.rxEnabled && !isPassThrough"
        class="frame-stats"
        title="解帧统计（点击重置）"
        @click="store.resetStats()"
      >
        解出 {{ store.frameCount }} 帧 · 坏帧 {{ store.frameErrorCount }} · 杂散
        {{ store.frameTrashCount }}B
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
      <button class="mini" @click="exportTemplates" title="导出全部模板为 JSON">导出</button>
      <button class="mini" @click="pickImport" title="从 JSON 导入模板">导入</button>
      <input
        ref="importInput"
        type="file"
        accept=".json"
        style="display: none"
        @change="onImportFile"
      />
      <button
        class="mini danger"
        @click="onRemove"
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
.frame-stats {
  color: var(--accent);
  font-size: 11.5px;
  cursor: pointer;
  background: var(--accent-soft);
  border-radius: var(--radius-sm);
  padding: 2px 8px;
  white-space: nowrap;
}
.frame-stats:hover {
  filter: brightness(1.1);
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
</style>
