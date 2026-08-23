# 生成 SerialAid 应用图标（macOS icns + Windows ico + png 各尺寸）
# 风格：深蓝渐变圆角方块 + 白色串口连接节点图案
from PIL import Image, ImageDraw
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

SIZE = 1024
# 渐变背景（深蓝 → 蓝紫）
img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# 圆角矩形渐变：逐行绘制
for y in range(SIZE):
    t = y / SIZE
    r = int(30 + (40 - 30) * t)
    g = int(80 + (90 - 80) * t)
    b = int(200 + (230 - 200) * t)
    d.rectangle([0, y, SIZE, y + 1], fill=(r, g, b, 255))

# 蒙版圆角
mask = Image.new("L", (SIZE, SIZE), 0)
md = ImageDraw.Draw(mask)
md.rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=220, fill=255)
img.putalpha(mask)

# 白色串口节点：左节点 + 连线 + 右节点（DB9 风格）
d = ImageDraw.Draw(img)
white = (255, 255, 255, 255)

# 左节点（圆形，带内部针脚点）
cx1, cy1 = 300, 512
r1 = 130
d.ellipse([cx1 - r1, cy1 - r1, cx1 + r1, cy1 + r1], fill=None, outline=white, width=36)
# 内部针脚 5 点
for px, py in [(cx1, cy1), (cx1 - 45, cy1 - 45), (cx1 + 45, cy1 - 45), (cx1 - 45, cy1 + 45), (cx1 + 45, cy1 + 45)]:
    d.ellipse([px - 22, py - 22, px + 22, py + 22], fill=white)

# 连线
d.line([(cx1 + r1, cy1), (724 - r1, cy1)], fill=white, width=36)

# 右节点（正方形，串口端子风格）
cx2, cy2 = 724, 512
r2 = 130
d.rounded_rectangle([cx2 - r2, cy2 - r2, cx2 + r2, cy2 + r2], radius=36, fill=None, outline=white, width=36)
# 端子 3 孔
for px in [cx2 - 55, cx2, cx2 + 55]:
    d.ellipse([px - 25, cy2 - 25, px + 25, cy2 + 25], fill=white)

outdir = ROOT / "src-tauri" / "icons"
outdir.mkdir(parents=True, exist_ok=True)
source = outdir / "app-icon-source.png"
img.save(source)

# 统一交给 Tauri 生成 macOS 完整 ICNS（含 1024px）、Windows ICO 及各平台 PNG。
subprocess.run(
    ["npm", "run", "tauri", "icon", str(source)],
    cwd=ROOT,
    check=True,
)

# 兼容仓库中早期命名，确保未被 tauri.conf 引用的备用图标也不会残留默认素材。
for size, name in [
    (256, "256x256.png"),
    (512, "512x512.png"),
    (512, "icon_512x512.png"),
]:
    img.resize((size, size), Image.Resampling.LANCZOS).save(outdir / name)

print(f"Application icons generated from {source}")
