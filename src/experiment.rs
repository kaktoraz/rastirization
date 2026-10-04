//! Модуль проведения вычислительных экспериментов.
//!
//! Сравнение методов сглаживания на наборе эталонных изображений
//! при разных уровнях аддитивного гауссова шума.

use crate::filters::{acsf, bilateral, box_filter, gaussian_filter, median_filter, AcsfParams};
use crate::img::{save_from_vec, GrayF};
use crate::metrics::{epi, estimate_noise_sigma, psnr, ssim};
use crate::noise::add_gaussian_noise;
use std::time::Instant;

/// Результат одного прогона фильтра.
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
        format!(
            "{},{},{},{},{:.4},{:.6},{:.6},{:.2}",
            self.image, self.method, self.sigma_noise, self.param, self.psnr, self.ssim, self.epi, self.time_ms
        )
    }
}

/// Прогон одного метода: возвращает отфильтрованное изображение и строку метрик.
fn eval(
    clean: &GrayF,
    _noisy: &GrayF,
    image_name: &str,
    method: &str,
    param: &str,
    sigma_noise: f64,
    f: impl FnOnce() -> GrayF,
) -> (GrayF, Row) {
    let t0 = Instant::now();
    let out = f();
    let time_ms = t0.elapsed().as_secs_f64() * 1000.0;
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

/// Полный бенчмарк: набор изображений x уровни шума x набор методов.
///
/// `save_sigma` — уровень шума, для которого сохраняются картинки-результаты
/// (для визуальных иллюстраций в отчёте). None — не сохранять.
pub fn run_bench(
    clean_dir: &str,
    out_dir: &str,
    sigmas: &[f64],
    seed: u64,
    save_sigma: Option<f64>,
) -> Vec<Row> {
    let images = crate::img::load_dir(clean_dir);
    let mut rows = Vec::new();
    std::fs::create_dir_all(format!("{}/images", out_dir)).ok();
    std::fs::create_dir_all(format!("{}/tables", out_dir)).ok();

    for (idx, (name, clean)) in images.iter().enumerate() {
        println!("=== изображение: {} ({}x{}) ===", name, clean.w, clean.h);
        for &sn in sigmas {
            let s = seed + 1000 * idx as u64 + (sn as u64);
            let noisy = add_gaussian_noise(clean, sn, s);
            let sn_est = estimate_noise_sigma(&noisy);
            println!(
                "  sigma={:>4}  (оценка по изображению: {:.2})",
                sn, sn_est
            );

            // 0) без обработки
            let (_, r) = eval(clean, &noisy, name, "raw", "-", sn, || noisy.clone());
            rows.push(r);

            // 1) фильтр среднего, окно 5x5
            let (out_box, r) = eval(clean, &noisy, name, "box5", "k=5", sn, || box_filter(&noisy, 5));
            rows.push(r);

            // 2) гауссов фильтр
            let (out_gauss, r) = eval(clean, &noisy, name, "gauss", "ss=1.5;r=3", sn, || {
                gaussian_filter(&noisy, 1.5, 3)
            });
            rows.push(r);

            // 3) медианный фильтр, окно 5x5
            let (out_med, r) = eval(clean, &noisy, name, "median5", "k=5", sn, || median_filter(&noisy, 5));
            rows.push(r);

            // 4) билатеральный фильтр с универсальными параметрами
            //    sigma_r привязан к автоматической оценке уровня шума
            let sr_univ = (sn_est as f32).max(1.0);
            let (out_bil, r) = eval(clean, &noisy, name, "bilateral", "ss=2.0;sr=1.0*sn_est;r=5", sn, || {
                bilateral(&noisy, 2.0, sr_univ, 5)
            });
            rows.push(r);

            // 5) билатеральный фильтр с подбором параметров «оракулом»
            //    (лучший PSNR по сетке — верхняя граница для этого класса фильтров)
            let mut best = (f64::NEG_INFINITY, 0f32, 0f32, None);
            for &ss in &[1.5f32, 2.0, 2.5] {
                for &k in &[0.6f32, 1.0, 1.5, 2.0, 3.0] {
                    let out = bilateral(&noisy, ss, (sn_est as f32) * k, 5);
                    let p = psnr(clean, &out, 255.0);
                    if p > best.0 {
                        best = (p, ss, k, Some(out));
                    }
                }
            }
            let (_, ss_b, k_b, out_opt) = best;
            let out_oracle = out_opt.unwrap();
            let t0 = Instant::now();
            let row = Row {
                image: name.clone(),
                method: "bilateral_oracle".into(),
                sigma_noise: sn,
                param: format!("ss={};sr={}*sn_est;r=5", ss_b, k_b),
                psnr: psnr(clean, &out_oracle, 255.0),
                ssim: ssim(clean, &out_oracle),
                epi: epi(clean, &out_oracle),
                time_ms: t0.elapsed().as_secs_f64() * 1000.0,
            };
            rows.push(row);

            // 6) АКСФ — собственный метод (все параметры по умолчанию, без подгонки)
            let p = AcsfParams::default();
            let t0 = Instant::now();
            let res = acsf(&noisy, &p);
            let time_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let row = Row {
                image: name.clone(),
                method: "acsf".into(),
                sigma_noise: sn,
                param: format!("ss={};kmin={};kmax={};c={}", p.sigma_s, p.k_min, p.k_max, p.c),
                psnr: psnr(clean, &res.img, 255.0),
                ssim: ssim(clean, &res.img),
                epi: epi(clean, &res.img),
                time_ms,
            };
            rows.push(row);

            // 7) АКСФ с фиксированным (не адаптивным) sigma_r по центру диапазона —
            //    контрольный вариант, показывающий вклад именно адаптации.
            let k_mid = (p.k_min + p.k_max) / 2.0;
            let (_out_flat, r) = eval(clean, &noisy, name, "acsf_flat", "sr=(kmin+kmax)/2*sn", sn, || {
                let mut pp = p;
                pp.k_min = k_mid;
                pp.k_max = k_mid;
                acsf(&noisy, &pp).img
            });
            rows.push(r);

            // Визуализация для выбранного уровня шума
            if let Some(ss_save) = save_sigma {
                if (ss_save - sn).abs() < 1e-9 {
                    let dir = format!("{}/images", out_dir);
                    save_from_vec(&format!("{}/{}_clean.png", dir, name), clean.w, clean.h, &clean.data).ok();
                    save_from_vec(&format!("{}/{}_noisy.png", dir, name), noisy.w, noisy.h, &noisy.data).ok();
                    save_from_vec(&format!("{}/{}_box5.png", dir, name), out_box.w, out_box.h, &out_box.data).ok();
                    save_from_vec(&format!("{}/{}_gauss.png", dir, name), out_gauss.w, out_gauss.h, &out_gauss.data).ok();
                    save_from_vec(&format!("{}/{}_median5.png", dir, name), out_med.w, out_med.h, &out_med.data).ok();
                    save_from_vec(&format!("{}/{}_bilateral.png", dir, name), out_bil.w, out_bil.h, &out_bil.data).ok();
                    save_from_vec(&format!("{}/{}_acsf.png", dir, name), res.img.w, res.img.h, &res.img.data).ok();
                    // карта структурной активности — как иллюстрация работы метода
                    let act_img = crate::filters::activity_to_image(&res.activity, res.img.w, res.img.h);
                    save_from_vec(&format!("{}/{}_activity.png", dir, name), act_img.w, act_img.h, &act_img.data).ok();
                }
            }
        }
    }
    rows
}

/// Параметрическое исследование: перебор значений одного параметра метода
/// при фиксированных остальных. Используется для калибровки параметров.
pub fn run_sweep(
    clean_dir: &str,
    param_name: &str,
    values: &[f64],
    sigmas: &[f64],
    seed: u64,
) -> Vec<(String, f64, f64, f64, f64)> {
    let images = crate::img::load_dir(clean_dir);
    let mut out = Vec::new();
    for &v in values {
        let mut p = AcsfParams::default();
        match param_name {
            "k_min" => p.k_min = v as f32,
            "k_max" => p.k_max = v as f32,
            "c" => p.c = v as f32,
            "sigma_s" => p.sigma_s = v as f32,
            "radius" => p.radius = v as usize,
            "radius_struct" => p.radius_struct = v as usize,
            other => panic!("неизвестный параметр: {}", other),
        }
        let mut psnrs = Vec::new();
        let mut ssims = Vec::new();
        let mut epis = Vec::new();
        for (idx, (_, clean)) in images.iter().enumerate() {
            for &sn in sigmas {
                let s = seed + 1000 * idx as u64 + sn as u64;
                let noisy = add_gaussian_noise(clean, sn, s);
                let res = acsf(&noisy, &p);
                psnrs.push(psnr(clean, &res.img, 255.0));
                ssims.push(ssim(clean, &res.img));
                epis.push(epi(clean, &res.img));
            }
        }
        let mean_epi = epis.iter().sum::<f64>() / epis.len() as f64;
        let mean_psnr = psnrs.iter().sum::<f64>() / psnrs.len() as f64;
        let mean_ssim = ssims.iter().sum::<f64>() / ssims.len() as f64;
        let min_psnr = psnrs.iter().cloned().fold(f64::INFINITY, f64::min);
        println!(
            "  {:>12} = {:<6} : PSNR ср. = {:.2} дБ, худший = {:.2} дБ, SSIM ср. = {:.4}, EPI ср. = {:.4}",
            param_name, v, mean_psnr, min_psnr, mean_ssim, mean_epi
        );
        out.push((param_name.to_string(), v, mean_psnr, mean_ssim, min_psnr));
    }
    out
}

/// Сравнение нескольких полных конфигураций АКСФ: средние и разбивка по изображениям.
/// Формат конфигурации: "sigma_s,k_min,k_max,c,radius,radius_struct".
pub fn run_configs(
    clean_dir: &str,
    configs: &[String],
    sigmas: &[f64],
    seed: u64,
) -> Vec<(String, Vec<(String, f64, f64, f64)>, f64, f64, f64)> {
    let images = crate::img::load_dir(clean_dir);
    let mut out = Vec::new();
    for cfg in configs {
        let vals: Vec<f64> = cfg.split(',').map(|s| s.trim().parse().expect("конфигурация: список чисел через запятую")).collect();
        assert_eq!(vals.len(), 6, "конфигурация: sigma_s,k_min,k_max,c,radius,radius_struct");
        let p = AcsfParams {
            sigma_s: vals[0] as f32,
            k_min: vals[1] as f32,
            k_max: vals[2] as f32,
            c: vals[3] as f32,
            radius: vals[4] as usize,
            radius_struct: vals[5] as usize,
        };
        let mut per_image = Vec::new();
        let (mut sp, mut ss_, mut se, mut cnt) = (0f64, 0f64, 0f64, 0f64);
        for (idx, (name, clean)) in images.iter().enumerate() {
            let (mut ip, mut is_, mut ie, mut ic) = (0f64, 0f64, 0f64, 0f64);
            for &sn in sigmas {
                let s = seed + 1000 * idx as u64 + sn as u64;
                let noisy = add_gaussian_noise(clean, sn, s);
                let res = acsf(&noisy, &p);
                ip += psnr(clean, &res.img, 255.0);
                is_ += ssim(clean, &res.img);
                ie += epi(clean, &res.img);
                ic += 1.0;
            }
            per_image.push((name.clone(), ip / ic, is_ / ic, ie / ic));
            sp += ip; ss_ += is_; se += ie; cnt += ic;
        }
        out.push((cfg.clone(), per_image, sp / cnt, ss_ / cnt, se / cnt));
    }
    out
}
