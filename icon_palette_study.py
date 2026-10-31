"""配色诊断与方案对比（不改正式资产，仅出对比图）。

结论先行：
1) 蓝(214°) 与 橙(21°) 相差 193°，接近正互补 —— 两个满饱和的互补色同框，
   边缘会产生「振动感」，这就是「突兀」的来源。
2) 橙有三个色调（纸背 #FF7328 / 折角 #EC601A / 渐变上端 #FFA35E），
   其中折角更暗更艳，看起来像「脏了一块」。
3) 橙分散在左侧竖条 + 底部横条 + 折角三处，面积接近主色，
   与蓝争夺主导权，没有 60/30/10 的层级。
4) 纸缝是纯底色（L* 98），紧挨 L* 48~85 的色块，形成硬边。

方案：
P1 当前（对照）
P2 降饱和 + 折角与纸背同色（结构不变，只收敛颜色）
P3 第二页改深蓝，橙只留折角（60/30/10 层级，推荐）
P4 P3 + 更浅的橙 + 暖白纸缝（最柔和）
"""

import importlib.util
import os

from PIL import Image, ImageDraw, ImageFilter

spec = importlib.util.spec_from_file_location("mk", os.path.join(os.path.dirname(__file__), "make_icon.py"))
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)

S, C, sc = mk.S, mk.S / 2, mk.sc


def build(back, front, flap, seam=None, wd=404, ht=470, k=145, off=(44, 26), rot=4):
    W, H, K = sc(wd), sc(ht), sc(k)
    fx, fy = C - W / 2 + sc(18), C - H / 2 - sc(2)
    front_m = mk._page(W, H, fx, fy, sc(42), K).rotate(rot, resample=Image.BICUBIC, center=(C, C))

    bx, by = fx - sc(off[0]), fy + sc(off[1])
    back_m = mk._page(W, H, bx, by, sc(42)).rotate(rot, resample=Image.BICUBIC, center=(C, C))
    cut = front_m.filter(ImageFilter.GaussianBlur(sc(6))).point(lambda v: 255 if v > 40 else 0)
    if seam:
        # 纸缝单独上色：把前片外扩一圈，作为前片与后片之间的那一层
        gap = cut.filter(ImageFilter.GaussianBlur(sc(5))).point(lambda v: 255 if v > 30 else 0)
        back_m = Image.composite(Image.new("L", (S, S), 0), back_m, gap)
        seam_m = Image.composite(gap, Image.new("L", (S, S), 0), Image.eval(front_m, lambda v: 255 - v))
    else:
        back_m = Image.composite(Image.new("L", (S, S), 0), back_m, cut)
        seam_m = None

    flap_m = mk._flap(W, H, fx, fy, K, 0).rotate(rot, resample=Image.BICUBIC, center=(C, C))

    layers = [(back_m, back[0], back[1])]
    if seam_m is not None:
        layers.append((seam_m, seam[0], seam[1]))
    layers += [(front_m, front[0], front[1]), (flap_m, flap[0], flap[1])]
    return mk.compose(layers)


CUR_BACK = ((255, 163, 94), (255, 115, 40))
CUR_FRONT = ((64, 145, 252), (33, 106, 234))
CUR_FLAP = ((250, 143, 74), (236, 96, 26))

VARIANTS = [
    ("P1 当前", dict(back=CUR_BACK, front=CUR_FRONT, flap=CUR_FLAP)),
    ("P2 降饱和 · 折角同色", dict(
        back=((255, 178, 122), (247, 132, 68)),
        front=((78, 140, 232), (44, 96, 200)),
        flap=((255, 178, 122), (247, 132, 68)),
    )),
    ("P3 深蓝第二页 · 橙为点缀", dict(
        back=((74, 108, 176), (43, 68, 128)),
        front=((64, 145, 252), (33, 106, 234)),
        flap=((255, 168, 108), (244, 124, 56)),
    )),
    ("P4 P3 + 暖白纸缝", dict(
        back=((84, 118, 186), (52, 78, 140)),
        front=((72, 149, 250), (38, 110, 232)),
        flap=((255, 176, 122), (247, 136, 66)),
        seam=((247, 244, 238), (238, 233, 224)),
    )),
]


def main():
    cell, pad = 250, 26
    w = len(VARIANTS) * cell + pad * (len(VARIANTS) + 1)
    h = cell + 130
    sheet = Image.new("RGBA", (w, h), (246, 247, 249, 255))
    d = ImageDraw.Draw(sheet)

    font = None
    from PIL import ImageFont
    for path in ("C:/Windows/Fonts/msyh.ttc", "/System/Library/Fonts/PingFang.ttc"):
        if os.path.exists(path):
            font = ImageFont.truetype(path, 17)
            break

    x = pad
    for name, kwargs in VARIANTS:
        im = build(**kwargs)
        sheet.alpha_composite(im.resize((cell - 20, cell - 20), Image.LANCZOS), (x + 10, 14))
        label = name
        if font:
            d.text((x + 12, cell + 14), label, font=font, fill=(60, 66, 80))
        else:
            d.text((x + 12, cell + 14), label, fill=(60, 66, 80))
        # 32px 小样
        sheet.alpha_composite(im.resize((32, 32), Image.LANCZOS), (x + 12, cell + 50))
        sheet.alpha_composite(im.resize((16, 16), Image.LANCZOS), (x + 52, cell + 58))
        x += cell

    sheet.save(os.path.join(os.path.dirname(__file__), "icon-candidates", "palette.png"))
    print("chart -> icon-candidates/palette.png")


if __name__ == "__main__":
    main()
