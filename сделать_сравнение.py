#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Сборка красивых сравнений «до/после» из папки, созданной командой demo.

Использование:
    python3 сделать_сравнение.py                 # папки по умолчанию: my_results
    python3 сделать_сравнение.py --dir my_results --out my_results/сравнения

Для каждой картинки <имя>_0_noisy.png собирается горизонтальная панель:
оригинал (если можно найти), зашумлённое, гауссов, билатеральный, АКСФ.
Требуется библиотека Pillow:  pip install pillow
"""
import argparse
import os
import re

from PIL import Image, ImageDraw, ImageFont

FONT_CANDIDATES = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "C:/Windows/Fonts/arial.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
]


def find_font(size):
    for p in FONT_CANDIDATES:
        if os.path.exists(p):
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()


def label(img, text, font):
    band_h = 26
    out = Image.new("RGB", (img.width, img.height + band_h), "white")
    out.paste(img, (0, 0))
    d = ImageDraw.Draw(out)
    w = d.textlength(text, font=font)
    d.text(((out.width - w) / 2, img.height + 4), text, font=font, fill=(20, 20, 20))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dir", default="my_results", help="папка с результатами demo")
    ap.add_argument("--out", default=None, help="куда сохранять сравнения")
    ap.add_argument("--max-width", type=int, default=1600, help="ширина итоговой картинки")
    args = ap.parse_args()

    out_dir = args.out or os.path.join(args.dir, "сравнения")
    os.makedirs(out_dir, exist_ok=True)
    font = find_font(17)

    # ищем зашумлённые версии: <имя>_0_noisy.png
    names = sorted(
        m.group(1) for f in os.listdir(args.dir) if (m := re.match(r"(.+)_0_noisy\.png$", f))
    )
    if not names:
        print(f"В папке '{args.dir}' нет файлов вида <имя>_0_noisy.png.")
        print("Сначала запустите:  ./target/release/smoothing_project demo -d my_images -o my_results --rgb")
        return

    for name in names:
        tiles = []
        # оригинал: поищем в my_images или в папке с исходниками
        for extra_dir in ("my_images", "."):
            for ext in (".jpg", ".jpeg", ".png", ".bmp"):
                cand = os.path.join(extra_dir, name + ext)
                if os.path.exists(cand) and extra_dir != args.dir:
                    im = Image.open(cand).convert("RGB")
                    tiles.append(label(im, "Оригинал", font))
                    break
            else:
                continue
            break

        variants = [
            ("_0_noisy.png", "Шум"),
            ("_1_gauss.png", "Гауссов"),
            ("_1_median5.png", "Медианный"),
            ("_1_bilateral.png", "Билатеральный"),
            ("_1_acsf.png", "АКСФ (наш метод)"),
        ]
        for suffix, lab in variants:
            p = os.path.join(args.dir, name + suffix)
            if os.path.exists(p):
                im = Image.open(p).convert("RGB")
                tiles.append(label(im, lab, font))

        if not tiles:
            continue

        gap = 10
        width = sum(t.width for t in tiles) + gap * (len(tiles) - 1)
        # при необходимости уменьшаем
        if width > args.max_width:
            scale = args.max_width / width
            tiles = [t.resize((max(1, int(t.width * scale)), max(1, int(t.height * scale)))) for t in tiles]
            width = sum(t.width for t in tiles) + gap * (len(tiles) - 1)
        height = max(t.height for t in tiles)
        sheet = Image.new("RGB", (width, height), "white")
        x = 0
        for t in tiles:
            sheet.paste(t, (x, 0))
            x += t.width + gap
        dst = os.path.join(out_dir, f"сравнение_{name}.png")
        sheet.save(dst)
        print("сохранено:", dst)

    print(f"\nГотово! Картинки-сравнения в папке: {out_dir}")


if __name__ == "__main__":
    main()
