#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Собрать воспроизводимые визуальные панели из результата ``bench``.

Картинки в ``results/images`` считаются промежуточными и игнорируются Git.
Скрипт сохраняет только готовые графические иллюстрации в ``results/plots``.
"""
from __future__ import annotations

import argparse
import csv
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, ImageOps

BASE = Path(__file__).resolve().parent
FONT_CANDIDATES = (
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/Library/Fonts/Arial Unicode.ttf",
    "C:/Windows/Fonts/arial.ttf",
)
CROPS = {
    "baboon": (60, 30, 220, 190),
    "board": (170, 120, 330, 280),
    "fruits": (60, 200, 220, 360),
    "astronaut": (170, 80, 330, 240),
    "grace_hopper": (170, 100, 330, 260),
    "gradient_circles": (150, 150, 310, 310),
}


def font(size: int, bold: bool = False):
    candidates = list(FONT_CANDIDATES)
    if bold:
        candidates.insert(0, "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
    for candidate in candidates:
        if Path(candidate).exists():
            return ImageFont.truetype(candidate, size)
    return ImageFont.load_default()


def read_metrics(path: Path, sigma: float) -> dict[tuple[str, str], tuple[float, float]]:
    metrics: dict[tuple[str, str], tuple[float, float]] = {}
    with path.open(encoding="utf-8", newline="") as handle:
        for row in csv.DictReader(handle):
            if abs(float(row["sigma_noise"]) - sigma) < 1e-9:
                metrics[(row["image"], row["method"])] = (float(row["psnr"]), float(row["ssim"]))
    return metrics


def crop(path: Path, box: tuple[int, int, int, int]) -> Image.Image:
    return Image.open(path).convert("L").crop(box)


def label(image: Image.Image, title: str, subtitle: str | None) -> Image.Image:
    title_font, sub_font = font(13, bold=True), font(10)
    band = 40 if subtitle else 23
    out = Image.new("L", (image.width, image.height + band), 255)
    out.paste(image, (0, 0))
    drawing = ImageDraw.Draw(out)
    width = drawing.textlength(title, font=title_font)
    drawing.text(((out.width - width) / 2, image.height + 3), title, font=title_font, fill=0)
    if subtitle:
        width = drawing.textlength(subtitle, font=sub_font)
        drawing.text(((out.width - width) / 2, image.height + 20), subtitle, font=sub_font, fill=70)
    return out


def compose(tiles: list[Image.Image], gap: int = 9) -> Image.Image:
    width = sum(tile.width for tile in tiles) + gap * (len(tiles) - 1)
    height = max(tile.height for tile in tiles)
    result = Image.new("L", (width, height), 255)
    x = 0
    for tile in tiles:
        result.paste(tile, (x, 0))
        x += tile.width + gap
    return result


def make_comparison(name: str, image_dir: Path, metrics: dict, plots: Path) -> None:
    required = ["clean", "noisy", "gauss", "perona_malik", "bilateral", "acsf"]
    if not all((image_dir / f"{name}_{suffix}.png").exists() for suffix in required):
        print(f"пропуск {name}: неполный набор промежуточных изображений")
        return
    box = CROPS.get(name)
    if box is None:
        source = Image.open(image_dir / f"{name}_clean.png")
        side = min(source.width, source.height, 160)
        box = ((source.width - side) // 2, (source.height - side) // 2, (source.width + side) // 2, (source.height + side) // 2)
    entries = [
        ("clean", "Эталон", "исходное"),
        ("noisy", "Шум", metric_text(metrics, name, "raw")),
        ("gauss", "Гауссов", metric_text(metrics, name, "gauss")),
        ("perona_malik", "Перона—Малик", metric_text(metrics, name, "perona_malik")),
        ("bilateral", "Билатеральный", metric_text(metrics, name, "bilateral")),
        ("acsf", "АКСФ", metric_text(metrics, name, "acsf")),
    ]
    tiles = [label(crop(image_dir / f"{name}_{suffix}.png", box), title, sub) for suffix, title, sub in entries]
    compose(tiles).save(plots / f"fig_visual_{name}.png")
    print(f"fig_visual_{name}.png")


def metric_text(metrics: dict, name: str, method: str) -> str:
    values = metrics.get((name, method))
    return f"PSNR {values[0]:.1f} дБ" if values else ""


def activity_panel(name: str, image_dir: Path, plots: Path, sigma: float) -> None:
    paths = {suffix: image_dir / f"{name}_{suffix}.png" for suffix in ("noisy", "activity", "acsf")}
    if not all(path.exists() for path in paths.values()):
        return
    box = CROPS.get(name, (0, 0, 160, 160))
    activity = crop(paths["activity"], box)
    activity = ImageOps.autocontrast(activity, cutoff=1)
    tiles = [
        label(crop(paths["noisy"], box), "Зашумлённое", f"σ={sigma:g}"),
        label(activity, "Карта активности A(p)", "светлее = больше структура"),
        label(crop(paths["acsf"], box), "АКСФ", "адаптация к A(p)"),
    ]
    compose(tiles, gap=12).save(plots / "fig_activity_map.png")
    print("fig_activity_map.png")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default="results", help="корень benchmark-результатов")
    parser.add_argument("--sigma", type=float, default=20.0, help="уровень шума, сохранённый с --save-sigma")
    args = parser.parse_args()
    root = (BASE / args.root).resolve()
    image_dir, plots = root / "images", root / "plots"
    plots.mkdir(parents=True, exist_ok=True)
    metrics = read_metrics(root / "tables" / "results.csv", args.sigma)
    preferred = ["baboon", "board", "fruits"]
    names = [name for name in preferred if (image_dir / f"{name}_clean.png").exists()]
    if not names:
        names = sorted(path.name.removesuffix("_clean.png") for path in image_dir.glob("*_clean.png"))[:3]
    for name in names:
        make_comparison(name, image_dir, metrics, plots)
    activity_panel("board" if "board" in names else names[0], image_dir, plots, args.sigma)


if __name__ == "__main__":
    main()
