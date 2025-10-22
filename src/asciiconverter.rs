use image::DynamicImage;
use eframe::egui;

#[derive(Clone, PartialEq)]
pub struct AsciiSettings {
    pub use_colors: bool,
    pub brightness: f32,
    pub contrast: f32,
    pub detail_level: DetailLevel,
    pub font_size: f32,
}

#[derive(Clone, PartialEq)]
pub enum DetailLevel {
    Low,
    Medium,
    High,
    VeryHigh,
    Custom(u32),
}

impl DetailLevel {
    pub fn get_width(&self) -> u32 {
        match self {
            DetailLevel::Low => 80,
            DetailLevel::Medium => 120,
            DetailLevel::High => 180,
            DetailLevel::VeryHigh => 250,
            DetailLevel::Custom(width) => *width,
        }
    }
    
    pub fn name(&self) -> &str {
        match self {
            DetailLevel::Low => "Low (80)",
            DetailLevel::Medium => "Medium (120)",
            DetailLevel::High => "High (180)",
            DetailLevel::VeryHigh => "Very High (250)",
            DetailLevel::Custom(_) => "Custom",
        }
    }
}

impl Default for AsciiSettings {
    fn default() -> Self {
        Self {
            use_colors: true,
            brightness: 1.2,
            contrast: 1.3,
            detail_level: DetailLevel::Medium,
            font_size: 12.0,
        }
    }
}

pub struct ConversionResult {
    pub ascii_art: String,
    pub colored_ascii: Vec<Vec<(egui::Color32, char)>>,
}

// Optimized RGB to HSV conversion
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    
    let h = if delta == 0.0 {
        0.0
    } else {
        let h_raw = if max == r {
            (g - b) / delta + (if g < b { 6.0 } else { 0.0 })
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        h_raw * 60.0
    };
    
    (h, s, v)
}

// Optimized HSV to RGB conversion
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (v, v, v);
    }
    
    let h_sector = h / 60.0;
    let sector = h_sector.floor() as i32;
    let fractional = h_sector - sector as f32;
    
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * fractional);
    let t = v * (1.0 - s * (1.0 - fractional));
    
    match sector % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

pub fn convert_image_to_ascii(
    image: DynamicImage,
    settings: &AsciiSettings,
    original_dimensions: (u32, u32),
) -> ConversionResult {
    let (orig_width, orig_height) = original_dimensions;
    
    let char_width = settings.detail_level.get_width();
    let char_height = ((char_width as f32 * orig_height as f32 / orig_width as f32) * 0.5) as u32;
    let char_width = char_width.max(10);
    let char_height = char_height.max(5);

    let resized = image.resize_exact(char_width, char_height, image::imageops::FilterType::Lanczos3);
    let rgb_img = resized.to_rgb8();

    // Character set ordered by actual visual density
    let chars: Vec<char> = "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ".chars().collect();

    let mut ascii_result = String::new();
    let mut colored_result = Vec::new();

    for y in 0..char_height {
        let mut row = Vec::new();
        for x in 0..char_width {
            let pixel = rgb_img.get_pixel(x, y);
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;

            // Perceptual luminance (ITU-R BT.709)
            let brightness = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            
            // Apply user adjustments
            let adjusted_brightness = ((brightness - 0.5) * settings.contrast + 0.5) * settings.brightness;
            let clamped_brightness = adjusted_brightness.clamp(0.0, 1.0);

            // Improved brightness curve mapping
            let curve_power = 1.5;
            let curved_brightness = clamped_brightness.powf(curve_power);
            
            // Map to character index
            let inverted = 1.0 - curved_brightness;
            let char_index = (inverted * (chars.len() - 1) as f32).round() as usize;
            let ascii_char = chars[char_index.min(chars.len() - 1)];

            ascii_result.push(ascii_char);
            
            if settings.use_colors {
                // Convert RGB to HSV for better color control
                let (h, s, v) = rgb_to_hsv(r, g, b);
                
                // Enhanced saturation with smart boosting (optimized)
                let saturation_boost = if v > 0.2 && v < 0.9 {
                    1.4
                } else if v >= 0.9 {
                    1.1
                } else {
                    1.6
                };
                
                let enhanced_s = (s * saturation_boost).min(1.0);
                let enhanced_v = if v < 0.3 { (v * 1.15).min(1.0) } else { v };
                
                // Convert back to RGB
                let (final_r, final_g, final_b) = hsv_to_rgb(h, enhanced_s, enhanced_v);
                
                let color = egui::Color32::from_rgb(
                    (final_r * 255.0) as u8,
                    (final_g * 255.0) as u8,
                    (final_b * 255.0) as u8
                );
                row.push((color, ascii_char));
            } else {
                let gray = (clamped_brightness * 255.0) as u8;
                let color = egui::Color32::from_gray(gray);
                row.push((color, ascii_char));
            }
        }
        ascii_result.push('\n');
        colored_result.push(row);
    }

    ConversionResult {
        ascii_art: ascii_result,
        colored_ascii: colored_result,
    }
}