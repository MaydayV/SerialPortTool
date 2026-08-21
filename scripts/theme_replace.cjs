// 批量把组件硬编码颜色替换为 CSS 变量
const { execSync } = require("child_process");
const fs = require("fs");

const files = [
  "src/components/ConnectionBar.vue",
  "src/components/ReceivePanel.vue",
  "src/components/SendPanel.vue",
  "src/components/ProtocolPanel.vue",
  "src/components/GraphPanel.vue",
];

const replacements = [
  // 背景类
  [/rgba\(255, 255, 255, 0\.65\)/g, "var(--bar-bg)"],
  [/rgba\(255, 255, 255, 0\.72\)/g, "var(--panel-bg)"],
  [/rgba\(255, 255, 255, 0\.5\)/g, "var(--bar-bg)"],
  [/rgba\(0, 0, 0, 0\.06\)/g, "var(--seg-bg)"],
  [/rgba\(0, 0, 0, 0\.03\)/g, "var(--edit-bg)"],
  // 文字色
  [/#1d1d1f/g, "var(--text-primary)"],
  [/#6e6e73/g, "var(--text-secondary)"],
  [/#98989d/g, "var(--text-tertiary)"],
  [/#48484a/g, "var(--text-secondary)"],
  [/#8e8e93/g, "var(--text-tertiary)"],
  // 控件
  [/background: #fff;/g, "background: var(--control-bg);"],
  [/background: #ffffff;/g, "background: var(--control-bg);"],
  [/border: 1px solid rgba\(0, 0, 0, 0\.12\)/g, "border: 1px solid var(--control-border)"],
  [/border: 1px solid rgba\(0, 0, 0, 0\.1\)/g, "border: 1px solid var(--control-border)"],
  [/border: 1px solid rgba\(0, 0, 0, 0\.07\)/g, "border-bottom: 1px solid var(--panel-border)"],
  [/color: #1d1d1f/g, "color: var(--text-primary)"],
  [/color: #48484a/g, "color: var(--text-secondary)"],
  [/color: #6e6e73/g, "color: var(--text-secondary)"],
  [/color: #98989d/g, "color: var(--text-tertiary)"],
  // 行
  [/border-bottom: 1px solid rgba\(0, 0, 0, 0\.03\)/g, "border-bottom: 1px solid var(--row-border)"],
  [/background: rgba\(10, 132, 255, 0\.04\)/g, "background: var(--row-tx-bg)"],
];

let changed = 0;
for (const f of files) {
  let s = fs.readFileSync(f, "utf8");
  const before = s;
  for (const [re, rep] of replacements) {
    s = s.replace(re, rep);
  }
  if (s !== before) {
    fs.writeFileSync(f, s);
    changed++;
    console.log(`updated ${f}`);
  }
}
console.log(`\n${changed}/${files.length} files updated`);
