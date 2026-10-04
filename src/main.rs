//! CLI учебного проекта по сглаживанию изображений.
//!
//! Основные команды:
//! * `denoise` — обработка одного изображения;
//! * `bench` — детерминированный полутоновый или YCbCr-эксперимент;
//! * `stats` — 30 парных независимых прогонов для статистического теста;
//! * `demo` — обработка личной папки с изображениями.

mod experiment;
mod filters;
mod img;
mod metrics;
mod noise;

use filters::AcsfParams;

/// Минимальный разбор аргументов `-key value` / `--key value`.
struct Args {
    map: std::collections::HashMap<String, String>,
}

impl Args {
    fn parse() -> Self {
        let values: Vec<String> = std::env::args().skip(1).collect();
        let mut map = std::collections::HashMap::new();
        let mut index = 0;
        while index < values.len() {
            if values[index].starts_with('-') && values[index].len() > 1 {
                let key = values[index].trim_start_matches('-').to_string();
                // Отрицательное число — значение, а не следующий ключ.
                let has_value = index + 1 < values.len()
                    && (!values[index + 1].starts_with('-')
                        || values[index + 1].parse::<f64>().is_ok());
                let value = if has_value {
                    values[index + 1].clone()
                } else {
                    "true".to_string()
                };
                map.insert(key, value);
                index += if has_value { 2 } else { 1 };
            } else {
                index += 1;
            }
        }
        Self { map }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or(default).to_string()
    }

    fn input(&self) -> &str {
        self.get("i")
            .or_else(|| self.get("input"))
            .expect("укажите входной файл: -i <путь> или --input <путь>")
    }

    fn output(&self) -> &str {
        self.get("o")
            .or_else(|| self.get("output"))
            .expect("укажите выходной файл: -o <путь> или --output <путь>")
    }
}

fn number_list(args: &Args, key: &str, default: &str) -> Vec<f64> {
    args.get_or(key, default)
        .split(',')
        .map(|value| value.trim().parse().expect("ожидался список чисел через запятую"))
        .collect()
}

fn main() {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() < 2 {
        usage();
        return;
    }
    let args = Args::parse();
    match raw[1].as_str() {
        "denoise" => cmd_denoise(&args),
        "noise" => cmd_noise(&args),
        "bench" => cmd_bench(&args),
        "stats" => cmd_stats(&args),
        "demo" => cmd_demo(&args),
        "sens" => cmd_sens(&args),
        "tune" => cmd_tune(&args),
        "sweep" => cmd_sweep(&args),
        _ => usage(),
    }
}

fn usage() {
    println!(
        r#"Проект «Сглаживание изображений» — эксперименты на Rust.

Подкоманды:
  denoise -i <вход> -o <выход> -m <метод> [--sigma 0] [--rgb]
          методы: box, gauss, median, perona, bilateral, acsf
          --rgb включает совместную фильтрацию в YCbCr (не независимый RGB).
  noise   -i <вход> -o <выход> --sigma <уровень> [--seed 12345]
  bench   -d <каталог> -o <результаты> --noise 5,10,15,20,30,40
          [--seed 12345] [--save-sigma 20] [--rgb]
  stats   -d <каталог> -o <результаты> [--runs 30]
          [--noise 5,10,15,20,30,40] [--seed 12345]
  demo    -d my_images -o my_results [--sigma 25] [--rgb]
  sens    -d <каталог> [--noise 5,10,15,20,30,40]
  tune    -d <каталог> --configs "ss,kmin,kmax,c,r,rs;..." [--noise 20,30]
  sweep   -d <каталог> --param <имя> --values <v1,v2,...> [--noise 20,30]

Примеры:
  cargo run --release -- bench -d data/clean -o results \
    --noise 5,10,15,20,30,40 --save-sigma 20
  cargo run --release -- bench --rgb -d data/color -o results/color \
    --noise 10,20,30 --save-sigma 20
  cargo run --release -- stats -d data/clean -o results --runs 30
"#
    );
}

fn acsf_params(args: &Args) -> AcsfParams {
    let mut params = AcsfParams::default();
    if let Some(value) = args.get("ss") {
        params.sigma_s = value.parse().expect("--ss: число");
    }
    if let Some(value) = args.get("kmin") {
        params.k_min = value.parse().expect("--kmin: число");
    }
    if let Some(value) = args.get("kmax") {
        params.k_max = value.parse().expect("--kmax: число");
    }
    if let Some(value) = args.get("c") {
        params.c = value.parse().expect("--c: число");
    }
    if let Some(value) = args.get("r") {
        params.radius = value.parse().expect("--r: целое число");
    }
    if let Some(value) = args.get("rs") {
        params.radius_struct = value.parse().expect("--rs: целое число");
    }
    params
}

fn apply_filter(noisy: &img::GrayF, method: &str, args: &Args) -> img::GrayF {
    match method {
        "box" => filters::box_filter(noisy, args.get_or("k", "5").parse().unwrap()),
        "gauss" => filters::gaussian_filter(
            noisy,
            args.get_or("ss", "1.5").parse().unwrap(),
            args.get_or("r", "3").parse().unwrap(),
        ),
        "median" => filters::median_filter(noisy, args.get_or("k", "5").parse().unwrap()),
        "perona" | "perona_malik" => filters::perona_malik(
            noisy,
            args.get_or("iter", "12").parse().unwrap(),
            args.get_or("lambda", "0.18").parse().unwrap(),
            args.get_or("kappa", "25").parse().unwrap(),
        ),
        "bilateral" => filters::bilateral(
            noisy,
            args.get_or("ss", "2.0").parse().unwrap(),
            args.get_or("sr", "25").parse().unwrap(),
            args.get_or("r", "5").parse().unwrap(),
        ),
        "acsf" => {
            let result = filters::acsf(noisy, &acsf_params(args));
            println!("  АКСФ: оценка sigma_n = {:.2}", result.sigma_n);
            result.img
        }
        other => panic!("неизвестный метод: {other} (box, gauss, median, perona, bilateral, acsf)"),
    }
}

fn apply_color_filter(noisy: &img::YCbCrF, method: &str, args: &Args) -> img::YCbCrF {
    match method {
        "box" => img::YCbCrF::new(
            filters::box_filter(&noisy.y, args.get_or("k", "5").parse().unwrap()),
            filters::box_filter(&noisy.cb, args.get_or("k", "5").parse().unwrap()),
            filters::box_filter(&noisy.cr, args.get_or("k", "5").parse().unwrap()),
        ),
        "gauss" => {
            let sigma_s = args.get_or("ss", "1.5").parse().unwrap();
            let radius = args.get_or("r", "3").parse().unwrap();
            img::YCbCrF::new(
                filters::gaussian_filter(&noisy.y, sigma_s, radius),
                filters::gaussian_filter(&noisy.cb, sigma_s, radius),
                filters::gaussian_filter(&noisy.cr, sigma_s, radius),
            )
        }
        "median" => {
            let size = args.get_or("k", "5").parse().unwrap();
            img::YCbCrF::new(
                filters::median_filter(&noisy.y, size),
                filters::median_filter(&noisy.cb, size),
                filters::median_filter(&noisy.cr, size),
            )
        }
        "perona" | "perona_malik" => {
            let iterations = args.get_or("iter", "12").parse().unwrap();
            let lambda = args.get_or("lambda", "0.18").parse().unwrap();
            let kappa = args.get_or("kappa", "25").parse().unwrap();
            img::YCbCrF::new(
                filters::perona_malik(&noisy.y, iterations, lambda, kappa),
                filters::perona_malik(&noisy.cb, iterations, lambda, kappa),
                filters::perona_malik(&noisy.cr, iterations, lambda, kappa),
            )
        }
        "bilateral" => filters::bilateral_ycbcr(
            noisy,
            args.get_or("ss", "2.0").parse().unwrap(),
            args.get_or("sr", "25").parse().unwrap(),
            args.get_or("r", "5").parse().unwrap(),
        ),
        "acsf" => {
            let result = filters::acsf_ycbcr(noisy, &acsf_params(args));
            println!("  Цветной АКСФ: оценка sigma_n(Y) = {:.2}", result.sigma_n);
            result.img
        }
        other => panic!("неизвестный цветной метод: {other}"),
    }
}

fn cmd_denoise(args: &Args) {
    let input = args.input();
    let output = args.output();
    let method = args.get_or("m", "acsf");
    let sigma: f64 = args.get_or("sigma", "0").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);

    if args.get("rgb").is_some() {
        let (r, g, b) = img::load_rgb(input).expect("не удалось открыть цветное изображение");
        let clean = img::rgb_to_ycbcr(&r, &g, &b);
        let noisy = if sigma > 0.0 {
            noise::add_rgb_noise_as_ycbcr(&r, &g, &b, sigma, seed)
        } else {
            clean.clone()
        };
        let out = apply_color_filter(&noisy, &method, args);
        img::save_ycbcr(output, &out).expect("не удалось сохранить результат");
        println!("Цветной результат YCbCr сохранён: {output}");
        if sigma > 0.0 {
            println!(
                "PSNR по яркости Y относительно эталона: {:.2} дБ",
                metrics::psnr(&clean.y, &out.y, 255.0)
            );
        }
        return;
    }

    let clean = img::GrayF::load(input).expect("не удалось открыть входное изображение");
    let noisy = if sigma > 0.0 {
        noise::add_gaussian_noise(&clean, sigma, seed)
    } else {
        clean.clone()
    };
    if sigma > 0.0 {
        println!(
            "Добавлен шум: sigma = {sigma:.1} (оценка: {:.2})",
            metrics::estimate_noise_sigma(&noisy)
        );
    }
    let out = apply_filter(&noisy, &method, args);
    out.save(output).expect("не удалось сохранить результат");
    println!("Результат сохранён: {output}");
    if sigma > 0.0 {
        println!("PSNR результата: {:.2} дБ", metrics::psnr(&clean, &out, 255.0));
    }
}

fn cmd_noise(args: &Args) {
    let input = args.input();
    let output = args.output();
    let sigma: f64 = args.get_or("sigma", "25").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let clean = img::GrayF::load(input).expect("не удалось открыть изображение");
    let noisy = noise::add_gaussian_noise(&clean, sigma, seed);
    noisy.save(output).expect("не удалось сохранить");
    println!(
        "Шум sigma={sigma:.1} сохранён в {output}; оценка {:.2}",
        metrics::estimate_noise_sigma(&noisy)
    );
}

fn write_rows(out_dir: &str, rows: &[experiment::Row]) -> String {
    std::fs::create_dir_all(format!("{out_dir}/tables")).expect("не удалось создать каталог таблиц");
    let mut csv = String::from(experiment::csv_header());
    csv.push('\n');
    for row in rows {
        csv.push_str(&row.to_csv());
        csv.push('\n');
    }
    let path = format!("{out_dir}/tables/results.csv");
    std::fs::write(&path, csv).expect("не удалось записать CSV");
    path
}

fn print_summary(rows: &[experiment::Row], methods: &[&str]) {
    println!("\n=== Средние значения ===");
    println!("{:<20} {:>10} {:>10} {:>10} {:>12}", "метод", "PSNR,дБ", "SSIM", "EPI", "время,мс");
    for method in methods {
        let selected: Vec<_> = rows.iter().filter(|row| row.method == *method).collect();
        if selected.is_empty() {
            continue;
        }
        let count = selected.len() as f64;
        let p = selected.iter().map(|row| row.psnr).sum::<f64>() / count;
        let s = selected.iter().map(|row| row.ssim).sum::<f64>() / count;
        let e = selected.iter().map(|row| row.epi).sum::<f64>() / count;
        if *method == "bilateral_oracle" {
            println!("{:<20} {:>10.2} {:>10.4} {:>10.4} {:>12}", method, p, s, e, "—");
        } else {
            let t = selected.iter().map(|row| row.time_ms).sum::<f64>() / count;
            println!("{:<20} {:>10.2} {:>10.4} {:>10.4} {:>12.1}", method, p, s, e, t);
        }
    }
}

fn cmd_bench(args: &Args) {
    let color = args.get("rgb").is_some();
    let default_dir = if color { "data/color" } else { "data/clean" };
    let default_out = if color { "results/color" } else { "results" };
    let dir = args.get_or("d", default_dir);
    let out_dir = args.get_or("o", default_out);
    let sigmas = number_list(args, "noise", "5,10,15,20,30,40");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let save_sigma = args.get("save-sigma").map(|value| value.parse().expect("--save-sigma: число"));

    let rows = if color {
        experiment::run_color_bench(&dir, &out_dir, &sigmas, seed, save_sigma)
    } else {
        experiment::run_bench(&dir, &out_dir, &sigmas, seed, save_sigma)
    };
    let path = write_rows(&out_dir, &rows);
    if color {
        print_summary(&rows, &["raw_ycbcr", "bilateral_ycbcr", "acsf_ycbcr"]);
    } else {
        print_summary(
            &rows,
            &[
                "raw",
                "box5",
                "gauss",
                "median5",
                "perona_malik",
                "bilateral",
                "bilateral_oracle",
                "acsf_flat",
                "acsf",
            ],
        );
    }
    println!("\nПодробная таблица: {path}");
}

fn cmd_stats(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let out_dir = args.get_or("o", "results");
    let sigmas = number_list(args, "noise", "5,10,15,20,30,40");
    let runs: usize = args.get_or("runs", "30").parse().expect("--runs: целое число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    println!("Парные независимые прогоны: {runs}; случаи в каждом прогоне: изображения × sigma");
    let rows = experiment::run_significance(&dir, &sigmas, runs, seed);
    std::fs::create_dir_all(format!("{out_dir}/tables")).expect("не удалось создать каталог таблиц");
    let mut csv = String::from(experiment::significance_csv_header());
    csv.push('\n');
    for row in &rows {
        csv.push_str(&row.to_csv());
        csv.push('\n');
    }
    let path = format!("{out_dir}/tables/significance_runs.csv");
    std::fs::write(&path, csv).expect("не удалось записать статистические прогоны");
    let mean_delta = rows.iter().map(experiment::SignificanceRow::delta_psnr).sum::<f64>() / rows.len() as f64;
    println!("Средняя парная разница АКСФ − bilateral: {mean_delta:+.3} дБ");
    println!("Сырые пары для t-теста: {path}");
}

fn image_files(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("не найден каталог '{dir}'"))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_lowercase())
                    .as_deref(),
                Some("png") | Some("jpg") | Some("jpeg") | Some("bmp")
            )
        })
        .collect();
    files.sort();
    files
}

fn cmd_demo(args: &Args) {
    let dir = args.get_or("d", "my_images");
    let out_dir = args.get_or("o", "my_results");
    let sigma: f64 = args.get_or("sigma", "25").parse().expect("--sigma: число");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let color = args.get("rgb").is_some();
    let files = image_files(&dir);
    if files.is_empty() {
        println!("В каталоге '{dir}' нет поддерживаемых изображений.");
        return;
    }
    std::fs::create_dir_all(&out_dir).expect("не удалось создать каталог результатов");
    let methods = ["gauss", "median", "perona", "bilateral", "acsf"];
    println!("Найдено изображений: {}; режим: {}", files.len(), if color { "YCbCr" } else { "полутон" });

    for (file_index, path) in files.iter().enumerate() {
        let name = path.file_stem().unwrap().to_string_lossy();
        let local_seed = seed.wrapping_add(file_index as u64 * 100);
        println!("=== {name} ===");
        if color {
            let (r, g, b) = img::load_rgb(path.to_str().unwrap()).expect("ошибка чтения изображения");
            let clean = img::rgb_to_ycbcr(&r, &g, &b);
            let noisy = if sigma > 0.0 {
                noise::add_rgb_noise_as_ycbcr(&r, &g, &b, sigma, local_seed)
            } else {
                clean.clone()
            };
            img::save_ycbcr(&format!("{out_dir}/{name}_0_noisy.png"), &noisy).ok();
            let baseline = metrics::psnr(&clean.y, &noisy.y, 255.0);
            println!("  шумное: PSNR(Y) = {baseline:.2} дБ");
            for method in methods {
                let result = apply_color_filter(&noisy, method, args);
                let value = metrics::psnr(&clean.y, &result.y, 255.0);
                img::save_ycbcr(&format!("{out_dir}/{name}_1_{method}.png"), &result).ok();
                println!("  {method:<12} PSNR(Y)={value:>6.2}  delta={:+.2}", value - baseline);
            }
        } else {
            let clean = img::GrayF::load(path.to_str().unwrap()).expect("ошибка чтения изображения");
            let noisy = if sigma > 0.0 {
                noise::add_gaussian_noise(&clean, sigma, local_seed)
            } else {
                clean.clone()
            };
            img::save_from_vec(&format!("{out_dir}/{name}_0_noisy.png"), noisy.w, noisy.h, &noisy.data).ok();
            let baseline = metrics::psnr(&clean, &noisy, 255.0);
            println!("  шумное: PSNR = {baseline:.2} дБ");
            for method in methods {
                let result = apply_filter(&noisy, method, args);
                let value = metrics::psnr(&clean, &result, 255.0);
                img::save_from_vec(&format!("{out_dir}/{name}_1_{method}.png"), result.w, result.h, &result.data).ok();
                println!("  {method:<12} PSNR={value:>6.2}  delta={:+.2}", value - baseline);
            }
        }
    }
    println!("Готово: результаты сохранены в '{out_dir}'.");
}

fn cmd_sens(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let sigmas = number_list(args, "noise", "5,10,15,20,30,40");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let images = img::load_dir(&dir);
    let params = AcsfParams::default();
    println!("{:<18} {:>7} {:>11} {:>12} {:>12} {:>10}", "изобр.", "sigma", "оценка", "PSNR авто", "PSNR точно", "разница");
    let (mut automatic, mut exact, mut count) = (0.0, 0.0, 0.0);
    for (image_index, (name, clean)) in images.iter().enumerate() {
        for &sigma in &sigmas {
            let noise_seed = seed + 1_000 * image_index as u64 + sigma as u64;
            let noisy = noise::add_gaussian_noise(clean, sigma, noise_seed);
            let automatic_result = filters::acsf(&noisy, &params);
            let exact_result = filters::acsf_with_sigma(&noisy, &params, sigma);
            let automatic_psnr = metrics::psnr(clean, &automatic_result.img, 255.0);
            let exact_psnr = metrics::psnr(clean, &exact_result.img, 255.0);
            println!("{:<18} {:>7.1} {:>11.2} {:>12.2} {:>12.2} {:>+10.2}", name, sigma, automatic_result.sigma_n, automatic_psnr, exact_psnr, exact_psnr - automatic_psnr);
            automatic += automatic_psnr;
            exact += exact_psnr;
            count += 1.0;
        }
    }
    println!("\nСреднее: авто={:.2} дБ, точный sigma={:.2} дБ, разница={:+.2} дБ", automatic / count, exact / count, (exact - automatic) / count);
}

fn cmd_tune(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let configs: Vec<String> = args
        .get("configs")
        .expect("укажите --configs \"ss,kmin,kmax,c,r,rs;...\"")
        .split(';')
        .map(|value| value.trim().to_string())
        .collect();
    let sigmas = number_list(args, "noise", "20,30");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let output = args.get_or("o", "results/tables/calibration.csv");
    if let Some(parent) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(parent).expect("не удалось создать каталог калибровки");
    }
    let result = experiment::run_configs(&dir, &configs, &sigmas, seed);
    let mut csv = String::from("config,mean_psnr,mean_ssim,mean_epi\n");
    println!("\n{:<40} {:>10} {:>10} {:>10}", "конфигурация", "PSNR", "SSIM", "EPI");
    for (config, per_image, psnr, ssim, epi) in result {
        println!("{config:<40} {psnr:>10.2} {ssim:>10.4} {epi:>10.4}");
        csv.push_str(&format!("\"{config}\",{psnr:.6},{ssim:.6},{epi:.6}\n"));
        for (name, p, s, e) in per_image {
            println!("  {name:<18} PSNR={p:.2} SSIM={s:.4} EPI={e:.4}");
        }
    }
    std::fs::write(&output, csv).expect("не удалось записать CSV калибровки");
    println!("Таблица калибровки: {output}");
}

fn cmd_sweep(args: &Args) {
    let dir = args.get_or("d", "data/clean");
    let parameter = args.get("param").expect("укажите --param");
    let values = number_list(args, "values", "");
    assert!(!values.is_empty(), "укажите --values v1,v2,...");
    let sigmas = number_list(args, "noise", "20,30");
    let seed: u64 = args.get_or("seed", "12345").parse().unwrap_or(12345);
    let result = experiment::run_sweep(&dir, parameter, &values, &sigmas, seed);
    std::fs::create_dir_all("results/tables").ok();
    let mut csv = String::from("param,value,mean_psnr,mean_ssim,min_psnr\n");
    for (name, value, mean_psnr, mean_ssim, minimum_psnr) in result {
        csv.push_str(&format!("{name},{value},{mean_psnr:.4},{mean_ssim:.6},{minimum_psnr:.4}\n"));
    }
    let path = format!("results/tables/sweep_{parameter}.csv");
    std::fs::write(&path, csv).expect("не удалось записать sweep CSV");
    println!("Таблица калибровки: {path}");
}
