//! Метрики качества восстановления изображения.
//!
//! Используются:
//!  * MSE  — средний квадрат ошибки;
//!  * PSNR — пиковое отношение сигнал/шум (дБ);
//!  * SSIM — индекс структурного сходства (Wang et al., 2004);
//!  * EPI  — индекс сохранения краёв (корреляция высокочастотных составляющих).

use crate::img::GrayF;
use std::f32::consts::PI;

/// Средний квадрат ошибки между изображениями a и b.
pub fn mse(a: &GrayF, b: &GrayF) -> f64 {
    assert_eq!(a.data.len(), b.data.len());
    let n = a.data.len() as f64;
    let s: f64 = a
        .data
        .iter()
        .zip(&b.data)
        .map(|(&x, &y)| {
            let d = x as f64 - y as f64;
            d * d
        })
        .sum();
    s / n
}

/// Пиковое отношение сигнал/шум, дБ. Для совпадающих изображений возвращается бесконечность.
pub fn psnr(a: &GrayF, b: &GrayF, peak: f64) -> f64 {
    let e = mse(a, b);
    if e <= 1e-12 {
        f64::INFINITY
    } else {
        10.0 * (peak * peak / e).log10()
    }
}

/// Гауссово ядро размера (2r+1) x (2r+1), нормированное к сумме 1.
fn gaussian_kernel(r: usize, sigma: f32) -> Vec<f32> {
    let n = 2 * r + 1;
    let mut k = vec![0f32; n * n];
    let mut sum = 0f32;
    for j in 0..n {
        for i in 0..n {
            let dx = i as f32 - r as f32;
            let dy = j as f32 - r as f32;
            let v = (-(dx * dx + dy * dy) / (2.0 * sigma * sigma)).exp();
            k[j * n + i] = v;
            sum += v;
        }
    }
    for v in k.iter_mut() {
        *v /= sum;
    }
    k
}

/// Свёртка изображения с ядром (края — продлением).
fn convolve(img: &GrayF, kernel: &[f32], r: usize) -> Vec<f32> {
    let n = 2 * r + 1;
    let mut out = vec![0f32; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut acc = 0f32;
            for j in 0..n {
                for i in 0..n {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    acc += kernel[j * n + i] * img.get(x + dx, y + dy);
                }
            }
            out[y as usize * img.w + x as usize] = acc;
        }
    }
    out
}

/// Индекс структурного сходства SSIM.
///
/// Стандартная схема: окно 7x7 с гауссовым взвешиванием (sigma = 1.5),
/// константы C1 = (K1*L)^2, C2 = (K2*L)^2 при K1 = 0.01, K2 = 0.03, L = 255.
/// Итоговое значение — среднее локальных SSIM по всем пикселям.
/// Одинаковая реализация используется для всех сравниваемых методов.
pub fn ssim(a: &GrayF, b: &GrayF) -> f64 {
    assert_eq!((a.w, a.h), (b.w, b.h));
    const L: f64 = 255.0;
    const K1: f64 = 0.01;
    const K2: f64 = 0.03;
    let c1 = (K1 * L) * (K1 * L);
    let c2 = (K2 * L) * (K2 * L);

    let r = 3usize; // окно 7x7
    let ker = gaussian_kernel(r, 1.5);

    // Локальные взвешенные средние
    let ma = convolve(a, &ker, r);
    let mb = convolve(b, &ker, r);

    let w = a.w;
    let mut total = 0f64;
    let mut count = 0f64;

    for y in 0..a.h as i64 {
        for x in 0..a.w as i64 {
            // Взвешенные дисперсии и ковариация
            let mut va = 0f64;
            let mut vb = 0f64;
            let mut cov = 0f64;
            for j in 0..(2 * r + 1) {
                for i in 0..(2 * r + 1) {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    let wgt = ker[j * (2 * r + 1) + i] as f64;
                    let xa = a.get(x + dx, y + dy) as f64
                        - ma[(y + dy).clamp(0, a.h as i64 - 1) as usize * w
                            + (x + dx).clamp(0, a.w as i64 - 1) as usize]
                            as f64;
                    let xb = b.get(x + dx, y + dy) as f64
                        - mb[(y + dy).clamp(0, b.h as i64 - 1) as usize * w
                            + (x + dx).clamp(0, b.w as i64 - 1) as usize]
                            as f64;
                    va += wgt * xa * xa;
                    vb += wgt * xb * xb;
                    cov += wgt * xa * xb;
                }
            }
            let mua = ma[y as usize * w + x as usize] as f64;
            let mub = mb[y as usize * w + x as usize] as f64;
            let s = ((2.0 * mua * mub + c1) * (2.0 * cov + c2))
                / ((mua * mua + mub * mub + c1) * (va + vb + c2));
            total += s;
            count += 1.0;
        }
    }
    total / count
}

/// Высокочастотная составляющая: результат фильтра Лапласа 3x3.
fn laplacian(img: &GrayF) -> Vec<f32> {
    let mut out = vec![0f32; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let v = 4.0 * img.get(x, y)
                - img.get(x - 1, y)
                - img.get(x + 1, y)
                - img.get(x, y - 1)
                - img.get(x, y + 1);
            out[y as usize * img.w + x as usize] = v;
        }
    }
    out
}

/// Индекс сохранения краёв (Edge Preservation Index, по мотивам Sattar et al.).
/// Корреляция Пирсона между высокочастотными составляющими
/// восстановленного и эталонного изображений. Чем ближе к 1 — тем лучше
/// сохранены границы и мелкие детали при удалении шума.
pub fn epi(clean: &GrayF, restored: &GrayF) -> f64 {
    let lc = laplacian(clean);
    let lr = laplacian(restored);
    let n = lc.len() as f64;
    let mc = lc.iter().map(|&v| v as f64).sum::<f64>() / n;
    let mr = lr.iter().map(|&v| v as f64).sum::<f64>() / n;
    let mut num = 0f64;
    let mut den1 = 0f64;
    let mut den2 = 0f64;
    for (a, b) in lc.iter().zip(&lr) {
        let da = *a as f64 - mc;
        let db = *b as f64 - mr;
        num += da * db;
        den1 += da * da;
        den2 += db * db;
    }
    if den1 < 1e-12 || den2 < 1e-12 {
        return 0.0;
    }
    num / (den1.sqrt() * den2.sqrt())
}

/// Оценка уровня шума по методу Иммеркера (Immerkær, 1996).
///
/// Среднее абсолютное значение отклика маски Лапласа
///     L = [1 -2 1; -2 4 -2; 1 -2 1]
/// связано со среднеквадратичным отклонением шума соотношением
///     sigma = sqrt(pi/2) * sum|I * L| / (6 * (W-2) * (H-2)).
/// Метод не требует «чистого» эталона — это важно для работы с реальными
/// фотографиями, где эталон неизвестен.
pub fn estimate_noise_sigma(img: &GrayF) -> f64 {
    let mut acc = 0f64;
    for y in 1..img.h as i64 - 1 {
        for x in 1..img.w as i64 - 1 {
            let r = img.get(x - 1, y - 1) - 2.0 * img.get(x, y - 1) + img.get(x + 1, y - 1)
                - 2.0 * img.get(x - 1, y)
                + 4.0 * img.get(x, y)
                - 2.0 * img.get(x + 1, y)
                + img.get(x - 1, y + 1)
                - 2.0 * img.get(x, y + 1)
                + img.get(x + 1, y + 1);
            acc += (r as f64).abs();
        }
    }
    let n = ((img.w - 2) * (img.h - 2)) as f64;
    (PI as f64 / 2.0).sqrt() * acc / (6.0 * n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_have_infinite_psnr() {
        let image = GrayF::new(3, 3, vec![42.0; 9]);
        assert!(psnr(&image, &image, 255.0).is_infinite());
    }

    #[test]
    fn constant_image_has_zero_noise_estimate() {
        let image = GrayF::new(12, 10, vec![123.0; 120]);
        assert!(estimate_noise_sigma(&image).abs() < 1e-12);
    }

    #[test]
    fn epi_is_one_for_an_identical_nonconstant_image() {
        let image = GrayF::new(5, 5, (0..25).map(|i| ((i * 17) % 251) as f32).collect());
        assert!((epi(&image, &image) - 1.0).abs() < 1e-10);
    }
}
