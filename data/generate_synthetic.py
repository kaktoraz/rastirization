#!/usr/bin/env python3
"""Создаёт детерминированную тестовую сцену «градиент + круги».

Сцена не заимствована из внешнего набора: она служит контролем на гладких
переходах, чётких границах и тонких синусоидальных деталях.
"""
from pathlib import Path
import math
from PIL import Image

ROOT = Path(__file__).resolve().parent
OUT = ROOT / "clean" / "gradient_circles.png"
SIZE = 512


def clamp(value: float) -> int:
    return round(max(0.0, min(255.0, value)))


def main() -> None:
    pixels: list[int] = []
    for y in range(SIZE):
        for x in range(SIZE):
            # Гладкий горизонтально-вертикальный градиент.
            value = 28.0 + 145.0 * x / (SIZE - 1) + 25.0 * y / (SIZE - 1)
            # Крупные объекты с отчётливыми границами.
            d1 = math.hypot(x - 160, y - 185)
            d2 = math.hypot(x - 345, y - 305)
            if d1 < 88:
                value = 210.0 - 0.75 * d1
            if d2 < 112:
                value = 66.0 + 0.80 * d2
            # Тонкие детали с ограниченной амплитудой.
            value += 10.0 * math.sin(0.25 * x) * math.cos(0.18 * y)
            pixels.append(clamp(value))
    image = Image.frombytes("L", (SIZE, SIZE), bytes(pixels))
    OUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(OUT)
    print(f"Saved {OUT.relative_to(ROOT.parent)}")


if __name__ == "__main__":
    main()
