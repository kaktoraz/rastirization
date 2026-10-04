#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Визуальные сравнения: фрагменты изображений (оригинал / зашумлённое /
результаты фильтров) с подписями и метриками, а также иллюстрация
карты структурной активности АКСФ.
"""
import os, csv
import numpy as np
from PIL import Image, ImageDraw, ImageFont

BASE = os.path.dirname(os.path.abspath(__file__))
IMG = os.path.join(BASE, "results", "images")
PLOTS = os.path.join(BASE, "results", "plots")
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_B = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"

# области для сравнения (x0, y0, x1, y1) — выбирались по наличию мелких деталей
CROPS = {
    "baboon": (60, 30, 220, 190),     # глаз и текстура меха — мелкие детали
    "board": (170, 120, 330, 280),    # микросхемы и дорожки платы
    "fruits": (60, 200, 220, 360),    # мякоть цитруса: мелкая зернистость
}

METRICS = {}
with open(os.path.join(BASE, "results", "tables", "results.csv")) as f:
    for r in csv.DictReader(f):
        if float(r["sigma_noise"]) == 25.0:
            METRICS[(r["image"], r["method"])] = (float(r["psnr"]), float(r["ssim"]))

def crop(path, box):
    return Image.open(path).convert("L").crop(box)

def label(img, text, sub=None, scale=1):
    """Подпись под фрагментом."""
    f1 = ImageFont.truetype(FONT_B, 13 * scale)
    f2 = ImageFont.truetype(FONT, 11 * scale)
    pad = 6 * scale
    band_h = (20 + (16 if sub else 0)) * scale
    out = Image.new("L", (img.width, img.height + band_h), 255)
    out.paste(img, (0, 0))
    d = ImageDraw.Draw(out)
    tw = d.textlength(text, font=f1)
    d.text(((out.width - tw) / 2, img.height + pad - 5 * scale), text, font=f1, fill=0)
    if sub:
        tw2 = d.textlength(sub, font=f2)
        d.text(((out.width - tw2) / 2, img.height + pad + 13 * scale), sub, font=f2, fill=60)
    return out

def make_row(image_name, scale=1):
    box = CROPS[image_name]
    tiles = []
    clean = crop(f"{IMG}/{image_name}_clean.png", box)
    noisy = crop(f"{IMG}/{image_name}_noisy.png", box)
    gauss = crop(f"{IMG}/{image_name}_gauss.png", box)
    bil = crop(f"{IMG}/{image_name}_bilateral.png", box)
    acsf = crop(f"{IMG}/{image_name}_acsf.png", box)
    pn = METRICS.get((image_name, "raw"), (0, 0))
    pg = METRICS.get((image_name, "gauss"), (0, 0))
    pb = METRICS.get((image_name, "bilateral"), (0, 0))
    pa = METRICS.get((image_name, "acsf"), (0, 0))
    tiles.append(label(clean, "Оригинал", "(эталон)", scale))
    tiles.append(label(noisy, "Шум σ=25", f"PSNR {pn[0]:.1f} дБ", scale))
    tiles.append(label(gauss, "Гауссов", f"PSNR {pg[0]:.1f} дБ", scale))
    tiles.append(label(bil, "Билатеральн.", f"PSNR {pb[0]:.1f} дБ", scale))
    tiles.append(label(acsf, "АКСФ (наш)", f"PSNR {pa[0]:.1f} дБ", scale))
    gap = 10 * scale
    W = sum(t.width for t in tiles) + gap * (len(tiles) - 1)
    H = max(t.height for t in tiles)
    sheet = Image.new("L", (W, H), 255)
    x = 0
    for t in tiles:
        sheet.paste(t, (x, 0))
        x += t.width + gap
    return sheet

for name in CROPS:
    sheet = make_row(name)
    sheet.save(os.path.join(PLOTS, f"fig_visual_{name}.png"))
    print(f"fig_visual_{name}.png", sheet.size)

# --- отдельно: зашумлённое / карта активности / результат АКСФ (board)
box = CROPS["board"]
noisy = crop(f"{IMG}/board_noisy.png", box)
for src, dst in [(f"{IMG}/board_activity.png", "activity")]:
    act = crop(src, box)
    # нормировка контраста карты для наглядности
    a = np.asarray(act, dtype=np.float32)
    lo, hi = np.percentile(a, 1), np.percentile(a, 99)
    a = np.clip((a - lo) / max(hi - lo, 1e-6) * 255, 0, 255).astype(np.uint8)
    act = Image.fromarray(a)
acsf = crop(f"{IMG}/board_acsf.png", box)
tiles = [label(noisy, "Зашумлённое", "σ=25", 1),
         label(act, "Карта активности A(p)", "ярче = сильнее структура", 1),
         label(acsf, "Результат АКСФ", "адаптация к A(p)", 1)]
gap = 12
W = sum(t.width for t in tiles) + gap * (len(tiles) - 1)
H = max(t.height for t in tiles)
sheet = Image.new("L", (W, H), 255)
x = 0
for t in tiles:
    sheet.paste(t, (x, 0))
    x += t.width + gap
sheet.save(os.path.join(PLOTS, "fig_activity_map.png"))
print("fig_activity_map.png", sheet.size)
