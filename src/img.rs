//! Работа с изображениями: загрузка/сохранение и базовая структура данных.
//!
//! Внутри все расчёты ведутся над полутоновым изображением (яркостью)
//! в формате f32, значения в диапазоне [0, 255].

use image::{GenericImageView, GrayImage};

/// Полутоновое изображение с плавающей точкой.
#[derive(Clone)]
pub struct GrayF {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>, // построчно, row-major: data[y*w + x]
}

impl GrayF {
    pub fn new(w: usize, h: usize, data: Vec<f32>) -> Self {
        assert_eq!(w * h, data.len(), "размер буфера не совпадает с размерами изображения");
        Self { w, h, data }
    }

    #[inline]
    pub fn get(&self, x: i64, y: i64) -> f32 {
        // Края обрабатываются продлением (clamp): ближайший пиксель.
        let xi = x.clamp(0, self.w as i64 - 1) as usize;
        let yi = y.clamp(0, self.h as i64 - 1) as usize;
        self.data[yi * self.w + xi]
    }

    pub fn clamp_to_bytes(&self) -> Vec<u8> {
        self.data.iter().map(|&v| v.round().clamp(0.0, 255.0) as u8).collect()
    }

    pub fn to_gray_image(&self) -> GrayImage {
        GrayImage::from_raw(self.w as u32, self.h as u32, self.clamp_to_bytes())
            .expect("не удалось собрать GrayImage")
    }

    pub fn save(&self, path: &str) -> image::ImageResult<()> {
        self.to_gray_image().save(path)
    }

    /// Загрузка изображения и перевод в полутон.
    /// Если файл цветной — яркость считается по стандарту Rec.709:
    /// Y = 0.2126*R + 0.7152*G + 0.0722*B.
    pub fn load(path: &str) -> image::ImageResult<Self> {
        let img = image::open(path)?;
        let (w, h) = img.dimensions();
        let rgb = img.to_rgb8();
        let mut data = Vec::with_capacity((w * h) as usize);
        for px in rgb.pixels() {
            let [r, g, b] = px.0;
            data.push(0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32);
        }
        Ok(GrayF::new(w as usize, h as usize, data))
    }
}

/// Загрузка цветного изображения как трёх отдельных каналов (R, G, B).
/// Каждый канал обрабатывается тем же фильтром независимо — это простейший
/// способ обобщить метод на цветные изображения.
pub fn load_rgb(path: &str) -> image::ImageResult<(GrayF, GrayF, GrayF)> {
    let img = image::open(path)?.to_rgb8();
    let (w, h) = img.dimensions();
    let (mut r, mut g, mut b) = (
        Vec::with_capacity((w * h) as usize),
        Vec::with_capacity((w * h) as usize),
        Vec::with_capacity((w * h) as usize),
    );
    for px in img.pixels() {
        let [rr, gg, bb] = px.0;
        r.push(rr as f32);
        g.push(gg as f32);
        b.push(bb as f32);
    }
    Ok((
        GrayF::new(w as usize, h as usize, r),
        GrayF::new(w as usize, h as usize, g),
        GrayF::new(w as usize, h as usize, b),
    ))
}

/// Сохранение трёх каналов как цветного PNG.
pub fn save_rgb(path: &str, r: &GrayF, g: &GrayF, b: &GrayF) -> image::ImageResult<()> {
    let (w, h) = (r.w as u32, r.h as u32);
    let (rb, gb, bb) = (r.clamp_to_bytes(), g.clamp_to_bytes(), b.clamp_to_bytes());
    let mut buf = Vec::with_capacity((w * h * 3) as usize);
    for i in 0..(w * h) as usize {
        buf.push(rb[i]);
        buf.push(gb[i]);
        buf.push(bb[i]);
    }
    let img = image::RgbImage::from_raw(w, h, buf).expect("не удалось собрать RGB-изображение");
    img.save(path)
}

/// Яркость цветного изображения по стандарту Rec.709 (для расчёта метрик).
pub fn luma(r: &GrayF, g: &GrayF, b: &GrayF) -> GrayF {
    let data = r
        .data
        .iter()
        .zip(&g.data)
        .zip(&b.data)
        .map(|((&rr, &gg), &bb)| 0.2126 * rr + 0.7152 * gg + 0.0722 * bb)
        .collect();
    GrayF::new(r.w, r.h, data)
}

/// Сохранение сырого полутонового буфера как PNG (утилита для экспериментов).
pub fn save_from_vec(path: &str, w: usize, h: usize, data: &[f32]) -> image::ImageResult<()> {
    let buf: Vec<u8> = data.iter().map(|&v| v.round().clamp(0.0, 255.0) as u8).collect();
    let img = GrayImage::from_raw(w as u32, h as u32, buf).expect("bad buffer");
    img.save(path)
}

/// Загрузка всех изображений из каталога (PNG/JPEG/BMP), отсортированных по имени.
pub fn load_dir(dir: &str) -> Vec<(String, GrayF)> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("не удалось открыть каталог с изображениями")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()).as_deref(),
                Some("png") | Some("jpg") | Some("jpeg") | Some("bmp")
            )
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            let img = GrayF::load(p.to_str().unwrap()).expect("ошибка загрузки изображения");
            (name, img)
        })
        .collect()
}
