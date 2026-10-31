"""整套配色方案（底色 + 页面 + 折角）备选。

之前只改页面和折角的颜色，底色一直是浅灰白，等于把图标锁死在「白纸文档」这一种
气质上。底色一旦变成深色或饱和色，标记就要反过来用浅色 —— 这才是应用图标常见的
「徽章」型做法，也更容易做出质感。

六套方案覆盖：浅底 / 深底 / 品牌色底 / 渐变底 / 暖砂底。
每套都给出浅色与深色任务栏上的 32px 实际效果。
"""

import importlib.util
import os

from PIL import Image, ImageDraw, ImageFilter, ImageFont

spec = importlib.util.spec_from_file_location(
    "mk", os.path.join(os.path.dirname(__file__), "make_icon.py")
)
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)
S, C, sc = mk.S, mk.S / 2, mk.sc


def hexc(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def bg_layer(bg, size=S):
    """底色：light=原浅灰白渐变 / solid / grad"""
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    kind = bg[0]
    if kind == "light":
        for y in range(size):
            d.line([(0, y), (size, y)], fill=mk.lerp(mk.BG_TOP, mk.BG_BOT, y / (size - 1)))
    elif kind == "solid":
        d.rectangle([0, 0, size, size], fill=bg[1])
    else:  # grad
        for y in range(size):
            d.line([(0, y), (size, y)], fill=mk.lerp(bg[1], bg[2], y / (size - 1)))
    img = img.convert("RGBA")

    if kind == "light":
        gloss = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        ImageDraw.Draw(gloss).ellipse(
            [-size * 0.3, -size * 0.8, size * 1.3, size * 0.34], fill=(255, 255, 255, 52)
        )
        gloss = gloss.filter(ImageFilter.GaussianBlur(size * 0.035))
        img = Image.alpha_composite(img, gloss)

    # 内描边：浅底用白色，深底用淡淡的高光
    edge = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    pts = mk.squircle_points(size / 2, size / 2, size / 2 - size * 0.007, mk.N)
    stroke = (255, 255, 255, 190) if kind == "light" else (255, 255, 255, 46)
    ImageDraw.Draw(edge).line(
        pts + [pts[0]], fill=stroke, width=max(1, int(size * 0.0045))
    )
    img = Image.alpha_composite(img, edge)
    img.putalpha(mk.squircle_mask(size))
    return img


def compose(bg, layers, shadow=80):
    base = bg_layer(bg)
    union = Image.new("L", (S, S), 0)
    for m, _, _ in layers:
        union = Image.composite(m, union, m)

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

    if shadow:
        sh = Image.new("RGBA", (S, S), (0, 0, 0, 0))
        black = Image.new("RGBA", (S, S), (20, 26, 38, 255))
        black.putalpha(union.point(lambda v: v * shadow // 255))
        sh.paste(black, (0, int(sc(10))), black)
        sh = sh.filter(ImageFilter.GaussianBlur(sc(14)))
        base = Image.alpha_composite(base, sh)

    for m, top, bot in layers:
        base = Image.alpha_composite(base, mk.tint(m, top, bot))
    return base


def build(scheme, small=False):
    bg, page, fold, shadow = scheme["bg"], scheme["page"], scheme["fold"], scheme["shadow"]
    if small:
        wd, ht, k, rot, fx_off = sc(450), sc(520), sc(168), 0, 0
    else:
        wd, ht, k, rot, fx_off = sc(404), sc(470), sc(140), 4, sc(10)
    fx, fy = C - wd / 2 + fx_off, C - ht / 2 - sc(2) if not small else C - ht / 2
    page_m = mk._page(wd, ht, fx, fy, sc(52 if small else 42), k)
    flap_m = mk._flap(wd, ht, fx, fy, k, 0)
    if rot:
        page_m = page_m.rotate(rot, resample=Image.BICUBIC, center=(C, C))
        flap_m = flap_m.rotate(rot, resample=Image.BICUBIC, center=(C, C))
    return compose(bg, [(page_m, *page), (flap_m, *fold)], shadow)


SCHEMES = [
    {
        "id": "T1",
        "name": "浅底 · 靛蓝页",
        "note": "延续现在的路线，最不冒险",
        "bg": ("light",),
        "page": (hexc("#3576E8"), hexc("#1740A8")),
        "fold": (hexc("#BFDBFE"), hexc("#A8CBFA")),
        "shadow": 80,
    },
    {
        "id": "T2",
        "name": "深底 · 奶油页",
        "note": "深底 + 暖白页，折角琥珀，像实体卡片",
        "bg": ("grad", hexc("#2A2F3A"), hexc("#171B22")),
        "page": (hexc("#F7F3EA"), hexc("#E6DECE")),
        "fold": (hexc("#F5A93B"), hexc("#E8871B")),
        "shadow": 0,
    },
    {
        "id": "T3",
        "name": "蓝底 · 白页",
        "note": "品牌色铺底，白页反白，最像成品 App",
        "bg": ("grad", hexc("#3E7BF0"), hexc("#1B4DB8")),
        "page": (hexc("#FFFFFF"), hexc("#EDF2FB")),
        "fold": (hexc("#A9C6FF"), hexc("#8FB2F5")),
        "shadow": 46,
    },
    {
        "id": "T4",
        "name": "紫底 · 白页",
        "note": "渐变紫底，白页，偏现代/创意工具",
        "bg": ("grad", hexc("#6D5CF5"), hexc("#4426C9")),
        "page": (hexc("#FFFFFF"), hexc("#F1EEFF")),
        "fold": (hexc("#D9CFFF"), hexc("#C3B4FB")),
        "shadow": 46,
    },
    {
        "id": "T5",
        "name": "暖砂底 · 石墨页",
        "note": "沙色底 + 深灰页 + 琥珀折角，最耐看",
        "bg": ("grad", hexc("#EAE2D5"), hexc("#D9CCB8")),
        "page": (hexc("#3F4956"), hexc("#232A35")),
        "fold": (hexc("#F5A93B"), hexc("#E8871B")),
        "shadow": 60,
    },
    {
        "id": "T6",
        "name": "墨绿底 · 米白页",
        "note": "深绿底 + 米白页 + 薄荷折角",
        "bg": ("grad", hexc("#13786F"), hexc("#0A4141")),
        "page": (hexc("#F6F3EC"), hexc("#E7E2D6")),
        "fold": (hexc("#5EEAD4"), hexc("#34D3BC")),
        "shadow": 0,
    },
]
def main():
    cell, pad = 300, 22
    cols, rows = 3, 2
    w = cols * cell + pad * (cols + 1)
    h = rows * (cell + 132) + pad * (rows + 1)
    sheet = Image.new("RGBA", (w, h), (243, 244, 246, 255))
    d = ImageDraw.Draw(sheet)
    f1 = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 17)
    f2 = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 12)

    for i, s in enumerate(SCHEMES):
        im = build(s)
        small = build(s, small=True)
        r, c = divmod(i, cols)
        x = pad + c * (cell + pad)
        y = pad + r * (cell + 132 + pad)

        sheet.alpha_composite(im.resize((cell - 34, cell - 34), Image.LANCZOS), (x + 17, y))
        d.text((x + 16, y + cell - 8), f"{s['id']} {s['name']}", font=f1, fill=(48, 54, 68))
        d.text((x + 16, y + cell + 15), s["note"], font=f2, fill=(122, 128, 142))

        # 浅底 / 深底任务栏各贴 32 + 16
        for j, bgh in enumerate(((236, 238, 242, 255), (32, 34, 40, 255))):
            strip = Image.new("RGBA", (92, 46), bgh)
            strip.alpha_composite(small.resize((32, 32), Image.LANCZOS), (6, 7))
            strip.alpha_composite(small.resize((16, 16), Image.LANCZOS), (48, 15))
            sheet.alpha_composite(strip, (x + 16 + j * 100, y + cell + 40))

        # 色卡：底 / 页 / 折角
        bg_chip = s["bg"][1] if len(s["bg"]) > 1 else mk.BG_BOT
        chips = [bg_chip, s["page"][0], s["fold"][0]]
        for k, col in enumerate(chips):
            d.rounded_rectangle(
                [x + cell - 118 + k * 32, y + cell + 44, x + cell - 96 + k * 32, y + cell + 70],
                radius=6,
                fill=col + (255,),
            )

    sheet.save(os.path.join(os.path.dirname(__file__), "icon-candidates", "themes.png"))
    print("-> icon-candidates/themes.png")


if __name__ == "__main__":
    main()
