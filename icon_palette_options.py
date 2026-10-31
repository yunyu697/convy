"""配色备选：形状（单页 + 折角）不变，只换色板。

评判标准：
1. 主色（页面）与点缀色（折角）的 L* 差 ≥ 25，否则 16px 下折角会消失
2. 两色不要接近互补（色相差 150~210° 且都高饱和时最刺眼）
3. 在浅色和深色任务栏上都要成立 —— 深色底用 32px 贴片验证
"""

import colorsys
import importlib.util
import os

from PIL import Image, ImageDraw, ImageFilter, ImageFont

spec = importlib.util.spec_from_file_location("mk", os.path.join(os.path.dirname(__file__), "make_icon.py"))
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)
S, C, sc = mk.S, mk.S / 2, mk.sc


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i : i + 2], 16) for i in (0, 2, 4))


def lstar(c):
    def lin(v):
        v /= 255
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    y = 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    return 116 * (y ** (1 / 3)) - 16 if y > 0.008856 else 903.3 * y


def hue(c):
    h, s, v = colorsys.rgb_to_hsv(*[x / 255 for x in c])
    return h * 360, s * 100


def build(page_top, page_bot, flap_top, flap_bot, wd=404, ht=470, k=140, rot=4):
    W, H, K = sc(wd), sc(ht), sc(k)
    fx, fy = C - W / 2 + sc(10), C - H / 2 - sc(2)
    page = mk._page(W, H, fx, fy, sc(42), K).rotate(rot, resample=Image.BICUBIC, center=(C, C))
    flap = mk._flap(W, H, fx, fy, K, 0).rotate(rot, resample=Image.BICUBIC, center=(C, C))
    return mk.compose([(page, page_top, page_bot), (flap, flap_top, flap_bot)])


PALETTES = [
    ("01 靛蓝单色", "#3576E8", "#1740A8", "#BFDBFE", "#A8CBFA", "同色系双阶，最稳，工具感"),
    ("02 靛蓝→紫", "#4F63F5", "#6B34E8", "#DCD2FF", "#CDBEFF", "同族渐变，科技感"),
    ("03 石墨+琥珀", "#3B4553", "#1E2430", "#F5A93B", "#EE8B1F", "中性主色 + 暖点缀，最耐看"),
    ("04 墨绿+薄荷", "#0F766E", "#0A4F4A", "#5EEAD4", "#34D3BC", "冷色非互补，安静"),
    ("05 深紫+珊瑚", "#6D28D9", "#4813A3", "#FB8FA0", "#F76F86", "个性强，偏潮流"),
    ("06 赭橙单色", "#E0701F", "#B94E12", "#FFC98F", "#FFB871", "全暖色，单色不打架"),
]


def main():
    cell, pad = 300, 22
    cols, rows = 3, 2
    w = cols * cell + pad * (cols + 1)
    h = rows * (cell + 128) + pad * (rows + 1)
    sheet = Image.new("RGBA", (w, h), (246, 247, 249, 255))
    d = ImageDraw.Draw(sheet)
    label_font = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 17)
    small_font = ImageFont.truetype("C:/Windows/Fonts/msyh.ttc", 13)

    for i, (name, pt, pb, ft, fb, note) in enumerate(PALETTES):
        im = build(rgb(pt), rgb(pb), rgb(ft), rgb(fb))
        r, c = divmod(i, cols)
        x = pad + c * (cell + pad)
        y = pad + r * (cell + 96 + pad)

        sheet.alpha_composite(im.resize((cell - 30, cell - 30), Image.LANCZOS), (x + 15, y))

        d.text((x + 14, y + cell - 6), name, font=label_font, fill=(52, 58, 72))
        diff = abs(lstar(rgb(pt)) - lstar(rgb(ft)))
        d.text((x + 14, y + cell + 18), f"L* 差 {diff:.0f} · {note}", font=small_font, fill=(120, 126, 140))

        # 色卡
        for j, hx in enumerate((pt, ft)):
            d.rounded_rectangle(
                [x + 14 + j * 32, y + cell + 46, x + 14 + j * 32 + 26, y + cell + 74],
                radius=6,
                fill=rgb(hx) + (255,),
            )

        # 深色任务栏验证：32 / 16px 贴在深底上
        dark = Image.new("RGBA", (96, 44), (32, 34, 40, 255))
        dark.alpha_composite(im.resize((32, 32), Image.LANCZOS), (8, 6))
        dark.alpha_composite(im.resize((16, 16), Image.LANCZOS), (52, 14))
        sheet.alpha_composite(dark, (x + cell - 116, y + cell + 44))

    sheet.save(os.path.join(os.path.dirname(__file__), "icon-candidates", "palettes.png"))
    print("-> icon-candidates/palettes.png")


if __name__ == "__main__":
    main()
