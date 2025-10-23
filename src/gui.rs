use eframe::egui;
use image::{DynamicImage, GenericImageView, RgbaImage};
use imageproc::drawing::draw_text_mut;
use ab_glyph::{FontRef, PxScale};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::asciiconverter::{AsciiSettings, DetailLevel, ConversionResult, convert_image_to_ascii};
use crate::ditherconverter::{DitherSettings, DitherAlgorithm, apply_dither};

const FONT_DATA: &[u8] = include_bytes!("../fonts/DejaVuSansMono.ttf");

// Embed icon files
const ICON_FOLDER: &[u8] = include_bytes!("../icons/folder.png");
const ICON_SAVE: &[u8] = include_bytes!("../icons/save.png");
const ICON_TEXT: &[u8] = include_bytes!("../icons/text.png");
const ICON_COPY: &[u8] = include_bytes!("../icons/copy.png");

pub struct AsciiArtApp {
    input_image: Option<DynamicImage>,
    ascii_art: String,
    colored_ascii: Vec<Vec<(egui::Color32, char)>>,
    pub settings: AsciiSettings,
    pub dither_settings: DitherSettings,
    image_path: String,
    original_dimensions: (u32, u32),
    processing: bool,
    ascii_applied: bool,
    dither_applied: bool,
    dithered_image: Option<RgbaImage>,
    result_receiver: Option<mpsc::Receiver<ConversionResult>>,
    file_dialog_receiver: Option<mpsc::Receiver<Option<PathBuf>>>,
    save_dialog_receiver: Option<mpsc::Receiver<Option<PathBuf>>>,
    status_message: Option<(String, egui::Color32)>,
    show_original: bool,
    // Caching for performance
    cached_preview: Option<egui::TextureHandle>,
    last_preview_settings: Option<(f32, bool)>,
    // Debouncing
    pending_update: bool,
    last_slider_change: Option<std::time::Instant>,
    // Icon textures
    icon_folder: Option<egui::TextureHandle>,
    icon_save: Option<egui::TextureHandle>,
    icon_text: Option<egui::TextureHandle>,
    icon_copy: Option<egui::TextureHandle>,
}

#[derive(Clone, Copy, PartialEq)]
enum SaveType {
    Image,
    Text,
}

impl Default for AsciiArtApp {
    fn default() -> Self {
        Self {
            input_image: None,
            ascii_art: String::new(),
            colored_ascii: Vec::new(),
            settings: AsciiSettings::default(),
            dither_settings: DitherSettings::default(),
            image_path: String::new(),
            original_dimensions: (0, 0),
            processing: false,
            ascii_applied: false,
            dither_applied: false,
            dithered_image: None,
            result_receiver: None,
            file_dialog_receiver: None,
            save_dialog_receiver: None,
            status_message: None,
            show_original: false,
            cached_preview: None,
            last_preview_settings: None,
            pending_update: false,
            last_slider_change: None,
            icon_folder: None,
            icon_save: None,
            icon_text: None,
            icon_copy: None,
        }
    }
}

impl AsciiArtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let mut app = Self::default();
        app.load_icons(&cc.egui_ctx);
        app
    }
    
    fn load_icons(&mut self, ctx: &egui::Context) {
        self.icon_folder = Self::load_icon_from_bytes(ctx, "folder", ICON_FOLDER);
        self.icon_save = Self::load_icon_from_bytes(ctx, "save", ICON_SAVE);
        self.icon_text = Self::load_icon_from_bytes(ctx, "text", ICON_TEXT);
        self.icon_copy = Self::load_icon_from_bytes(ctx, "copy", ICON_COPY);
    }
    
    fn load_icon_from_bytes(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
        match image::load_from_memory(bytes) {
            Ok(img) => {
                let size = [img.width() as usize, img.height() as usize];
                let rgba = img.to_rgba8();
                let pixels = rgba.as_flat_samples();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    size,
                    pixels.as_slice(),
                );
                Some(ctx.load_texture(name, color_image, egui::TextureOptions::default()))
            }
            Err(e) => {
                eprintln!("Failed to load icon {}: {}", name, e);
                None
            }
        }
    }
    
    fn image_button(&self, ui: &mut egui::Ui, texture: &Option<egui::TextureHandle>, text: &str) -> egui::Response {
        if let Some(tex) = texture {
            let size = egui::vec2(20.0, 20.0);
            ui.add(egui::ImageButton::new(egui::ImageSource::Texture(egui::load::SizedTexture {
                id: tex.id(),
                size,
            })).frame(false))
        } else {
            ui.button(text)
        }
    }

    fn render_ascii_to_image(colored_ascii: &[Vec<(egui::Color32, char)>], font_size: f32, use_colors: bool) -> Result<RgbaImage, String> {
        if colored_ascii.is_empty() {
            return Err("No ASCII art to render".to_string());
        }

        let char_height = colored_ascii.len();
        let char_width = colored_ascii[0].len();
        
        if char_width == 0 {
            return Err("Invalid ASCII art dimensions".to_string());
        }

        let font = FontRef::try_from_slice(FONT_DATA)
            .map_err(|e| format!("Failed to load font: {:?}", e))?;

        let scale = PxScale::from(font_size);
        
        let char_pixel_height = font_size * 1.2;
        let char_pixel_width = font_size * 0.6;
        
        let img_width = (char_width as f32 * char_pixel_width).ceil() as u32;
        let img_height = (char_height as f32 * char_pixel_height).ceil() as u32;
        
        let mut img = RgbaImage::from_pixel(img_width, img_height, image::Rgba([0, 0, 0, 255]));

        for (row_idx, row) in colored_ascii.iter().enumerate() {
            for (col_idx, (color, ch)) in row.iter().enumerate() {
                let x = (col_idx as f32 * char_pixel_width) as i32;
                let y = (row_idx as f32 * char_pixel_height) as i32;
                
                let text_color = if use_colors {
                    let c = color.to_array();
                    image::Rgba([c[0], c[1], c[2], 255])
                } else {
                    let gray = ((color.r() as u32 + color.g() as u32 + color.b() as u32) / 3) as u8;
                    image::Rgba([gray, gray, gray, 255])
                };

                draw_text_mut(&mut img, text_color, x, y, scale, &font, &ch.to_string());
            }
        }

        Ok(img)
    }

    fn load_image(&mut self, path: &str) -> Result<(), String> {
        match image::open(path) {
            Ok(img) => {
                self.original_dimensions = img.dimensions();
                self.input_image = Some(img.clone());
                self.image_path = path.to_string();
                self.status_message = None;
                self.ascii_applied = false;
                self.ascii_art = String::new();
                self.colored_ascii = Vec::new();
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to load image: {}", e);
                self.status_message = Some((error_msg.clone(), egui::Color32::RED));
                Err(error_msg)
            }
        }
    }

    fn apply_ascii_filter(&mut self) {
        self.ascii_applied = true;
        self.dither_applied = false;
        self.start_conversion();
    }
    
    fn apply_dither_filter(&mut self) {
        if let Some(image) = &self.input_image {
            self.dithered_image = Some(apply_dither(image.clone(), &self.dither_settings));
            self.dither_applied = true;
            self.ascii_applied = false;
            self.cached_preview = None;
        }
    }

    fn start_conversion(&mut self) {
        if let Some(image) = self.input_image.clone() {
            let settings = self.settings.clone();
            let original_dimensions = self.original_dimensions;
            let (sender, receiver) = mpsc::channel();
            
            self.result_receiver = Some(receiver);
            self.processing = true;
            
            thread::spawn(move || {
                let result = convert_image_to_ascii(image, &settings, original_dimensions);
                let _ = sender.send(result);
            });
        }
    }

    fn check_conversion_result(&mut self) {
        if let Some(receiver) = &self.result_receiver {
            if let Ok(result) = receiver.try_recv() {
                self.ascii_art = result.ascii_art;
                self.colored_ascii = result.colored_ascii;
                self.processing = false;
                self.result_receiver = None;
                self.cached_preview = None;
                self.last_preview_settings = None;
            }
        }
    }

    fn check_file_dialog_result(&mut self) {
        if let Some(receiver) = &self.file_dialog_receiver {
            if let Ok(path_option) = receiver.try_recv() {
                if let Some(path) = path_option {
                    if let Some(path_str) = path.to_str() {
                        let _ = self.load_image(path_str);
                    }
                }
                self.file_dialog_receiver = None;
            }
        }
    }

    fn check_save_dialog_result(&mut self) {
        if let Some(receiver) = &self.save_dialog_receiver {
            if let Ok(_) = receiver.try_recv() {
                self.status_message = Some((
                    "✓ File saved!".to_string(),
                    egui::Color32::from_rgb(100, 200, 100)
                ));
                self.save_dialog_receiver = None;
            }
        }
    }

    fn update_conversion(&mut self) {
        if !self.processing && self.ascii_applied {
            self.start_conversion();
        }
    }
    
    fn schedule_update(&mut self) {
        self.pending_update = true;
        self.last_slider_change = Some(std::time::Instant::now());
    }
    
    fn check_pending_updates(&mut self) {
        if self.pending_update {
            if let Some(last_change) = self.last_slider_change {
                if last_change.elapsed().as_millis() > 300 {
                    self.pending_update = false;
                    self.update_conversion();
                }
            }
        }
    }
}

impl eframe::App for AsciiArtApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_conversion_result();
        self.check_file_dialog_result();
        self.check_save_dialog_result();
        self.check_pending_updates();

        // Left sidebar
        egui::SidePanel::left("control_panel")
            .default_width(300.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("ASCII Art Converter");
                ui.add_space(10.0);
                
                egui::ScrollArea::vertical()
                    .id_salt("sidebar_scroll")
                    .show(ui, |ui| {
                        // File Section
                        ui.push_id("file_section", |ui| {
                            egui::CollapsingHeader::new("📁 File")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if self.image_button(ui, &self.icon_folder, "Open").on_hover_text("Open Image").clicked() 
                                            && !self.processing && self.file_dialog_receiver.is_none() {
                                            let (sender, receiver) = mpsc::channel();
                                            self.file_dialog_receiver = Some(receiver);
                                            
                                            thread::spawn(move || {
                                                let result = rfd::FileDialog::new()
                                                    .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                                                    .pick_file();
                                                let _ = sender.send(result);
                                            });
                                        }
                                        
                                        ui.add(egui::TextEdit::singleline(&mut self.image_path)
                                            .hint_text("Image path...")
                                            .desired_width(ui.available_width()));
                                    });
                                    
                                    if self.file_dialog_receiver.is_some() {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("Opening...");
                                        });
                                    }
                                    
                                    if self.original_dimensions != (0, 0) {
                                        ui.label(format!("📐 {} x {} px", 
                                            self.original_dimensions.0, self.original_dimensions.1));
                                    }
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        // Filters Section
                        ui.heading("🎨 Filters");
                        ui.add_space(5.0);
                        
                        // ASCII Filter
                        ui.push_id("ascii_filter", |ui| {
                            egui::CollapsingHeader::new("ASCII Art")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let has_image = self.input_image.is_some();
                                    
                                    if ui.add_enabled(has_image && !self.processing, 
                                        egui::Button::new(if self.ascii_applied { "✓ Applied" } else { "Apply Filter" })
                                        .min_size(egui::vec2(ui.available_width(), 30.0)))
                                        .clicked() {
                                        self.apply_ascii_filter();
                                    }
                                    
                                    if self.ascii_applied {
                                        if ui.button("Remove Filter").clicked() {
                                            self.ascii_applied = false;
                                            self.ascii_art = String::new();
                                            self.colored_ascii = Vec::new();
                                        }
                                    }
                                    
                                    ui.add_space(10.0);
                                    
                                    let enabled = self.ascii_applied;
                                    
                                    ui.add_enabled_ui(enabled, |ui| {
                                        ui.label("Settings:");
                                        ui.add_space(5.0);
                                        
                                        ui.horizontal(|ui| {
                                            ui.label("Colors:");
                                            if ui.checkbox(&mut self.settings.use_colors, "").changed() {
                                                self.schedule_update();
                                            }
                                        });
                                        
                                        ui.add_space(5.0);
                                        
                                        ui.label("Detail Level:");
                                        let current_detail = self.settings.detail_level.clone();
                                        egui::ComboBox::from_id_salt("detail_level")
                                            .selected_text(current_detail.name())
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.settings.detail_level, DetailLevel::Low, DetailLevel::Low.name());
                                                ui.selectable_value(&mut self.settings.detail_level, DetailLevel::Medium, DetailLevel::Medium.name());
                                                ui.selectable_value(&mut self.settings.detail_level, DetailLevel::High, DetailLevel::High.name());
                                                ui.selectable_value(&mut self.settings.detail_level, DetailLevel::VeryHigh, DetailLevel::VeryHigh.name());
                                                ui.selectable_value(&mut self.settings.detail_level, DetailLevel::Custom(100), "Custom");
                                            });
                                        
                                        if let DetailLevel::Custom(width) = &mut self.settings.detail_level {
                                            ui.add(egui::Slider::new(width, 50..=400).text("chars"));
                                            if ui.button("Apply").clicked() {
                                                self.update_conversion();
                                            }
                                        }
                                        
                                        if current_detail != self.settings.detail_level && !matches!(self.settings.detail_level, DetailLevel::Custom(_)) {
                                            self.update_conversion();
                                        }
                                        
                                        ui.add_space(5.0);
                                        
                                        ui.label("Brightness:");
                                        if ui.add(egui::Slider::new(&mut self.settings.brightness, 0.1..=2.0).step_by(0.1)).changed() {
                                            self.schedule_update();
                                        }
                                        
                                        ui.label("Contrast:");
                                        if ui.add(egui::Slider::new(&mut self.settings.contrast, 0.1..=2.0).step_by(0.1)).changed() {
                                            self.schedule_update();
                                        }
                                        
                                        ui.add_space(5.0);
                                        
                                        ui.label("Font Size:");
                                        if ui.add(egui::Slider::new(&mut self.settings.font_size, 6.0..=24.0).text("pt").step_by(1.0)).changed() {
                                            self.cached_preview = None;
                                        }
                                    });
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        // Placeholder for future filters
                        ui.push_id("dither_filter", |ui| {
                            egui::CollapsingHeader::new("Dither")
                                .default_open(false)
                                .show(ui, |ui| {
                                    let has_image = self.input_image.is_some();
                                    
                                    if ui.add_enabled(has_image && !self.processing, 
                                        egui::Button::new(if self.dither_applied { "✓ Applied" } else { "Apply Filter" })
                                        .min_size(egui::vec2(ui.available_width(), 30.0)))
                                        .clicked() {
                                        self.apply_dither_filter();
                                    }
                                    
                                    if self.dither_applied {
                                        if ui.button("Remove Filter").clicked() {
                                            self.dither_applied = false;
                                            self.dithered_image = None;
                                            self.cached_preview = None;
                                        }
                                    }
                                    
                                    ui.add_space(10.0);
                                    
                                    let enabled = self.dither_applied || has_image;
                                    
                                    ui.add_enabled_ui(enabled, |ui| {
                                        ui.label("Settings:");
                                        ui.add_space(5.0);
                                        
                                        ui.label("Algorithm:");
                                        let current_algo = self.dither_settings.algorithm.clone();
                                        egui::ComboBox::from_id_salt("dither_algorithm")
                                            .selected_text(current_algo.name())
                                            .show_ui(ui, |ui| {
                                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::FloydSteinberg, DitherAlgorithm::FloydSteinberg.name());
                                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Atkinson, DitherAlgorithm::Atkinson.name());
                                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Ordered, DitherAlgorithm::Ordered.name());
                                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Threshold, DitherAlgorithm::Threshold.name());
                                            });
                                        
                                        if current_algo != self.dither_settings.algorithm && self.dither_applied {
                                            self.apply_dither_filter();
                                        }
                                        
                                        ui.add_space(5.0);
                                        
                                        if self.dither_settings.algorithm != DitherAlgorithm::Threshold {
                                            ui.label("Color Levels:");
                                            if ui.add(egui::Slider::new(&mut self.dither_settings.color_levels, 2..=16).text("levels")).changed() && self.dither_applied {
                                                self.apply_dither_filter();
                                            }
                                        } else {
                                            ui.label("Threshold:");
                                            if ui.add(egui::Slider::new(&mut self.dither_settings.threshold, 0.0..=255.0).text("value")).changed() && self.dither_applied {
                                                self.apply_dither_filter();
                                            }
                                        }
                                    });
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        // View Options
                        ui.push_id("view_section", |ui| {
                            egui::CollapsingHeader::new("👁 View")
                                .default_open(true)
                                .show(ui, |ui| {
                                    ui.checkbox(&mut self.show_original, "Show Original");
                                });
                        });
                        
                        ui.add_space(5.0);
                        
                        // Export Section
                        ui.push_id("export_section", |ui| {
                            egui::CollapsingHeader::new("💾 Export")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let can_save_ascii = self.save_dialog_receiver.is_none() && !self.colored_ascii.is_empty() && self.ascii_applied;
                                    let can_save_dither = self.save_dialog_receiver.is_none() && self.dither_applied;
                                    let can_save = can_save_ascii || can_save_dither;
                                    
                                    ui.horizontal(|ui| {
                                        if ui.add_enabled(can_save, egui::Button::new("Image")).on_hover_text("Save as PNG/JPEG").clicked() {
                                            let (sender, receiver) = mpsc::channel();
                                            self.save_dialog_receiver = Some(receiver);
                                            
                                            if self.dither_applied {
                                                // Save dithered image directly
                                                let dithered = self.dithered_image.clone();
                                                thread::spawn(move || {
                                                    if let Some(path) = rfd::FileDialog::new()
                                                        .add_filter("PNG", &["png"])
                                                        .add_filter("JPEG", &["jpg", "jpeg"])
                                                        .set_file_name("dithered.png")
                                                        .save_file() {
                                                        if let Some(img) = dithered {
                                                            let _ = img.save(&path);
                                                        }
                                                    }
                                                    let _ = sender.send(None);
                                                });
                                            } else {
                                                // Save ASCII art
                                                let colored_ascii = self.colored_ascii.clone();
                                                let font_size = self.settings.font_size;
                                                let use_colors = self.settings.use_colors;
                                                
                                                thread::spawn(move || {
                                                    if let Some(path) = rfd::FileDialog::new()
                                                        .add_filter("PNG", &["png"])
                                                        .add_filter("JPEG", &["jpg", "jpeg"])
                                                        .set_file_name("ascii_art.png")
                                                        .save_file() {
                                                        let _ = Self::render_ascii_to_image(&colored_ascii, font_size, use_colors)
                                                            .and_then(|img| img.save(&path).map_err(|e| e.to_string()));
                                                    }
                                                    let _ = sender.send(None);
                                                });
                                            }
                                        }

                                        if ui.add_enabled(can_save_ascii, egui::Button::new("Text")).on_hover_text("Save as TXT").clicked() {
                                            let ascii_art = self.ascii_art.clone();
                                            let (sender, receiver) = mpsc::channel();
                                            self.save_dialog_receiver = Some(receiver);
                                            
                                            thread::spawn(move || {
                                                if let Some(path) = rfd::FileDialog::new()
                                                    .add_filter("Text", &["txt"])
                                                    .set_file_name("ascii_art.txt")
                                                    .save_file() {
                                                    let _ = std::fs::write(&path, &ascii_art);
                                                }
                                                                                                    let _ = sender.send(None);
                                            });
                                        }

                                        if ui.add_enabled(can_save_ascii, egui::Button::new("Copy")).on_hover_text("Copy to Clipboard").clicked() {
                                            ui.output_mut(|o| o.copied_text = self.ascii_art.clone());
                                            self.status_message = Some((
                                                "✓ Copied!".to_string(),
                                                egui::Color32::from_rgb(100, 200, 100)
                                            ));
                                        }
                                    });
                                    
                                    if self.save_dialog_receiver.is_some() {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label("Saving...");
                                        });
                                    }
                                });
                        });
                        
                        ui.add_space(10.0);
                        
                        // Status
                        if self.processing {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.colored_label(egui::Color32::from_rgb(255, 200, 100), "Processing...");
                            });
                        }
                        
                        if let Some((message, color)) = &self.status_message {
                            ui.colored_label(*color, message);
                        }
                    });
            });

        // Central panel with info overlay
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.input_image.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 2.0 - 50.0);
                    ui.heading("📸 Drop an image or use File → Open");
                    ui.label("Supported formats: PNG, JPG, BMP, GIF, WebP");
                });
            } else {
                egui::ScrollArea::both()
                    .id_salt("preview_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.show_original || (!self.ascii_applied && !self.dither_applied) {
                            if let Some(input_image) = &self.input_image {
                                let rgba = input_image.to_rgba8();
                                let size = [input_image.width() as usize, input_image.height() as usize];
                                let pixels = rgba.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                
                                let texture = ui.ctx().load_texture("original_image", color_image, egui::TextureOptions::default());
                                let available_size = ui.available_size();
                                let texture_size = texture.size_vec2();
                                let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(3.0).max(0.1);
                                let display_size = texture_size * scale;
                                
                                ui.image(egui::ImageSource::Texture(egui::load::SizedTexture {
                                    id: texture.id(),
                                    size: display_size,
                                }));
                            }
                        } else if self.dither_applied {
                            if let Some(dithered) = &self.dithered_image {
                                let size = [dithered.width() as usize, dithered.height() as usize];
                                let pixels = dithered.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                
                                let texture = ui.ctx().load_texture("dithered_image", color_image, egui::TextureOptions::default());
                                let available_size = ui.available_size();
                                let texture_size = texture.size_vec2();
                                let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(3.0).max(0.1);
                                let display_size = texture_size * scale;
                                
                                ui.image(egui::ImageSource::Texture(egui::load::SizedTexture {
                                    id: texture.id(),
                                    size: display_size,
                                }));
                            }
                        } else if self.ascii_applied && !self.colored_ascii.is_empty() {
                            let preview_font_size = 8.0;
                            let current_settings = (preview_font_size, self.settings.use_colors);
                            
                            let needs_regenerate = self.cached_preview.is_none() || 
                                                   self.last_preview_settings != Some(current_settings);
                            
                            if needs_regenerate {
                                match Self::render_ascii_to_image(&self.colored_ascii, preview_font_size, self.settings.use_colors) {
                                    Ok(img) => {
                                        let size = [img.width() as usize, img.height() as usize];
                                        let pixels = img.as_flat_samples();
                                        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                        
                                        self.cached_preview = Some(ui.ctx().load_texture(
                                            "ascii_rendered", 
                                            color_image, 
                                            egui::TextureOptions::default()
                                        ));
                                        self.last_preview_settings = Some(current_settings);
                                    }
                                    Err(e) => {
                                        ui.colored_label(egui::Color32::RED, format!("Preview error: {}", e));
                                    }
                                }
                            }
                            
                            if let Some(texture) = &self.cached_preview {
                                let available_size = ui.available_size();
                                let texture_size = texture.size_vec2();
                                let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(3.0).max(0.1);
                                let display_size = texture_size * scale;
                                
                                ui.image(egui::ImageSource::Texture(egui::load::SizedTexture {
                                    id: texture.id(),
                                    size: display_size,
                                }));
                            }
                        }
                    });
            }
        });
        
        // Info overlay - drawn last so it's on top
        if self.ascii_applied && !self.colored_ascii.is_empty() && !self.processing {
            let char_width = self.colored_ascii[0].len();
            let char_height = self.colored_ascii.len();
            let char_pixel_width = self.settings.font_size * 0.6;
            let char_pixel_height = self.settings.font_size * 1.2;
            let out_width = (char_width as f32 * char_pixel_width).ceil() as u32;
            let out_height = (char_height as f32 * char_pixel_height).ceil() as u32;
            
            egui::Window::new("info_overlay")
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0))
                .title_bar(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("ASCII: {}×{} chars", char_width, char_height))
                        .color(egui::Color32::WHITE)
                        .size(13.0));
                    ui.label(egui::RichText::new(format!("Output: {}×{} px", out_width, out_height))
                        .color(egui::Color32::WHITE)
                        .size(13.0));
                });
        }

        if self.processing || self.file_dialog_receiver.is_some() || self.save_dialog_receiver.is_some() || self.pending_update {
            ctx.request_repaint();
        }
    }
}