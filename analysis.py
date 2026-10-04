#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Автоматический анализ результатов эксперимента.

По умолчанию читает ``results/tables/results.csv``. Флаг ``--color``
переключает корень на ``results/color``. Скрипт создаёт графики, Markdown-
таблицы и ``summary.json``; отчёт и презентация читают именно эти сгенерированные
данные, поэтому числа не дублируются вручную.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
from pathlib import Path

import matplotlib
import numpy as np
from PIL import Image

matplotlib.use("Agg")
import matplotlib.pyplot as plt

ROOT_REPO = Path(__file__).resolve().parent

LABELS = {
    "raw": "Без обработки",
    "box5": "Среднее 5×5",
    "gauss": "Гауссов",
    "median5": "Медианный 5×5",
    "perona_malik": "Перона—Малик",
    "bilateral": "Билатеральный",
    "bilateral_oracle": "Билатеральный («оракул»)",
    "acsf_flat": "АКСФ без адаптации",
    "acsf": "АКСФ",
    "raw_ycbcr": "Без обработки (YCbCr)",
    "bilateral_ycbcr": "Билатеральный (YCbCr)",
    "acsf_ycbcr": "АКСФ (YCbCr)",
}
COLORS = {
    "raw": "#999999",
    "box5": "#c4a35a",
    "gauss": "#4c9f70",
    "median5": "#b07cc6",
    "perona_malik": "#8a5a44",
    "bilateral": "#3f6fb5",
    "bilateral_oracle": "#1f2d5c",
    "acsf_flat": "#e08a3c",
    "acsf": "#d62728",
    "raw_ycbcr": "#999999",
    "bilateral_ycbcr": "#3f6fb5",
    "acsf_ycbcr": "#d62728",
}
GRAY_ORDER = [
    "raw",
    "box5",
    "gauss",
    "median5",
    "perona_malik",
    "bilateral",
    "bilateral_oracle",
    "acsf_flat",
    "acsf",
]
COLOR_ORDER = ["raw_ycbcr", "bilateral_ycbcr", "acsf_ycbcr"]

plt.rcParams.update(
    {
        "font.family": "DejaVu Sans",
        "font.size": 10.5,
        "axes.grid": True,
        "grid.alpha": 0.28,
        "figure.dpi": 140,
    }
)


def read_rows(path: Path) -> list[dict]:
    with path.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if not rows:
        raise RuntimeError(f"Пустой CSV: {path}")
    for row in rows:
        row["sigma_noise"] = float(row["sigma_noise"])
        for key in ("psnr", "ssim", "epi", "time_ms"):
            row[key] = float(row[key])
    return rows


def mean(rows: list[dict], method: str, key: str, sigma: float | None = None, image: str | None = None) -> float:
    selected = [
        row
        for row in rows
        if row["method"] == method
        and (sigma is None or row["sigma_noise"] == sigma)
        and (image is None or row["image"] == image)
    ]
    if not selected:
        return float("nan")
    return float(np.mean([row[key] for row in selected]))


def fmt(value: float, digits: int = 2) -> str:
    return f"{value:.{digits}f}".replace(".", ",")


def _beta_continued_fraction(a: float, b: float, x: float) -> float:
    """Continued fraction for the regularised incomplete beta function.

    This small Numerical-Recipes-style implementation avoids a SciPy runtime
    dependency while retaining a two-sided Student t probability suitable for
    the paired significance table.
    """
    tiny = 1e-300
    c = 1.0
    d = 1.0 - (a + b) * x / (a + 1.0)
    d = tiny if abs(d) < tiny else d
    d = 1.0 / d
    h = d
    for m in range(1, 201):
        m2 = 2.0 * m
        numerator = m * (b - m) * x / ((a - 1.0 + m2) * (a + m2))
        d = 1.0 + numerator * d
        d = tiny if abs(d) < tiny else d
        c = 1.0 + numerator / c
        c = tiny if abs(c) < tiny else c
        d = 1.0 / d
        h *= d * c
        numerator = -(a + m) * (a + b + m) * x / ((a + m2) * (a + 1.0 + m2))
        d = 1.0 + numerator * d
        d = tiny if abs(d) < tiny else d
        c = 1.0 + numerator / c
        c = tiny if abs(c) < tiny else c
        d = 1.0 / d
        step = d * c
        h *= step
        if abs(step - 1.0) < 3e-14:
            break
    return h


def regularized_beta(a: float, b: float, x: float) -> float:
    """I_x(a,b), stable enough for the t-test tails used in this project."""
    if x <= 0.0:
        return 0.0
    if x >= 1.0:
        return 1.0
    front = math.exp(
        math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b)
        + a * math.log(x)
        + b * math.log1p(-x)
    )
    if x < (a + 1.0) / (a + b + 2.0):
        return front * _beta_continued_fraction(a, b, x) / a
    return 1.0 - front * _beta_continued_fraction(b, a, 1.0 - x) / b


def paired_t_test(deltas: np.ndarray) -> tuple[float, float]:
    """Return t and two-sided p for a one-sample paired-difference t-test."""
    n = len(deltas)
    if n < 2:
        raise ValueError("Для парного t-теста нужны как минимум две пары")
    standard_deviation = float(np.std(deltas, ddof=1))
    if standard_deviation < 1e-15:
        return (math.copysign(math.inf, float(deltas.mean())), 0.0)
    statistic = float(deltas.mean()) / (standard_deviation / math.sqrt(n))
    degrees = n - 1
    x = degrees / (degrees + statistic * statistic)
    return statistic, regularized_beta(degrees / 2.0, 0.5, x)


def grayscale_512_performance(rows: list[dict]) -> dict[str, float | int]:
    """Aggregate ACSF timing only for real 512×512 source scenes.

    Image dimensions are read from ``data/clean`` rather than copied into the
    prose, so the result remains tied to the experimental inputs and CSV.
    """
    square_names: set[str] = set()
    for name in {row["image"] for row in rows}:
        candidates = list((ROOT_REPO / "data" / "clean").glob(f"{name}.*"))
        if not candidates:
            continue
        with Image.open(candidates[0]) as source:
            if source.size == (512, 512):
                square_names.add(name)
    timings = [
        row["time_ms"]
        for row in rows
        if row["method"] == "acsf" and row["image"] in square_names
    ]
    return {
        "images": len(square_names),
        "measurements": len(timings),
        "acsf_time_ms": float(np.mean(timings)) if timings else float("nan"),
    }


def available(order: list[str], rows: list[dict]) -> list[str]:
    present = {row["method"] for row in rows}
    return [method for method in order if method in present]


def write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def line_plot(
    rows: list[dict],
    sigmas: list[float],
    methods: list[str],
    key: str,
    ylabel: str,
    title: str,
    destination: Path,
) -> None:
    fig, ax = plt.subplots(figsize=(7.3, 4.45))
    for method in methods:
        values = [mean(rows, method, key, sigma=value) for value in sigmas]
        style = "--" if method.startswith("raw") else ("-." if "oracle" in method or "flat" in method else "-")
        ax.plot(
            sigmas,
            values,
            style,
            color=COLORS[method],
            marker="o",
            ms=4,
            lw=2.7 if method.startswith("acsf") else 1.65,
            label=LABELS[method],
        )
    ax.set_xlabel("Среднеквадратичное отклонение шума σ, ед. яркости")
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    ax.legend(fontsize=8.6, ncol=2)
    fig.tight_layout()
    fig.savefig(destination, bbox_inches="tight")
    plt.close(fig)


def write_metric_tables(rows: list[dict], sigmas: list[float], images: list[str], methods: list[str], tables: Path) -> None:
    for key, stem, digits in (("psnr", "psnr", 2), ("ssim", "ssim", 4), ("epi", "epi", 4)):
        header = "| Метод | " + " | ".join(f"σ={value:g}" for value in sigmas) + " | Среднее |"
        separator = "|---|" + "---|" * (len(sigmas) + 1)
        lines = [header, separator]
        for method in methods:
            cells = [fmt(mean(rows, method, key, sigma=value), digits) for value in sigmas]
            lines.append(f"| {LABELS[method]} | " + " | ".join(cells) + f" | **{fmt(mean(rows, method, key), digits)}** |")
        write(tables / f"table_{stem}_by_sigma.md", "\n".join(lines) + "\n")

    selected_sigma = 20.0 if 20.0 in sigmas else sigmas[len(sigmas) // 2]
    header = "| Метод | " + " | ".join(images) + " |"
    lines = [header, "|---|" + "---|" * len(images)]
    for method in methods:
        values = [fmt(mean(rows, method, "psnr", sigma=selected_sigma, image=image)) for image in images]
        lines.append(f"| {LABELS[method]} | " + " | ".join(values) + " |")
    write(tables / "table_psnr_by_image_sigma20.md", "\n".join(lines) + "\n")


def write_summary_table(rows: list[dict], methods: list[str], tables: Path) -> dict[str, dict[str, float | str]]:
    lines = ["| Метод | PSNR, дБ | SSIM | EPI | Время, мс |", "|---|---|---|---|---|"]
    summary: dict[str, dict[str, float | str]] = {}
    for method in methods:
        values = {
            "psnr": mean(rows, method, "psnr"),
            "ssim": mean(rows, method, "ssim"),
            "epi": mean(rows, method, "epi"),
            "time_ms": mean(rows, method, "time_ms"),
        }
        shown_time = "—" if method == "bilateral_oracle" else fmt(values["time_ms"], 1)
        lines.append(
            f"| {LABELS[method]} | {fmt(values['psnr'])} | {fmt(values['ssim'], 4)} | "
            f"{fmt(values['epi'], 4)} | {shown_time} |"
        )
        summary[method] = values
    write(tables / "table_summary.md", "\n".join(lines) + "\n")
    return summary


def summary_bars(rows: list[dict], methods: list[str], destination: Path, title: str) -> None:
    shown = [method for method in methods if method not in {"raw", "box5"}]
    fig, axes = plt.subplots(1, 3, figsize=(15.8, 4.8))
    for axis, (key, ylabel, digits) in zip(
        axes,
        (("psnr", "PSNR, дБ", 2), ("ssim", "SSIM", 3), ("epi", "EPI", 3)),
    ):
        values = [mean(rows, method, key) for method in shown]
        bars = axis.bar(range(len(shown)), values, color=[COLORS[m] for m in shown], edgecolor="#333", linewidth=0.6)
        axis.set_xticks(range(len(shown)))
        axis.set_xticklabels([LABELS[m].replace("Билатеральный ", "Билат. ") for m in shown], rotation=30, ha="right", fontsize=8.5)
        axis.set_ylabel(ylabel)
        delta = max(values) - min(values)
        axis.set_ylim(min(values) - max(delta * 0.35, 0.02), max(values) + max(delta * 0.23, 0.02))
        for bar, value in zip(bars, values):
            axis.text(bar.get_x() + bar.get_width() / 2, value, f"{value:.{digits}f}", ha="center", va="bottom", fontsize=8)
    fig.suptitle(title, y=1.02)
    fig.tight_layout()
    fig.savefig(destination, bbox_inches="tight")
    plt.close(fig)


def time_plot(rows: list[dict], methods: list[str], destination: Path) -> None:
    shown = [method for method in methods if method != "bilateral_oracle"]
    values = [mean(rows, method, "time_ms") for method in shown]
    fig, ax = plt.subplots(figsize=(7.3, 4.35))
    bars = ax.barh(range(len(shown)), values, color=[COLORS[m] for m in shown], edgecolor="#333")
    ax.set_yticks(range(len(shown)))
    ax.set_yticklabels([LABELS[m] for m in shown])
    ax.invert_yaxis()
    ax.set_xlabel("Среднее время фильтрации, мс")
    ax.set_xlim(0, max(values) * 1.22)
    for bar, value in zip(bars, values):
        ax.text(value + max(values) * 0.015, bar.get_y() + bar.get_height() / 2, f"{value:.1f}", va="center", fontsize=9)
    fig.tight_layout()
    fig.savefig(destination, bbox_inches="tight")
    plt.close(fig)


def analyse_significance(tables: Path) -> dict | None:
    source = tables / "significance_runs.csv"
    if not source.exists():
        return None
    with source.open(encoding="utf-8", newline="") as handle:
        pairs = list(csv.DictReader(handle))
    bilateral = np.array([float(row["bilateral_psnr"]) for row in pairs])
    acsf = np.array([float(row["acsf_psnr"]) for row in pairs])
    delta = acsf - bilateral
    statistic, two_sided = paired_t_test(delta)
    one_sided = float(two_sided / 2.0) if delta.mean() > 0 else float(1.0 - two_sided / 2.0)
    result = {
        "n": int(len(delta)),
        "bilateral_psnr": float(bilateral.mean()),
        "acsf_psnr": float(acsf.mean()),
        "delta_psnr": float(delta.mean()),
        "delta_std": float(delta.std(ddof=1)),
        "t_statistic": float(statistic),
        "p_two_sided": float(two_sided),
        "p_one_sided": one_sided,
        "significant": bool(one_sided < 0.05 and delta.mean() > 0),
    }
    conclusion = "АКСФ значимо лучше билатерального фильтра (p < 0,05)." if result["significant"] else "Значимое превосходство не подтверждено при p < 0,05."
    lines = [
        "| Показатель | Значение |",
        "|---|---:|",
        "| Критерий | Парный t-тест, односторонняя альтернатива ACSF > bilateral |",
        f"| Независимых seed | {result['n']} |",
        f"| Средний PSNR bilateral, дБ | {fmt(result['bilateral_psnr'], 3)} |",
        f"| Средний PSNR АКСФ, дБ | {fmt(result['acsf_psnr'], 3)} |",
        f"| Средняя парная разница, дБ | {fmt(result['delta_psnr'], 3)} |",
        f"| t-статистика | {result['t_statistic']:.3f} |",
        f"| p (двустороннее) | {result['p_two_sided']:.3e} |",
        f"| p (одностороннее) | {result['p_one_sided']:.3e} |",
        f"| Вывод | {conclusion} |",
    ]
    write(tables / "significance_summary.md", "\n".join(lines) + "\n")
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=None, help="корень результатов (по умолчанию results или results/color)")
    parser.add_argument("--color", action="store_true", help="анализировать цветной YCbCr-эксперимент")
    args = parser.parse_args()

    root = Path(args.root) if args.root else ROOT_REPO / ("results/color" if args.color else "results")
    tables = root / "tables"
    plots = root / "plots"
    plots.mkdir(parents=True, exist_ok=True)
    rows = read_rows(tables / "results.csv")
    sigmas = sorted({row["sigma_noise"] for row in rows})
    images = sorted({row["image"] for row in rows})
    methods = available(COLOR_ORDER if args.color else GRAY_ORDER, rows)

    if args.color:
        plot_methods = methods
        line_plot(rows, sigmas, plot_methods, "psnr", "PSNR в YCbCr, дБ", "Цветное шумоподавление в YCbCr", plots / "fig_color_psnr_vs_sigma.png")
        line_plot(rows, sigmas, plot_methods, "ssim", "SSIM в YCbCr", "Структурное сходство цветных изображений", plots / "fig_color_ssim_vs_sigma.png")
        summary_bars(rows, methods, plots / "fig_color_summary.png", "Цветной эксперимент: средние метрики в YCbCr")
        write_metric_tables(rows, sigmas, images, methods, tables)
        summary = write_summary_table(rows, methods, tables)
        output = {"mode": "YCbCr", "images": images, "sigmas": sigmas, "methods": summary}
    else:
        plot_methods = [method for method in ("raw", "gauss", "perona_malik", "bilateral", "bilateral_oracle", "acsf") if method in methods]
        line_plot(rows, sigmas, plot_methods, "psnr", "PSNR, дБ", "Качество шумоподавления: PSNR от уровня шума", plots / "fig_psnr_vs_sigma.png")
        line_plot(rows, sigmas, plot_methods, "ssim", "SSIM", "Структурное сходство SSIM от уровня шума", plots / "fig_ssim_vs_sigma.png")
        line_plot(rows, sigmas, [m for m in plot_methods if m != "raw"], "epi", "EPI", "Сохранение краёв и деталей (EPI)", plots / "fig_epi_vs_sigma.png")
        summary_bars(rows, methods, plots / "fig_summary_bars.png", "Сравнение методов: средние по всем изображениям и уровням шума")
        time_plot(rows, methods, plots / "fig_time.png")
        write_metric_tables(rows, sigmas, images, methods, tables)
        summary = write_summary_table(rows, methods, tables)
        output = {
            "mode": "grayscale",
            "images": images,
            "sigmas": sigmas,
            "methods": summary,
            "performance_512": grayscale_512_performance(rows),
            "significance": analyse_significance(tables),
        }

    with (tables / "summary.json").open("w", encoding="utf-8") as handle:
        json.dump(output, handle, ensure_ascii=False, indent=2)
    print(f"Готово: {root}")
    print((tables / "table_summary.md").read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
