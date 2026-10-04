//! Набор фильтров сглаживания (шумоподавления) для полутоновых изображений.
//!
//! Реализованы:
//!  * фильтр среднего (box);
//!  * гауссов фильтр;
//!  * медианный фильтр;
//!  * билатеральный фильтр (классический, с постоянными параметрами);
//!  * АКСФ — адаптивный контрастно-структурный фильтр (собственный метод проекта).
//!
//! Все функции принимают и возвращают GrayF, не изменяя вход.

use crate::img::GrayF;
use crate::metrics::estimate_noise_sigma;

/// Предвычисленная таблица пространственных весов гауссова/билатерального ядра.
fn spatial_table(r: usize, sigma_s: f32) -> Vec<f32> {
    let n = 2 * r + 1;
    let mut t = vec![0f32; n * n];
    for j in 0..n {
        for i in 0..n {
            let dx = i as f32 - r as f32;
            let dy = j as f32 - r as f32;
            t[j * n + i] = (-(dx * dx + dy * dy) / (2.0 * sigma_s * sigma_s)).exp();
        }
    }
    t
}

// ---------------------------------------------------------------------------
// 1. Фильтр скользящего среднего (box filter), окно k x k
// ---------------------------------------------------------------------------
pub fn box_filter(img: &GrayF, k: usize) -> GrayF {
    assert!(k % 2 == 1, "размер окна должен быть нечётным");
    let r = (k / 2) as i64;
    let mut out = vec![0f32; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut s = 0f32;
            for dy in -r..=r {
                for dx in -r..=r {
                    s += img.get(x + dx, y + dy);
                }
            }
            out[(y as usize) * img.w + x as usize] = s / (k * k) as f32;
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 2. Гауссов фильтр
// ---------------------------------------------------------------------------
pub fn gaussian_filter(img: &GrayF, sigma_s: f32, r: usize) -> GrayF {
    let n = 2 * r + 1;
    let t = spatial_table(r, sigma_s);
    // нормировка ядра
    let sum: f32 = t.iter().sum();
    let mut out = vec![0f32; img.data.len()];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut acc = 0f32;
            for j in 0..n {
                for i in 0..n {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    acc += t[j * n + i] * img.get(x + dx, y + dy);
                }
            }
            out[(y as usize) * img.w + x as usize] = acc / sum;
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 3. Медианный фильтр, окно k x k
// ---------------------------------------------------------------------------
pub fn median_filter(img: &GrayF, k: usize) -> GrayF {
    assert!(k % 2 == 1, "размер окна должен быть нечётным");
    let r = (k / 2) as i64;
    let mut out = vec![0f32; img.data.len()];
    let mut buf = vec![0f32; k * k];
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut idx = 0;
            for dy in -r..=r {
                for dx in -r..=r {
                    buf[idx] = img.get(x + dx, y + dy);
                    idx += 1;
                }
            }
            // частичная сортировка: нам нужен только средний элемент
            buf.sort_by(|a, b| a.partial_cmp(b).unwrap());
            out[(y as usize) * img.w + x as usize] = buf[buf.len() / 2];
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 4. Классический билатеральный фильтр
//
//    w(p, q) = exp( -|p-q|^2 / (2*sigma_s^2) ) * exp( -(I(p)-I(q))^2 / (2*sigma_r^2) )
//
//    Пространственный вес отвечает за «геометрическую близость»,
//    весовой вес (range weight) — за похожесть яркости: пиксели с сильно
//    отличающейся яркостью (то есть границы объектов) почти не усредняются.
// ---------------------------------------------------------------------------
pub fn bilateral(img: &GrayF, sigma_s: f32, sigma_r: f32, r: usize) -> GrayF {
    let n = 2 * r + 1;
    let t = spatial_table(r, sigma_s);
    let mut out = vec![0f32; img.data.len()];
    let inv2sr2 = 1.0 / (2.0 * sigma_r * sigma_r);
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let center = img.get(x, y);
            let mut wsum = 0f32;
            let mut acc = 0f32;
            for j in 0..n {
                for i in 0..n {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    let v = img.get(x + dx, y + dy);
                    let d = v - center;
                    let w = t[j * n + i] * (-(d * d) * inv2sr2).exp();
                    wsum += w;
                    acc += w * v;
                }
            }
            out[(y as usize) * img.w + x as usize] = acc / wsum;
        }
    }
    GrayF::new(img.w, img.h, out)
}

// ---------------------------------------------------------------------------
// 5. Оценка локальной структурной активности
//
//    Для каждого пикселя в окне (2*r+1)^2 считается выборочная (несмещённая)
//    дисперсия. Из неё вычитается дисперсия шума, известная по его глобальной
//    оценке. Получается оценка дисперсии «полезного сигнала»:
//
//        A(p) = sqrt( max(0, var_local(p) - sigma_n^2) )
//
//    A(p) ~ 0  — гладкая область (виден только шум);
//    A(p) >> 0 — контур, текстура, мелкая деталь.
// ---------------------------------------------------------------------------
pub fn structural_activity(img: &GrayF, r_struct: usize, sigma_n: f64) -> Vec<f32> {
    let n = (2 * r_struct + 1) as f64;
    let n_pix = n * n;
    let mut out = vec![0f32; img.data.len()];
    let rs = r_struct as i64;
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut s1 = 0f64;
            let mut s2 = 0f64;
            for dy in -rs..=rs {
                for dx in -rs..=rs {
                    let v = img.get(x + dx, y + dy) as f64;
                    s1 += v;
                    s2 += v * v;
                }
            }
            let var = (s2 - s1 * s1 / n_pix) / (n_pix - 1.0); // несмещённая оценка
            let a = (var - sigma_n * sigma_n).max(0.0).sqrt();
            out[(y as usize) * img.w + x as usize] = a as f32;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 6. АКСФ — адаптивный контрастно-структурный фильтр (собственный метод)
//
//    Идея. В классическом билатеральном фильтре параметр sigma_r постоянен
//    для всего изображения, поэтому приходится выбирать компромисс:
//     * маленький sigma_r — в гладких областях шум остаётся;
//     * большой sigma_r  — сглаживаются контуры и мелкие детали.
//
//    В АКСФ параметр sigma_r вычисляется для КАЖДОГО пикселя по карте
//    структурной активности A(p):
//
//        sigma_r(p) = sigma_n * ( k_min + (k_max - k_min) *
//                                 exp( -(A(p) / (c * sigma_n))^2 ) )
//
//     * в гладких областях (A ~ 0):  sigma_r -> k_max * sigma_n  (сглаживаем смело,
//       шум с амплитудой порядка sigma_n уверенно подавляется);
//     * на контурах и текстурах (A >> sigma_n): sigma_r -> k_min * sigma_n
//       (фильтр становится «осторожным» и почти не размывает структуру).
//
//    Ключевые свойства метода:
//     1) параметры k_min, k_max, c — универсальные и не подбираются заново
//        под каждое изображение или уровень шума;
//     2) уровень шума sigma_n оценивается автоматически по самому изображению
//        (оценка Иммеркера), то есть метод самонастраивается;
//     3) карта A(p) очищена от вклада шума (шум-компенсация), поэтому
//        адаптация не «обманывается» шумом в гладких областях.
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct AcsfParams {
    pub sigma_s: f32, // пространственный масштаб
    pub k_min: f32,   // нижний коэффициент sigma_r (структурные области)
    pub k_max: f32,   // верхний коэффициент sigma_r (гладкие области)
    pub c: f32,       // ширина зоны перехода между режимами (в единицах sigma_n)
    pub radius: usize,        // радиус окна фильтрации
    pub radius_struct: usize, // радиус окна оценки структурной активности
}

impl Default for AcsfParams {
    /// Параметры по умолчанию подобраны в эксперименте по калибровке
    /// (см. отчёт, раздел «Калибровка параметров»).
    fn default() -> Self {
        Self { sigma_s: 2.0, k_min: 0.7, k_max: 3.2, c: 1.2, radius: 5, radius_struct: 2 }
    }
}

/// Результат работы АКСФ вместе со служебной информацией.
pub struct AcsfResult {
    pub img: GrayF,
    pub sigma_n: f64,       // автоматическая оценка уровня шума
    pub activity: Vec<f32>, // карта структурной активности A(p)
}

pub fn acsf(noisy: &GrayF, p: &AcsfParams) -> AcsfResult {
    // Шаг 1: автоматическая оценка уровня шума по изображению.
    let sigma_n = estimate_noise_sigma(noisy).max(1.0);

    // Шаг 2: карта структурной активности (шум-компенсированная).
    let activity = structural_activity(noisy, p.radius_struct, sigma_n);

    // Шаг 3: фильтрация с адаптивным sigma_r(p).
    let r = p.radius;
    let n = 2 * r + 1;
    let t = spatial_table(r, p.sigma_s);
    let mut out = vec![0f32; noisy.data.len()];
    let sn = sigma_n as f32;
    for y in 0..noisy.h as i64 {
        for x in 0..noisy.w as i64 {
            let center = noisy.get(x, y);
            let idx = (y as usize) * noisy.w + x as usize;
            let a = activity[idx];
            // адаптивный диапазонный параметр
            let t_ = (a / (p.c * sn)).powi(2);
            let sigma_r = (sn * (p.k_min + (p.k_max - p.k_min) * (-t_).exp())).max(1e-3);
            let inv2sr2 = 1.0 / (2.0 * sigma_r * sigma_r);
            let mut wsum = 0f32;
            let mut acc = 0f32;
            for j in 0..n {
                for i in 0..n {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    let v = noisy.get(x + dx, y + dy);
                    let d = v - center;
                    let w = t[j * n + i] * (-(d * d) * inv2sr2).exp();
                    wsum += w;
                    acc += w * v;
                }
            }
            out[idx] = acc / wsum;
        }
    }
    AcsfResult { img: GrayF::new(noisy.w, noisy.h, out), sigma_n, activity }
}

/// Вариант АКСФ с принудительно заданным уровнем шума (для анализа
/// чувствительности метода к точности его оценки).
pub fn acsf_with_sigma(noisy: &GrayF, p: &AcsfParams, sigma_n: f64) -> AcsfResult {
    let sigma_n = sigma_n.max(1.0);
    let activity = structural_activity(noisy, p.radius_struct, sigma_n);
    let r = p.radius;
    let n = 2 * r + 1;
    let t = spatial_table(r, p.sigma_s);
    let mut out = vec![0f32; noisy.data.len()];
    let sn = sigma_n as f32;
    for y in 0..noisy.h as i64 {
        for x in 0..noisy.w as i64 {
            let center = noisy.get(x, y);
            let idx = (y as usize) * noisy.w + x as usize;
            let a = activity[idx];
            let t_ = (a / (p.c * sn)).powi(2);
            let sigma_r = (sn * (p.k_min + (p.k_max - p.k_min) * (-t_).exp())).max(1e-3);
            let inv2sr2 = 1.0 / (2.0 * sigma_r * sigma_r);
            let mut wsum = 0f32;
            let mut acc = 0f32;
            for j in 0..n {
                for i in 0..n {
                    let dx = i as i64 - r as i64;
                    let dy = j as i64 - r as i64;
                    let v = noisy.get(x + dx, y + dy);
                    let d = v - center;
                    let w = t[j * n + i] * (-(d * d) * inv2sr2).exp();
                    wsum += w;
                    acc += w * v;
                }
            }
            out[idx] = acc / wsum;
        }
    }
    AcsfResult { img: GrayF::new(noisy.w, noisy.h, out), sigma_n, activity }
}

/// Визуализация карты структурной активности (нормировка в 0..255 для сохранения).
pub fn activity_to_image(a: &[f32], w: usize, h: usize) -> GrayF {
    let max = a.iter().cloned().fold(0f32, f32::max).max(1e-6);
    let data = a.iter().map(|&v| (v / max * 255.0).min(255.0)).collect();
    GrayF::new(w, h, data)
}
