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

// Optimized HSV conversion with lookup table approach
#[inline]
fn rgb_to_hsv_fast(r: f32, g: f32, b: f32) -> (f32, f32) {
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    
    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    
    (s, v) // Only return what we need
}

#[inline]
fn enhance_color(r: f32, g: f32, b: f32) -> (u8, u8, u8) {
    let (s, v) = rgb_to_hsv_fast(r, g, b);
    
    // Simplified saturation boost
    let boost = if v > 0.2 && v < 0.9 { 1.4 } else if v >= 0.9 { 1.1 } else { 1.6 };
    let enhanced_s = (s * boost).min(1.0);
    let enhanced_v = if v < 0.3 { (v * 1.15).min(1.0) } else { v };
    
    // Fast color enhancement without full HSV conversion
    let scale = enhanced_v / v.max(0.001);
    let sat_scale = 1.0 + (enhanced_s - s);
    
    let avg = (r + g + b) / 3.0;
    let r_final = (avg + (r - avg) * (1.0 + sat_scale)) * scale;
    let g_final = (avg + (g - avg) * (1.0 + sat_scale)) * scale;
    let b_final = (avg + (b - avg) * (1.0 + sat_scale)) * scale;
    
    (
        (r_final * 255.0).clamp(0.0, 255.0) as u8,
        (g_final * 255.0).clamp(0.0, 255.0) as u8,
        (b_final * 255.0).clamp(0.0, 255.0) as u8,
    )
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

    // Use faster triangle filter for preview
    let resized = image.resize_exact(char_width, char_height, image::imageops::FilterType::Triangle);
    let rgb_img = resized.to_rgb8();

    // Static character lookup
    const CHARS: &[u8] = b"$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrjft/\\|()1{}[]?-_+~<>i!lI;:,\"^`'. ";
    let chars_len = CHARS.len();

    // Pre-calculate contrast and brightness adjustments
    let brightness_mult = settings.brightness;
    let contrast_mult = settings.contrast;
    let curve_power = 1.5f32;
    
    let mut ascii_result = String::with_capacity((char_width as usize + 1) * char_height as usize);
    let mut colored_result = Vec::with_capacity(char_height as usize);

    for y in 0..char_height {
        let mut row = Vec::with_capacity(char_width as usize);
        for x in 0..char_width {
            let pixel = rgb_img.get_pixel(x, y);
            let r = pixel[0] as f32 * (1.0 / 255.0);
            let g = pixel[1] as f32 * (1.0 / 255.0);
            let b = pixel[2] as f32 * (1.0 / 255.0);

            // Fast luminance calculation
            let brightness = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let adjusted = ((brightness - 0.5) * contrast_mult + 0.5) * brightness_mult;
            let clamped = adjusted.clamp(0.0, 1.0);
            let curved = clamped.powf(curve_power);
            
            // Fast character lookup
            let inverted = 1.0 - curved;
            let char_index = (inverted * (chars_len - 1) as f32) as usize;
            let ascii_char = CHARS[char_index.min(chars_len - 1)] as char;

            ascii_result.push(ascii_char);
            
            if settings.use_colors {
                let (final_r, final_g, final_b) = enhance_color(r, g, b);
                let color = egui::Color32::from_rgb(final_r, final_g, final_b);
                row.push((color, ascii_char));
            } else {
                let gray = (clamped * 255.0) as u8;
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