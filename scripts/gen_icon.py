# 生成 SerialPortTool 应用图标（macOS icns + Windows ico + png 各尺寸）
# 风格：深蓝渐变圆角方块 + 白色串口连接节点图案
from PIL import Image, ImageDraw
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]

SIZE = 1024
# Keep a macOS-style optical margin so the Finder icon is not visually larger
# than neighboring applications in a DMG window.
ART_SIZE = 860
OFFSET = (SIZE - ART_SIZE) // 2
SCALE = ART_SIZE / SIZE
# 渐变背景（深蓝 → 蓝紫）
img = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
art = Image.new("RGBA", (ART_SIZE, ART_SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(art)

# 圆角矩形渐变：逐行绘制
for y in range(ART_SIZE):
    t = y / ART_SIZE
    r = int(30 + (40 - 30) * t)
    g = int(80 + (90 - 80) * t)
    b = int(200 + (230 - 200) * t)
    d.rectangle([0, y, ART_SIZE, y + 1], fill=(r, g, b, 255))

# 蒙版圆角
mask = Image.new("L", (ART_SIZE, ART_SIZE), 0)
md = ImageDraw.Draw(mask)
md.rounded_rectangle([0, 0, ART_SIZE - 1, ART_SIZE - 1], radius=185, fill=255)
art.putalpha(mask)
img.alpha_composite(art, (OFFSET, OFFSET))

# 白色串口节点：左节点 + 连线 + 右节点（DB9 风格）
d = ImageDraw.Draw(img)
white = (255, 255, 255, 255)

# 左节点（圆形，带内部针脚点）
cx1, cy1 = 300 * SCALE + OFFSET, 512 * SCALE + OFFSET
r1 = 130 * SCALE
d.ellipse([cx1 - r1, cy1 - r1, cx1 + r1, cy1 + r1], fill=None, outline=white, width=round(36 * SCALE))
# 内部针脚 5 点
for px, py in [(cx1, cy1), (cx1 - 45 * SCALE, cy1 - 45 * SCALE), (cx1 + 45 * SCALE, cy1 - 45 * SCALE), (cx1 - 45 * SCALE, cy1 + 45 * SCALE), (cx1 + 45 * SCALE, cy1 + 45 * SCALE)]:
    pin = 22 * SCALE
    d.ellipse([px - pin, py - pin, px + pin, py + pin], fill=white)

# 右节点（正方形，串口端子风格）
cx2, cy2 = 724 * SCALE + OFFSET, 512 * SCALE + OFFSET
r2 = 130 * SCALE

# 连线
d.line([(cx1 + r1, cy1), (cx2 - r2, cy2)], fill=white, width=round(36 * SCALE))

d.rounded_rectangle([cx2 - r2, cy2 - r2, cx2 + r2, cy2 + r2], radius=round(36 * SCALE), fill=None, outline=white, width=round(36 * SCALE))
# 端子 3 孔
for px in [cx2 - 55 * SCALE, cx2, cx2 + 55 * SCALE]:
    pin = 25 * SCALE
    d.ellipse([px - pin, cy2 - pin, px + pin, cy2 + pin], fill=white)

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
