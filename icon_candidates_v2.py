"""Convy 图标备选（第二版）。

关键改进：
1. **箭头与环带是一条连续多边形**：先取中心线采样点 → 沿法线偏移出内外边 →
   末端直接延伸出箭头倒钩与尖端。箭头基部与环带同宽，不会出现「贴上去的三角」。
2. 图案放大到画布 ~62%，接近 macOS / Windows 图标的视觉占比。
3. 每个方案额外渲染 32px 缩略图，验证小尺寸辨识度。

输出 icon-candidates/v2/
"""

import math
import os

from PIL import Image, ImageDraw, ImageFilter

S = 2048
N = 5.2
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "icon-candidates", "v2")

BG_TOP = (250, 251, 253)
BG_BOT = (237, 240, 245)
BLUE_TOP, BLUE_BOT = (64, 145, 252), (33, 106, 234)
ORANGE_TOP, ORANGE_BOT = (255, 163, 94), (255, 115, 40)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def sc(v):
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
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    for y in range(size):
        d.line([(0, y), (size, y)], fill=lerp(BG_TOP, BG_BOT, y / (size - 1)))
    img = img.convert("RGBA")

    gloss = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    ImageDraw.Draw(gloss).ellipse(
        [-size * 0.3, -size * 0.8, size * 1.3, size * 0.34], fill=(255, 255, 255, 52)
    )
    gloss = gloss.filter(ImageFilter.GaussianBlur(size * 0.035))
    img = Image.alpha_composite(img, gloss)

    edge = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    pts = squircle_points(size / 2, size / 2, size / 2 - size * 0.007, N)
    ImageDraw.Draw(edge).line(pts + [pts[0]], fill=(255, 255, 255, 190), width=max(1, int(size * 0.0045)))
    img = Image.alpha_composite(img, edge)
    img.putalpha(squircle_mask(size))
    return img


# ---------------------------------------------------------------- 几何工具


def sample_arc(cx, cy, r, a0, a1, n=260):
    return [
        (
            cx + r * math.cos(math.radians(a0 + (a1 - a0) * i / (n - 1))),
            cy + r * math.sin(math.radians(a0 + (a1 - a0) * i / (n - 1))),
        )
        for i in range(n)
    ]


def sample_squircle_path(cx, cy, a, frac0, frac1, n=300, expo=None):
    pts = squircle_points(cx, cy, a, expo or N, steps=2000)
    total = len(pts)
    i0 = int(total * frac0) % total
    out = []
    span = (frac1 - frac0) % 1.0 or 1.0
    for i in range(n):
        out.append(pts[int((i0 + span * total * i / (n - 1)) % total)])
    return out


def _unit(dx, dy):
    ln = math.hypot(dx, dy) or 1.0
    return dx / ln, dy / ln


def ribbon(pts, w, head=None, cap_start=True, head_barb=0.44, head_len=1.5):
    """中心线 → 单条闭合多边形（环带 + 箭头 + 圆头）。

    head: None | "end" | "start"（箭头朝向该端的外侧）
    """
    n = len(pts)
    left, right = [], []
    for i, p in enumerate(pts):
        j0, j1 = max(i - 1, 0), min(i + 1, n - 1)
        tx, ty = _unit(pts[j1][0] - pts[j0][0], pts[j1][1] - pts[j0][1])
        nx, ny = -ty, tx
        left.append((p[0] + nx * w / 2, p[1] + ny * w / 2))
        right.append((p[0] - nx * w / 2, p[1] - ny * w / 2))

    poly = list(left)
    if head == "end":
        p, prev = pts[-1], pts[-3] if n > 2 else pts[0]
        tx, ty = _unit(p[0] - prev[0], p[1] - prev[1])
        nx, ny = -ty, tx
        barb = w / 2 + w * head_barb
        poly.append((p[0] + nx * barb, p[1] + ny * barb))
        poly.append((p[0] + tx * w * head_len, p[1] + ty * w * head_len))
        poly.append((p[0] - nx * barb, p[1] - ny * barb))
    poly += list(reversed(right))
    if head == "start":
        p, nxt = pts[0], pts[2] if n > 2 else pts[-1]
        tx, ty = _unit(p[0] - nxt[0], p[1] - nxt[1])
        nx, ny = -ty, tx
        barb = w / 2 + w * head_barb
        poly.append((p[0] - nx * barb, p[1] - ny * barb))
        poly.append((p[0] + tx * w * head_len, p[1] + ty * w * head_len))
        poly.append((p[0] + nx * barb, p[1] + ny * barb))

    m = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(m)
    d.polygon(poly, fill=255)

    # 圆头：起点（或未加箭头的那一端）补一个圆
    cap_end = pts[-1] if head != "end" else None
    cap_start = pts[0] if head != "start" else None
    for c in (cap_end, cap_start) if cap_start else (cap_end,):
        if c:
            d.ellipse([c[0] - w / 2, c[1] - w / 2, c[0] + w / 2, c[1] + w / 2], fill=255)
    return m


def tint(mask, top, bot):
    grad = Image.new("RGB", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        gd.line([(0, y), (S, y)], fill=lerp(top, bot, y / (S - 1)))
    out = grad.convert("RGBA")
    out.putalpha(mask)
    return out


def compose(bg, layers):
    union = Image.new("L", (S, S), 0)
    for m, _, _ in layers:
        union = Image.composite(m, union, m)

    sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    black = Image.new("RGBA", (S, S), (34, 52, 82, 255))
    black.putalpha(union.point(lambda v: v * 82 // 255))
    sh.paste(black, (0, int(sc(11))), black)
    sh = sh.filter(ImageFilter.GaussianBlur(sc(15)))
    img = Image.alpha_composite(bg, sh)

    for m, top, bot in layers:
        img = Image.alpha_composite(img, tint(m, top, bot))
    return img


# ---------------------------------------------------------------- 方案

C = S / 2
R = sc(238)      # 中心线半径
W = sc(74)       # 环带宽度


def icon_a():
    """双弧循环：两段 150° 弧，各自带箭头，180° 旋转对称。"""
    bg = background(S)
    blue = ribbon(sample_arc(C, C, R, 196, 344), W, head="end")
    orange = ribbon(sample_arc(C, C, R, 16, 164), W, head="end")
    return compose(bg, [(blue, BLUE_TOP, BLUE_BOT), (orange, ORANGE_TOP, ORANGE_BOT)])


def icon_b():
    """单环箭头：蓝色环几乎闭合，缺口处一个橙色箭头接上。"""
    bg = background(S)
    blue = ribbon(sample_arc(C, C, R, 250, 466), W)
    orange = ribbon(sample_arc(C, C, R, 466, 508), W, head="end")
    return compose(bg, [(blue, BLUE_TOP, BLUE_BOT), (orange, ORANGE_TOP, ORANGE_BOT)])


def icon_c():
    """品牌字形 C：粗环开口向右，下端收成橙色箭头。"""
    bg = background(S)
    r2, w2 = sc(232), sc(86)
    blue = ribbon(sample_arc(C, C, r2, -62, 224), w2)
    orange = ribbon(sample_arc(C, C, r2, 224, 306), w2, head="end")
    return compose(bg, [(blue, BLUE_TOP, BLUE_BOT), (orange, ORANGE_TOP, ORANGE_BOT)])


def icon_d():
    """方环循环：沿 squircle 走的闭环，两处箭头，贴合图标外形。"""
    bg = background(S)
    a = sc(268)
    w2 = sc(74)
    blue = ribbon(sample_squircle_path(C, C, a, 0.03, 0.455, expo=3.1), w2, head="end")
    orange = ribbon(sample_squircle_path(C, C, a, 0.535, 0.96, expo=3.1), w2, head="end")
    return compose(bg, [(blue, BLUE_TOP, BLUE_BOT), (orange, ORANGE_TOP, ORANGE_BOT)])


def icon_e():
    """双向箭头 ⇄：上蓝右行、下橙左行，形似「来回转换」。"""
    bg = background(S)
    w2 = sc(104)
    gap = sc(60)
    x0, x1 = C - sc(232), C + sc(232)

    up = ribbon([(x0, C - gap), (x1, C - gap)], w2, head="end")
    down = ribbon([(x1, C + gap), (x0, C + gap)], w2, head="end")
    return compose(bg, [(up, BLUE_TOP, BLUE_BOT), (down, ORANGE_TOP, ORANGE_BOT)])


def icon_f():
    """折纸双片（定稿）：橙色后片错位露出，蓝色前片右上折起一角。"""
    bg = background(S)
    wd, ht = sc(400), sc(468)

    def blank():
        m = Image.new("L", (S, S), 0)
        return m, ImageDraw.Draw(m)

    def rot(mask, angle):
        return mask.rotate(angle, resample=Image.BICUBIC, center=(C, C))

    # 前片：蓝色，右上角切出折角
    k = sc(126)
    fx, fy = C - wd / 2 + sc(14), C - ht / 2 - sc(2)
    front, df = blank()
    df.rounded_rectangle([fx, fy, fx + wd, fy + ht], radius=sc(44), fill=255)
    df.polygon(
        [(fx + wd - k, fy - 4), (fx + wd + 4, fy + k), (fx + wd - k, fy + k)], fill=0
    )
    front = rot(front, 5)

    # 后片：橙色，向左下错位；前片位置挖空形成叠放缝隙
    back, db = blank()
    bx, by = C - wd / 2 - sc(26), C - ht / 2 - sc(12)
    db.rounded_rectangle([bx, by, bx + wd, by + ht], radius=sc(44), fill=255)
    back = rot(back, -7)
    seam = front.filter(ImageFilter.GaussianBlur(sc(7))).point(lambda v: 255 if v > 40 else 0)
    back = Image.composite(Image.new("L", (S, S), 0), back, seam)

    # 折角：橙色三角填进切口，沿对角线内缩形成细缝
    gap = sc(6)
    flap, dfl = blank()
    dfl.polygon(
        [
            (fx + wd - k + gap, fy + gap),
            (fx + wd - gap, fy + k - gap),
            (fx + wd - k + gap, fy + k - gap),
        ],
        fill=255,
    )
    flap = rot(flap, 5)
    outside = front.point(lambda v: 255 - v)
    flap = Image.composite(flap, Image.new("L", (S, S), 0), outside)

    return compose(
        bg,
        [
            (back, ORANGE_TOP, ORANGE_BOT),
            (front, BLUE_TOP, BLUE_BOT),
            (flap, (250, 143, 74), (236, 96, 26)),
        ],
    )


def icon_f_small():
    """小尺寸变体（16~48px）：去掉后片、放大折角，保证一眼可辨。"""
    bg = background(S)
    wd, ht = sc(440), sc(508)
    k = sc(176)

    fx, fy = C - wd / 2, C - ht / 2
    page = Image.new("L", (S, S), 0)
    d = ImageDraw.Draw(page)
    d.rounded_rectangle([fx, fy, fx + wd, fy + ht], radius=sc(52), fill=255)
    d.polygon([(fx + wd - k, fy - 4), (fx + wd + 4, fy + k), (fx + wd - k, fy + k)], fill=0)

    gap = sc(14)
    flap = Image.new("L", (S, S), 0)
    ImageDraw.Draw(flap).polygon(
        [
            (fx + wd - k + gap, fy + gap),
            (fx + wd - gap, fy + k - gap),
            (fx + wd - k + gap, fy + k - gap),
        ],
        fill=255,
    )
    outside = page.point(lambda v: 255 - v)
    flap = Image.composite(flap, Image.new("L", (S, S), 0), outside)

    return compose(
        bg,
        [
            (page, BLUE_TOP, BLUE_BOT),
            (flap, (250, 143, 74), (236, 96, 26)),
        ],
    )


CANDIDATES = [
    ("A", "双弧循环", icon_a),
    ("B", "单环箭头", icon_b),
    ("C", "字形 C", icon_c),
    ("D", "方环循环", icon_d),
    ("E", "双向箭头", icon_e),
    ("F", "折纸双片", icon_f),
    ("F", "折纸·小尺寸", icon_f_small),
]


def contact_sheet(items):
    from PIL import ImageFont

    cell, pad = 300, 24
    label_h, small_h = 40, 62
    cols, rows = 3, 2
    w = cols * cell + pad * (cols + 1)
    h = rows * (cell + label_h + small_h) + pad * (rows + 1)
    sheet = Image.new("RGBA", (w, h), (246, 247, 249, 255))

    font = None
    for path in (
        "C:/Windows/Fonts/msyh.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ):
        if os.path.exists(path):
            try:
                font = ImageFont.truetype(path, 21)
                break
            except OSError:
                continue

    d = ImageDraw.Draw(sheet)
    for i, (key, name, im) in enumerate(items):
        r, c = divmod(i, cols)
        x = pad + c * (cell + pad)
        y = pad + r * (cell + label_h + small_h + pad)
        sheet.alpha_composite(im.resize((cell, cell), Image.LANCZOS), (x, y))
        label = f"{key}   {name}"
        if font:
            d.text((x + 3, y + cell + 8), label, font=font, fill=(64, 70, 84))
        # 小尺寸验证：32 / 48 px
        for k, s in enumerate((32, 48)):
            sheet.alpha_composite(
                im.resize((s, s), Image.LANCZOS), (x + 3 + k * 56, y + cell + label_h - 4)
            )
    return sheet


def main():
    os.makedirs(OUT, exist_ok=True)
    items = []
    for key, name, fn in CANDIDATES:
        im = fn()
        im.resize((512, 512), Image.LANCZOS).save(os.path.join(OUT, f"{key}-{name}.png"))
        im.resize((32, 32), Image.LANCZOS).save(os.path.join(OUT, f"{key}-32.png"))
        items.append((key, name, im))
    contact_sheet(items).convert("RGB").save(os.path.join(OUT, "contact-sheet.png"))
    print("ok ->", OUT)


if __name__ == "__main__":
    main()
