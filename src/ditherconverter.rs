use image::{DynamicImage, RgbaImage, Rgba};

#[derive(Clone, PartialEq)]
pub struct DitherSettings {
    pub algorithm: DitherAlgorithm,
    pub color_levels: u8,
    pub threshold: f32,
}

#[derive(Clone, PartialEq)]
pub enum DitherAlgorithm {
    FloydSteinberg,
    Atkinson,
    Ordered,
    Threshold,
}

impl DitherAlgorithm {
    pub fn name(&self) -> &str {
        match self {
            DitherAlgorithm::FloydSteinberg => "Floyd-Steinberg",
            DitherAlgorithm::Atkinson => "Atkinson",
            DitherAlgorithm::Ordered => "Ordered (Bayer)",
            DitherAlgorithm::Threshold => "Threshold",
        }
    }
}

impl Default for DitherSettings {
    fn default() -> Self {
        Self {
            algorithm: DitherAlgorithm::FloydSteinberg,
            color_levels: 2,
            threshold: 128.0,
        }
    }
}

pub fn apply_dither(image: DynamicImage, settings: &DitherSettings) -> RgbaImage {
    let mut img = image.to_rgba8();
    
    match settings.algorithm {
        DitherAlgorithm::FloydSteinberg => floyd_steinberg_dither(&mut img, settings.color_levels),
        DitherAlgorithm::Atkinson => atkinson_dither(&mut img, settings.color_levels),
        DitherAlgorithm::Ordered => ordered_dither(&mut img, settings.color_levels),
        DitherAlgorithm::Threshold => threshold_dither(&mut img, settings.threshold),
    }
    
    img
}

fn quantize_color(value: u8, levels: u8) -> u8 {
    let step = 255.0 / (levels - 1) as f32;
    let quantized = ((value as f32 / step).round() * step) as u8;
    quantized.min(255)
}

fn floyd_steinberg_dither(img: &mut RgbaImage, levels: u8) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let old_r = pixel[0];
            let old_g = pixel[1];
            let old_b = pixel[2];
            
            let new_r = quantize_color(old_r, levels);
            let new_g = quantize_color(old_g, levels);
            let new_b = quantize_color(old_b, levels);
            
            img.put_pixel(x as u32, y as u32, Rgba([new_r, new_g, new_b, 255]));
            
            let err_r = old_r as i32 - new_r as i32;
            let err_g = old_g as i32 - new_g as i32;
            let err_b = old_b as i32 - new_b as i32;
            
            // Distribute error to neighboring pixels
            distribute_error(img, x + 1, y, err_r, err_g, err_b, 7.0 / 16.0);
            distribute_error(img, x - 1, y + 1, err_r, err_g, err_b, 3.0 / 16.0);
            distribute_error(img, x, y + 1, err_r, err_g, err_b, 5.0 / 16.0);
            distribute_error(img, x + 1, y + 1, err_r, err_g, err_b, 1.0 / 16.0);
        }
    }
}

fn atkinson_dither(img: &mut RgbaImage, levels: u8) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let old_r = pixel[0];
            let old_g = pixel[1];
            let old_b = pixel[2];
            
            let new_r = quantize_color(old_r, levels);
            let new_g = quantize_color(old_g, levels);
            let new_b = quantize_color(old_b, levels);
            
            img.put_pixel(x as u32, y as u32, Rgba([new_r, new_g, new_b, 255]));
            
            let err_r = old_r as i32 - new_r as i32;
            let err_g = old_g as i32 - new_g as i32;
            let err_b = old_b as i32 - new_b as i32;
            
            // Atkinson dithering (lighter, more artistic)
            let factor = 1.0 / 8.0;
            distribute_error(img, x + 1, y, err_r, err_g, err_b, factor);
            distribute_error(img, x + 2, y, err_r, err_g, err_b, factor);
            distribute_error(img, x - 1, y + 1, err_r, err_g, err_b, factor);
            distribute_error(img, x, y + 1, err_r, err_g, err_b, factor);
            distribute_error(img, x + 1, y + 1, err_r, err_g, err_b, factor);
            distribute_error(img, x, y + 2, err_r, err_g, err_b, factor);
        }
    }
}

fn ordered_dither(img: &mut RgbaImage, levels: u8) {
    // Bayer matrix 4x4
    let bayer_matrix = [
        [0, 8, 2, 10],
        [12, 4, 14, 6],
        [3, 11, 1, 9],
        [15, 7, 13, 5],
    ];
    
    let width = img.width();
    let height = img.height();
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let threshold = (bayer_matrix[(y % 4) as usize][(x % 4) as usize] as f32 / 16.0 - 0.5) * 255.0 / (levels as f32);
            
            let new_r = quantize_color((pixel[0] as f32 + threshold) as u8, levels);
            let new_g = quantize_color((pixel[1] as f32 + threshold) as u8, levels);
            let new_b = quantize_color((pixel[2] as f32 + threshold) as u8, levels);
            
            img.put_pixel(x, y, Rgba([new_r, new_g, new_b, 255]));
        }
    }
}

fn threshold_dither(img: &mut RgbaImage, threshold: f32) {
    let width = img.width();
    let height = img.height();
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let gray = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32) as u8;
            
            let value = if gray as f32 > threshold { 255 } else { 0 };
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
}

fn distribute_error(img: &mut RgbaImage, x: i32, y: i32, err_r: i32, err_g: i32, err_b: i32, factor: f32) {
    if x >= 0 && x < img.width() as i32 && y >= 0 && y < img.height() as i32 {
        let pixel = img.get_pixel(x as u32, y as u32);
        let new_r = (pixel[0] as i32 + (err_r as f32 * factor) as i32).clamp(0, 255) as u8;
        let new_g = (pixel[1] as i32 + (err_g as f32 * factor) as i32).clamp(0, 255) as u8;
        let new_b = (pixel[2] as i32 + (err_b as f32 * factor) as i32).clamp(0, 255) as u8;
        
        img.put_pixel(x as u32, y as u32, Rgba([new_r, new_g, new_b, 255]));
    }
}