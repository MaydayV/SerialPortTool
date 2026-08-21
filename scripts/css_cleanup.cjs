// 清理重复 CSS：删除旧玻璃样式块（--btn-glass-* 时代遗留），只保留技术审美实心块
const fs = require("fs");

const files = [
  "src/components/ConnectionBar.vue",
  "src/components/ReceivePanel.vue",
  "src/components/SendPanel.vue",
  "src/components/ProtocolPanel.vue",
  "src/components/GraphPanel.vue",
];

function stripGlass(s) {
  // 删除从 "/* 玻璃质感" 注释开始，到 "/* ===== 技术审美覆盖" 之前的所有块
  const startMark = "/* 玻璃质感";
  const endMark = "/* ===== 技术审美覆盖：实心纯色 ===== */";
  const si = s.indexOf(startMark);
  const ei = s.indexOf(endMark);
  if (si >= 0 && ei > si) {
    // 保留 endMark 本身，删除 startMark..endMark 之前
    return s.slice(0, si) + s.slice(ei);
  }
  return s;
}

let changed = 0;
for (const f of files) {
  let s = fs.readFileSync(f, "utf8");
  const before = s.length;
  s = stripGlass(s);
  // 压缩多余空行
  s = s.replace(/\n{3,}/g, "\n\n");
  if (s.length !== before) {
    fs.writeFileSync(f, s);
    changed++;
    console.log(
      `cleaned ${f}: ${before} -> ${s.length} chars (-${before - s.length})`
    );
  } else {
    console.log(`SKIP ${f}: no glass block found`);
  }
}
console.log(`\n${changed}/${files.length} cleaned`);
