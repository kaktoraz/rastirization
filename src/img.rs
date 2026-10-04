//! Работа с изображениями: загрузка, сохранение и базовые структуры данных.
//!
//! Основное представление — `GrayF`: яркость в формате `f32` в диапазоне
//! `0..=255`. Для цветного режима используется полнокадровое пространство
//! YCbCr: адаптация выполняется по яркости Y, а Cb/Cr участвуют в совместном
//! взвешивании соседей.

use image::{GenericImageView, GrayImage};

/// Полутоновое изображение с плавающей точкой, построчно (row-major).
#[derive(Clone, Debug)]
pub struct GrayF {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl GrayF {
    pub fn new(w: usize, h: usize, data: Vec<f32>) -> Self {
        assert_eq!(
            w * h,
            data.len(),
            "размер буфера не совпадает с размерами изображения"
        );
        assert!(w > 0 && h > 0, "изображение не должно быть пустым");
        Self { w, h, data }
    }

    /// Доступ с продолжением края ближайшим пикселем.
    #[inline]
    pub fn get(&self, x: i64, y: i64) -> f32 {
        let xi = x.clamp(0, self.w as i64 - 1) as usize;
        let yi = y.clamp(0, self.h as i64 - 1) as usize;
        self.data[yi * self.w + xi]
    }

    pub fn clamp_to_bytes(&self) -> Vec<u8> {
        self.data
            .iter()
            .map(|&v| v.round().clamp(0.0, 255.0) as u8)
            .collect()
    }

    pub fn to_gray_image(&self) -> GrayImage {
        GrayImage::from_raw(self.w as u32, self.h as u32, self.clamp_to_bytes())
            .expect("не удалось собрать GrayImage")
    }

    pub fn save(&self, path: &str) -> image::ImageResult<()> {
        self.to_gray_image().save(path)
    }

    /// Загружает изображение и переводит его в яркость Rec.709.
    pub fn load(path: &str) -> image::ImageResult<Self> {
        let img = image::open(path)?;
        let (w, h) = img.dimensions();
        let rgb = img.to_rgb8();
        let mut data = Vec::with_capacity((w * h) as usize);
        for px in rgb.pixels() {
            let [r, g, b] = px.0;
            data.push(0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32);
        }
        Ok(Self::new(w as usize, h as usize, data))
    }
}

/// Полноразмерное представление изображения в YCbCr (BT.601, full range).
/// Значения Y, Cb и Cr хранятся в одном диапазоне `0..=255`; нейтральная
/// цветность соответствует Cb = Cr = 128.
#[derive(Clone, Debug)]
pub struct YCbCrF {
    pub y: GrayF,
    pub cb: GrayF,
    pub cr: GrayF,
}

impl YCbCrF {
    pub fn new(y: GrayF, cb: GrayF, cr: GrayF) -> Self {
        assert_eq!((y.w, y.h), (cb.w, cb.h), "размеры каналов YCbCr различаются");
        assert_eq!((y.w, y.h), (cr.w, cr.h), "размеры каналов YCbCr различаются");
        Self { y, cb, cr }
    }

    pub fn w(&self) -> usize {
        self.y.w
    }

    pub fn h(&self) -> usize {
        self.y.h
    }
}

/// Загружает цветное изображение как три независимых RGB-канала.
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

/// Сохраняет три RGB-канала как цветное изображение.
pub fn save_rgb(path: &str, r: &GrayF, g: &GrayF, b: &GrayF) -> image::ImageResult<()> {
    assert_eq!((r.w, r.h), (g.w, g.h), "размеры RGB-каналов различаются");
    assert_eq!((r.w, r.h), (b.w, b.h), "размеры RGB-каналов различаются");
    let (w, h) = (r.w as u32, r.h as u32);
    let (rb, gb, bb) = (r.clamp_to_bytes(), g.clamp_to_bytes(), b.clamp_to_bytes());
    let mut buf = Vec::with_capacity((w * h * 3) as usize);
    for i in 0..(w * h) as usize {
        buf.extend_from_slice(&[rb[i], gb[i], bb[i]]);
    }
    let img = image::RgbImage::from_raw(w, h, buf).expect("не удалось собрать RGB-изображение");
    img.save(path)
}

/// Преобразует RGB в YCbCr BT.601 с полным диапазоном.
pub fn rgb_to_ycbcr(r: &GrayF, g: &GrayF, b: &GrayF) -> YCbCrF {
    assert_eq!((r.w, r.h), (g.w, g.h), "размеры RGB-каналов различаются");
    assert_eq!((r.w, r.h), (b.w, b.h), "размеры RGB-каналов различаются");
    let mut y = Vec::with_capacity(r.data.len());
    let mut cb = Vec::with_capacity(r.data.len());
    let mut cr = Vec::with_capacity(r.data.len());
    for ((&rr, &gg), &bb) in r.data.iter().zip(&g.data).zip(&b.data) {
        y.push(0.299 * rr + 0.587 * gg + 0.114 * bb);
        cb.push(128.0 - 0.168_736 * rr - 0.331_264 * gg + 0.5 * bb);
        cr.push(128.0 + 0.5 * rr - 0.418_688 * gg - 0.081_312 * bb);
    }
    YCbCrF::new(
        GrayF::new(r.w, r.h, y),
        GrayF::new(r.w, r.h, cb),
        GrayF::new(r.w, r.h, cr),
    )
}

/// Преобразует YCbCr BT.601 с полным диапазоном обратно в RGB.
pub fn ycbcr_to_rgb(img: &YCbCrF) -> (GrayF, GrayF, GrayF) {
    let mut r = Vec::with_capacity(img.y.data.len());
    let mut g = Vec::with_capacity(img.y.data.len());
    let mut b = Vec::with_capacity(img.y.data.len());
    for ((&yy, &cb), &cr) in img.y.data.iter().zip(&img.cb.data).zip(&img.cr.data) {
        let db = cb - 128.0;
        let dr = cr - 128.0;
        r.push(yy + 1.402 * dr);
        g.push(yy - 0.344_136 * db - 0.714_136 * dr);
        b.push(yy + 1.772 * db);
    }
    (
        GrayF::new(img.w(), img.h(), r),
        GrayF::new(img.w(), img.h(), g),
        GrayF::new(img.w(), img.h(), b),
    )
}

/// Загружает изображение сразу в YCbCr.
pub fn load_ycbcr(path: &str) -> image::ImageResult<YCbCrF> {
    let (r, g, b) = load_rgb(path)?;
    Ok(rgb_to_ycbcr(&r, &g, &b))
}

/// Сохраняет изображение YCbCr, предварительно переводя его в RGB.
pub fn save_ycbcr(path: &str, img: &YCbCrF) -> image::ImageResult<()> {
    let (r, g, b) = ycbcr_to_rgb(img);
    save_rgb(path, &r, &g, &b)
}

/// Яркость цветного RGB-изображения по Rec.709 (для обратной совместимости CLI).
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

/// Сохраняет полутоновый буфер как PNG; используется в эксперименте.
pub fn save_from_vec(path: &str, w: usize, h: usize, data: &[f32]) -> image::ImageResult<()> {
    let buf: Vec<u8> = data
        .iter()
        .map(|&v| v.round().clamp(0.0, 255.0) as u8)
        .collect();
    let img = GrayImage::from_raw(w as u32, h as u32, buf).expect("bad buffer");
    img.save(path)
}

fn image_paths(dir: &str) -> Vec<std::path::PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .expect("не удалось открыть каталог с изображениями")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase())
                    .as_deref(),
                Some("png") | Some("jpg") | Some("jpeg") | Some("bmp")
            )
        })
        .collect();
    files.sort();
    files
}

/// Загружает все изображения из каталога как полутоновые, в порядке имени.
pub fn load_dir(dir: &str) -> Vec<(String, GrayF)> {
    image_paths(dir)
        .into_iter()
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            let img = GrayF::load(p.to_str().unwrap()).expect("ошибка загрузки изображения");
            (name, img)
        })
        .collect()
}

/// Загружает все изображения из каталога как YCbCr, в порядке имени.
pub fn load_ycbcr_dir(dir: &str) -> Vec<(String, YCbCrF)> {
    image_paths(dir)
        .into_iter()
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            let img = load_ycbcr(p.to_str().unwrap()).expect("ошибка загрузки изображения");
            (name, img)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ycbcr_round_trip_preserves_rgb_values() {
        let r = GrayF::new(2, 2, vec![0.0, 12.0, 200.0, 255.0]);
        let g = GrayF::new(2, 2, vec![30.0, 255.0, 40.0, 128.0]);
        let b = GrayF::new(2, 2, vec![255.0, 90.0, 10.0, 32.0]);
        let encoded = rgb_to_ycbcr(&r, &g, &b);
        let (rr, gg, bb) = ycbcr_to_rgb(&encoded);
        for (expected, restored) in r.data.iter().zip(&rr.data) {
            assert!((expected - restored).abs() < 1e-3);
        }
        for (expected, restored) in g.data.iter().zip(&gg.data) {
            assert!((expected - restored).abs() < 1e-3);
        }
        for (expected, restored) in b.data.iter().zip(&bb.data) {
            assert!((expected - restored).abs() < 1e-3);
        }
    }

    #[test]
    fn gray_edge_extension_uses_nearest_pixel() {
        let img = GrayF::new(2, 2, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(img.get(-99, -1), 1.0);
        assert_eq!(img.get(99, -1), 2.0);
        assert_eq!(img.get(-1, 99), 3.0);
        assert_eq!(img.get(99, 99), 4.0);
    }
}
