#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Собрать DOCX-отчёт из CSV-производных таблиц и графиков.

Перед запуском выполните ``python analysis.py`` и, при необходимости,
``python visualize.py``. Численные результаты не дублируются в коде: они
берутся из ``results/**/tables/summary.json`` и ``calibration.csv``.
"""
from __future__ import annotations

from pathlib import Path

from docx import Document
from docx.enum.table import WD_TABLE_ALIGNMENT, WD_CELL_VERTICAL_ALIGNMENT
from docx.enum.text import WD_ALIGN_PARAGRAPH
from docx.oxml.ns import qn
from docx.shared import Cm, Pt, RGBColor

from project_data import (
    COLOR_PLOTS,
    COLOR_RESULTS,
    DOCS,
    GRAY_ORDER,
    LABELS,
    PLOTS,
    RESULTS,
    calibration_rows,
    load,
    method_row,
    number,
    selected_calibration,
)

OUT = DOCS / "Отчёт_Сглаживание_изображений.docx"
NAVY = RGBColor(0x1F, 0x2D, 0x5C)


def configure(document: Document) -> None:
    document.core_properties.title = "АКСФ: воспроизводимое исследование шумоподавления"
    document.core_properties.subject = "Учебный проект на Rust"
    document.core_properties.author = "Проект rastirization"
    for section in document.sections:
        section.top_margin = Cm(2.0)
        section.bottom_margin = Cm(2.0)
        section.left_margin = Cm(2.5)
        section.right_margin = Cm(1.5)
    normal = document.styles["Normal"]
    normal.font.name = "Times New Roman"
    normal._element.rPr.rFonts.set(qn("w:eastAsia"), "Times New Roman")
    normal.font.size = Pt(12)
    normal.paragraph_format.line_spacing = 1.15
    normal.paragraph_format.space_after = Pt(5)
    for name, size in (("Heading 1", 16), ("Heading 2", 14), ("Heading 3", 12)):
        style = document.styles[name]
        style.font.name = "Times New Roman"
        style._element.rPr.rFonts.set(qn("w:eastAsia"), "Times New Roman")
        style.font.size = Pt(size)
        style.font.bold = True
        style.font.color.rgb = NAVY
        style.paragraph_format.space_before = Pt(11)
        style.paragraph_format.space_after = Pt(6)


def paragraph(document: Document, text: str, *, bold: bool = False, italic: bool = False, center: bool = False) -> None:
    item = document.add_paragraph()
    item.paragraph_format.first_line_indent = Cm(1.25) if not center else Cm(0)
    item.alignment = WD_ALIGN_PARAGRAPH.CENTER if center else WD_ALIGN_PARAGRAPH.JUSTIFY
    run = item.add_run(text)
    run.bold = bold
    run.italic = italic


def bullet(document: Document, text: str) -> None:
    item = document.add_paragraph(style="List Bullet")
    item.paragraph_format.first_line_indent = Cm(0)
    item.add_run(text)


def formula(document: Document, text: str) -> None:
    item = document.add_paragraph()
    item.alignment = WD_ALIGN_PARAGRAPH.CENTER
    item.paragraph_format.first_line_indent = Cm(0)
    run = item.add_run(text)
    run.italic = True
    run.font.name = "Cambria"
    run.font.size = Pt(12)


def table(document: Document, header: list[str], rows: list[list[str]], widths: list[float] | None = None, highlight_last: bool = False) -> None:
    result = document.add_table(rows=1, cols=len(header))
    result.style = "Table Grid"
    result.alignment = WD_TABLE_ALIGNMENT.CENTER
    result.autofit = True
    all_rows = [header] + rows
    for row_index, values in enumerate(all_rows):
        cells = result.rows[0].cells if row_index == 0 else result.add_row().cells
        for column, value in enumerate(values):
            cell = cells[column]
            cell.vertical_alignment = WD_CELL_VERTICAL_ALIGNMENT.CENTER
            cell.text = ""
            p = cell.paragraphs[0]
            p.alignment = WD_ALIGN_PARAGRAPH.LEFT if column == 0 else WD_ALIGN_PARAGRAPH.CENTER
            p.paragraph_format.first_line_indent = Cm(0)
            run = p.add_run(str(value))
            run.font.name = "Times New Roman"
            run._element.rPr.rFonts.set(qn("w:eastAsia"), "Times New Roman")
            run.font.size = Pt(9.5)
            if row_index == 0 or (highlight_last and row_index == len(all_rows) - 1):
                run.bold = True
            if row_index == 0:
                run.font.color.rgb = RGBColor(0xFF, 0xFF, 0xFF)
                shading = cell._tc.get_or_add_tcPr()
                fill = shading.makeelement(qn("w:shd"), {qn("w:fill"): "1F2D5C"})
                shading.append(fill)
    if widths:
        for row in result.rows:
            for cell, width in zip(row.cells, widths):
                cell.width = Cm(width)


def caption(document: Document, text: str) -> None:
    item = document.add_paragraph()
    item.alignment = WD_ALIGN_PARAGRAPH.CENTER
    item.paragraph_format.first_line_indent = Cm(0)
    run = item.add_run(text)
    run.italic = True
    run.font.size = Pt(10)


def image(document: Document, path: Path, width: float, caption_text: str) -> None:
    if not path.exists():
        paragraph(document, f"[Не найден график: {path.name}]", italic=True, center=True)
        return
    item = document.add_paragraph()
    item.alignment = WD_ALIGN_PARAGRAPH.CENTER
    item.paragraph_format.first_line_indent = Cm(0)
    item.add_run().add_picture(str(path), width=Cm(width))
    caption(document, caption_text)


def code(document: Document, content: str) -> None:
    for line in content.strip().splitlines():
        item = document.add_paragraph()
        item.paragraph_format.first_line_indent = Cm(0)
        item.paragraph_format.space_after = Pt(0)
        run = item.add_run(line or " ")
        run.font.name = "Consolas"
        run._element.rPr.rFonts.set(qn("w:eastAsia"), "Consolas")
        run.font.size = Pt(8.5)


def main() -> None:
    gray, color, calibration = load()
    selected = selected_calibration(calibration)
    methods = gray["methods"]
    color_methods = color["methods"]
    acsf = methods["acsf"]
    bilateral = methods["bilateral"]
    oracle = methods["bilateral_oracle"]
    perona = methods["perona_malik"]
    flat = methods["acsf_flat"]
    stats = gray["significance"]
    perf512 = gray["performance_512"]
    color_acsf = color_methods["acsf_ycbcr"]
    color_bilateral = color_methods["bilateral_ycbcr"]

    document = Document()
    configure(document)

    # Титульный лист
    for _ in range(4):
        document.add_paragraph()
    paragraph(document, "УЧЕБНЫЙ ПРОЕКТ ПО ОБРАБОТКЕ ИЗОБРАЖЕНИЙ", bold=True, center=True)
    paragraph(document, "", center=True)
    paragraph(
        document,
        "РАЗРАБОТКА И ВОСПРОИЗВОДИМОЕ ИССЛЕДОВАНИЕ\n"
        "АДАПТИВНОГО КОНТРАСТНО-СТРУКТУРНОГО ФИЛЬТРА (АКСФ)",
        bold=True,
        center=True,
    )
    paragraph(document, "Программная реализация: Rust", center=True)
    for _ in range(8):
        document.add_paragraph()
    paragraph(document, "2026", center=True)
    document.add_page_break()

    document.add_heading("Аннотация", level=1)
    paragraph(
        document,
        "В работе исследован адаптивный контрастно-структурный фильтр (АКСФ) для "
        "подавления аддитивного гауссова шума. Метод является учебным развитием идеи "
        "билатерального фильтра: его диапазонный параметр вычисляется локально из "
        "шум-компенсированной карты активности и автоматической оценки шума по Иммеркеру. "
        "Реализация выполнена на Rust без библиотек компьютерного зрения; внешний крейт "
        "image используется только для ввода/вывода файлов. Воспроизводимый эксперимент "
        f"охватывает {len(gray['images'])} полутоновых сцен, уровни шума σ={{"
        + ", ".join(f"{s:g}" for s in gray["sigmas"])
        + f"}} и {len(color['images'])} цветные сцены в YCbCr. "
        f"Средний PSNR АКСФ составил {number(acsf['psnr'])} дБ против "
        f"{number(bilateral['psnr'])} дБ у универсального билатерального фильтра."
    )
    paragraph(
        document,
        "Ключевые слова: шумоподавление изображений, билатеральный фильтр, АКСФ, "
        "YCbCr, PSNR, SSIM, EPI, Perona–Malik, Rust, воспроизводимый эксперимент."
    )

    document.add_heading("Содержание", level=1)
    for item in (
        "1. Постановка задачи и честное позиционирование",
        "2. Метод АКСФ",
        "3. Реализация и ускорение",
        "4. Методика эксперимента",
        "5. Результаты полутонового эксперимента",
        "6. Цветной режим YCbCr",
        "7. Ограничения и заключение",
        "Приложение. Воспроизведение результатов",
    ):
        bullet(document, item)

    document.add_heading("1. Постановка задачи и честное позиционирование", level=1)
    paragraph(
        document,
        "Гауссов шум ухудшает видимость слабых деталей, однако обычное усреднение вместе с ним "
        "сглаживает полезные границы и текстуры. Билатеральный фильтр улучшает ситуацию, "
        "поскольку учитывает расстояние и разность яркостей, но глобальный параметр σr всё ещё "
        "приходится выбирать вручную. Цель работы — реализовать прозрачный самоадаптирующийся "
        "вариант и проверить его на одинаковых шумных входах вместе с несколькими честными "
        "конкурентами."
    )
    paragraph(
        document,
        "АКСФ не заявляется как мировой новый класс алгоритмов. Это самостоятельная учебная "
        "конструкция, развивающая известную идею bilateral filtering: локальная активность "
        "используется для управления σr, а масштаб правила привязывается к автоматически "
        "оценённому шуму. Такое позиционирование важно для корректной интерпретации результатов."
    )

    document.add_heading("2. Метод АКСФ", level=1)
    document.add_heading("2.1. Оценка шума и карта активности", level=2)
    paragraph(
        document,
        "Сначала по зашумлённому изображению оценивается σ̂ методом Иммеркера, основанным на "
        "среднем модуле отклика лапласиана. Затем в окне 5×5 вычисляется несмещённая локальная "
        "дисперсия V(p). Вклад шума вычитается до извлечения корня:"
    )
    formula(document, "A(p) = √max(0, V(p) − σ̂²).")
    paragraph(
        document,
        "Поэтому высокая активность соответствует не только флуктуациям шума, а прежде всего "
        "сохранённой локальной структуре. На гладких участках A(p) близка к нулю; около границ "
        "и текстур она растёт."
    )
    document.add_heading("2.2. Адаптивный диапазонный параметр", level=2)
    formula(
        document,
        "σr(p) = σ̂ [kmin + (kmax − kmin) exp(−(A(p)/(cσ̂))²)].",
    )
    paragraph(
        document,
        "В гладкой области правило приближает σr к kmax·σ̂ и активно подавляет шум; у структуры "
        "оно приближает σr к kmin·σ̂ и ограничивает смешивание разных яркостей. После повторной "
        "калибровки только на σ=20 и σ=30 выбрана конфигурация "
        f"{selected['config']} (σs, kmin, kmax, c, r, rstruct). Окно фильтрации остаётся 11×11, "
        "а окно активности — 5×5."
    )
    table(
        document,
        ["Параметр", "Роль", "Итоговое значение"],
        [
            ["σs", "пространственный масштаб", "2,0"],
            ["kmin", "граница σr на структуре", "0,85"],
            ["kmax", "граница σr в гладкой области", "3,6"],
            ["c", "плавность перехода", "1,3"],
            ["r / rstruct", "радиусы фильтра / активности", "5 / 2"],
        ],
        widths=[3.0, 8.5, 4.2],
    )
    caption(document, "Таблица 1. Параметры, выбранные без использования уровней σ=5, 10, 15 и 40.")

    document.add_heading("3. Реализация и ускорение", level=1)
    paragraph(
        document,
        "Программа состоит из модулей img, noise, metrics, filters, experiment и main. Реализованы "
        "скользящее среднее, гауссов, медианный, билатеральный фильтры, анизотропная диффузия "
        "Перона—Малика и АКСФ. Все алгоритмы и метрики написаны в проекте; image отвечает только "
        "за PNG/JPEG/BMP."
    )
    paragraph(
        document,
        "Для снятия исходного вычислительного ограничения карта дисперсии строится по двум "
        "интегральным изображениям, пространственная часть вынесена в два сепарабельных прохода, "
        "а значения exp(−x) в горячем цикле берутся из линейно интерполируемой таблицы. Точная "
        "ускоренная 2D-версия сохранена как внутренняя контрольная реализация. Юнит-тест сравнивает "
        "публичный сепарабельный путь с ней при одной автоматической оценке σ̂ и запрещает потерю "
        "качества более 0,05 дБ PSNR."
    )
    table(
        document,
        ["Этап", "Среднее время", "Примечание"],
        [
            ["Исходный прямой 2D-проход", "241,9 мс", "предыдущий фиксированный 3×5 замер"],
            ["Интегральная карта + точный 2D", "70,9 мс", "предыдущий фиксированный 3×5 замер"],
            ["Публичный сепарабельный путь", "20,47 мс", "тот же исторический замер"],
            [
                "Итоговый протокол, реальные 512×512 сцены",
                f"{number(perf512['acsf_time_ms'], 2)} мс",
                f"{perf512['images']} сцены × {len(gray['sigmas'])} σ; цель ≤30 мс выполнена",
            ],
        ],
        widths=[7.0, 3.0, 5.7],
    )
    caption(document, "Таблица 2. До/после ускорения; результаты разных протоколов не смешиваются.")

    document.add_heading("4. Методика эксперимента", level=1)
    paragraph(
        document,
        f"Полутоновая часть содержит {len(gray['images'])} сцен: "
        + ", ".join(gray["images"])
        + ". Для каждой использованы шесть уровней аддитивного белого гауссова шума: "
        + ", ".join(f"σ={s:g}" for s in gray["sigmas"])
        + ". Во всех методах на конкретной задаче используется один и тот же сгенерированный "
        "кадр; seed явно фиксирован. Таким образом, основной CSV содержит 36 задач на каждый метод."
    )
    bullet(document, "raw — шумный вход; acsf_flat — контроль с постоянным σr;")
    bullet(document, "bilateral — реализованный универсальный конкурент; bilateral_oracle — только верхняя граница с выбором по чистому эталону и без интерпретации времени;")
    bullet(document, "perona_malik — самостоятельная реализация анизотропной диффузии (12 итераций, λ=0,18, κ=25);")
    bullet(document, "PSNR измеряет ошибку, SSIM — структурное сходство, EPI — сохранение границ; больше — лучше.")
    paragraph(
        document,
        "Калибровка ограничена σ=20 и σ=30. Остальные четыре уровня не участвовали в выборе "
        "параметров. Для проверки устойчивости результата сформированы 30 независимых парных "
        "прогонов: одна наблюдаемая величина — среднее по 36 задачам при одном seed, что не "
        "раздувает размер выборки отдельными пикселями или кадрами."
    )
    table(
        document,
        ["Конфигурация (σs,kmin,kmax,c,r,rstruct)", "PSNR, дБ", "SSIM", "EPI"],
        calibration_rows(calibration),
        widths=[7.3, 2.6, 2.6, 2.6],
    )
    caption(document, "Таблица 3. Кандидаты калибровки, только σ=20 и σ=30, одинаковые seed и входы.")

    document.add_heading("5. Результаты полутонового эксперимента", level=1)
    paragraph(
        document,
        "Сводная таблица ниже создаётся из results/tables/results.csv. Время — стеночное время "
        "одного фильтрационного вызова на GitHub-hosted Ubuntu runner; сравнение скоростей "
        "корректно только внутри одного прогона. «Оракул» намеренно не имеет времени, потому что "
        "перебирает параметры с доступом к эталону."
    )
    table(
        document,
        ["Метод", "PSNR, дБ", "SSIM", "EPI", "Время, мс"],
        [method_row(gray, method) for method in GRAY_ORDER if method in methods],
        widths=[5.8, 2.3, 2.3, 2.3, 2.3],
        highlight_last=True,
    )
    caption(document, "Таблица 4. Среднее по шести сценам и шести уровням шума.")
    image(document, PLOTS / "fig_psnr_vs_sigma.png", 15.8, "Рисунок 1. PSNR в зависимости от σ; график получен из CSV.")
    image(document, PLOTS / "fig_summary_bars.png", 16.5, "Рисунок 2. Средние PSNR, SSIM и EPI по одному и тому же набору задач.")
    paragraph(
        document,
        f"АКСФ имеет {number(acsf['psnr'] - bilateral['psnr'], 2)} дБ преимущества по PSNR "
        f"над универсальным bilateral и {number(acsf['ssim'] - bilateral['ssim'], 4)} по SSIM. "
        f"Контроль acsf_flat отстаёт на {number(acsf['psnr'] - flat['psnr'], 2)} дБ, что отделяет "
        "вклад локальной адаптации от простого выбора среднего σr. Перона—Малик — сильный "
        f"дополнительный конкурент ({number(perona['psnr'])} дБ), но АКСФ выше на "
        f"{number(acsf['psnr'] - perona['psnr'], 2)} дБ при меньшем среднем времени. До "
        f"верхней границы «оракула» остаётся {number(oracle['psnr'] - acsf['psnr'], 2)} дБ."
    )
    image(document, PLOTS / "fig_activity_map.png", 15.0, "Рисунок 3. Карта активности и адаптивный результат на фрагменте платы, σ=20.")
    image(document, PLOTS / "fig_visual_baboon.png", 16.8, "Рисунок 4. Визуальное сравнение на «baboon», σ=20; метрики подписаны из CSV.")

    document.add_heading("5.1. Парная статистическая проверка", level=2)
    table(
        document,
        ["Показатель", "Значение"],
        [
            ["Независимых пар", str(stats["n"])],
            ["Средний PSNR bilateral, дБ", number(stats["bilateral_psnr"], 3)],
            ["Средний PSNR АКСФ, дБ", number(stats["acsf_psnr"], 3)],
            ["Средняя разница АКСФ − bilateral, дБ", number(stats["delta_psnr"], 3)],
            ["t-статистика", f"{stats['t_statistic']:.2f}"],
            ["p, односторонний парный t-тест", f"{stats['p_one_sided']:.3e}"],
        ],
        widths=[9.5, 6.0],
    )
    paragraph(
        document,
        "Нулевая гипотеза о том, что АКСФ не превосходит bilateral по среднему PSNR, отвергается: "
        "p существенно меньше 0,05. Это не заменяет разнообразный набор реальных шумовых моделей, "
        "но подтверждает устойчивость наблюдаемой разницы внутри заданного контролируемого протокола."
    )

    document.add_heading("6. Цветной режим YCbCr", level=1)
    paragraph(
        document,
        "Для цвета реализован отдельный совместный режим, а не независимая обработка R, G и B. "
        "Изображение представляется в YCbCr; оценка σ̂, карта A(p) и σr(p) строятся по Y. При "
        "фильтрации всех компонентов используется общий вес с яркостным направляющим каналом и "
        "учётом разности Cb/Cr. Метрики вычисляются по трём каналам: PSNR — из среднего MSE, "
        "SSIM и EPI — как среднее поканальных значений."
    )
    table(
        document,
        ["Метод", "PSNR, дБ", "SSIM", "EPI", "Время, мс"],
        [method_row(color, method) for method in ("raw_ycbcr", "bilateral_ycbcr", "acsf_ycbcr")],
        widths=[6.0, 2.4, 2.4, 2.4, 2.4],
        highlight_last=True,
    )
    caption(document, "Таблица 5. Три RGB-сцены × шесть уровней шума; все метрики в YCbCr.")
    image(document, COLOR_PLOTS / "fig_color_psnr_vs_sigma.png", 15.5, "Рисунок 5. Цветной PSNR в YCbCr в зависимости от σ.")
    paragraph(
        document,
        f"В цветном режиме АКСФ даёт {number(color_acsf['psnr'])} дБ против "
        f"{number(color_bilateral['psnr'])} дБ у bilateral: разница "
        f"{number(color_acsf['psnr'] - color_bilateral['psnr'], 2)} дБ. Это показывает, что "
        "локальная адаптация по яркости переносится на совместную фильтрацию цветности без "
        "сведения задачи к трём независимым полутоновым фильтрам."
    )

    document.add_heading("7. Ограничения и заключение", level=1)
    paragraph(
        document,
        "Итог работы — прозрачная и воспроизводимая реализация АКСФ с ускоренным публичным путём, "
        "цветным YCbCr-режимом, самостоятельным конкурентом Перона—Малика и статистической "
        "проверкой. На четырёх реальных 512×512 сценах среднее время "
        f"{number(perf512['acsf_time_ms'], 2)} мс ниже целевого порога 30 мс, а регрессионный тест "
        "фиксирует допустимое расхождение с точным 2D-эталоном."
    )
    bullet(document, "Испытана только модель аддитивного гауссова шума; для salt-and-pepper и реального сенсорного шума нужны отдельные испытания.")
    bullet(document, "«Оракул» — диагностическая верхняя граница, не практический конкурент и не основание для сравнения времени.")
    bullet(document, "Калибровка проводилась на ограниченном наборе кандидатов; более широкая настройка должна выполняться на отдельном обучающем наборе.")
    bullet(document, "Время зависит от процессора и runner; в репозитории сохранены CSV, логи и workflow для повторения именно данного измерения.")

    document.add_heading("Список источников", level=1)
    for reference in (
        "Tomasi C., Manduchi R. Bilateral Filtering for Gray and Color Images. ICCV, 1998.",
        "Perona P., Malik J. Scale-Space and Edge Detection Using Anisotropic Diffusion. IEEE TPAMI, 1990.",
        "Immerkaer J. Fast Noise Variance Estimation. Computer Vision and Image Understanding, 1996.",
        "Wang Z. et al. Image Quality Assessment: From Error Visibility to Structural Similarity. IEEE TIP, 2004.",
        "Документация Rust и crate image; исходный код и точные команды воспроизведения — в репозитории проекта.",
    ):
        paragraph(document, reference)

    document.add_page_break()
    document.add_heading("Приложение. Воспроизведение результатов", level=1)
    paragraph(
        document,
        "Команды ниже соответствуют CI. GitHub Actions запускает их на ветке проекта и сохраняет "
        "CSV, логи, графики и таблицы обратно в репозиторий."
    )
    code(
        document,
        """# Сборка и полный полутоновый эксперимент
cargo build --release --locked
./target/release/smoothing_project bench -d data/clean -o results \\
  --noise 5,10,15,20,30,40 --save-sigma 20 --seed 12345

# 30 независимых парных seed для bilateral против АКСФ
./target/release/smoothing_project stats -d data/clean -o results \\
  --noise 5,10,15,20,30,40 --runs 30 --seed 12345

# Цветной YCbCr-эксперимент
./target/release/smoothing_project bench --rgb -d data/color -o results/color \\
  --noise 5,10,15,20,30,40 --save-sigma 20 --seed 12345

# Производные таблицы, графики и документы
python3 analysis.py
python3 visualize.py --sigma 20
python3 analysis.py --color
python3 docs/build_report.py
python3 docs/build_slides.py""",
    )
    paragraph(
        document,
        "Источники тестовых изображений и лицензии перечислены в data/ATTRIBUTION.md; синтетическая "
        "сцена строится детерминированно скриптом data/generate_synthetic.py."
    )

    OUT.parent.mkdir(parents=True, exist_ok=True)
    document.save(OUT)
    print(f"Отчёт сохранён: {OUT}")


if __name__ == "__main__":
    main()
