"""Convy 应用图标生成器（定稿：蓝底白纸 + 折角）。

设计说明
--------
蓝色 squircle 底板 + 一张纯白纸，右上角折起露出淡蓝折角 —— 表达「文件被
重新装订成另一种格式」。底色用品牌蓝并与界面强调色 #185fa5 同源，页面反白，
属于 App 图标里最稳的「徽章型」构图：白色形状本身就有对比度，不需要靠阴影
撑轮廓，所以 16px 在小任务栏上仍然读得出来。

配色历程（记录以免重犯）
------------------------
早期版本是浅灰白底 + 蓝页 + 橙页：蓝(214°)橙(21°)相差 193°、饱和度又都在
80% 以上，等于两个互补色平分画面互相打架；后来橙后退到折角一处（面积~10%），
但浅底白噪 achromatic 背景让整体偏「文档图标素材」而非「应用图标」。最终把
底色也纳入配色：单主色 + 反白主体 + 同色系浅调点缀。

小尺寸策略
----------
16/24/32/48px 下两张纸会糊成一团，因此提供**简化变体**：折角放大、无倾角、
页面加大。.ico 里按尺寸混装两套美术（Vista+ 支持 PNG 帧），这与 macOS /
Windows 原生图标的做法一致。

输出（src-tauri/icons/）
  32x32 64x64 128x128 128x128@2x icon.png
  icon.ico（16/24/32/48 简化 + 64/128/256 完整）
  icon.icns（Big Sur 边距 + 投影变体）
  Square*Logo / StoreLogo
项目根目录另存 icon-src.png / icon-src-bigsur.png 两张 1024 母版。
"""

import math
import os
import struct

from PIL import Image, ImageDraw, ImageFilter

S = 2048  # 渲染尺寸（超采样后缩到目标尺寸）
N = 5.2
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "src-tauri", "icons")

# --- 配色：brand 蓝底 + 反白页面 + 同色系折角 ----------------------------
BG_MODE = "grad"  # light | grad | solid
BG_TOP = (62, 123, 240)  # #3E7BF0
BG_BOT = (27, 77, 184)  # #1B4DB8
PAGE_TOP, PAGE_BOT = (255, 255, 255), (237, 242, 251)  # #FFFFFF → #EDF2FB
FOLD_TOP, FOLD_BOT = (169, 198, 255), (143, 178, 245)  # #A9C6FF → #8FB2F5
# 页面主体的阴影强度：彩色底上不用太重，否则白页边缘发脏
SHADOW_ALPHA = 46
SHADOW_RGB = (20, 26, 38)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def sc(v):
    """1024 设计网格 → 渲染画布"""
    return v * S / 1024.0


def squircle_points(cx, cy, a, n, steps=1600):
    out = []
    for i in range(steps):
        t = 2 * math.pi * i / steps
        c, s = math.cos(t), math.sin(t)
        out.append(
            (
                cx + a * math.copysign(abs(c) ** (2.0 / n), c),
                cy + a * math.copysign(abs(s) ** (2.0 / n), s),
            )
        )
    return out


def squircle_mask(size, inset=0.0):
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).polygon(
        squircle_points(size / 2, size / 2, size / 2 - inset, N), fill=255
    )
    return m


def background(size):
    """squircle 底板。浅底走 .light（白底 + 玻璃高光 + 白色内描边），
    彩色/深色底走渐变并对描边换成弱白色，避免贴边出现一圈脏光。"""
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    if BG_MODE == "solid":
        d.rectangle([0, 0, size, size], fill=BG_TOP)
    else:
        for y in range(size):
            d.line([(0, y), (size, y)], fill=lerp(BG_TOP, BG_BOT, y / (size - 1)))
    img = img.convert("RGBA")

    if BG_MODE == "light":
        gloss = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        ImageDraw.Draw(gloss).ellipse(
            [-size * 0.3, -size * 0.8, size * 1.3, size * 0.34], fill=(255, 255, 255, 52)
        )
        gloss = gloss.filter(ImageFilter.GaussianBlur(size * 0.035))
        img = Image.alpha_composite(img, gloss)

    edge = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    pts = squircle_points(size / 2, size / 2, size / 2 - size * 0.007, N)
    stroke = (255, 255, 255, 190) if BG_MODE == "light" else (255, 255, 255, 46)
    ImageDraw.Draw(edge).line(
        pts + [pts[0]], fill=stroke, width=max(1, int(size * 0.0045))
    )
    img = Image.alpha_composite(img, edge)
    img.putalpha(squircle_mask(size))
    return img


def tint(mask, top, bot):
    grad = Image.new("RGB", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        gd.line([(0, y), (S, y)], fill=lerp(top, bot, y / (S - 1)))
    out = grad.convert("RGBA")
    out.putalpha(mask)
    return out


def compose(layers):
    """自动居中 + 柔和投影 + 逐层上色。layers: [(mask, top, bot)]"""
    bg = background(S)
    union = Image.new("L", (S, S), 0)
    for m, _, _ in layers:
        union = Image.composite(m, union, m)

    # 按整体包围盒做光学居中：旋转后的页面会把重心带偏，靠手工偏移永远差一点
    box = union.getbbox()
    if box:
        dx = int(S / 2 - (box[0] + box[2]) / 2)
        dy = int(S / 2 - (box[1] + box[3]) / 2)
        if dx or dy:
            layers = [
                (m.transform((S, S), Image.AFFINE, (1, 0, -dx, 0, 1, -dy)), t, b)
                for m, t, b in layers
            ]
            union = union.transform((S, S), Image.AFFINE, (1, 0, -dx, 0, 1, -dy))

    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    black = Image.new("RGBA", (S, S), SHADOW_RGB + (255,))
    black.putalpha(union.point(lambda v: v * SHADOW_ALPHA // 255))
    sh.paste(black, (0, int(sc(10))), black)
    sh = sh.filter(ImageFilter.GaussianBlur(sc(14)))
    img = Image.alpha_composite(bg, sh)

    for m, top, bot in layers:
        img = Image.alpha_composite(img, tint(m, top, bot))
    return img


def _page(w, h, x, y, radius, corner_cut=0):
    """一张纸；corner_cut > 0 时切掉右上角的直角三角形。"""
    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    d.rounded_rectangle([x, y, x + w, y + h], radius=radius, fill=255)
    if corner_cut:
        k = corner_cut
        # 切掉外角本身（直角在 x+w,y），页面留下一条斜边
        d.polygon([(x + w - k, y - 4), (x + w + 4, y - 4), (x + w + 4, y + k)], fill=0)
    return m


def _flap(w, h, x, y, k, inset=0.0):
    """折角：被翻折过来的纸背，正好覆盖斜边内侧的三角形。

    `inset` 只沿斜边收缩一点，露出纸的厚度；两条直角边与页面齐平，
    否则折角会看起来像「浮在页面上的一块三角」。
    """
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).polygon(
        [
            (x + w - k + inset * 1.4, y + inset * 1.4),
            (x + w - inset * 1.4, y + k - inset * 1.4),
            (x + w - k, y + k),
        ],
        fill=255,
    )
    return m


def icon_full():
    """完整版（≥64px）：白页面 + 右上折角，置于蓝色底板上。"""
    C = S / 2
    wd, ht = sc(404), sc(470)
    k = sc(140)
    angle = 4
    fx, fy = C - wd / 2 + sc(10), C - ht / 2 - sc(2)

    page = _page(wd, ht, fx, fy, sc(42), k).rotate(angle, resample=Image.BICUBIC, center=(C, C))
    flap = _flap(wd, ht, fx, fy, k, 0).rotate(angle, resample=Image.BICUBIC, center=(C, C))

    return compose([(page, PAGE_TOP, PAGE_BOT), (flap, FOLD_TOP, FOLD_BOT)])


def icon_small():
    """简化版：单页 + 放大折角（≤48px 使用）。"""
    C = S / 2
    wd, ht = sc(450), sc(520)
    k = sc(168)
    fx, fy = C - wd / 2, C - ht / 2

    page = _page(wd, ht, fx, fy, sc(52), k)
    flap = _flap(wd, ht, fx, fy, k, 0)

    return compose([(page, PAGE_TOP, PAGE_BOT), (flap, FOLD_TOP, FOLD_BOT)])


def with_big_sur_margin(master):
    """macOS Big Sur 风格：内容 80% + 投影，透明画布，供 .icns 使用。"""
    scale = 0.80
    inner = int(S * scale)
    content = master.resize((inner, inner), Image.LANCZOS)
    canvas = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    off = (S - inner) // 2

    alpha = content.split()[3].point(lambda v: v * 90 // 255)
    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    black = Image.new("RGBA", (inner, inner), (30, 41, 59, 255))
    black.putalpha(alpha)
    sh.paste(black, (off, off + int(S * 0.016)), black)
    sh = sh.filter(ImageFilter.GaussianBlur(S * 0.018))
    canvas = Image.alpha_composite(canvas, sh)
    canvas.paste(content, (off, off), content)
    return canvas


def write_ico(path, frames):
    """手写多尺寸 ICO：每个尺寸可以放不同美术（PIL 自带的只支持等比缩放同一张）。

    frames: [(px, RGBA Image)]
    """
    frames = sorted(frames, key=lambda f: f[0])
    payload = []
    for size, im in frames:
        tmp = os.path.join(HERE, f".ico-{size}.png")
        im.save(tmp, format="PNG")
        with open(tmp, "rb") as f:
            payload.append(f.read())
        os.remove(tmp)

    header = struct.pack("<HHH", 0, 1, len(frames))
    offset = 6 + 16 * len(frames)
    entries = b""
    for (size, _), data in zip(frames, payload):
        dim = 0 if size >= 256 else size
        entries += struct.pack(
            "<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset
        )
        offset += len(data)

    with open(path, "wb") as f:
        f.write(header)
        f.write(entries)
        for data in payload:
            f.write(data)


def main():
    os.makedirs(OUT, exist_ok=True)
    full = icon_full()
    small = icon_small()

    def art(size):
        return (small if size <= 48 else full).resize((size, size), Image.LANCZOS)

    def save(name, size):
        art(size).save(os.path.join(OUT, name))

    for name, size in [
        ("32x32.png", 32),
        ("64x64.png", 64),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
        ("icon.png", 512),
        ("Square30x30Logo.png", 30),
        ("Square44x44Logo.png", 44),
        ("Square71x71Logo.png", 71),
        ("Square89x89Logo.png", 89),
        ("Square107x107Logo.png", 107),
        ("Square142x142Logo.png", 142),
        ("Square150x150Logo.png", 150),
        ("Square284x284Logo.png", 284),
        ("Square310x310Logo.png", 310),
        ("StoreLogo.png", 30),
    ]:
        save(name, size)

    # Windows 图标：小尺寸用简化美术，大尺寸用完整美术
    write_ico(
        os.path.join(OUT, "icon.ico"),
        [(s, art(s)) for s in (16, 24, 32, 48, 64, 128, 256)],
    )

    # macOS：Big Sur 边距 + 投影
    bigsur_full = with_big_sur_margin(full)
    bigsur_small = with_big_sur_margin(small)
    bigsur_full.resize((1024, 1024), Image.LANCZOS).save(
        os.path.join(HERE, "icon-src-bigsur.png")
    )
    layers = []
    for s in (16, 32, 64, 128, 256, 512, 1024):
        src = bigsur_small if s <= 32 else bigsur_full
        layers.append(src.resize((s, s), Image.LANCZOS))
    layers[-1].save(os.path.join(OUT, "icon.icns"), append_images=layers[:-1])

    full.resize((1024, 1024), Image.LANCZOS).save(os.path.join(HERE, "icon-src.png"))
    print("icon set written to", OUT)


if __name__ == "__main__":
    main()
