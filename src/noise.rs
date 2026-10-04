//! Генерация аддитивного гауссова шума.
//!
//! Реализация без внешних крейтов: собственный генератор (xorshift64*)
//! и преобразование Бокса—Мюллера для получения нормально распределённых чисел.
//! Важно для воспроизводимости эксперимента: при одном и том же seed
//! зашумлённое изображение получается одинаковым при каждом запуске.

/// Генератор псевдослучайных чисел xorshift64*.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Равномерное число в [0, 1).
    #[inline]
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Нормальное число N(0, 1) методом Бокса—Мюллера.
    #[inline]
    pub fn normal(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-12);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Наложение аддитивного белого гауссова шума со среднеквадратичным
/// отклонением `sigma` (в единицах яркости 0..255).
/// Значения за границами [0, 255] ограничиваются.
pub fn add_gaussian_noise(img: &crate::img::GrayF, sigma: f64, seed: u64) -> crate::img::GrayF {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(img.data.len());
    for &v in &img.data {
        let n = rng.normal() * sigma;
        out.push((v as f64 + n).clamp(0.0, 255.0) as f32);
    }
    crate::img::GrayF::new(img.w, img.h, out)
}

/// Добавляет независимый гауссов шум к RGB-изображению и переводит результат
/// в YCbCr. Это соответствует модели шума цветной камеры: один и тот же
/// уровень sigma задаётся в исходных каналах R, G, B, но разные seed не дают
/// искусственной корреляции между ними.
pub fn add_rgb_noise_as_ycbcr(
    r: &crate::img::GrayF,
    g: &crate::img::GrayF,
    b: &crate::img::GrayF,
    sigma: f64,
    seed: u64,
) -> crate::img::YCbCrF {
    let nr = add_gaussian_noise(r, sigma, seed);
    let ng = add_gaussian_noise(g, sigma, seed.wrapping_add(1));
    let nb = add_gaussian_noise(b, sigma, seed.wrapping_add(2));
    crate::img::rgb_to_ycbcr(&nr, &ng, &nb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::img::GrayF;

    #[test]
    fn rng_is_deterministic_for_same_seed() {
        let mut left = Rng::new(1234);
        let mut right = Rng::new(1234);
        for _ in 0..20 {
            assert_eq!(left.next_u64(), right.next_u64());
        }
    }

    #[test]
    fn zero_sigma_does_not_change_image() {
        let image = GrayF::new(3, 2, vec![0.0, 1.0, 127.0, 240.0, 254.0, 255.0]);
        let noisy = add_gaussian_noise(&image, 0.0, 7);
        assert_eq!(image.data, noisy.data);
    }
}
