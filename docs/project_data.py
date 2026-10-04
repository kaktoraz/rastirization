"""Общие данные для генераторов отчёта и презентации.

Никаких чисел результата здесь не хранится: они читаются из JSON/CSV, которые
создаёт ``analysis.py`` из воспроизводимого ``results.csv``.
"""
from __future__ import annotations

import csv
import json
import os
from pathlib import Path

BASE = Path(__file__).resolve().parents[1]
# CI renders documents from its just-produced artifact before the artifact is
# copied back into results/. Local invocations use the repository default.
RESULTS = Path(os.environ.get("ACSF_RESULTS_ROOT", BASE / "results"))
COLOR_RESULTS = RESULTS / "color"
PLOTS = RESULTS / "plots"
COLOR_PLOTS = COLOR_RESULTS / "plots"
DOCS = BASE / "docs"

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


def load_json(path: Path) -> dict:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def load() -> tuple[dict, dict, list[dict]]:
    """Return grayscale summary, colour summary and calibration candidates."""
    gray = load_json(RESULTS / "tables" / "summary.json")
    color = load_json(COLOR_RESULTS / "tables" / "summary.json")
    with (RESULTS / "tables" / "calibration.csv").open(encoding="utf-8", newline="") as handle:
        calibration = list(csv.DictReader(handle))
    return gray, color, calibration


def number(value: float, digits: int = 2) -> str:
    """Russian decimal presentation for a report/table."""
    return f"{float(value):.{digits}f}".replace(".", ",")


def method_row(summary: dict, method: str) -> list[str]:
    values = summary["methods"][method]
    time = "—" if method == "bilateral_oracle" else number(values["time_ms"], 1)
    return [
        LABELS[method],
        number(values["psnr"], 2),
        number(values["ssim"], 4),
        number(values["epi"], 4),
        time,
    ]


def calibration_rows(calibration: list[dict]) -> list[list[str]]:
    return [
        [
            row["config"],
            number(float(row["mean_psnr"]), 3),
            number(float(row["mean_ssim"]), 4),
            number(float(row["mean_epi"]), 4),
        ]
        for row in calibration
    ]


def selected_calibration(calibration: list[dict]) -> dict:
    """The row with the largest calibration PSNR, used by AcsfParams::default."""
    return max(calibration, key=lambda row: float(row["mean_psnr"]))
