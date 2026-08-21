// UI 控件高端化：组件内按钮/输入样式批量升级
const fs = require("fs");

const files = [
  "src/components/ConnectionBar.vue",
  "src/components/ReceivePanel.vue",
  "src/components/SendPanel.vue",
  "src/components/ProtocolPanel.vue",
  "src/components/GraphPanel.vue",
];

// 1) 组件内自定义按钮类 → 追加玻璃质感（保留原有布局类，增强视觉）
const buttonEnhance = `
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
`;

// 2) 主按钮（打开/发送）→ 渐变主按钮
const primaryEnhance = `
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
`;

let changed = 0;
for (const f of files) {
  let s = fs.readFileSync(f, "utf8");
  const before = s;

  // 删除旧的按钮/输入重复定义（避免覆盖全局）——只删 background/border/shadow 冲突行
  // 更稳妥：在 </style> 前追加增强样式（scoped 后加载覆盖同特异性）
  s = s.replace(/<\/style>/, `${buttonEnhance}\n${primaryEnhance}\n</style>`);

  // 修正 select 在 scoped 里的 padding-right 冲突（全局已 !important）
  if (s !== before) {
    fs.writeFileSync(f, s);
    changed++;
    console.log(`enhanced ${f}`);
  }
}
console.log(`\n${changed}/${files.length} files enhanced`);
