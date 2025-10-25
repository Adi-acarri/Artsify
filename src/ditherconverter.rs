use image::{DynamicImage, RgbaImage, Rgba};

#[derive(Clone, PartialEq)]
pub struct DitherSettings {
    pub algorithm: DitherAlgorithm,
    pub color_levels: u8,
    pub threshold: f32,
    pub black_point: f32,
    pub white_point: f32,
    pub custom_black: [u8; 3],
    pub custom_white: [u8; 3],
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
            black_point: 0.0,
            white_point: 255.0,
            custom_black: [0, 0, 0],
            custom_white: [255, 255, 255],
        }
    }
}

pub fn apply_dither(image: DynamicImage, settings: &DitherSettings) -> RgbaImage {
    // Fast grayscale conversion
    let gray_img = image.to_luma8();
    let (width, height) = gray_img.dimensions();
    let mut img = RgbaImage::new(width, height);
    
    // Pre-calculate black/white point adjustment
    let range = settings.white_point - settings.black_point;
    let scale = 255.0 / range;
    
    // Apply black/white point adjustment
    for y in 0..height {
        for x in 0..width {
            let gray = gray_img.get_pixel(x, y)[0] as f32;
            let adjusted = ((gray - settings.black_point) * scale).clamp(0.0, 255.0) as u8;
            img.put_pixel(x, y, Rgba([adjusted, adjusted, adjusted, 255]));
        }
    }
    
    // Apply dithering (fastest algorithms prioritized)
    match settings.algorithm {
        DitherAlgorithm::Threshold => threshold_dither(&mut img, settings),
        DitherAlgorithm::Ordered => ordered_dither(&mut img, settings),
        DitherAlgorithm::FloydSteinberg => floyd_steinberg_dither(&mut img, settings),
        DitherAlgorithm::Atkinson => atkinson_dither(&mut img, settings),
    }
    
    // Apply custom colors if not default B&W
    if settings.custom_black != [0, 0, 0] || settings.custom_white != [255, 255, 255] {
        apply_custom_colors(&mut img, settings);
    }
    
    img
}

fn apply_custom_colors(img: &mut RgbaImage, settings: &DitherSettings) {
    let (width, height) = (img.width(), img.height());
    
    // Pre-calculate color differences for faster interpolation
    let dr = settings.custom_white[0] as i16 - settings.custom_black[0] as i16;
    let dg = settings.custom_white[1] as i16 - settings.custom_black[1] as i16;
    let db = settings.custom_white[2] as i16 - settings.custom_black[2] as i16;
    
    for y in 0..height {
        for x in 0..width {
            let gray = img.get_pixel(x, y)[0] as i16;
            
            // Fast integer interpolation
            let r = (settings.custom_black[0] as i16 + (dr * gray) / 255) as u8;
            let g = (settings.custom_black[1] as i16 + (dg * gray) / 255) as u8;
            let b = (settings.custom_black[2] as i16 + (db * gray) / 255) as u8;
            
            img.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
}

fn quantize_gray(value: u8, levels: u8) -> u8 {
    let step = 255.0 / (levels - 1) as f32;
    let quantized = ((value as f32 / step).round() * step) as u8;
    quantized.min(255)
}

fn floyd_steinberg_dither(img: &mut RgbaImage, settings: &DitherSettings) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let old_gray = pixel[0];
            
            let new_gray = quantize_gray(old_gray, settings.color_levels);
            
            img.put_pixel(x as u32, y as u32, Rgba([new_gray, new_gray, new_gray, 255]));
            
            let err = old_gray as i32 - new_gray as i32;
            
            // Distribute error to neighboring pixels
            distribute_error_gray(img, x + 1, y, err, 7.0 / 16.0);
            distribute_error_gray(img, x - 1, y + 1, err, 3.0 / 16.0);
            distribute_error_gray(img, x, y + 1, err, 5.0 / 16.0);
            distribute_error_gray(img, x + 1, y + 1, err, 1.0 / 16.0);
        }
    }
}

fn atkinson_dither(img: &mut RgbaImage, settings: &DitherSettings) {
    let width = img.width() as i32;
    let height = img.height() as i32;
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x as u32, y as u32);
            let old_gray = pixel[0];
            
            let new_gray = quantize_gray(old_gray, settings.color_levels);
            
            img.put_pixel(x as u32, y as u32, Rgba([new_gray, new_gray, new_gray, 255]));
            
            let err = old_gray as i32 - new_gray as i32;
            
            // Atkinson dithering (lighter, more artistic)
            let factor = 1.0 / 8.0;
            distribute_error_gray(img, x + 1, y, err, factor);
            distribute_error_gray(img, x + 2, y, err, factor);
            distribute_error_gray(img, x - 1, y + 1, err, factor);
            distribute_error_gray(img, x, y + 1, err, factor);
            distribute_error_gray(img, x + 1, y + 1, err, factor);
            distribute_error_gray(img, x, y + 2, err, factor);
        }
    }
}

fn ordered_dither(img: &mut RgbaImage, settings: &DitherSettings) {
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
            let threshold = (bayer_matrix[(y % 4) as usize][(x % 4) as usize] as f32 / 16.0 - 0.5) * 255.0 / (settings.color_levels as f32);
            
            let new_gray = quantize_gray((pixel[0] as f32 + threshold) as u8, settings.color_levels);
            
            img.put_pixel(x, y, Rgba([new_gray, new_gray, new_gray, 255]));
        }
    }
}

fn threshold_dither(img: &mut RgbaImage, settings: &DitherSettings) {
    let width = img.width();
    let height = img.height();
    
    for y in 0..height {
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let gray = pixel[0];
            
            let value = if gray as f32 > settings.threshold { 255 } else { 0 };
            
            img.put_pixel(x, y, Rgba([value, value, value, 255]));
        }
    }
}

fn distribute_error_gray(img: &mut RgbaImage, x: i32, y: i32, err: i32, factor: f32) {
    if x >= 0 && x < img.width() as i32 && y >= 0 && y < img.height() as i32 {
        let pixel = img.get_pixel(x as u32, y as u32);
        let new_gray = (pixel[0] as i32 + (err as f32 * factor) as i32).clamp(0, 255) as u8;
        
        img.put_pixel(x as u32, y as u32, Rgba([new_gray, new_gray, new_gray, 255]));
    }
}