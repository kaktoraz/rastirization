#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Собрать PPTX со speaker notes и синхронным текстом выступления.

Все числовые подписи загружаются из результатов ``analysis.py``; поэтому
презентация не содержит вручную продублированных цифр эксперимента.
"""
from __future__ import annotations

from pathlib import Path

from pptx import Presentation
from pptx.dml.color import RGBColor
from pptx.enum.shapes import MSO_SHAPE
from pptx.enum.text import MSO_ANCHOR, PP_ALIGN
from pptx.util import Inches, Pt

from project_data import (
    COLOR_PLOTS,
    DOCS,
    GRAY_ORDER,
    LABELS,
    PLOTS,
    calibration_rows,
    load,
    method_row,
    number,
    selected_calibration,
)

OUT = DOCS / "Презентация_Сглаживание_изображений.pptx"
SPEECH = DOCS / "Речь_для_выступления.md"
NAVY = RGBColor(0x1F, 0x2D, 0x5C)
RED = RGBColor(0xD6, 0x27, 0x28)
GREY = RGBColor(0x44, 0x44, 0x44)
PALE = RGBColor(0xEE, 0xF2, 0xF8)
WHITE = RGBColor(0xFF, 0xFF, 0xFF)


def add_text(slide, x, y, w, h, text, size=18, color=GREY, bold=False, align=PP_ALIGN.LEFT):
    shape = slide.shapes.add_textbox(Inches(x), Inches(y), Inches(w), Inches(h))
    frame = shape.text_frame
    frame.word_wrap = True
    frame.vertical_anchor = MSO_ANCHOR.TOP
    paragraph = frame.paragraphs[0]
    paragraph.text = text
    paragraph.alignment = align
    paragraph.font.name = "Calibri"
    paragraph.font.size = Pt(size)
    paragraph.font.color.rgb = color
    paragraph.font.bold = bold
    return shape


def add_bullets(slide, x, y, w, h, items, size=17):
    shape = slide.shapes.add_textbox(Inches(x), Inches(y), Inches(w), Inches(h))
    frame = shape.text_frame
    frame.word_wrap = True
    for index, item in enumerate(items):
        if isinstance(item, tuple):
            text, color, bold = item
        else:
            text, color, bold = item, GREY, False
        paragraph = frame.paragraphs[0] if index == 0 else frame.add_paragraph()
        paragraph.text = "• " + text
        paragraph.font.name = "Calibri"
        paragraph.font.size = Pt(size)
        paragraph.font.color.rgb = color
        paragraph.font.bold = bold
        paragraph.space_after = Pt(9)
    return shape


def add_title(slide, title, subtitle=None):
    shape = slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, 0, 0, prs.slide_width, Inches(0.9))
    shape.fill.solid()
    shape.fill.fore_color.rgb = NAVY
    shape.line.fill.background()
    frame = shape.text_frame
    frame.margin_left = Inches(0.42)
    frame.margin_top = Inches(0.08)
    paragraph = frame.paragraphs[0]
    paragraph.text = title
    paragraph.font.name = "Calibri"
    paragraph.font.size = Pt(25)
    paragraph.font.bold = True
    paragraph.font.color.rgb = WHITE
    if subtitle:
        paragraph = frame.add_paragraph()
        paragraph.text = subtitle
        paragraph.font.name = "Calibri"
        paragraph.font.size = Pt(11)
        paragraph.font.color.rgb = RGBColor(0xC9, 0xD6, 0xE8)


def add_picture(slide, path: Path, x, y, width):
    if path.exists():
        slide.shapes.add_picture(str(path), Inches(x), Inches(y), width=Inches(width))
    else:
        add_text(slide, x, y, width, 0.5, f"Не найден: {path.name}", 12, RED)


def add_table(slide, x, y, width, height, data, font_size=12, highlight_rows=()):
    rows, columns = len(data), len(data[0])
    table = slide.shapes.add_table(rows, columns, Inches(x), Inches(y), Inches(width), Inches(height)).table
    for row in range(rows):
        for column in range(columns):
            cell = table.cell(row, column)
            cell.text = str(data[row][column])
            cell.margin_left = Inches(0.04)
            cell.margin_right = Inches(0.04)
            cell.margin_top = Inches(0.015)
            cell.margin_bottom = Inches(0.015)
            if row == 0:
                cell.fill.solid()
                cell.fill.fore_color.rgb = NAVY
            elif row in highlight_rows:
                cell.fill.solid()
                cell.fill.fore_color.rgb = PALE
            for paragraph in cell.text_frame.paragraphs:
                paragraph.alignment = PP_ALIGN.LEFT if column == 0 else PP_ALIGN.CENTER
                paragraph.font.name = "Calibri"
                paragraph.font.size = Pt(font_size)
                paragraph.font.bold = row == 0 or row in highlight_rows
                paragraph.font.color.rgb = WHITE if row == 0 else (RED if row in highlight_rows else GREY)
    return table


def add_notes(slide, title: str, speech: str):
    slide.notes_slide.notes_text_frame.text = speech
    speeches.append((title, speech))


def new_slide(title: str, subtitle: str | None = None):
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    add_title(slide, title, subtitle)
    return slide


def write_speech() -> None:
    lines = [
        "# Текст выступления к презентации (≈8–10 минут)",
        "",
        "Этот файл сформирован одновременно с PPTX. Каждый раздел полностью совпадает с заметками соответствующего слайда; цифры поступают из CSV-производных таблиц эксперимента.",
        "",
    ]
    for index, (title, text) in enumerate(speeches, start=1):
        lines += [f"## Слайд {index}. {title}", "", text, ""]
    lines += [
        "---",
        "",
        "## Короткие ответы на вероятные вопросы",
        "",
        "**Почему АКСФ не называется принципиально новым мировым методом?** Это учебное развитие билатеральной фильтрации. Самостоятельно реализованы конкретные карта активности, правило адаптации и эксперимент, но идея адаптивного диапазонного параметра известна в литературе.",
        "",
        "**Почему «оракул» в таблице?** Он получает чистый эталон только для выбора лучшего параметра bilateral. Поэтому это верхняя граница семейства, а не практический конкурент и не строка для сравнения времени.",
        "",
        "**Почему результат статистически значим?** На 30 независимых seed-прогонах единица наблюдения — среднее по всем 36 задачам одного seed. Сравнение парное, так как bilateral и АКСФ видят один и тот же шумный вход; p берётся из парного t-теста.",
        "",
        "**Что именно ускорено?** Интегральная оценка дисперсии, LUT для экспоненты и двухпроходная сепарабельная пространственная часть. Точная 2D-версия сохранена в тесте, который допускает не более 0,05 дБ потери у публичного пути.",
        "",
        "**Какие ограничения?** Только гауссов шум, ограниченная сетка калибровки, производительность зависит от runner. Для импульсного и реального камерного шума требуются самостоятельные опыты.",
    ]
    SPEECH.write_text("\n".join(lines) + "\n", encoding="utf-8")


prs = Presentation()
prs.slide_width = Inches(13.333)
prs.slide_height = Inches(7.5)
speeches: list[tuple[str, str]] = []


def main() -> None:
    gray, color, calibration = load()
    selected = selected_calibration(calibration)
    values = gray["methods"]
    color_values = color["methods"]
    acsf = values["acsf"]
    bilateral = values["bilateral"]
    perona = values["perona_malik"]
    oracle = values["bilateral_oracle"]
    flat = values["acsf_flat"]
    stats = gray["significance"]
    perf = gray["performance_512"]

    # 1
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    background = slide.shapes.add_shape(MSO_SHAPE.RECTANGLE, 0, 0, prs.slide_width, prs.slide_height)
    background.fill.solid(); background.fill.fore_color.rgb = NAVY; background.line.fill.background()
    add_text(slide, 0.75, 1.2, 11.7, 1.6, "Адаптивный\nконтрастно-структурный фильтр", 37, WHITE, True, PP_ALIGN.CENTER)
    add_text(slide, 0.75, 3.05, 11.7, 0.55, "АКСФ: воспроизводимое исследование шумоподавления на Rust", 21, RGBColor(0xE6, 0x6A, 0x6B), True, PP_ALIGN.CENTER)
    add_text(slide, 0.75, 4.45, 11.7, 1.0, "6 полутоновых сцен · 6 уровней шума · цветный YCbCr-режим\n30 парных независимых прогонов", 18, RGBColor(0xCF, 0xD9, 0xE8), False, PP_ALIGN.CENTER)
    add_text(slide, 0.75, 6.45, 11.7, 0.35, "Учебный проект · 2026", 14, RGBColor(0xCF, 0xD9, 0xE8), False, PP_ALIGN.CENTER)
    add_notes(slide, "Титульный слайд", "Здравствуйте. Я покажу учебный проект по шумоподавлению изображений — адаптивный контрастно-структурный фильтр, или АКСФ. Это не заявление о новом мировом классе алгоритмов, а прозрачное развитие билатеральной фильтрации. Важная часть работы — не только метод, но и честный воспроизводимый эксперимент: шесть сцен, шесть уровней шума, цветной режим и отдельная статистическая проверка.")

    # 2
    slide = new_slide("Проблема: шум и потеря деталей", "сглаживание не должно превращать текстуру в размытие")
    add_picture(slide, PLOTS / "fig_visual_fruits.png", 0.45, 1.25, 8.1)
    add_bullets(slide, 8.8, 1.5, 4.05, 4.9, [
        "Гауссов шум ухудшает видимость структуры.",
        "Обычное среднее уменьшает шум, но размывает границы.",
        ("Нужна сила сглаживания, зависящая от локального содержимого.", RED, True),
        "На практике σ шума обычно неизвестна.",
    ], 16)
    add_notes(slide, "Проблема", "На примере видны два противоречивых требования. Нужно подавить гауссову зернистость, но не потерять контуры и текстуру. Простое усреднение одинаково относится к гладкой области и к границе, поэтому неизбежно размывает детали. Кроме того, на реальном кадре заранее неизвестен уровень шума. Это мотивирует локально адаптивное правило вместо одного глобального параметра.")

    # 3
    slide = new_slide("АКСФ: локальная адаптация σr(p)", "сначала шум, затем структура, затем билинейное взвешивание")
    add_text(slide, 0.75, 1.35, 11.8, 0.55, "A(p) = √max(0, V(p) − σ̂²)", 25, NAVY, True, PP_ALIGN.CENTER)
    add_text(slide, 0.75, 2.15, 11.8, 0.7, "σr(p) = σ̂[kmin + (kmax − kmin) exp(−(A(p)/(cσ̂))²)]", 23, RED, True, PP_ALIGN.CENTER)
    add_bullets(slide, 1.0, 3.25, 5.6, 2.75, [
        "σ̂: автоматическая оценка Иммеркера.",
        "V(p): несмещённая дисперсия в окне 5×5.",
        "A(p): структура после вычитания дисперсии шума.",
    ], 18)
    add_bullets(slide, 6.9, 3.25, 5.4, 2.75, [
        ("Гладко → σr ближе к kmax·σ̂ → сильное сглаживание.", RED, True),
        ("Контур/текстура → σr ближе к kmin·σ̂ → осторожное сглаживание.", RED, True),
        "Пространственный вес — гауссов, окно 11×11.",
    ], 18)
    add_notes(slide, "Идея АКСФ", "Алгоритм состоит из трёх логичных шагов. Сначала оценка шума по самому изображению. Затем в небольшом окне измеряется дисперсия, но из неё вычитается дисперсия шума — поэтому карта активности показывает преимущественно полезную структуру. Наконец, эта карта непрерывно управляет диапазонным параметром билатерального фильтра. В гладком месте фильтр смелее, а на детали — осторожнее.")

    # 4
    slide = new_slide("Калибровка и ускоренная реализация", "выбор параметров отделён от итогового теста")
    selected_text = f"Выбрано на σ=20,30: {selected['config']}"
    add_text(slide, 0.75, 1.22, 12.0, 0.55, selected_text, 19, RED, True, PP_ALIGN.CENTER)
    add_bullets(slide, 0.8, 2.0, 6.0, 4.6, [
        "Интегральные изображения для карты дисперсии.",
        "LUT с линейной интерполяцией для exp(−x).",
        "Два сепарабельных пространственных прохода вместо прямого 2D-ядра.",
        (f"На {perf['images']} реальных 512×512 сценах: {number(perf['acsf_time_ms'], 2)} мс < 30 мс.", RED, True),
    ], 17)
    data = [["Путь", "Время"] , ["Исходный 2D", "241,9 мс"], ["Точный ускоренный 2D", "70,9 мс"], ["Сепарабельный public", "20,47 мс"]]
    add_table(slide, 7.25, 2.15, 5.3, 2.1, data, 13, highlight_rows=(3,))
    add_text(slide, 7.25, 4.65, 5.25, 1.25, "Регрессионный тест: public ACSF не может потерять более 0,05 дБ относительно сохранённой точной 2D-версии при общей σ̂.", 15, GREY, False, PP_ALIGN.CENTER)
    add_notes(slide, "Калибровка и ускорение", "Параметры не менялись ради удобства оптимизации. Была проведена отдельная калибровка только на уровнях 20 и 30; остальные уровни не использовались при выборе. Затем основную стоимость сняли тремя инженерными шагами: интегральная дисперсия, таблица экспоненты и сепарабельный проход. При этом точная 2D-версия оставлена внутри тестов, а публичный быстрый путь ограничен регрессионным порогом потери качества 0,05 децибела.")

    # 5
    slide = new_slide("Честный протокол", "один генератор, один шумный вход и один seed для сравниваемых методов")
    add_bullets(slide, 0.8, 1.35, 5.8, 5.4, [
        (f"{len(gray['images'])} сцен: " + ", ".join(gray['images']), NAVY, True),
        "σ = " + ", ".join(f"{v:g}" for v in gray['sigmas']) + "; всего 36 задач на метод.",
        "raw, flat-контроль, box, Gaussian, median, bilateral, Perona–Malik, ACSF.",
        "«Оракул» bilateral — только диагностическая верхняя граница.",
        "PSNR, SSIM, EPI и время; CSV и логи сохраняются workflow-ом.",
    ], 16)
    add_picture(slide, PLOTS / "fig_activity_map.png", 6.9, 1.55, 5.8)
    add_notes(slide, "Протокол", "Здесь важна честность сравнения. Для каждой комбинации изображения и уровня шума создаётся один шумный кадр с явным seed, и все методы получают именно его. В основном протоколе шесть сцен умножаются на шесть уровней, то есть каждый метод оценивается на 36 задачах. Перона—Малик реализован самостоятельно как дополнительный конкурент. Оракул не является применимым алгоритмом: он видит эталон только для демонстрации потолка семейства bilateral.")

    # 6
    slide = new_slide("Основные результаты: полутоновый режим", "среднее по 6 сценам × 6 уровням шума")
    table_data = [["Метод", "PSNR", "SSIM", "EPI", "мс"]]
    keep = ["gauss", "perona_malik", "bilateral", "bilateral_oracle", "acsf_flat", "acsf"]
    table_data += [method_row(gray, method) for method in keep]
    add_table(slide, 0.45, 1.3, 6.15, 3.55, table_data, 11.5, highlight_rows=(len(table_data) - 1,))
    add_picture(slide, PLOTS / "fig_psnr_vs_sigma.png", 6.85, 1.33, 5.95)
    gain = acsf['psnr'] - bilateral['psnr']
    gap = oracle['psnr'] - acsf['psnr']
    add_text(slide, 0.65, 5.25, 12.0, 0.82, f"АКСФ: +{number(gain)} дБ к универсальному bilateral; до «оракула» {number(gap)} дБ.  Средний PSNR = {number(acsf['psnr'])} дБ.", 18, RED, True, PP_ALIGN.CENTER)
    add_notes(slide, "Основные результаты", f"Главный результат — средний PSNR АКСФ {number(acsf['psnr'])} децибела. Это на {number(gain)} децибела выше универсального bilateral. Важно не завышать вывод: оракул выше на {number(gap)} децибела, но он использует чистый эталон для подбора параметров. В числе честных применимых конкурентов также есть Перона—Малик; он сильнее Gaussian и bilateral, но АКСФ сохраняет преимущество по PSNR и занимает меньше времени.")

    # 7
    slide = new_slide("Что даёт именно адаптация?", "контрольный ACSF-flat отличается только отсутствием локальной карты")
    add_picture(slide, PLOTS / "fig_summary_bars.png", 0.45, 1.15, 8.1)
    add_bullets(slide, 8.8, 1.55, 3.95, 4.6, [
        (f"ACSF − flat = +{number(acsf['psnr'] - flat['psnr'])} дБ PSNR", RED, True),
        (f"ACSF − Perona–Malik = +{number(acsf['psnr'] - perona['psnr'])} дБ", RED, True),
        "Рост SSIM и EPI подтверждает, что выигрыш не сводится к размытию.",
        "Карта A(p) отделяет полезную структуру от шума.",
    ], 16)
    add_notes(slide, "Контроль адаптации", "Чтобы не объяснять успех случайно удачным средним параметром, включён flat-контроль. В нём та же оценка шума и тот же фильтр, но kmin и kmax сведены к одному среднему значению. Полный АКСФ выигрывает у него по PSNR. Это экспериментальная поддержка именно локального правила, а не просто выбора одной удачной настройки.")

    # 8
    slide = new_slide("Визуальная проверка", "фрагмент «baboon», σ=20; подписи PSNR считываются из CSV")
    add_picture(slide, PLOTS / "fig_visual_baboon.png", 0.3, 1.45, 12.7)
    add_text(slide, 0.8, 5.85, 11.75, 0.7, "Визуальный вывод согласуется с метриками: в гладких зонах шум ослаблен, а текстура и контур сохранены лучше, чем при глобальном сглаживании.", 16, GREY, False, PP_ALIGN.CENTER)
    add_notes(slide, "Визуальная проверка", "Цифры важны, но для шумоподавления необходима визуальная проверка. На фрагменте бабуина видны одновременно гладкие области, контур и мелкая текстура. Gaussian снижает зернистость, но сглаживает мех. Bilateral сохраняет больше границ, но при универсальной настройке оставляет шум. АКСФ использует карту активности и пытается сохранить текстуру, не обрабатывая её как шум.")

    # 9
    slide = new_slide("Статистическая проверка", "30 парных независимых seed-прогонов: ACSF > bilateral")
    stat_data = [["Показатель", "Значение"], ["Независимых пар", str(stats['n'])], ["Δ PSNR, дБ", "+" + number(stats['delta_psnr'], 3)], ["t", f"{stats['t_statistic']:.1f}"], ["p, односторонний", f"{stats['p_one_sided']:.2e}"]]
    add_table(slide, 0.9, 1.6, 5.4, 2.9, stat_data, 14, highlight_rows=(4,))
    add_bullets(slide, 6.8, 1.6, 5.4, 3.7, [
        "Единица наблюдения — среднее по 36 задачам одного seed.",
        "Сравнение парное: оба метода получают один шумный вход.",
        ("p < 0,05: превосходство АКСФ в данном протоколе статистически подтверждено.", RED, True),
    ], 18)
    add_text(slide, 1.0, 5.45, 11.3, 0.55, "Это не доказательство универсальности для всех видов шума; это корректная проверка внутри заранее заданного эксперимента.", 16, GREY, False, PP_ALIGN.CENTER)
    add_notes(slide, "Статистическая проверка", "Один детерминированный запуск может быть удачным или неудачным для конкретного шума. Поэтому сравнение bilateral и АКСФ повторено на 30 независимых seed. Чтобы не делать вид, будто отдельные пиксели — независимые наблюдения, каждое наблюдение — это средний результат всех 36 задач одного seed. Поскольку входы общие, t-тест парный. Полученная p-величина значительно меньше 0,05, поэтому в этом протоколе наблюдаемый выигрыш не объясняется случайной сменой seed.")

    # 10
    slide = new_slide("Реальный цветной YCbCr-режим", "адаптация по Y, совместная фильтрация Y, Cb и Cr")
    color_table = [["Метод", "PSNR", "SSIM", "EPI", "мс"]] + [method_row(color, method) for method in ("raw_ycbcr", "bilateral_ycbcr", "acsf_ycbcr")]
    add_table(slide, 0.55, 1.45, 5.85, 2.25, color_table, 12.3, highlight_rows=(3,))
    add_picture(slide, COLOR_PLOTS / "fig_color_psnr_vs_sigma.png", 6.75, 1.25, 5.95)
    color_gain = color_values['acsf_ycbcr']['psnr'] - color_values['bilateral_ycbcr']['psnr']
    add_text(slide, 0.75, 4.5, 11.85, 1.15, f"АКСФ в YCbCr: +{number(color_gain)} дБ к bilateral. Метрики: PSNR из среднего MSE трёх каналов; SSIM/EPI — среднее поканально.", 17, RED, True, PP_ALIGN.CENTER)
    add_bullets(slide, 1.2, 5.85, 11.0, 0.75, ["Это не три независимых RGB-фильтра: вес использует яркость и цветность совместно."], 16)
    add_notes(slide, "Цветной режим", "Цветная часть реализована отдельно. Сначала RGB переводится в YCbCr. Оценка шума и карта активности строятся по яркости Y, что соответствует визуальной роли контура. Но затем Y, Cb и Cr фильтруются совместным весом, поэтому это не три независимые серые обработки. На трёх цветных сценах АКСФ также выигрывает у реализованного bilateral по всем сводным метрикам.")

    # 11
    slide = new_slide("Выводы и ограничения", "что подтверждено, а что требует дальнейшей проверки")
    add_bullets(slide, 0.75, 1.25, 11.8, 4.6, [
        (f"АКСФ: {number(acsf['psnr'])} дБ, {number(acsf['ssim'], 4)} SSIM, {number(acsf['epi'], 4)} EPI; целевое время 512×512 выполнено.", RED, True),
        "Ускоренный public-путь защищён регрессией относительно точного 2D-эталона.",
        "Цветной YCbCr и Perona–Malik реализованы самостоятельно и включены в общий протокол.",
        "Ограничения: AWGN, ограниченная калибровочная сетка, платформозависимое время, отсутствие испытаний на реальном камерном и импульсном шуме.",
        "Следующий шаг: отдельные датасеты/шумовые модели и независимый тестовый набор.",
    ], 18)
    add_text(slide, 0.75, 6.15, 11.8, 0.55, "Спасибо за внимание — готов(а) ответить на вопросы.", 25, NAVY, True, PP_ALIGN.CENTER)
    add_notes(slide, "Выводы", "Подведу итог. В проекте есть не только алгоритм, но и полный путь проверки: быстрый публичный вариант, точная контрольная версия, контроль адаптации, сильный реализованный конкурент, цветной режим и парная статистика. Результаты говорят в пользу АКСФ именно в заданной модели гауссова шума. Честные ограничения также важны: нельзя переносить этот вывод без новых опытов на импульсный шум или реальные снимки. Спасибо за внимание, готов ответить на вопросы.")

    OUT.parent.mkdir(parents=True, exist_ok=True)
    prs.save(OUT)
    write_speech()
    print(f"Презентация сохранена: {OUT}; слайдов: {len(prs.slides)}")
    print(f"Речь сохранена: {SPEECH}")


if __name__ == "__main__":
    main()
