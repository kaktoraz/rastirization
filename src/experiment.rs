//! Проведение воспроизводимых вычислительных экспериментов.
//!
//! Все методы получают один и тот же зашумлённый кадр, seed явно передаётся
//! в генератор. Это сохраняет честность сравнения между фильтрами.

use crate::filters::{
    acsf, acsf_ycbcr, bilateral, bilateral_ycbcr, box_filter, gaussian_filter, median_filter,
    perona_malik, AcsfParams,
};
use crate::img::{save_from_vec, save_ycbcr, GrayF, YCbCrF};
use crate::metrics::{epi, estimate_noise_sigma, mse, psnr, ssim};
use crate::noise::add_gaussian_noise;
use std::time::Instant;

/// Одна строка основного эксперимента.
pub struct Row {
    pub image: String,
    pub method: String,
    pub sigma_noise: f64,
    pub param: String,
    pub psnr: f64,
    pub ssim: f64,
    pub epi: f64,
    pub time_ms: f64,
}

pub fn csv_header() -> &'static str {
    "image,method,sigma_noise,param,psnr,ssim,epi,time_ms"
}

impl Row {
    pub fn to_csv(&self) -> String {
        // `param` использует `;`, поэтому не нарушает запятую как CSV-разделитель.
        format!(
            "{},{},{},{},{:.4},{:.6},{:.6},{:.2}",
            self.image,
            self.method,
            self.sigma_noise,
            self.param,
            self.psnr,
            self.ssim,
            self.epi,
            self.time_ms
        )
    }
}

/// Измерение полутонового метода и общих метрик.
fn eval_gray(
    clean: &GrayF,
    image_name: &str,
    method: &str,
    param: &str,
    sigma_noise: f64,
    f: impl FnOnce() -> GrayF,
) -> (GrayF, Row) {
    let started = Instant::now();
    let out = f();
    let time_ms = started.elapsed().as_secs_f64() * 1000.0;
    let row = Row {
        image: image_name.to_string(),
        method: method.to_string(),
        sigma_noise,
        param: param.to_string(),
        psnr: psnr(clean, &out, 255.0),
        ssim: ssim(clean, &out),
        epi: epi(clean, &out),
        time_ms,
    };
    (out, row)
}

/// Метрики цвета вычисляются целиком в YCbCr: PSNR — по среднему MSE трёх
/// компонент, SSIM и EPI — средние одноимённых метрик для Y, Cb, Cr.
fn ycbcr_metrics(clean: &YCbCrF, restored: &YCbCrF) -> (f64, f64, f64) {
    let error =
        (mse(&clean.y, &restored.y) + mse(&clean.cb, &restored.cb) + mse(&clean.cr, &restored.cr))
            / 3.0;
    let color_psnr = if error <= 1e-12 {
        f64::INFINITY
    } else {
        10.0 * (255.0 * 255.0 / error).log10()
    };
    let color_ssim = (ssim(&clean.y, &restored.y)
        + ssim(&clean.cb, &restored.cb)
        + ssim(&clean.cr, &restored.cr))
        / 3.0;
    let color_epi =
        (epi(&clean.y, &restored.y) + epi(&clean.cb, &restored.cb) + epi(&clean.cr, &restored.cr))
            / 3.0;
    (color_psnr, color_ssim, color_epi)
}

fn eval_color(
    clean: &YCbCrF,
    image_name: &str,
    method: &str,
    param: &str,
    sigma_noise: f64,
    f: impl FnOnce() -> YCbCrF,
) -> (YCbCrF, Row) {
    let started = Instant::now();
    let out = f();
    let time_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (quality_psnr, quality_ssim, quality_epi) = ycbcr_metrics(clean, &out);
    let row = Row {
        image: image_name.to_string(),
        method: method.to_string(),
        sigma_noise,
        param: param.to_string(),
        psnr: quality_psnr,
        ssim: quality_ssim,
        epi: quality_epi,
        time_ms,
    };
    (out, row)
}

/// Полный полутоновый бенчмарк: изображения × уровни шума × методы.
///
/// Для варианта «оракул» время не интерпретируется: это перебор сетки, а не
/// применимый метод. В CSV он остаётся нулём, а анализ выводит «—».
pub fn run_bench(
    clean_dir: &str,
    out_dir: &str,
    sigmas: &[f64],
    seed: u64,
    save_sigma: Option<f64>,
) -> Vec<Row> {
    let images = crate::img::load_dir(clean_dir);
    let mut rows = Vec::new();
    std::fs::create_dir_all(format!("{out_dir}/images")).ok();
    std::fs::create_dir_all(format!("{out_dir}/tables")).ok();

    for (image_index, (name, clean)) in images.iter().enumerate() {
        println!("=== изображение: {name} ({}x{}) ===", clean.w, clean.h);
        for &sigma in sigmas {
            let noise_seed = seed + 1_000 * image_index as u64 + sigma as u64;
            let noisy = add_gaussian_noise(clean, sigma, noise_seed);
            let sigma_estimate = estimate_noise_sigma(&noisy);
            println!("  sigma={sigma:>4}  (оценка по изображению: {sigma_estimate:.2})");

            let (_, row) = eval_gray(clean, name, "raw", "-", sigma, || noisy.clone());
            rows.push(row);

            let (out_box, row) =
                eval_gray(clean, name, "box5", "k=5", sigma, || box_filter(&noisy, 5));
            rows.push(row);

            let (out_gauss, row) = eval_gray(clean, name, "gauss", "ss=1.5;r=3", sigma, || {
                gaussian_filter(&noisy, 1.5, 3)
            });
            rows.push(row);

            let (out_median, row) = eval_gray(clean, name, "median5", "k=5", sigma, || {
                median_filter(&noisy, 5)
            });
            rows.push(row);

            // Дополнительный конкурент: параметры фиксированы для всего набора,
            // kappa масштабируется только автоматической оценкой шума.
            let pm_kappa = (1.5 * sigma_estimate as f32).max(1.0);
            let (out_pm, row) = eval_gray(
                clean,
                name,
                "perona_malik",
                "iter=12;lambda=0.18;kappa=1.5*sn_est",
                sigma,
                || perona_malik(&noisy, 12, 0.18, pm_kappa),
            );
            rows.push(row);

            let universal_sigma_r = (sigma_estimate as f32).max(1.0);
            let (out_bilateral, row) = eval_gray(
                clean,
                name,
                "bilateral",
                "ss=2.0;sr=1.0*sn_est;r=5",
                sigma,
                || bilateral(&noisy, 2.0, universal_sigma_r, 5),
            );
            rows.push(row);

            // Оракул видит чистый кадр только для выбора лучшей строки сетки.
            let mut best = (f64::NEG_INFINITY, 0.0f32, 0.0f32, None);
            for &sigma_s in &[1.5f32, 2.0, 2.5] {
                for &multiplier in &[0.6f32, 1.0, 1.5, 2.0, 3.0] {
                    let candidate = bilateral(&noisy, sigma_s, universal_sigma_r * multiplier, 5);
                    let score = psnr(clean, &candidate, 255.0);
                    if score > best.0 {
                        best = (score, sigma_s, multiplier, Some(candidate));
                    }
                }
            }
            let (_, best_sigma_s, best_multiplier, best_image) = best;
            let out_oracle = best_image.expect("сетка оракула не должна быть пустой");
            let row = Row {
                image: name.clone(),
                method: "bilateral_oracle".into(),
                sigma_noise: sigma,
                param: format!("ss={best_sigma_s};sr={best_multiplier}*sn_est;r=5"),
                psnr: psnr(clean, &out_oracle, 255.0),
                ssim: ssim(clean, &out_oracle),
                epi: epi(clean, &out_oracle),
                time_ms: 0.0,
            };
            rows.push(row);

            let params = AcsfParams::default();
            let acsf_param = format!(
                "ss={};kmin={};kmax={};c={};r={};rs={}",
                params.sigma_s,
                params.k_min,
                params.k_max,
                params.c,
                params.radius,
                params.radius_struct
            );
            let (out_acsf, row) = eval_gray(clean, name, "acsf", &acsf_param, sigma, || {
                acsf(&noisy, &params).img
            });
            rows.push(row);

            let middle = (params.k_min + params.k_max) / 2.0;
            let (_, row) = eval_gray(
                clean,
                name,
                "acsf_flat",
                "sr=(kmin+kmax)/2*sn_est",
                sigma,
                || {
                    let mut flat = params;
                    flat.k_min = middle;
                    flat.k_max = middle;
                    acsf(&noisy, &flat).img
                },
            );
            rows.push(row);

            if save_sigma.is_some_and(|value| (value - sigma).abs() < 1e-9) {
                let image_dir = format!("{out_dir}/images");
                save_from_vec(
                    &format!("{image_dir}/{name}_clean.png"),
                    clean.w,
                    clean.h,
                    &clean.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_noisy.png"),
                    noisy.w,
                    noisy.h,
                    &noisy.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_box5.png"),
                    out_box.w,
                    out_box.h,
                    &out_box.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_gauss.png"),
                    out_gauss.w,
                    out_gauss.h,
                    &out_gauss.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_median5.png"),
                    out_median.w,
                    out_median.h,
                    &out_median.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_perona_malik.png"),
                    out_pm.w,
                    out_pm.h,
                    &out_pm.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_bilateral.png"),
                    out_bilateral.w,
                    out_bilateral.h,
                    &out_bilateral.data,
                )
                .ok();
                save_from_vec(
                    &format!("{image_dir}/{name}_acsf.png"),
                    out_acsf.w,
                    out_acsf.h,
                    &out_acsf.data,
                )
                .ok();
                let activity = acsf(&noisy, &params).activity;
                let activity_image = crate::filters::activity_to_image(&activity, clean.w, clean.h);
                save_from_vec(
                    &format!("{image_dir}/{name}_activity.png"),
                    activity_image.w,
                    activity_image.h,
                    &activity_image.data,
                )
                .ok();
            }
        }
    }
    rows
}

/// Полный цветной эксперимент в YCbCr. Для всех трёх компонент добавляется
/// независимый аддитивный гауссов шум с одинаковым sigma; адаптация АКСФ
/// проводится только по Y, а совместный фильтр использует также Cb и Cr.
pub fn run_color_bench(
    clean_dir: &str,
    out_dir: &str,
    sigmas: &[f64],
    seed: u64,
    save_sigma: Option<f64>,
) -> Vec<Row> {
    let images = crate::img::load_ycbcr_dir(clean_dir);
    let mut rows = Vec::new();
    std::fs::create_dir_all(format!("{out_dir}/images")).ok();
    std::fs::create_dir_all(format!("{out_dir}/tables")).ok();

    for (image_index, (name, clean)) in images.iter().enumerate() {
        println!(
            "=== цветное изображение: {name} ({}x{}) ===",
            clean.w(),
            clean.h()
        );
        for &sigma in sigmas {
            let noise_seed = seed + 1_000 * image_index as u64 + sigma as u64;
            let noisy = YCbCrF::new(
                add_gaussian_noise(&clean.y, sigma, noise_seed),
                add_gaussian_noise(&clean.cb, sigma, noise_seed.wrapping_add(1)),
                add_gaussian_noise(&clean.cr, sigma, noise_seed.wrapping_add(2)),
            );
            let sigma_estimate = estimate_noise_sigma(&noisy.y);
            println!("  sigma={sigma:>4}  (оценка по Y: {sigma_estimate:.2})");

            let (_, row) = eval_color(clean, name, "raw_ycbcr", "-", sigma, || noisy.clone());
            rows.push(row);

            let range_sigma = (sigma_estimate as f32).max(1.0);
            let (out_bilateral, row) = eval_color(
                clean,
                name,
                "bilateral_ycbcr",
                "ss=2.0;srY=1.0*sn_est;r=5;sc=1.5*srY",
                sigma,
                || bilateral_ycbcr(&noisy, 2.0, range_sigma, 5),
            );
            rows.push(row);

            let params = AcsfParams::default();
            let acsf_param = format!(
                "ss={};kmin={};kmax={};c={};r={};rs={};sc=1.5*srY",
                params.sigma_s,
                params.k_min,
                params.k_max,
                params.c,
                params.radius,
                params.radius_struct
            );
            let (out_acsf, row) = eval_color(clean, name, "acsf_ycbcr", &acsf_param, sigma, || {
                acsf_ycbcr(&noisy, &params).img
            });
            rows.push(row);

            if save_sigma.is_some_and(|value| (value - sigma).abs() < 1e-9) {
                let image_dir = format!("{out_dir}/images");
                save_ycbcr(&format!("{image_dir}/{name}_clean.png"), clean).ok();
                save_ycbcr(&format!("{image_dir}/{name}_noisy.png"), &noisy).ok();
                save_ycbcr(
                    &format!("{image_dir}/{name}_bilateral_ycbcr.png"),
                    &out_bilateral,
                )
                .ok();
                save_ycbcr(&format!("{image_dir}/{name}_acsf_ycbcr.png"), &out_acsf).ok();
            }
        }
    }
    rows
}

/// Одна независимая пара средних значений для статистического теста.
pub struct SignificanceRow {
    pub run: usize,
    pub seed: u64,
    pub cases: usize,
    pub bilateral_psnr: f64,
    pub acsf_psnr: f64,
}

pub fn significance_csv_header() -> &'static str {
    "run,seed,cases,bilateral_psnr,acsf_psnr,delta_psnr"
}

impl SignificanceRow {
    pub fn delta_psnr(&self) -> f64 {
        self.acsf_psnr - self.bilateral_psnr
    }

    pub fn to_csv(&self) -> String {
        format!(
            "{},{},{},{:.6},{:.6},{:.6}",
            self.run,
            self.seed,
            self.cases,
            self.bilateral_psnr,
            self.acsf_psnr,
            self.delta_psnr()
        )
    }
}

/// 30 (или заданное число) независимых seed. Единицей наблюдения является
/// среднее по изображениям и уровням шума для одного seed — следовательно,
/// парный t-тест не искусственно раздувает число наблюдений кадрами.
pub fn run_significance(
    clean_dir: &str,
    sigmas: &[f64],
    runs: usize,
    initial_seed: u64,
) -> Vec<SignificanceRow> {
    let images = crate::img::load_dir(clean_dir);
    let params = AcsfParams::default();
    let mut output = Vec::with_capacity(runs);
    for run in 0..runs {
        let run_seed = initial_seed.wrapping_add(10_000_000 * run as u64);
        let mut bilateral_scores = Vec::new();
        let mut acsf_scores = Vec::new();
        for (image_index, (_, clean)) in images.iter().enumerate() {
            for &sigma in sigmas {
                let noise_seed = run_seed + 1_000 * image_index as u64 + sigma as u64;
                let noisy = add_gaussian_noise(clean, sigma, noise_seed);
                let sigma_estimate = estimate_noise_sigma(&noisy).max(1.0) as f32;
                let bilateral_out = bilateral(&noisy, 2.0, sigma_estimate, 5);
                let acsf_out = acsf(&noisy, &params).img;
                bilateral_scores.push(psnr(clean, &bilateral_out, 255.0));
                acsf_scores.push(psnr(clean, &acsf_out, 255.0));
            }
        }
        let cases = bilateral_scores.len();
        let bilateral_psnr = bilateral_scores.iter().sum::<f64>() / cases as f64;
        let acsf_psnr = acsf_scores.iter().sum::<f64>() / cases as f64;
        println!(
            "  run {:>2}/{runs}: bilateral={bilateral_psnr:.3} dB, ACSF={acsf_psnr:.3} dB, delta={:+.3} dB",
            run + 1,
            acsf_psnr - bilateral_psnr
        );
        output.push(SignificanceRow {
            run: run + 1,
            seed: run_seed,
            cases,
            bilateral_psnr,
            acsf_psnr,
        });
    }
    output
}

/// Параметрическое исследование: перебор одного параметра АКСФ.
pub fn run_sweep(
    clean_dir: &str,
    param_name: &str,
    values: &[f64],
    sigmas: &[f64],
    seed: u64,
) -> Vec<(String, f64, f64, f64, f64)> {
    let images = crate::img::load_dir(clean_dir);
    let mut out = Vec::new();
    for &value in values {
        let mut params = AcsfParams::default();
        match param_name {
            "k_min" => params.k_min = value as f32,
            "k_max" => params.k_max = value as f32,
            "c" => params.c = value as f32,
            "sigma_s" => params.sigma_s = value as f32,
            "radius" => params.radius = value as usize,
            "radius_struct" => params.radius_struct = value as usize,
            other => panic!("неизвестный параметр: {other}"),
        }
        let mut psnrs = Vec::new();
        let mut ssims = Vec::new();
        let mut epis = Vec::new();
        for (image_index, (_, clean)) in images.iter().enumerate() {
            for &sigma in sigmas {
                let noise_seed = seed + 1_000 * image_index as u64 + sigma as u64;
                let noisy = add_gaussian_noise(clean, sigma, noise_seed);
                let result = acsf(&noisy, &params).img;
                psnrs.push(psnr(clean, &result, 255.0));
                ssims.push(ssim(clean, &result));
                epis.push(epi(clean, &result));
            }
        }
        let mean_psnr = psnrs.iter().sum::<f64>() / psnrs.len() as f64;
        let mean_ssim = ssims.iter().sum::<f64>() / ssims.len() as f64;
        let mean_epi = epis.iter().sum::<f64>() / epis.len() as f64;
        let minimum_psnr = psnrs.iter().copied().fold(f64::INFINITY, f64::min);
        println!(
            "  {param_name:>12} = {value:<6} : PSNR ср. = {mean_psnr:.2} дБ, худший = {minimum_psnr:.2} дБ, SSIM ср. = {mean_ssim:.4}, EPI ср. = {mean_epi:.4}"
        );
        out.push((
            param_name.to_string(),
            value,
            mean_psnr,
            mean_ssim,
            minimum_psnr,
        ));
    }
    out
}

/// Сравнение нескольких полных конфигураций АКСФ (для калибровки).
pub fn run_configs(
    clean_dir: &str,
    configs: &[String],
    sigmas: &[f64],
    seed: u64,
) -> Vec<(String, Vec<(String, f64, f64, f64)>, f64, f64, f64)> {
    let images = crate::img::load_dir(clean_dir);
    let mut output = Vec::new();
    for config in configs {
        let values: Vec<f64> = config
            .split(',')
            .map(|part| {
                part.trim()
                    .parse()
                    .expect("конфигурация: список чисел через запятую")
            })
            .collect();
        assert_eq!(
            values.len(),
            6,
            "конфигурация: sigma_s,k_min,k_max,c,radius,radius_struct"
        );
        let params = AcsfParams {
            sigma_s: values[0] as f32,
            k_min: values[1] as f32,
            k_max: values[2] as f32,
            c: values[3] as f32,
            radius: values[4] as usize,
            radius_struct: values[5] as usize,
        };
        let mut per_image = Vec::new();
        let (mut total_psnr, mut total_ssim, mut total_epi, mut total_count) = (0.0, 0.0, 0.0, 0.0);
        for (image_index, (name, clean)) in images.iter().enumerate() {
            let (mut image_psnr, mut image_ssim, mut image_epi, mut image_count) =
                (0.0, 0.0, 0.0, 0.0);
            for &sigma in sigmas {
                let noise_seed = seed + 1_000 * image_index as u64 + sigma as u64;
                let noisy = add_gaussian_noise(clean, sigma, noise_seed);
                let result = acsf(&noisy, &params).img;
                image_psnr += psnr(clean, &result, 255.0);
                image_ssim += ssim(clean, &result);
                image_epi += epi(clean, &result);
                image_count += 1.0;
            }
            per_image.push((
                name.clone(),
                image_psnr / image_count,
                image_ssim / image_count,
                image_epi / image_count,
            ));
            total_psnr += image_psnr;
            total_ssim += image_ssim;
            total_epi += image_epi;
            total_count += image_count;
        }
        output.push((
            config.clone(),
            per_image,
            total_psnr / total_count,
            total_ssim / total_count,
            total_epi / total_count,
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_uses_semicolons_inside_parameter_field() {
        let row = Row {
            image: "scene".into(),
            method: "acsf".into(),
            sigma_noise: 20.0,
            param: "ss=2.0;kmin=0.7".into(),
            psnr: 25.0,
            ssim: 0.8,
            epi: 0.6,
            time_ms: 12.3,
        };
        assert_eq!(row.to_csv().split(',').count(), 8);
    }

    #[test]
    fn significance_delta_is_paired_difference() {
        let row = SignificanceRow {
            run: 1,
            seed: 12345,
            cases: 36,
            bilateral_psnr: 24.1,
            acsf_psnr: 25.25,
        };
        assert!((row.delta_psnr() - 1.15).abs() < 1e-12);
    }
}
