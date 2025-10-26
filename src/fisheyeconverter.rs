use image::{DynamicImage, RgbaImage, Rgba};

#[derive(Clone, PartialEq)]
pub struct FisheyeSettings {
    pub strength: f32,
    pub zoom: f32,
    pub center_x: f32,
    pub center_y: f32,
}

impl Default for FisheyeSettings {
    fn default() -> Self {
        Self {
            strength: 0.5,
            zoom: 1.0,
            center_x: 0.5,
            center_y: 0.5,
        }
    }
}

pub fn apply_fisheye(image: DynamicImage, settings: &FisheyeSettings) -> RgbaImage {
    let rgba_img = image.to_rgba8();
    let (width, height) = rgba_img.dimensions();
    let mut output = RgbaImage::new(width, height);
    
    let w = width as f32;
    let h = height as f32;
    
    let cx = w * settings.center_x;
    let cy = h * settings.center_y;
    
    let max_radius = ((w * w + h * h) / 4.0).sqrt();
    let strength_factor = if settings.strength >= 0.0 {
        1.0 + settings.strength * 2.0
    } else {
        1.0 / (1.0 - settings.strength * 2.0)
    };
    
    for y in 0..height {
        for x in 0..width {
            let px = x as f32;
            let py = y as f32;
            
            let dx = px - cx;
            let dy = py - cy;
            let distance = (dx * dx + dy * dy).sqrt();
            
            if distance < 0.1 {
                output.put_pixel(x, y, *rgba_img.get_pixel(x, y));
                continue;
            }
            
            let normalized_distance = distance / max_radius;
            let distorted_distance = normalized_distance.powf(strength_factor);
            let scale = distorted_distance * max_radius / distance * settings.zoom;
            
            let src_x = cx + dx * scale;
            let src_y = cy + dy * scale;
            
            let pixel = sample_bilinear(&rgba_img, src_x, src_y, width, height);
            output.put_pixel(x, y, pixel);
        }
    }
    
    output
}

#[inline]
fn sample_bilinear(img: &RgbaImage, x: f32, y: f32, width: u32, height: u32) -> Rgba<u8> {
    if x < 0.0 || y < 0.0 || x >= (width - 1) as f32 || y >= (height - 1) as f32 {
        return Rgba([0, 0, 0, 0]);
    }
    
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    
    let p00 = img.get_pixel(x0, y0);
    let p10 = img.get_pixel(x1, y0);
    let p01 = img.get_pixel(x0, y1);
    let p11 = img.get_pixel(x1, y1);
    
    let mut result = [0u8; 4];
    for i in 0..4 {
        let v00 = p00[i] as f32;
        let v10 = p10[i] as f32;
        let v01 = p01[i] as f32;
        let v11 = p11[i] as f32;
        
        let v0 = v00 * (1.0 - fx) + v10 * fx;
        let v1 = v01 * (1.0 - fx) + v11 * fx;
        let v = v0 * (1.0 - fy) + v1 * fy;
        
        result[i] = v.clamp(0.0, 255.0) as u8;
    }
    
    Rgba(result)
}