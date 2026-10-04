#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Анализ результатов эксперимента: сводные таблицы (Markdown) и графики.
Читает results/tables/results.csv, пишет:
  results/plots/*.png      — графики для отчёта и презентации
  results/tables/*.md      — готовые таблицы для вставки в доклад
"""
import csv, os
from collections import defaultdict
import numpy as np
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

BASE = os.path.dirname(os.path.abspath(__file__))
CSV = os.path.join(BASE, "results", "tables", "results.csv")
PLOTS = os.path.join(BASE, "results", "plots")
TABLES = os.path.join(BASE, "results", "tables")
os.makedirs(PLOTS, exist_ok=True)

plt.rcParams.update({
    "font.family": "DejaVu Sans",
    "font.size": 11,
    "axes.grid": True,
    "grid.alpha": 0.3,
    "figure.dpi": 130,
})

METHODS = ["raw", "box5", "gauss", "median5", "bilateral", "bilateral_oracle", "acsf_flat", "acsf"]
RU = {
    "raw": "Без обработки",
    "box5": "Среднее 5×5",
    "gauss": "Гауссов",
    "median5": "Медианный 5×5",
    "bilateral": "Билатеральный",
    "bilateral_oracle": "Билатеральный (оракул)",
    "acsf_flat": "АКСФ без адаптации",
    "acsf": "АКСФ (наш метод)",
}
COLORS = {
    "raw": "#999999", "box5": "#c4a35a", "gauss": "#4c9f70", "median5": "#b07cc6",
    "bilateral": "#3f6fb5", "bilateral_oracle": "#1f2d5c", "acsf_flat": "#e08a3c", "acsf": "#d62728",
}

rows = []
with open(CSV) as f:
    for r in csv.DictReader(f):
        r["sigma_noise"] = float(r["sigma_noise"])
        for k in ("psnr", "ssim", "epi", "time_ms"):
            r[k] = float(r[k])
        rows.append(r)

sigmas = sorted({r["sigma_noise"] for r in rows})
images = sorted({r["image"] for r in rows})

def avg(method, sigma=None, image=None, key="psnr"):
    sel = [r for r in rows if r["method"] == method
           and (sigma is None or r["sigma_noise"] == sigma)
           and (image is None or r["image"] == image)]
    return float(np.mean([r[key] for r in sel])) if sel else float("nan")

# ---------------------------------------------------------------- графики
def line_plot(key, ylabel, fname, methods, title):
    fig, ax = plt.subplots(figsize=(7.2, 4.4))
    for m in methods:
        ys = [avg(m, s, key=key) for s in sigmas]
        style = "--" if m == "raw" else ("-." if m in ("bilateral_oracle", "acsf_flat") else "-")
        lw = 2.6 if m == "acsf" else 1.7
        ax.plot(sigmas, ys, style, color=COLORS[m], lw=lw, marker="o", ms=4, label=RU[m])
    ax.set_xlabel("Среднеквадратичное отклонение шума σ, ед. яркости")
    ax.set_ylabel(ylabel)
    ax.set_title(title)
    ax.legend(fontsize=9, ncol=2)
    fig.tight_layout()
    fig.savefig(os.path.join(PLOTS, fname), bbox_inches="tight")
    plt.close(fig)

line_plot("psnr", "PSNR, дБ", "fig_psnr_vs_sigma.png",
          ["raw", "gauss", "median5", "bilateral", "bilateral_oracle", "acsf"],
          "Качество шумоподавления: PSNR от уровня шума (среднее по 3 изображениям)")
line_plot("ssim", "SSIM", "fig_ssim_vs_sigma.png",
          ["raw", "gauss", "median5", "bilateral", "bilateral_oracle", "acsf"],
          "Структурное сходство SSIM от уровня шума")
line_plot("epi", "EPI", "fig_epi_vs_sigma.png",
          ["gauss", "median5", "bilateral", "bilateral_oracle", "acsf"],
          "Сохранение краёв и деталей (EPI) от уровня шума")

# --- сводные столбчатые диаграммы (среднее по всем σ)
fig, axes = plt.subplots(1, 3, figsize=(15.5, 4.8))
ms = ["gauss", "median5", "bilateral", "bilateral_oracle", "acsf_flat", "acsf"]
SHORT = {
    "gauss": "Гауссов", "median5": "Медианный", "bilateral": "Билатеральный",
    "bilateral_oracle": "«Оракул»\n(верхняя\nграница)", "acsf_flat": "АКСФ без\nадаптации", "acsf": "АКСФ\n(наш метод)",
}
for ax, (key, ylab) in zip(axes, [("psnr", "PSNR, дБ"), ("ssim", "SSIM"), ("epi", "EPI")]):
    vals = [avg(m, key=key) for m in ms]
    cols = [COLORS[m] for m in ms]
    bars = ax.bar(range(len(ms)), vals, color=cols,
                  edgecolor="#333333", linewidth=0.6,
                  hatch=["" if m != "acsf" else "//" for m in ms])
    ax.set_xticks(range(len(ms)))
    ax.set_xticklabels([SHORT[m].replace("\n", " ") for m in ms], fontsize=9.5, rotation=30, ha="right")
    ax.set_ylabel(ylab)
    lo = min(vals)
    ax.set_ylim(lo - (max(vals) - lo) * 0.35, max(vals) + (max(vals) - lo) * 0.25)
    for b, v in zip(bars, vals):
        ax.text(b.get_x() + b.get_width() / 2, v, f"{v:.3f}" if key != "psnr" else f"{v:.2f}",
                ha="center", va="bottom", fontsize=8.6)
axes[0].set_title("Средние метрики качества (по всем σ)")
fig.suptitle("Сравнение методов сглаживания: средние по всем изображениям и уровням шума", y=1.02)
fig.tight_layout()
fig.savefig(os.path.join(PLOTS, "fig_summary_bars.png"), bbox_inches="tight")
plt.close(fig)

# --- время работы
fig, ax = plt.subplots(figsize=(7.0, 4.0))
ms2 = ["raw", "box5", "gauss", "median5", "bilateral", "acsf_flat", "acsf"]
vals = [avg(m, key="time_ms") for m in ms2]
bars = ax.barh(range(len(ms2)), vals, color=[COLORS[m] for m in ms2], edgecolor="#333")
ax.set_yticks(range(len(ms2)))
ax.set_yticklabels([RU[m] for m in ms2])
ax.invert_yaxis()
ax.set_xlabel("Среднее время обработки, мс (изображения 512×512…640×480)")
for b, v in zip(bars, vals):
    ax.text(v + 2, b.get_y() + b.get_height() / 2, f"{v:.0f}", va="center", fontsize=9)
ax.set_xlim(0, max(vals) * 1.18)
fig.tight_layout()
fig.savefig(os.path.join(PLOTS, "fig_time.png"))
plt.close(fig)

# ---------------------------------------------------------------- таблицы
def fmt(x, nd=2):
    return f"{x:.{nd}f}"

os.makedirs(TABLES, exist_ok=True)

# Таблица: метрики по уровням шума (среднее по изображениям)
for key, name, nd in [("psnr", "psnr", 2), ("ssim", "ssim", 4), ("epi", "epi", 4)]:
    lines = [f"| Метод | " + " | ".join(f"σ={int(s)}" for s in sigmas) + " | Среднее |",
             "|---|---|" + "---|" * (len(sigmas) + 1)]
    for m in METHODS:
        cells = [fmt(avg(m, s, key=key), nd) for s in sigmas]
        body = " | ".join(cells)
        mean_all = fmt(avg(m, key=key), nd)
        lines.append(f"| {RU[m]} | {body} | **{mean_all}** |")
    open(os.path.join(TABLES, f"table_{name}_by_sigma.md"), "w").write("\n".join(lines) + "\n")

# Таблица: по изображениям при σ=25
lines = ["| Метод | Бабуин | Плата | Фрукты |", "|---|---|---|---|"]
for m in METHODS:
    lines.append(f"| {RU[m]} | " + " | ".join(fmt(avg(m, 25, img, 'psnr'), 2) for img in images) + " |")
open(os.path.join(TABLES, "table_psnr_by_image_sigma25.md"), "w").write("\n".join(lines) + "\n")

# Таблица: итоговая сводка
lines = ["| Метод | PSNR, дБ | SSIM | EPI | Время, мс |", "|---|---|---|---|---|"]
for m in METHODS:
    t = "-" if m == "bilateral_oracle" else fmt(avg(m, key="time_ms"), 1)
    lines.append(f"| {RU[m]} | {fmt(avg(m, key='psnr'))} | {fmt(avg(m, key='ssim'), 4)} | {fmt(avg(m, key='epi'), 4)} | {t} |")
open(os.path.join(TABLES, "table_summary.md"), "w").write("\n".join(lines) + "\n")

print("Графики:")
for f in sorted(os.listdir(PLOTS)):
    print("  results/plots/" + f)
print("Таблицы:")
for f in sorted(os.listdir(TABLES)):
    print("  results/tables/" + f)
print()
print(open(os.path.join(TABLES, "table_summary.md")).read())
