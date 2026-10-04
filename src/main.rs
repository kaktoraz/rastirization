//! Программа для учебного проекта «Разработка и исследование методов
//! сглаживания изображений».
//!
//! Подкоманды:
//!   denoise  — отфильтровать одно изображение выбранным методом;
//!   noise    — добавить к изображению гауссов шум (для экспериментов);
//!   bench    — полный сравнительный эксперимент (все методы x все уровни шума);
//!   sweep    — калибровка параметров АКСФ перебором одного параметра.
//!
//! Примеры:
//!   cargo run --release -- denoise -i data/clean/baboon.png -o out.png -m acsf
//!   cargo run --release -- bench -d data/clean -o results --noise 10,20,30,40
//!   cargo run --release -- sweep -d data/clean --param k_max --values 1.6,1.8,2.0,2.2,2.4

mod experiment;
mod filters;
mod img;
mod metrics;
mod noise;

use filters::{acsf, AcsfParams};

/// Простейший разбор аргументов командной строки: пары вида `--key value`.
struct Args {
    map: std::collections::HashMap<String, String>,
}

impl Args {
    fn parse() -> Self {
        let mut map = std::collections::HashMap::new();
        let v: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < v.len() {
            if v[i].starts_with('-') && v[i].len() > 1 {
                let key = v[i].trim_start_matches('-').to_string();
                // значение — следующий аргумент, если он не похож на новый ключ
                // (ключ начинается с '-' и не является числом, напр. "-25.0")
                let next_is_value = i + 1 < v.len()
                    && (!v[i + 1].starts_with('-') || v[i + 1].parse::<f64>().is_ok());
                let val = if next_is_value { v[i + 1].clone() } else { "true".to_string() };
                map.insert(key, val);
                i += if next_is_value { 2 } else { 1 };
            } else {
                i += 1;
            }
        }
        Self { map }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or(default).to_string()
    }
}

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() < 2 {
        usage();
        return;
    }
    let cmd = raw[1].as_str();
    let args = Args::parse();

    match cmd {
        "denoise" => cmd_denoise(&args),
        "noise" => cmd_noise(&args),
        "bench" => cmd_bench(&args),
        "sweep" => cmd_sweep(&args),
        "tune" => cmd_tune(&args),
        "sens" => cmd_sens(&args),
        "demo" => cmd_demo(&args),
        _ => usage(),
    }
}

fn usage() {
    println!(
        r#"Проект «Сглаживание изображений» — программа экспериментов.

Подкоманды:
  denoise  -i <вход> -o <выход> -m <метод> [--param ...]
           методы: box, gauss, median, bilateral, acsf
           общие параметры: --sigma <уровень шума, для добавления шума> (по умолчанию 0)
           box/median: --k <размер окна>
           gauss: --ss <sigma_s> --r <радиус>
           bilateral: --ss <sigma_s> --sr <sigma_r> --r <радиус>
           acsf: --ss <sigma_s> --kmin --kmax --c --r --rs

  noise    -i <вход> -o <выход> --sigma <уровень шума> [--seed <число>]

  bench    -d <каталог с чистыми изображениями> -o <каталог результатов>
           [--noise 10,20,30,40] [--seed 12345] [--save-sigma 25]

  sweep    -d <каталог> --param <k_min|k_max|c|sigma_s|radius|radius_struct>
           --values v1,v2,... [--noise 20,30] [--seed 12345]

  tune     -d <каталог> --configs "ss,kmin,kmax,c,r,rs;..." [--noise 20,30]
           сравнение полных конфигураций АКСФ (среда — средние и по изображениям)

  sens     -d <каталог> [--noise 10,20,30,40]
           анализ чувствительности АКСФ к точности оценки уровня шума:
           автоматическая оценка vs точное известное значение sigma_n

  demo     -d <каталог с вашими картинками> -o <куда сохранить>
           [--sigma 25] [--rgb] [--seed 12345]
           Обрабатывает все картинки из каталога всеми методами сразу:
           результат — папка с готовыми изображениями (зашумлённое + каждый метод).
           Пример: кидаете фото в папку my_images и запускаете
                   cargo run --release -- demo -d my_images -o my_results --rgb --sigma 25
"#
    );
}

fn cmd_denoise(args: &Args) {
    let input = args.get("i").expect("укажите входной файл: -i <путь>");
    let output = args.get("o").expect("укажите выходной файл: -o <путь>");
    let method = args.get_or("m", "acsf");
    let sigma: f64 = args.get_or("sigma", "0").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let use_rgb = args.get("rgb").is_some();

    if use_rgb {
        // Цветной режим: изображение раскладывается на каналы R, G, B,
        // каждый канал фильтруется независимо, затем каналы собираются обратно.
        let (cr, cg, cb) = img::load_rgb(input).expect("не удалось открыть входное изображение");
        let nr = noise::add_gaussian_noise(&cr, sigma, seed);
        let ng = noise::add_gaussian_noise(&cg, sigma, seed + 1);
        let nb = noise::add_gaussian_noise(&cb, sigma, seed + 2);
        let (fr, fg, fb) = if sigma > 0.0 { (nr, ng, nb) } else { (cr.clone(), cg.clone(), cb.clone()) };
        let (orr, og, ob) = (
            apply_filter(&fr, &method, args),
            apply_filter(&fg, &method, args),
            apply_filter(&fb, &method, args),
        );
        img::save_rgb(output, &orr, &og, &ob).expect("не удалось сохранить результат");
        println!("Результат (цветной) сохранён: {}", output);
        if sigma > 0.0 {
            let clean_luma = img::luma(&cr, &cg, &cb);
            let out_luma = img::luma(&orr, &og, &ob);
            println!(
                "PSNR результата относительно чистого изображения: {:.2} дБ",
                metrics::psnr(&clean_luma, &out_luma, 255.0)
            );
        }
        return;
    }

    let clean = img::GrayF::load(input).expect("не удалось открыть входное изображение");
    let noisy = if sigma > 0.0 { noise::add_gaussian_noise(&clean, sigma, seed) } else { clean.clone() };

    if sigma > 0.0 {
        println!("Добавлен шум: sigma = {:.1} (оценка по изображению: {:.2})", sigma, metrics::estimate_noise_sigma(&noisy));
    }

    let out = apply_filter(&noisy, &method, args);
    out.save(output).expect("не удалось сохранить результат");
    println!("Результат сохранён: {}", output);

    if sigma > 0.0 {
        println!(
            "PSNR результата относительно чистого изображения: {:.2} дБ",
            metrics::psnr(&clean, &out, 255.0)
        );
    }
}

/// Применение выбранного фильтра к одному изображению (полутоновому).
/// Используется и в обычном режиме, и для каждого цветового канала.
fn apply_filter(noisy: &img::GrayF, method: &str, args: &Args) -> img::GrayF {
    match method {
        "box" => {
            let k: usize = args.get_or("k", "5").parse().unwrap();
            filters::box_filter(noisy, k)
        }
        "gauss" => {
            let ss: f32 = args.get_or("ss", "1.5").parse().unwrap();
            let r: usize = args.get_or("r", "3").parse().unwrap();
            filters::gaussian_filter(noisy, ss, r)
        }
        "median" => {
            let k: usize = args.get_or("k", "5").parse().unwrap();
            filters::median_filter(noisy, k)
        }
        "bilateral" => {
            let ss: f32 = args.get_or("ss", "2.0").parse().unwrap();
            let sr: f32 = args.get_or("sr", "25").parse().unwrap();
            let r: usize = args.get_or("r", "5").parse().unwrap();
            filters::bilateral(noisy, ss, sr, r)
        }
        "acsf" => {
            let mut p = AcsfParams::default();
            if let Some(v) = args.get("ss") { p.sigma_s = v.parse().unwrap(); }
            if let Some(v) = args.get("kmin") { p.k_min = v.parse().unwrap(); }
            if let Some(v) = args.get("kmax") { p.k_max = v.parse().unwrap(); }
            if let Some(v) = args.get("c") { p.c = v.parse().unwrap(); }
            if let Some(v) = args.get("r") { p.radius = v.parse().unwrap(); }
            if let Some(v) = args.get("rs") { p.radius_struct = v.parse().unwrap(); }
            let res = acsf(noisy, &p);
            println!("  АКСФ: автоматическая оценка уровня шума sigma_n = {:.2}", res.sigma_n);
            res.img
        }
        other => panic!("неизвестный метод: {} (box, gauss, median, bilateral, acsf)", other),
    }
}

fn cmd_noise(args: &Args) {
    let input = args.get("i").expect("укажите входной файл: -i <путь>");
    let output = args.get("o").expect("укажите выходной файл: -o <путь>");
    let sigma: f64 = args.get_or("sigma", "25").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let clean = img::GrayF::load(input).expect("не удалось открыть изображение");
    let noisy = noise::add_gaussian_noise(&clean, sigma, seed);
    noisy.save(output).expect("не удалось сохранить");
    println!(
        "Шум sigma = {:.1} добавлен, файл сохранён: {} (оценённый уровень шума: {:.2})",
        sigma, output, metrics::estimate_noise_sigma(&noisy)
    );
}

fn cmd_bench(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let out_dir = args.get_or("o", "results");
    let sigmas: Vec<f64> = args
        .get_or("noise", "10,20,30,40")
        .split(',')
        .map(|s| s.trim().parse().expect("--noise: список чисел через запятую"))
        .collect();
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let save_sigma: Option<f64> = args.get("save-sigma").map(|v| v.parse().unwrap());

    let rows = experiment::run_bench(&dir, &out_dir, &sigmas, seed, save_sigma);

    // запись CSV
    let mut csv = String::from(experiment::csv_header());
    csv.push('\n');
    for r in &rows {
        csv.push_str(&r.to_csv());
        csv.push('\n');
    }
    let path = format!("{}/tables/results.csv", out_dir);
    std::fs::write(&path, csv).expect("не удалось записать CSV");

    // сводная таблица по средним значениям
    println!("\n=== Средние значения по всем изображениям и уровням шума ===");
    let methods = ["raw", "box5", "gauss", "median5", "bilateral", "bilateral_oracle", "acsf_flat", "acsf"];
    println!("{:<18} {:>10} {:>10} {:>10} {:>12}", "метод", "PSNR,дБ", "SSIM", "EPI", "время,мс");
    for m in methods {
        let sel: Vec<&experiment::Row> = rows.iter().filter(|r| r.method == m).collect();
        if sel.is_empty() { continue; }
        let n = sel.len() as f64;
        let p = sel.iter().map(|r| r.psnr).sum::<f64>() / n;
        let s = sel.iter().map(|r| r.ssim).sum::<f64>() / n;
        let e = sel.iter().map(|r| r.epi).sum::<f64>() / n;
        let t = sel.iter().filter(|r| r.method != "bilateral_oracle").map(|r| r.time_ms).sum::<f64>()
            / sel.iter().filter(|r| r.method != "bilateral_oracle").count().max(1) as f64;
        println!("{:<18} {:>10.2} {:>10.4} {:>10.4} {:>12.1}", m, p, s, e, t);
    }
    println!("\nПодробная таблица: {}", path);
}

/// Массовая обработка пользовательских картинок всеми методами сразу.
/// Для каждого изображения создаются: зашумлённая версия и результаты
/// всех фильтров, а также печатается таблица метрик.
fn cmd_demo(args: &Args) {
    let dir = args.get_or("d", "my_images");
    let out_dir = args.get_or("o", "my_results");
    let sigma: f64 = args.get_or("sigma", "25").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let use_rgb = args.get("rgb").is_some();

    let files = {
        let mut v: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|_| panic!("не найден каталог '{}' — создайте его и положите туда картинки", dir))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()).as_deref(),
                    Some("png") | Some("jpg") | Some("jpeg") | Some("bmp") | Some("webp") | Some("tif") | Some("tiff")
                )
            })
            .collect();
        v.sort();
        v
    };
    if files.is_empty() {
        println!("В каталоге '{}' нет картинок (png/jpg/jpeg/bmp/webp/tif).", dir);
        return;
    }
    std::fs::create_dir_all(&out_dir).expect("не удалось создать каталог результатов");

    let methods = ["gauss", "median", "bilateral", "acsf"];
    let method_names = ["gauss", "median5", "bilateral", "acsf"];
    println!("Найдено картинок: {}", files.len());
    println!("Уровень шума: sigma = {} | режим: {}\n", sigma, if use_rgb { "цветной" } else { "полутон" });

    for path in &files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        println!("=== {} ===", stem);

        let mut per_method_psnr: Vec<(String, f64)> = Vec::new();
        let base_psnr: f64;

        if use_rgb {
            let (cr, cg, cb) = img::load_rgb(path.to_str().unwrap()).expect("ошибка чтения файла");
            let (nr, ng, nb) = if sigma > 0.0 {
                (noise::add_gaussian_noise(&cr, sigma, seed),
                 noise::add_gaussian_noise(&cg, sigma, seed + 1),
                 noise::add_gaussian_noise(&cb, sigma, seed + 2))
            } else {
                (cr.clone(), cg.clone(), cb.clone())
            };
            let clean_luma = img::luma(&cr, &cg, &cb);
            let noisy_luma = img::luma(&nr, &ng, &nb);
            img::save_rgb(&format!("{}/{}_0_noisy.png", out_dir, stem), &nr, &ng, &nb).ok();
            base_psnr = metrics::psnr(&clean_luma, &noisy_luma, 255.0);
            println!("  зашумлённое: PSNR = {:.2} дБ", base_psnr);

            for (m, mn) in methods.iter().zip(method_names.iter()) {
                let (fr, fg, fb) = (
                    apply_filter(&nr, m, args),
                    apply_filter(&ng, m, args),
                    apply_filter(&nb, m, args),
                );
                let out_luma = img::luma(&fr, &fg, &fb);
                let p = metrics::psnr(&clean_luma, &out_luma, 255.0);
                img::save_rgb(&format!("{}/{}_1_{}.png", out_dir, stem, mn), &fr, &fg, &fb).ok();
                per_method_psnr.push((mn.to_string(), p));
            }
        } else {
            let clean = img::GrayF::load(path.to_str().unwrap()).expect("ошибка чтения файла");
            let noisy = if sigma > 0.0 { noise::add_gaussian_noise(&clean, sigma, seed) } else { clean.clone() };
            img::save_from_vec(&format!("{}/{}_0_noisy.png", out_dir, stem), noisy.w, noisy.h, &noisy.data).ok();
            base_psnr = metrics::psnr(&clean, &noisy, 255.0);
            println!("  зашумлённое: PSNR = {:.2} дБ", base_psnr);
            for (m, mn) in methods.iter().zip(method_names.iter()) {
                let f = apply_filter(&noisy, m, args);
                let p = metrics::psnr(&clean, &f, 255.0);
                img::save_from_vec(&format!("{}/{}_1_{}.png", out_dir, stem, mn), f.w, f.h, &f.data).ok();
                per_method_psnr.push((mn.to_string(), p));
            }
        }

        println!("  {:<14} {:>10} {:>12}", "метод", "PSNR,дБ", "к шумному");
        for (mn, p) in &per_method_psnr {
            println!("  {:<14} {:>10.2} {:>+12.2}", mn, p, p - base_psnr);
        }
        let best = per_method_psnr.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();
        println!("  лучший результат: {} ({:.2} дБ)\n", best.0, best.1);
    }
    println!("Готово! Результаты в папке '{}':", out_dir);
    println!("  <имя>_0_noisy.png    — зашумлённое изображение");
    println!("  <имя>_1_acsf.png     — результат нашего метода АКСФ");
    println!("  <имя>_1_gauss.png, _1_median5.png, _1_bilateral.png — для сравнения");
}

fn cmd_sens(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let sigmas: Vec<f64> = args
        .get_or("noise", "10,20,30,40")
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let images = img::load_dir(&dir);
    let p = AcsfParams::default();
    println!("{:<10} {:>7} {:>12} {:>12} {:>12} {:>12} {:>10}", "изобр.", "sigma", "оценка sigma", "PSNR(оценка)", "PSNR(точное)", "разница", "SSIM(точн.)");
    let (mut sp1, mut sp2, mut ss2, mut n) = (0f64, 0f64, 0f64, 0f64);
    for (idx, (name, clean)) in images.iter().enumerate() {
        for &sn in &sigmas {
            let s = seed + 1000 * idx as u64 + sn as u64;
            let noisy = noise::add_gaussian_noise(clean, sn, s);
            let r1 = acsf(&noisy, &p);
            let r2 = filters::acsf_with_sigma(&noisy, &p, sn);
            let p1 = metrics::psnr(clean, &r1.img, 255.0);
            let p2 = metrics::psnr(clean, &r2.img, 255.0);
            let s2 = metrics::ssim(clean, &r2.img);
            println!("{:<10} {:>7.1} {:>12.2} {:>12.2} {:>12.2} {:>+12.2} {:>10.4}", name, sn, r1.sigma_n, p1, p2, p2 - p1, s2);
            sp1 += p1; sp2 += p2; ss2 += s2; n += 1.0;
        }
    }
    println!("\nИтого: средний PSNR при автоматической оценке {:.2} дБ, при точном sigma_n {:.2} дБ (разница {:+.2} дБ), средний SSIM(точн.) {:.4}",
        sp1 / n, sp2 / n, (sp2 - sp1) / n, ss2 / n);
}

fn cmd_tune(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let configs: Vec<String> = args
        .get("configs")
        .expect("укажите --configs \"ss,kmin,kmax,c,r,rs;...\"")
        .split(';')
        .map(|s| s.trim().to_string())
        .collect();
    let sigmas: Vec<f64> = args
        .get_or("noise", "20,30")
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);

    let res = experiment::run_configs(&dir, &configs, &sigmas, seed);
    println!("\n{:<40} {:>10} {:>10} {:>10}", "конфигурация (ss,kmin,kmax,c,r,rs)", "PSNR ср.", "SSIM ср.", "EPI ср.");
    for (cfg, per, mp, ms, me) in &res {
        println!("{:<40} {:>10.2} {:>10.4} {:>10.4}", cfg, mp, ms, me);
        for (name, p, s_, e) in per {
            println!("    {:<14} PSNR={:.2}  SSIM={:.4}  EPI={:.4}", name, p, s_, e);
        }
    }
}

fn cmd_sweep(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let param = args.get("param").expect("укажите --param <имя параметра>").to_string();
    let values: Vec<f64> = args
        .get("values")
        .expect("укажите --values v1,v2,...")
        .split(',')
        .map(|s| s.trim().parse().expect("--values: список чисел"))
        .collect();
    let sigmas: Vec<f64> = args
        .get_or("noise", "20,30")
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);

    println!("Калибровка параметра '{}' при уровнях шума {:?}", param, sigmas);
    let res = experiment::run_sweep(&dir, &param, &values, &sigmas, seed);
    let mut csv = String::from("param,value,mean_psnr,mean_ssim,min_psnr\n");
    for (p, v, mp, ms, minp) in res {
        csv.push_str(&format!("{},{},{:.4},{:.6},{:.4}\n", p, v, mp, ms, minp));
    }
    std::fs::write(format!("results/tables/sweep_{}.csv", param), csv).ok();
}
