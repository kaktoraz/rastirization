//! Набор фильтров сглаживания (шумоподавления).
//!
//! Все алгоритмы, кроме чтения и записи файлов, реализованы в проекте. Ядро
//! АКСФ сохраняет исходную формулу, но вычисляется быстрее: локальная
//! дисперсия строится по интегральным изображениям, а экспонента в горячем
//! цикле заменена линейно-интерполированной таблицей с ограниченной ошибкой.

use crate::img::{GrayF, YCbCrF};
use crate::metrics::estimate_noise_sigma;
use std::thread;

/// Аргументы экспоненты больше этого значения дают вес меньше 1.2e-7 и в
/// фильтре отбрасываются. Шаг таблицы 16/8192, максимальная ошибка линейной
/// интерполяции для exp(-x) меньше 5e-7.
const EXP_ARGUMENT_MAX: f32 = 16.0;
const EXP_LUT_STEPS: usize = 8192;

/// Быстрая аппроксимация exp(-x), предназначенная только для неотрицательных x.
struct ExpLut {
    values: Vec<f32>,
    scale: f32,
}

impl ExpLut {
    fn new() -> Self {
        let mut values = Vec::with_capacity(EXP_LUT_STEPS + 1);
        for i in 0..=EXP_LUT_STEPS {
            let x = EXP_ARGUMENT_MAX * i as f32 / EXP_LUT_STEPS as f32;
            values.push((-x).exp());
        }
        Self {
            values,
            scale: EXP_LUT_STEPS as f32 / EXP_ARGUMENT_MAX,
        }
    }

    #[inline]
    fn value(&self, x: f32) -> f32 {
        if x <= 0.0 {
            return 1.0;
        }
        if x >= EXP_ARGUMENT_MAX {
            return 0.0;
        }
        let pos = x * self.scale;
        let left = pos as usize;
        let fraction = pos - left as f32;
        self.values[left] + fraction * (self.values[left + 1] - self.values[left])
    }
}

/// Предвычисленные веса 2D гауссового пространственного ядра.
fn spatial_table(radius: usize, sigma_s: f32) -> Vec<f32> {
    let side = 2 * radius + 1;
    let mut table = vec![0.0; side * side];
    let denom = 2.0 * sigma_s.max(1e-3).powi(2);
    for y in 0..side {
        for x in 0..side {
            let dx = x as f32 - radius as f32;
            let dy = y as f32 - radius as f32;
            table[y * side + x] = (-(dx * dx + dy * dy) / denom).exp();
        }
    }
    table
}

/// Строит изображение с симметрично добавленной рамкой из ближайших пикселей.
/// После этого горячие циклы фильтров не вызывают `GrayF::get` и не проверяют
/// границы для каждого соседа.
fn padded_data(img: &GrayF, radius: usize) -> (Vec<f32>, usize) {
    let padded_w = img.w + 2 * radius;
    let padded_h = img.h + 2 * radius;
    let mut padded = vec![0.0; padded_w * padded_h];
    for py in 0..padded_h {
        let sy = py.saturating_sub(radius).min(img.h - 1);
        for px in 0..padded_w {
            let sx = px.saturating_sub(radius).min(img.w - 1);
            padded[py * padded_w + px] = img.data[sy * img.w + sx];
        }
    }
    (padded, padded_w)
}

/// Число потоков для независимой обработки строк. Для маленьких тестовых
/// картинок распараллеливание невыгодно; верхняя граница не даёт создавать
/// чрезмерно много короткоживущих потоков.
fn worker_count(width: usize, height: usize) -> usize {
    if width * height < 32 * 1024 {
        return 1;
    }
    thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(height)
        .min(8)
        .max(1)
}

// ---------------------------------------------------------------------------
// 1. Фильтр скользящего среднего
// ---------------------------------------------------------------------------
pub fn box_filter(img: &GrayF, k: usize) -> GrayF {
    assert!(k % 2 == 1, "размер окна должен быть нечётным");
    let radius = k / 2;
    let (padded, padded_w) = padded_data(img, radius);
    let padded_h = img.h + 2 * radius;
    let integral_w = padded_w + 1;
    let mut integral = vec![0.0f64; integral_w * (padded_h + 1)];
    for y in 0..padded_h {
        for x in 0..padded_w {
            let at = (y + 1) * integral_w + x + 1;
            integral[at] = padded[y * padded_w + x] as f64
                + integral[y * integral_w + x + 1]
                + integral[(y + 1) * integral_w + x]
                - integral[y * integral_w + x];
        }
    }
    let mut out = vec![0.0; img.data.len()];
    let area = (k * k) as f64;
    for y in 0..img.h {
        for x in 0..img.w {
            let right = x + k;
            let bottom = y + k;
            let sum = integral[bottom * integral_w + right]
                - integral[y * integral_w + right]
                - integral[bottom * integral_w + x]
                + integral[y * integral_w + x];
            out[y * img.w + x] = (sum / area) as f32;
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 2. Гауссов фильтр (точно разделяемая свёртка)
// ---------------------------------------------------------------------------
pub fn gaussian_filter(img: &GrayF, sigma_s: f32, radius: usize) -> GrayF {
    let side = 2 * radius + 1;
    let mut kernel = vec![0.0; side];
    let mut sum = 0.0;
    let denom = 2.0 * sigma_s.max(1e-3).powi(2);
    for i in 0..side {
        let d = i as f32 - radius as f32;
        let v = (-(d * d) / denom).exp();
        kernel[i] = v;
        sum += v;
    }
    for v in &mut kernel {
        *v /= sum;
    }

    // 2D гауссиан точно раскладывается на горизонтальное и вертикальное ядра.
    let mut horizontal = vec![0.0; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut acc = 0.0;
            for i in 0..side {
                acc += kernel[i] * img.get(x + i as i64 - radius as i64, y);
            }
            horizontal[y as usize * img.w + x as usize] = acc;
        }
    }
    let tmp = GrayF::new(img.w, img.h, horizontal);
    let mut out = vec![0.0; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut acc = 0.0;
            for i in 0..side {
                acc += kernel[i] * tmp.get(x, y + i as i64 - radius as i64);
            }
            out[y as usize * img.w + x as usize] = acc;
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 3. Медианный фильтр
// ---------------------------------------------------------------------------
pub fn median_filter(img: &GrayF, k: usize) -> GrayF {
    assert!(k % 2 == 1, "размер окна должен быть нечётным");
    let radius = (k / 2) as i64;
    let mut out = vec![0.0; img.data.len()];
    let mut values = vec![0.0; k * k];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut index = 0;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    values[index] = img.get(x + dx, y + dy);
                    index += 1;
                }
            }
            values.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out[y as usize * img.w + x as usize] = values[values.len() / 2];
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 4. Быстрый билатеральный фильтр с постоянным sigma_r
// ---------------------------------------------------------------------------

fn process_gray_rows(
    out: &mut [f32],
    start_y: usize,
    width: usize,
    side: usize,
    padded_w: usize,
    padded: &[f32],
    input: &[f32],
    sigma_r: &[f32],
    spatial: &[f32],
    exp_lut: &ExpLut,
) {
    for local_y in 0..(out.len() / width) {
        let y = start_y + local_y;
        let row = &mut out[local_y * width..(local_y + 1) * width];
        for x in 0..width {
            let index = y * width + x;
            let center = input[index];
            let inv_two_sigma_sq = 1.0 / (2.0 * sigma_r[index].max(1e-3).powi(2));
            let mut weighted_sum = 0.0;
            let mut weights = 0.0;
            let top_left = y * padded_w + x;
            for j in 0..side {
                let source_row = top_left + j * padded_w;
                let kernel_row = j * side;
                for i in 0..side {
                    let value = padded[source_row + i];
                    let difference = value - center;
                    let weight = spatial[kernel_row + i]
                        * exp_lut.value(difference * difference * inv_two_sigma_sq);
                    weights += weight;
                    weighted_sum += weight * value;
                }
            }
            row[x] = weighted_sum / weights;
        }
    }
}

fn bilateral_from_sigma_map(
    img: &GrayF,
    radius: usize,
    spatial: &[f32],
    sigma_r: &[f32],
    exp_lut: &ExpLut,
) -> GrayF {
    assert_eq!(img.data.len(), sigma_r.len());
    let (padded, padded_w) = padded_data(img, radius);
    let side = 2 * radius + 1;
    let mut out = vec![0.0; img.data.len()];
    let workers = worker_count(img.w, img.h);
    if workers == 1 {
        process_gray_rows(
            &mut out,
            0,
            img.w,
            side,
            padded_w,
            &padded,
            &img.data,
            sigma_r,
            spatial,
            exp_lut,
        );
    } else {
        let rows_per_chunk = img.h.div_ceil(workers);
        // Явные срезы позволяют потокам совместно читать одну рамку без
        // перемещения вектора в первую замкнутую функцию.
        let padded_ref: &[f32] = &padded;
        thread::scope(|scope| {
            for (chunk_index, out_chunk) in out.chunks_mut(rows_per_chunk * img.w).enumerate() {
                let start_y = chunk_index * rows_per_chunk;
                scope.spawn(move || {
                    process_gray_rows(
                        out_chunk,
                        start_y,
                        img.w,
                        side,
                        padded_w,
                        padded_ref,
                        &img.data,
                        sigma_r,
                        spatial,
                        exp_lut,
                    );
                });
            }
        });
    }
    GrayF::new(img.w, img.h, out)
}

/// Одномерное пространственное ядро. Два прохода дают точный 2D гауссиан в
/// отсутствии range-веса и быструю, общепринятую сепарабельную аппроксимацию
/// билатерального фильтра в общем случае.
fn spatial_line(radius: usize, sigma_s: f32) -> Vec<f32> {
    let side = 2 * radius + 1;
    let denom = 2.0 * sigma_s.max(1e-3).powi(2);
    (0..side)
        .map(|i| {
            let d = i as f32 - radius as f32;
            (-(d * d) / denom).exp()
        })
        .collect()
}

fn process_horizontal_rows(
    out: &mut [f32],
    start_y: usize,
    width: usize,
    radius: usize,
    padded_w: usize,
    padded: &[f32],
    input: &[f32],
    sigma_r: &[f32],
    line: &[f32],
    exp_lut: &ExpLut,
) {
    for local_y in 0..(out.len() / width) {
        let y = start_y + local_y;
        let row = &mut out[local_y * width..(local_y + 1) * width];
        for x in 0..width {
            let index = y * width + x;
            let center = input[index];
            let inv_two_sigma_sq = 1.0 / (2.0 * sigma_r[index].max(1e-3).powi(2));
            let source = (y + radius) * padded_w + x;
            let mut weighted_sum = 0.0;
            let mut weights = 0.0;
            for (i, &spatial) in line.iter().enumerate() {
                let value = padded[source + i];
                let difference = value - center;
                let weight = spatial * exp_lut.value(difference * difference * inv_two_sigma_sq);
                weights += weight;
                weighted_sum += weight * value;
            }
            row[x] = weighted_sum / weights;
        }
    }
}

fn process_vertical_rows(
    out: &mut [f32],
    start_y: usize,
    width: usize,
    radius: usize,
    padded_w: usize,
    padded_values: &[f32],
    padded_guidance: &[f32],
    input_guidance: &[f32],
    sigma_r: &[f32],
    line: &[f32],
    exp_lut: &ExpLut,
) {
    for local_y in 0..(out.len() / width) {
        let y = start_y + local_y;
        let row = &mut out[local_y * width..(local_y + 1) * width];
        for x in 0..width {
            let index = y * width + x;
            let center = input_guidance[index];
            let inv_two_sigma_sq = 1.0 / (2.0 * sigma_r[index].max(1e-3).powi(2));
            let source = y * padded_w + x + radius;
            let mut weighted_sum = 0.0;
            let mut weights = 0.0;
            for (j, &spatial) in line.iter().enumerate() {
                let at = source + j * padded_w;
                let difference = padded_guidance[at] - center;
                let weight = spatial * exp_lut.value(difference * difference * inv_two_sigma_sq);
                weights += weight;
                weighted_sum += weight * padded_values[at];
            }
            row[x] = weighted_sum / weights;
        }
    }
}

fn parallel_horizontal(
    img: &GrayF,
    radius: usize,
    sigma_r: &[f32],
    line: &[f32],
    exp_lut: &ExpLut,
) -> GrayF {
    let (padded, padded_w) = padded_data(img, radius);
    let mut out = vec![0.0; img.data.len()];
    let workers = worker_count(img.w, img.h);
    if workers == 1 {
        process_horizontal_rows(
            &mut out,
            0,
            img.w,
            radius,
            padded_w,
            &padded,
            &img.data,
            sigma_r,
            line,
            exp_lut,
        );
    } else {
        let rows_per_chunk = img.h.div_ceil(workers);
        let padded_ref: &[f32] = &padded;
        thread::scope(|scope| {
            for (chunk_index, out_chunk) in out.chunks_mut(rows_per_chunk * img.w).enumerate() {
                let start_y = chunk_index * rows_per_chunk;
                scope.spawn(move || {
                    process_horizontal_rows(
                        out_chunk,
                        start_y,
                        img.w,
                        radius,
                        padded_w,
                        padded_ref,
                        &img.data,
                        sigma_r,
                        line,
                        exp_lut,
                    );
                });
            }
        });
    }
    GrayF::new(img.w, img.h, out)
}

fn separable_bilateral_from_sigma_map(
    img: &GrayF,
    radius: usize,
    sigma_s: f32,
    sigma_r: &[f32],
    exp_lut: &ExpLut,
) -> GrayF {
    // Первый проход использует исходную яркость и sigma_r строки.
    let horizontal = parallel_horizontal(img, radius, sigma_r, &spatial_line(radius, sigma_s), exp_lut);

    // Во втором проходе значения берутся из горизонтально сглаженного кадра,
    // а range-весы — из исходной яркости. Поэтому границы определяются той же
    // наблюдаемой сценой, что и в исходной 2D формуле.
    let (padded_values, padded_w) = padded_data(&horizontal, radius);
    let (padded_guidance, guidance_w) = padded_data(img, radius);
    assert_eq!(padded_w, guidance_w);
    let line = spatial_line(radius, sigma_s);
    let mut out = vec![0.0; img.data.len()];
    let workers = worker_count(img.w, img.h);
    if workers == 1 {
        process_vertical_rows(
            &mut out,
            0,
            img.w,
            radius,
            padded_w,
            &padded_values,
            &padded_guidance,
            &img.data,
            sigma_r,
            &line,
            exp_lut,
        );
    } else {
        let rows_per_chunk = img.h.div_ceil(workers);
        let values_ref: &[f32] = &padded_values;
        let guidance_ref: &[f32] = &padded_guidance;
        let line_ref: &[f32] = &line;
        thread::scope(|scope| {
            for (chunk_index, out_chunk) in out.chunks_mut(rows_per_chunk * img.w).enumerate() {
                let start_y = chunk_index * rows_per_chunk;
                scope.spawn(move || {
                    process_vertical_rows(
                        out_chunk,
                        start_y,
                        img.w,
                        radius,
                        padded_w,
                        values_ref,
                        guidance_ref,
                        &img.data,
                        sigma_r,
                        line_ref,
                        exp_lut,
                    );
                });
            }
        });
    }
    GrayF::new(img.w, img.h, out)
}

/// Классический билатеральный фильтр.
///
/// w(p,q) = exp(-|p-q|²/(2 sigma_s²)) exp(-(I(p)-I(q))²/(2 sigma_r²)).
pub fn bilateral(img: &GrayF, sigma_s: f32, sigma_r: f32, radius: usize) -> GrayF {
    let spatial = spatial_table(radius, sigma_s);
    let exp_lut = ExpLut::new();
    let ranges = vec![sigma_r.max(1e-3); img.data.len()];
    bilateral_from_sigma_map(img, radius, &spatial, &ranges, &exp_lut)
}

// ---------------------------------------------------------------------------
// 5. Карта структурной активности через интегральные изображения
// ---------------------------------------------------------------------------

/// Шум-компенсированная локальная активность
/// A(p) = sqrt(max(0, V(p) - sigma_n²)), где V — несмещённая дисперсия в
/// окне `(2 * r_struct + 1)²`. Продление края совпадает с `GrayF::get`.
pub fn structural_activity(img: &GrayF, r_struct: usize, sigma_n: f64) -> Vec<f32> {
    let side = 2 * r_struct + 1;
    let count = (side * side) as f64;
    let (padded, padded_w) = padded_data(img, r_struct);
    let padded_h = img.h + 2 * r_struct;
    let integral_w = padded_w + 1;
    let mut sum = vec![0.0f64; integral_w * (padded_h + 1)];
    let mut sum_sq = vec![0.0f64; integral_w * (padded_h + 1)];

    for y in 0..padded_h {
        for x in 0..padded_w {
            let value = padded[y * padded_w + x] as f64;
            let at = (y + 1) * integral_w + x + 1;
            sum[at] = value + sum[y * integral_w + x + 1] + sum[(y + 1) * integral_w + x]
                - sum[y * integral_w + x];
            sum_sq[at] = value * value
                + sum_sq[y * integral_w + x + 1]
                + sum_sq[(y + 1) * integral_w + x]
                - sum_sq[y * integral_w + x];
        }
    }

    let mut activity = vec![0.0; img.data.len()];
    let noise_var = sigma_n * sigma_n;
    for y in 0..img.h {
        for x in 0..img.w {
            let right = x + side;
            let bottom = y + side;
            let s1 = sum[bottom * integral_w + right]
                - sum[y * integral_w + right]
                - sum[bottom * integral_w + x]
                + sum[y * integral_w + x];
            let s2 = sum_sq[bottom * integral_w + right]
                - sum_sq[y * integral_w + right]
                - sum_sq[bottom * integral_w + x]
                + sum_sq[y * integral_w + x];
            let variance = (s2 - s1 * s1 / count) / (count - 1.0);
            activity[y * img.w + x] = (variance - noise_var).max(0.0).sqrt() as f32;
        }
    }
    activity
}

// ---------------------------------------------------------------------------
// 6. АКСФ — адаптивный контрастно-структурный фильтр
// ---------------------------------------------------------------------------

/// Параметры АКСФ. Калиброванные значения по умолчанию намеренно не меняются.
#[derive(Clone, Copy, Debug)]
pub struct AcsfParams {
    pub sigma_s: f32,
    pub k_min: f32,
    pub k_max: f32,
    pub c: f32,
    pub radius: usize,
    pub radius_struct: usize,
}

impl Default for AcsfParams {
    fn default() -> Self {
        Self {
            sigma_s: 2.0,
            k_min: 0.7,
            k_max: 3.2,
            c: 1.2,
            radius: 5,
            radius_struct: 2,
        }
    }
}

/// Результат работы АКСФ вместе со служебной информацией.
pub struct AcsfResult {
    pub img: GrayF,
    pub sigma_n: f64,
    pub activity: Vec<f32>,
}

fn sigma_map(activity: &[f32], p: &AcsfParams, sigma_n: f64, exp_lut: &ExpLut) -> Vec<f32> {
    let sn = sigma_n.max(1.0) as f32;
    let denom = (p.c * sn).abs().max(1e-3);
    activity
        .iter()
        .map(|&a| {
            let t = (a / denom).powi(2);
            (sn * (p.k_min + (p.k_max - p.k_min) * exp_lut.value(t))).max(1e-3)
        })
        .collect()
}

/// Быстрая, но всё ещё 2D версия нужна для регрессионной проверки точности
/// LUT и интегральной карты активности. Основной АКСФ ниже использует
/// сепарабельную аппроксимацию именно пространственного прохода.
fn acsf_2d_at_known_sigma(noisy: &GrayF, p: &AcsfParams, sigma_n: f64) -> AcsfResult {
    let sigma_n = sigma_n.max(1.0);
    let activity = structural_activity(noisy, p.radius_struct, sigma_n);
    let exp_lut = ExpLut::new();
    let ranges = sigma_map(&activity, p, sigma_n, &exp_lut);
    let spatial = spatial_table(p.radius, p.sigma_s);
    let img = bilateral_from_sigma_map(noisy, p.radius, &spatial, &ranges, &exp_lut);
    AcsfResult { img, sigma_n, activity }
}

fn acsf_at_known_sigma(noisy: &GrayF, p: &AcsfParams, sigma_n: f64) -> AcsfResult {
    let sigma_n = sigma_n.max(1.0);
    let activity = structural_activity(noisy, p.radius_struct, sigma_n);
    let exp_lut = ExpLut::new();
    let ranges = sigma_map(&activity, p, sigma_n, &exp_lut);
    let img = separable_bilateral_from_sigma_map(noisy, p.radius, p.sigma_s, &ranges, &exp_lut);
    AcsfResult { img, sigma_n, activity }
}

/// АКСФ с автоматической оценкой уровня шума по Иммеркеру.
pub fn acsf(noisy: &GrayF, p: &AcsfParams) -> AcsfResult {
    acsf_at_known_sigma(noisy, p, estimate_noise_sigma(noisy))
}

/// Вариант АКСФ с известным уровнем шума, используемый в анализе чувствительности.
pub fn acsf_with_sigma(noisy: &GrayF, p: &AcsfParams, sigma_n: f64) -> AcsfResult {
    acsf_at_known_sigma(noisy, p, sigma_n)
}

// ---------------------------------------------------------------------------
// 7. Перона—Малик: честный дополнительный конкурент
// ---------------------------------------------------------------------------

/// Анизотропная диффузия Перона—Малика с функцией проводимости
/// c(d)=exp(-(d/kappa)²). `lambda` автоматически ограничивается 0.25 —
/// условием устойчивости явной четырёхсвязной схемы.
pub fn perona_malik(img: &GrayF, iterations: usize, lambda: f32, kappa: f32) -> GrayF {
    let lambda = lambda.clamp(0.0, 0.25);
    let kappa_sq = kappa.max(1e-3).powi(2);
    let exp_lut = ExpLut::new();
    let mut current = img.clone();
    for _ in 0..iterations {
        let mut next = vec![0.0; current.data.len()];
        for y in 0..current.h as i64 {
            for x in 0..current.w as i64 {
                let center = current.get(x, y);
                let north = current.get(x, y - 1) - center;
                let south = current.get(x, y + 1) - center;
                let west = current.get(x - 1, y) - center;
                let east = current.get(x + 1, y) - center;
                let flow = exp_lut.value(north * north / kappa_sq) * north
                    + exp_lut.value(south * south / kappa_sq) * south
                    + exp_lut.value(west * west / kappa_sq) * west
                    + exp_lut.value(east * east / kappa_sq) * east;
                next[y as usize * current.w + x as usize] = center + lambda * flow;
            }
        }
        current = GrayF::new(current.w, current.h, next);
    }
    current
}

// ---------------------------------------------------------------------------
// 8. Цветной АКСФ в YCbCr
// ---------------------------------------------------------------------------

/// Результат цветного АКСФ. Карта активности всегда относится к яркости Y.
pub struct ColorAcsfResult {
    pub img: YCbCrF,
    pub sigma_n: f64,
    pub activity: Vec<f32>,
}

fn process_ycbcr_rows(
    out: &mut [[f32; 3]],
    start_y: usize,
    width: usize,
    side: usize,
    padded_w: usize,
    padded_y: &[f32],
    padded_cb: &[f32],
    padded_cr: &[f32],
    input: &YCbCrF,
    sigma_r_y: &[f32],
    chroma_factor: f32,
    spatial: &[f32],
    exp_lut: &ExpLut,
) {
    let chroma_factor_sq = chroma_factor.max(1e-3).powi(2);
    for local_y in 0..(out.len() / width) {
        let y = start_y + local_y;
        let row = &mut out[local_y * width..(local_y + 1) * width];
        for x in 0..width {
            let index = y * width + x;
            let center_y = input.y.data[index];
            let center_cb = input.cb.data[index];
            let center_cr = input.cr.data[index];
            let inv_two_sigma_y_sq = 1.0 / (2.0 * sigma_r_y[index].max(1e-3).powi(2));
            let mut weights = 0.0;
            let mut acc_y = 0.0;
            let mut acc_cb = 0.0;
            let mut acc_cr = 0.0;
            let top_left = y * padded_w + x;
            for j in 0..side {
                let source_row = top_left + j * padded_w;
                let kernel_row = j * side;
                for i in 0..side {
                    let at = source_row + i;
                    let yy = padded_y[at];
                    let cb = padded_cb[at];
                    let cr = padded_cr[at];
                    let dy = yy - center_y;
                    let dcb = cb - center_cb;
                    let dcr = cr - center_cr;
                    // Совместный цветовой вес: адаптивная яркость + цветность.
                    let range = (dy * dy + (dcb * dcb + dcr * dcr) / chroma_factor_sq)
                        * inv_two_sigma_y_sq;
                    let weight = spatial[kernel_row + i] * exp_lut.value(range);
                    weights += weight;
                    acc_y += weight * yy;
                    acc_cb += weight * cb;
                    acc_cr += weight * cr;
                }
            }
            row[x] = [acc_y / weights, acc_cb / weights, acc_cr / weights];
        }
    }
}

fn ycbcr_from_sigma_map(
    img: &YCbCrF,
    radius: usize,
    spatial: &[f32],
    sigma_r_y: &[f32],
    chroma_factor: f32,
    exp_lut: &ExpLut,
) -> YCbCrF {
    assert_eq!(img.y.data.len(), sigma_r_y.len());
    let (padded_y, padded_w) = padded_data(&img.y, radius);
    let (padded_cb, cb_w) = padded_data(&img.cb, radius);
    let (padded_cr, cr_w) = padded_data(&img.cr, radius);
    assert_eq!(padded_w, cb_w);
    assert_eq!(padded_w, cr_w);
    let side = 2 * radius + 1;
    let mut out = vec![[0.0; 3]; img.y.data.len()];
    let workers = worker_count(img.w(), img.h());
    if workers == 1 {
        process_ycbcr_rows(
            &mut out,
            0,
            img.w(),
            side,
            padded_w,
            &padded_y,
            &padded_cb,
            &padded_cr,
            img,
            sigma_r_y,
            chroma_factor,
            spatial,
            exp_lut,
        );
    } else {
        let rows_per_chunk = img.h().div_ceil(workers);
        let padded_y_ref: &[f32] = &padded_y;
        let padded_cb_ref: &[f32] = &padded_cb;
        let padded_cr_ref: &[f32] = &padded_cr;
        thread::scope(|scope| {
            for (chunk_index, out_chunk) in out.chunks_mut(rows_per_chunk * img.w()).enumerate() {
                let start_y = chunk_index * rows_per_chunk;
                scope.spawn(move || {
                    process_ycbcr_rows(
                        out_chunk,
                        start_y,
                        img.w(),
                        side,
                        padded_w,
                        padded_y_ref,
                        padded_cb_ref,
                        padded_cr_ref,
                        img,
                        sigma_r_y,
                        chroma_factor,
                        spatial,
                        exp_lut,
                    );
                });
            }
        });
    }
    let mut y = Vec::with_capacity(out.len());
    let mut cb = Vec::with_capacity(out.len());
    let mut cr = Vec::with_capacity(out.len());
    for pixel in out {
        y.push(pixel[0]);
        cb.push(pixel[1]);
        cr.push(pixel[2]);
    }
    YCbCrF::new(
        GrayF::new(img.w(), img.h(), y),
        GrayF::new(img.w(), img.h(), cb),
        GrayF::new(img.w(), img.h(), cr),
    )
}

/// Совместный билатеральный фильтр для цвета с постоянным sigma_r яркости.
pub fn bilateral_ycbcr(
    noisy: &YCbCrF,
    sigma_s: f32,
    sigma_r_y: f32,
    radius: usize,
) -> YCbCrF {
    let exp_lut = ExpLut::new();
    let ranges = vec![sigma_r_y.max(1e-3); noisy.y.data.len()];
    let spatial = spatial_table(radius, sigma_s);
    ycbcr_from_sigma_map(noisy, radius, &spatial, &ranges, 1.5, &exp_lut)
}

/// Цветной вариант АКСФ: sigma_r(p) и карта активности вычисляются только по
/// Y; все три канала фильтруются совместным YCbCr-весом.
pub fn acsf_ycbcr(noisy: &YCbCrF, p: &AcsfParams) -> ColorAcsfResult {
    let sigma_n = estimate_noise_sigma(&noisy.y).max(1.0);
    let activity = structural_activity(&noisy.y, p.radius_struct, sigma_n);
    let exp_lut = ExpLut::new();
    let ranges = sigma_map(&activity, p, sigma_n, &exp_lut);
    let spatial = spatial_table(p.radius, p.sigma_s);
    let img = ycbcr_from_sigma_map(noisy, p.radius, &spatial, &ranges, 1.5, &exp_lut);
    ColorAcsfResult {
        img,
        sigma_n,
        activity,
    }
}

/// Нормирует карту активности для сохранения как визуализации.
pub fn activity_to_image(activity: &[f32], width: usize, height: usize) -> GrayF {
    let max = activity.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    let data = activity
        .iter()
        .map(|&v| (v / max * 255.0).min(255.0))
        .collect();
    GrayF::new(width, height, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::psnr;
    use crate::noise::add_gaussian_noise;

    fn synthetic_image(width: usize, height: usize) -> GrayF {
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let gradient = 35.0 + 180.0 * x as f32 / (width - 1) as f32;
                let circle = if ((x as f32 - width as f32 * 0.62).powi(2)
                    + (y as f32 - height as f32 * 0.45).powi(2))
                    .sqrt()
                    < width as f32 * 0.18
                {
                    35.0
                } else {
                    0.0
                };
                let texture = 10.0 * (x as f32 * 0.34).sin() * (y as f32 * 0.21).cos();
                data.push((gradient + circle + texture).clamp(0.0, 255.0));
            }
        }
        GrayF::new(width, height, data)
    }

    fn direct_activity(img: &GrayF, radius: usize, sigma: f64) -> Vec<f32> {
        let side = 2 * radius + 1;
        let count = (side * side) as f64;
        let mut out = vec![0.0; img.data.len()];
        for y in 0..img.h as i64 {
            for x in 0..img.w as i64 {
                let mut sum = 0.0f64;
                let mut sum_sq = 0.0f64;
                for dy in -(radius as i64)..=(radius as i64) {
                    for dx in -(radius as i64)..=(radius as i64) {
                        let value = img.get(x + dx, y + dy) as f64;
                        sum += value;
                        sum_sq += value * value;
                    }
                }
                let variance = (sum_sq - sum * sum / count) / (count - 1.0);
                out[y as usize * img.w + x as usize] = (variance - sigma * sigma).max(0.0).sqrt() as f32;
            }
        }
        out
    }

    fn exact_acsf_with_sigma(noisy: &GrayF, p: &AcsfParams, sigma: f64) -> GrayF {
        let activity = direct_activity(noisy, p.radius_struct, sigma);
        let spatial = spatial_table(p.radius, p.sigma_s);
        let side = 2 * p.radius + 1;
        let mut out = vec![0.0; noisy.data.len()];
        let sn = sigma as f32;
        for y in 0..noisy.h as i64 {
            for x in 0..noisy.w as i64 {
                let index = y as usize * noisy.w + x as usize;
                let a = activity[index];
                let t = (a / (p.c * sn)).powi(2);
                let sigma_r = (sn * (p.k_min + (p.k_max - p.k_min) * (-t).exp())).max(1e-3);
                let inv_two_sigma_sq = 1.0 / (2.0 * sigma_r * sigma_r);
                let center = noisy.get(x, y);
                let mut weighted_sum = 0.0;
                let mut weights = 0.0;
                for j in 0..side {
                    for i in 0..side {
                        let value = noisy.get(
                            x + i as i64 - p.radius as i64,
                            y + j as i64 - p.radius as i64,
                        );
                        let d = value - center;
                        let weight = spatial[j * side + i] * (-(d * d) * inv_two_sigma_sq).exp();
                        weighted_sum += weight * value;
                        weights += weight;
                    }
                }
                out[index] = weighted_sum / weights;
            }
        }
        GrayF::new(noisy.w, noisy.h, out)
    }

    #[test]
    fn integral_activity_matches_direct_window_calculation() {
        let img = synthetic_image(31, 27);
        let fast = structural_activity(&img, 2, 7.5);
        let direct = direct_activity(&img, 2, 7.5);
        let max_difference = fast
            .iter()
            .zip(&direct)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_difference < 1e-3, "max difference = {max_difference}");
    }

    #[test]
    fn exp_lut_has_small_interpolation_error() {
        let lut = ExpLut::new();
        for i in 0..1000 {
            let x = EXP_ARGUMENT_MAX * i as f32 / 999.0;
            assert!((lut.value(x) - (-x).exp()).abs() < 1e-5);
        }
    }

    #[test]
    fn optimized_acsf_matches_exact_reference() {
        let clean = synthetic_image(56, 48);
        let noisy = add_gaussian_noise(&clean, 20.0, 12345);
        let p = AcsfParams::default();
        let exact = exact_acsf_with_sigma(&noisy, &p, 20.0);
        let fast_2d = acsf_2d_at_known_sigma(&noisy, &p, 20.0).img;
        assert!(
            psnr(&exact, &fast_2d, 255.0) > 60.0,
            "ускоренная 2D-версия заметно отличается от точного эталона"
        );
    }

    #[test]
    fn acsf_keeps_constant_image_constant() {
        let input = GrayF::new(40, 30, vec![127.0; 40 * 30]);
        let out = acsf(&input, &AcsfParams::default()).img;
        assert!(out.data.iter().all(|&value| (value - 127.0).abs() < 1e-4));
    }

    #[test]
    fn perona_malik_keeps_constant_image_constant() {
        let input = GrayF::new(21, 19, vec![91.0; 21 * 19]);
        let out = perona_malik(&input, 12, 0.2, 25.0);
        assert!(out.data.iter().all(|&value| (value - 91.0).abs() < 1e-4));
    }

    #[test]
    fn color_acsf_returns_finite_channels() {
        let y = synthetic_image(24, 20);
        let cb = GrayF::new(24, 20, (0..480).map(|i| 105.0 + (i % 9) as f32).collect());
        let cr = GrayF::new(24, 20, (0..480).map(|i| 150.0 - (i % 7) as f32).collect());
        let input = YCbCrF::new(y, cb, cr);
        let result = acsf_ycbcr(&input, &AcsfParams::default());
        assert_eq!((result.img.w(), result.img.h()), (24, 20));
        assert!(result.img.y.data.iter().all(|x| x.is_finite()));
        assert!(result.img.cb.data.iter().all(|x| x.is_finite()));
        assert!(result.img.cr.data.iter().all(|x| x.is_finite()));
    }
}
