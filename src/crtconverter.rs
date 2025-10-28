use image::{DynamicImage, RgbaImage, Rgba};

#[derive(Clone, PartialEq)]
pub struct CrtSettings {
    pub scanline_density: f32,
    pub brightness_gain: f32,
    pub curvature: f32,
    pub bezel_size: f32,
    pub vignette_strength: f32,
    pub bg_opacity: u8,
    pub bg_color: [u8; 3],
}

impl Default for CrtSettings {
    fn default() -> Self {
        Self {
            scanline_density: 2.0,
            brightness_gain: 1.2,
            curvature: 0.1,
            bezel_size: 0.05,
            vignette_strength: 0.5,
            bg_opacity: 255,
            bg_color: [20, 20, 20],
        }
    }
}

pub fn apply_crt(image: DynamicImage, settings: &CrtSettings) -> RgbaImage {
    let rgba_img = image.to_rgba8();
    let (width, height) = rgba_img.dimensions();
    
    // Calculate final dimensions with bezel
    let bezel_pixels_w = (width as f32 * settings.bezel_size) as u32;
    let bezel_pixels_h = (height as f32 * settings.bezel_size) as u32;
    let final_width = width + bezel_pixels_w * 2;
    let final_height = height + bezel_pixels_h * 2;
    
    let mut output = RgbaImage::new(final_width, final_height);
    
    // Fill background
    for y in 0..final_height {
        for x in 0..final_width {
            output.put_pixel(x, y, Rgba([
                settings.bg_color[0],
                settings.bg_color[1],
                settings.bg_color[2],
                settings.bg_opacity
            ]));
        }
    }
    
    let w = width as f32;
    let h = height as f32;
    let center_x = w / 2.0;
    let center_y = h / 2.0;
    
    for y in 0..height {
        for x in 0..width {
            let px = x as f32;
            let py = y as f32;
            
            // Normalize to -1 to 1
            let nx = (px - center_x) / center_x;
            let ny = (py - center_y) / center_y;
            
            // Apply curvature distortion
            let r2 = nx * nx + ny * ny;
            let distortion = 1.0 + settings.curvature * r2;
            
            let curved_x = center_x + nx * center_x * distortion;
            let curved_y = center_y + ny * center_y * distortion;
            
            // Check if within bounds
            if curved_x >= 0.0 && curved_x < w && curved_y >= 0.0 && curved_y < h {
                let pixel = sample_bilinear(&rgba_img, curved_x, curved_y, width, height);
                
                // Apply scanlines
                let scanline_mod = (py % settings.scanline_density) / settings.scanline_density;
                let scanline_factor = 0.7 + 0.3 * scanline_mod;
                
                // Apply brightness gain
                let mut r = (pixel[0] as f32 * settings.brightness_gain * scanline_factor).min(255.0) as u8;
                let mut g = (pixel[1] as f32 * settings.brightness_gain * scanline_factor).min(255.0) as u8;
                let mut b = (pixel[2] as f32 * settings.brightness_gain * scanline_factor).min(255.0) as u8;
                
                // Apply vignette
                let dist_from_center = ((nx * nx + ny * ny).sqrt() * settings.vignette_strength).min(1.0);
                let vignette_factor = 1.0 - dist_from_center;
                
                r = (r as f32 * vignette_factor) as u8;
                g = (g as f32 * vignette_factor) as u8;
                b = (b as f32 * vignette_factor) as u8;
                
                // Add slight RGB shift for CRT effect
                let shift = (nx.abs() * 2.0) as i32;
                let out_x = (x + bezel_pixels_w) as i32;
                let out_y = (y + bezel_pixels_h) as i32;
                
                // Place pixel with bezel offset
                if out_x >= 0 && out_x < final_width as i32 && out_y >= 0 && out_y < final_height as i32 {
                    output.put_pixel(out_x as u32, out_y as u32, Rgba([r, g, b, 255]));
                    
                    // Subtle chromatic aberration
                    if shift > 0 && out_x + shift < final_width as i32 {
                        let existing = output.get_pixel((out_x + shift) as u32, out_y as u32);
                        let blended_r = ((existing[0] as u16 + r as u16) / 2) as u8;
                        output.put_pixel((out_x + shift) as u32, out_y as u32, 
                            Rgba([blended_r, existing[1], existing[2], 255]));
                    }
                }
            }
        }
    }
    
    // Add screen glare effect
    add_screen_glare(&mut output, bezel_pixels_w, bezel_pixels_h, width, height);
    
    output
}

fn add_screen_glare(img: &mut RgbaImage, bezel_w: u32, bezel_h: u32, content_w: u32, content_h: u32) {
    let center_x = bezel_w + content_w / 2;
    let center_y = bezel_h + content_h / 2;
    
    for y in bezel_h..(bezel_h + content_h) {
        for x in bezel_w..(bezel_w + content_w) {
            let dx = x as f32 - center_x as f32;
            let dy = y as f32 - center_y as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            let max_dist = ((content_w * content_w + content_h * content_h) as f32).sqrt() / 2.0;
            
            let glare = ((1.0 - (dist / max_dist)) * 15.0).max(0.0) as u8;
            
            let pixel = img.get_pixel(x, y);
            let new_r = (pixel[0] as u16 + glare as u16).min(255) as u8;
            let new_g = (pixel[1] as u16 + glare as u16).min(255) as u8;
            let new_b = (pixel[2] as u16 + glare as u16).min(255) as u8;
            
            img.put_pixel(x, y, Rgba([new_r, new_g, new_b, 255]));
        }
    }
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