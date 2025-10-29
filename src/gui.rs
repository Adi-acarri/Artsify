use eframe::egui;
use image::{DynamicImage, GenericImageView, RgbaImage};
use imageproc::drawing::draw_text_mut;
use ab_glyph::{FontRef, PxScale};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::asciiconverter::{AsciiSettings, DetailLevel, ConversionResult, convert_image_to_ascii};
use crate::ditherconverter::{DitherSettings, DitherAlgorithm, apply_dither};
use crate::fisheyeconverter::{FisheyeSettings, apply_fisheye};
use crate::crtconverter::{CrtSettings, apply_crt};

const FONT_DATA: &[u8] = include_bytes!("../fonts/DejaVuSansMono.ttf");

pub struct AsciiArtApp {
    input_image: Option<DynamicImage>,
    ascii_art: String,
    colored_ascii: Vec<Vec<(egui::Color32, char)>>,
    pub settings: AsciiSettings,
    pub dither_settings: DitherSettings,
    pub fisheye_settings: FisheyeSettings,
    pub crt_settings: CrtSettings,
    image_path: String,
    original_dimensions: (u32, u32),
    processing: bool,
    active_filter: ActiveFilter,
    dithered_image: Option<RgbaImage>,
    fisheye_image: Option<RgbaImage>,
    crt_image: Option<RgbaImage>,
    result_receiver: Option<mpsc::Receiver<ConversionResult>>,
    file_dialog_receiver: Option<mpsc::Receiver<Option<PathBuf>>>,
    save_dialog_receiver: Option<mpsc::Receiver<Option<PathBuf>>>,
    status_message: Option<(String, egui::Color32)>,
    cached_preview: Option<egui::TextureHandle>,
    cached_original: Option<egui::TextureHandle>,
    cached_dither: Option<egui::TextureHandle>,
    cached_fisheye: Option<egui::TextureHandle>,
    cached_crt: Option<egui::TextureHandle>,
    last_preview_settings: Option<(f32, bool)>,
    pending_update: bool,
    last_slider_change: Option<std::time::Instant>,
    zoom_level: f32,
}

#[derive(Clone, PartialEq)]
enum ActiveFilter {
    None,
    Ascii,
    Dither,
    Fisheye,
    Crt,
}

impl ActiveFilter {
    #[allow(dead_code)]
    fn name(&self) -> &str {
        match self {
            ActiveFilter::None => "None",
            ActiveFilter::Ascii => "ASCII Art",
            ActiveFilter::Dither => "Dither",
            ActiveFilter::Fisheye => "Fisheye",
            ActiveFilter::Crt => "CRT Monitor",
        }
    }
}

impl Default for AsciiArtApp {
    fn default() -> Self {
        Self {
            input_image: None,
            ascii_art: String::new(),
            colored_ascii: Vec::new(),
            settings: AsciiSettings::default(),
            dither_settings: DitherSettings::default(),
            fisheye_settings: FisheyeSettings::default(),
            crt_settings: CrtSettings::default(),
            image_path: String::new(),
            original_dimensions: (0, 0),
            processing: false,
            active_filter: ActiveFilter::None,
            dithered_image: None,
            fisheye_image: None,
            crt_image: None,
            result_receiver: None,
            file_dialog_receiver: None,
            save_dialog_receiver: None,
            status_message: None,
            cached_preview: None,
            cached_original: None,
            cached_dither: None,
            cached_fisheye: None,
            cached_crt: None,
            last_preview_settings: None,
            pending_update: false,
            last_slider_change: None,
            zoom_level: 1.0,
        }
    }
}

impl AsciiArtApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self::default()
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
        let font = FontRef::try_from_slice(FONT_DATA).map_err(|e| format!("Failed to load font: {:?}", e))?;
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
                let rgb_img = img.to_rgb8();
                self.input_image = Some(DynamicImage::ImageRgb8(rgb_img));
                self.image_path = path.to_string();
                self.status_message = None;
                self.active_filter = ActiveFilter::None;
                self.ascii_art = String::new();
                self.colored_ascii = Vec::new();
                self.cached_original = None;
                self.cached_preview = None;
                self.cached_dither = None;
                self.cached_fisheye = None;
                self.cached_crt = None;
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
        self.active_filter = ActiveFilter::Ascii;
        self.start_conversion();
    }
    
    fn apply_dither_filter(&mut self) {
        if let Some(image) = &self.input_image {
            self.dithered_image = Some(apply_dither(image.clone(), &self.dither_settings));
            self.active_filter = ActiveFilter::Dither;
            self.cached_dither = None;
        }
    }
    
    fn apply_fisheye_filter(&mut self) {
        if let Some(image) = &self.input_image {
            self.fisheye_image = Some(apply_fisheye(image.clone(), &self.fisheye_settings));
            self.active_filter = ActiveFilter::Fisheye;
            self.cached_fisheye = None;
        }
    }
    
    fn apply_crt_filter(&mut self) {
        if let Some(image) = &self.input_image {
            self.crt_image = Some(apply_crt(image.clone(), &self.crt_settings));
            self.active_filter = ActiveFilter::Crt;
            self.cached_crt = None;
        }
    }
    
    fn remove_filter(&mut self) {
        self.active_filter = ActiveFilter::None;
        self.ascii_art = String::new();
        self.colored_ascii = Vec::new();
        self.dithered_image = None;
        self.fisheye_image = None;
        self.crt_image = None;
        self.cached_preview = None;
        self.cached_dither = None;
        self.cached_fisheye = None;
        self.cached_crt = None;
    }

    fn rotate_left(&mut self) {
        if let Some(img) = &self.input_image {
            self.input_image = Some(img.rotate270());
            self.original_dimensions = self.input_image.as_ref().unwrap().dimensions();
            self.cached_original = None;
            if self.active_filter != ActiveFilter::None {
                self.reapply_current_filter();
            }
        }
    }

    fn rotate_right(&mut self) {
        if let Some(img) = &self.input_image {
            self.input_image = Some(img.rotate90());
            self.original_dimensions = self.input_image.as_ref().unwrap().dimensions();
            self.cached_original = None;
            if self.active_filter != ActiveFilter::None {
                self.reapply_current_filter();
            }
        }
    }

    fn flip_horizontal(&mut self) {
        if let Some(img) = &self.input_image {
            self.input_image = Some(img.fliph());
            self.cached_original = None;
            if self.active_filter != ActiveFilter::None {
                self.reapply_current_filter();
            }
        }
    }

    fn flip_vertical(&mut self) {
        if let Some(img) = &self.input_image {
            self.input_image = Some(img.flipv());
            self.cached_original = None;
            if self.active_filter != ActiveFilter::None {
                self.reapply_current_filter();
            }
        }
    }

    fn reapply_current_filter(&mut self) {
        match self.active_filter {
            ActiveFilter::Ascii => self.apply_ascii_filter(),
            ActiveFilter::Dither => self.apply_dither_filter(),
            ActiveFilter::Fisheye => self.apply_fisheye_filter(),
            ActiveFilter::Crt => self.apply_crt_filter(),
            ActiveFilter::None => {}
        }
    }

    fn reset_all(&mut self) {
        self.settings = AsciiSettings::default();
        self.dither_settings = DitherSettings::default();
        self.fisheye_settings = FisheyeSettings::default();
        self.crt_settings = CrtSettings::default();
        if self.active_filter != ActiveFilter::None {
            self.reapply_current_filter();
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
                self.status_message = Some(("✓ File saved!".to_string(), egui::Color32::from_rgb(100, 200, 100)));
                self.save_dialog_receiver = None;
            }
        }
    }

    fn update_conversion(&mut self) {
        if !self.processing && self.active_filter == ActiveFilter::Ascii {
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

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("📁 Open").clicked() && !self.processing && self.file_dialog_receiver.is_none() {
                        let (sender, receiver) = mpsc::channel();
                        self.file_dialog_receiver = Some(receiver);
                        thread::spawn(move || {
                            let result = rfd::FileDialog::new()
                                .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                                .pick_file();
                            let _ = sender.send(result);
                        });
                        ui.close_menu();
                    }

                    let can_save_ascii = self.save_dialog_receiver.is_none() && !self.colored_ascii.is_empty() && self.active_filter == ActiveFilter::Ascii;
                    let can_save_dither = self.save_dialog_receiver.is_none() && self.active_filter == ActiveFilter::Dither;
                    let can_save_fisheye = self.save_dialog_receiver.is_none() && self.active_filter == ActiveFilter::Fisheye;
                    let can_save_crt = self.save_dialog_receiver.is_none() && self.active_filter == ActiveFilter::Crt;
                    let can_save = can_save_ascii || can_save_dither || can_save_fisheye || can_save_crt;

                    if ui.add_enabled(can_save, egui::Button::new("💾 Save Image")).clicked() {
                        let (sender, receiver) = mpsc::channel();
                        self.save_dialog_receiver = Some(receiver);
                        
                        if self.active_filter == ActiveFilter::Dither {
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
                        } else if self.active_filter == ActiveFilter::Fisheye {
                            let fisheye = self.fisheye_image.clone();
                            thread::spawn(move || {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PNG", &["png"])
                                    .add_filter("JPEG", &["jpg", "jpeg"])
                                    .set_file_name("fisheye.png")
                                    .save_file() {
                                    if let Some(img) = fisheye {
                                        let _ = img.save(&path);
                                    }
                                }
                                let _ = sender.send(None);
                            });
                        } else if self.active_filter == ActiveFilter::Crt {
                            let crt = self.crt_image.clone();
                            thread::spawn(move || {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PNG", &["png"])
                                    .add_filter("JPEG", &["jpg", "jpeg"])
                                    .set_file_name("crt.png")
                                    .save_file() {
                                    if let Some(img) = crt {
                                        let _ = img.save(&path);
                                    }
                                }
                                let _ = sender.send(None);
                            });
                        } else {
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
                        ui.close_menu();
                    }

                    if ui.add_enabled(can_save_ascii, egui::Button::new("📄 Export Text")).clicked() {
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
                        ui.close_menu();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let has_image = self.input_image.is_some();
                    if ui.add_enabled(has_image, egui::Button::new("🔄 Reset All")).clicked() {
                        self.reset_all();
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.active_filter == ActiveFilter::Ascii && !self.colored_ascii.is_empty(), 
                                      egui::Button::new("📋 Copy ASCII")).clicked() {
                        ui.output_mut(|o| o.copied_text = self.ascii_art.clone());
                        self.status_message = Some(("✓ Copied!".to_string(), egui::Color32::from_rgb(100, 200, 100)));
                        ui.close_menu();
                    }
                });

                ui.menu_button("Transform", |ui| {
                    let has_image = self.input_image.is_some();
                    if ui.add_enabled(has_image, egui::Button::new("↶ Rotate Left")).clicked() {
                        self.rotate_left();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::Button::new("↷ Rotate Right")).clicked() {
                        self.rotate_right();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add_enabled(has_image, egui::Button::new("⇄ Flip Horizontal")).clicked() {
                        self.flip_horizontal();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::Button::new("⇅ Flip Vertical")).clicked() {
                        self.flip_vertical();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Filters", |ui| {
                    let has_image = self.input_image.is_some();
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::None, "None")).clicked() {
                        self.remove_filter();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Ascii, "ASCII Art")).clicked() {
                        self.apply_ascii_filter();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Dither, "Dither")).clicked() {
                        self.apply_dither_filter();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Fisheye, "Fisheye")).clicked() {
                        self.apply_fisheye_filter();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Crt, "CRT Monitor")).clicked() {
                        self.apply_crt_filter();
                        ui.close_menu();
                    }
                });

                ui.separator();
                if self.processing {
                    ui.spinner();
                    ui.label("Processing...");
                }
                if self.file_dialog_receiver.is_some() {
                    ui.spinner();
                    ui.label("Opening...");
                }
                if self.save_dialog_receiver.is_some() {
                    ui.spinner();
                    ui.label("Saving...");
                }
                if let Some((message, color)) = &self.status_message {
                    ui.colored_label(*color, message);
                }
                if self.original_dimensions != (0, 0) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(format!("📐 {} x {} px", self.original_dimensions.0, self.original_dimensions.1));
                    });
                }
            });
        });

        egui::SidePanel::left("control_panel").default_width(300.0).resizable(true).show(ctx, |ui| {
            ui.heading("Filter Settings");
            ui.add_space(10.0);
            
            // Action buttons at top - full width
            ui.horizontal(|ui| {
                let button_width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                
                if ui.add_sized([button_width, 40.0], egui::Button::new("Import")).clicked() && !self.processing && self.file_dialog_receiver.is_none() {
                    let (sender, receiver) = mpsc::channel();
                    self.file_dialog_receiver = Some(receiver);
                    thread::spawn(move || {
                        let result = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif", "webp"])
                            .pick_file();
                        let _ = sender.send(result);
                    });
                }
                
                let can_export = self.input_image.is_some() && self.save_dialog_receiver.is_none();
                if ui.add_enabled_ui(can_export, |ui| {
                    ui.add_sized([button_width, 40.0], egui::Button::new("Export"))
                }).inner.clicked() {
                    let (sender, receiver) = mpsc::channel();
                    self.save_dialog_receiver = Some(receiver);
                    
                    if self.active_filter == ActiveFilter::Dither {
                        let dithered = self.dithered_image.clone();
                        thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("PNG", &["png"])
                                .add_filter("JPEG", &["jpg", "jpeg"])
                                .set_file_name("output.png")
                                .save_file() {
                                if let Some(img) = dithered {
                                    let _ = img.save(&path);
                                }
                            }
                            let _ = sender.send(None);
                        });
                    } else if self.active_filter == ActiveFilter::Fisheye {
                        let fisheye = self.fisheye_image.clone();
                        thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("PNG", &["png"])
                                .add_filter("JPEG", &["jpg", "jpeg"])
                                .set_file_name("output.png")
                                .save_file() {
                                if let Some(img) = fisheye {
                                    let _ = img.save(&path);
                                }
                            }
                            let _ = sender.send(None);
                        });
                    } else if self.active_filter == ActiveFilter::Crt {
                        let crt = self.crt_image.clone();
                        thread::spawn(move || {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("PNG", &["png"])
                                .add_filter("JPEG", &["jpg", "jpeg"])
                                .set_file_name("output.png")
                                .save_file() {
                                if let Some(img) = crt {
                                    let _ = img.save(&path);
                                }
                            }
                            let _ = sender.send(None);
                        });
                    } else if self.active_filter == ActiveFilter::Ascii && !self.colored_ascii.is_empty() {
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
            });
            
            ui.add_space(5.0);
            
            // Zoom buttons
            ui.horizontal(|ui| {
                let button_width = (ui.available_width() - ui.spacing().item_spacing.x * 2.0) / 3.0;
                
                if ui.add_sized([button_width, 40.0], egui::Button::new("Zoom In")).clicked() {
                    self.zoom_level = (self.zoom_level * 1.2).min(5.0);
                }
                if ui.add_sized([button_width, 40.0], egui::Button::new("Zoom Out")).clicked() {
                    self.zoom_level = (self.zoom_level / 1.2).max(0.1);
                }
                if ui.add_sized([button_width, 40.0], egui::Button::new("Reset Zoom")).clicked() {
                    self.zoom_level = 1.0;
                }
            });
            
            ui.add_space(10.0);
            
            // Filter dropdown
            ui.label("Filter:");
            let has_image = self.input_image.is_some();
            let current_filter = self.active_filter.clone();
            egui::ComboBox::from_id_salt("filter_selector")
                .selected_text(current_filter.name())
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::None, "None")).clicked() {
                        self.remove_filter();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Ascii, "ASCII Art")).clicked() {
                        self.apply_ascii_filter();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Dither, "Dither")).clicked() {
                        self.apply_dither_filter();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Fisheye, "Fisheye")).clicked() {
                        self.apply_fisheye_filter();
                    }
                    if ui.add_enabled(has_image, egui::SelectableLabel::new(self.active_filter == ActiveFilter::Crt, "CRT Monitor")).clicked() {
                        self.apply_crt_filter();
                    }
                });
            
            ui.add_space(15.0);
            
            egui::ScrollArea::vertical().id_salt("sidebar_scroll").show(ui, |ui| {
                match self.active_filter {
                    ActiveFilter::Ascii => {
                        egui::CollapsingHeader::new("ASCII Settings").default_open(true).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Colors:");
                                if ui.checkbox(&mut self.settings.use_colors, "").changed() {
                                    self.schedule_update();
                                }
                            });
                            ui.add_space(5.0);
                            ui.label("Detail Level:");
                            let current_detail = self.settings.detail_level.clone();
                            egui::ComboBox::from_id_salt("detail_level").selected_text(current_detail.name()).show_ui(ui, |ui| {
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
                    }
                    ActiveFilter::Dither => {
                        egui::CollapsingHeader::new("Dither Settings").default_open(true).show(ui, |ui| {
                            ui.label("Algorithm:");
                            let current_algo = self.dither_settings.algorithm.clone();
                            egui::ComboBox::from_id_salt("dither_algorithm").selected_text(current_algo.name()).show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::FloydSteinberg, DitherAlgorithm::FloydSteinberg.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Atkinson, DitherAlgorithm::Atkinson.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Jarvis, DitherAlgorithm::Jarvis.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Stucki, DitherAlgorithm::Stucki.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Burkes, DitherAlgorithm::Burkes.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Sierra, DitherAlgorithm::Sierra.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Ordered, DitherAlgorithm::Ordered.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Threshold, DitherAlgorithm::Threshold.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Scanline, DitherAlgorithm::Scanline.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Pattern, DitherAlgorithm::Pattern.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Random, DitherAlgorithm::Random.name());
                                ui.selectable_value(&mut self.dither_settings.algorithm, DitherAlgorithm::Halftone, DitherAlgorithm::Halftone.name());
                            });
                            if current_algo != self.dither_settings.algorithm {
                                self.apply_dither_filter();
                            }
                            ui.add_space(5.0);
                            if self.dither_settings.algorithm != DitherAlgorithm::Threshold {
                                ui.label("Color Levels:");
                                let mut levels = self.dither_settings.color_levels as i32;
                                if ui.add(egui::Slider::new(&mut levels, 2..=16).text("levels")).changed() {
                                    self.dither_settings.color_levels = levels as u8;
                                    self.apply_dither_filter();
                                }
                            } else {
                                ui.label("Threshold:");
                                let mut thresh = self.dither_settings.threshold as i32;
                                if ui.add(egui::Slider::new(&mut thresh, 0..=255).text("value")).changed() {
                                    self.dither_settings.threshold = thresh as f32;
                                    self.apply_dither_filter();
                                }
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Tone Adjustments:");
                            
                            ui.label("Contrast:");
                            let mut contrast_int = (self.dither_settings.contrast * 100.0) as i32;
                            if ui.add(egui::Slider::new(&mut contrast_int, 50..=200).text("%")).changed() {
                                self.dither_settings.contrast = contrast_int as f32 / 100.0;
                                self.apply_dither_filter();
                            }
                            
                            ui.label("Midtones:");
                            let mut midtones_int = (self.dither_settings.midtones * 100.0) as i32;
                            if ui.add(egui::Slider::new(&mut midtones_int, -100..=100).text("shift")).changed() {
                                self.dither_settings.midtones = midtones_int as f32 / 100.0;
                                self.apply_dither_filter();
                            }
                            
                            ui.label("Highlights:");
                            let mut highlights_int = (self.dither_settings.highlights * 100.0) as i32;
                            if ui.add(egui::Slider::new(&mut highlights_int, 50..=150).text("%")).changed() {
                                self.dither_settings.highlights = highlights_int as f32 / 100.0;
                                self.apply_dither_filter();
                            }
                            
                            ui.label("Luminance Threshold:");
                            let mut lum = self.dither_settings.luminance_threshold as i32;
                            if ui.add(egui::Slider::new(&mut lum, 0..=255).text("level")).changed() {
                                self.dither_settings.luminance_threshold = lum as f32;
                                self.apply_dither_filter();
                            }
                            
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Blur:");
                            let mut blur_val = self.dither_settings.blur as i32;
                            if ui.add(egui::Slider::new(&mut blur_val, 0..=5).text("amount")).changed() {
                                self.dither_settings.blur = blur_val as f32;
                                self.apply_dither_filter();
                            }
                        });
                    }
                    ActiveFilter::Fisheye => {
                        egui::CollapsingHeader::new("Fisheye Settings").default_open(true).show(ui, |ui| {
                            ui.label("Strength:");
                            if ui.add(egui::Slider::new(&mut self.fisheye_settings.strength, -0.9..=0.9).text("distortion").step_by(0.05))
                                .on_hover_text("Positive = barrel (fisheye), Negative = pincushion").changed() {
                                self.apply_fisheye_filter();
                            }
                            ui.add_space(5.0);
                            ui.label("Zoom:");
                            if ui.add(egui::Slider::new(&mut self.fisheye_settings.zoom, 0.5..=2.0).text("scale").step_by(0.05)).changed() {
                                self.apply_fisheye_filter();
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Center Point:");
                            ui.horizontal(|ui| {
                                ui.label("X:");
                                if ui.add(egui::Slider::new(&mut self.fisheye_settings.center_x, 0.0..=1.0).text("position").step_by(0.01)).changed() {
                                    self.apply_fisheye_filter();
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Y:");
                                if ui.add(egui::Slider::new(&mut self.fisheye_settings.center_y, 0.0..=1.0).text("position").step_by(0.01)).changed() {
                                    self.apply_fisheye_filter();
                                }
                            });
                            ui.add_space(5.0);
                            if ui.button("Reset Center").clicked() {
                                self.fisheye_settings.center_x = 0.5;
                                self.fisheye_settings.center_y = 0.5;
                                self.apply_fisheye_filter();
                            }
                        });
                    }
                    ActiveFilter::Crt => {
                        egui::CollapsingHeader::new("CRT Settings").default_open(true).show(ui, |ui| {
                            ui.label("Scanline Density:");
                            if ui.add(egui::Slider::new(&mut self.crt_settings.scanline_density, 1.0..=5.0).text("density").step_by(0.5)).changed() {
                                self.apply_crt_filter();
                            }
                            ui.add_space(5.0);
                            ui.label("Brightness Gain:");
                            if ui.add(egui::Slider::new(&mut self.crt_settings.brightness_gain, 0.5..=2.0).text("gain").step_by(0.1)).changed() {
                                self.apply_crt_filter();
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Screen Curvature:");
                            if ui.add(egui::Slider::new(&mut self.crt_settings.curvature, 0.0..=0.3).text("curve").step_by(0.05)).changed() {
                                self.apply_crt_filter();
                            }
                            ui.add_space(5.0);
                            ui.label("Bezel Size:");
                            if ui.add(egui::Slider::new(&mut self.crt_settings.bezel_size, 0.0..=0.2).text("size").step_by(0.01)).changed() {
                                self.apply_crt_filter();
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Vignette:");
                            if ui.add(egui::Slider::new(&mut self.crt_settings.vignette_strength, 0.0..=1.0).text("strength").step_by(0.05)).changed() {
                                self.apply_crt_filter();
                            }
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label("Background:");
                            ui.horizontal(|ui| {
                                ui.label("Color:");
                                let mut bg_color = egui::Color32::from_rgb(
                                    self.crt_settings.bg_color[0],
                                    self.crt_settings.bg_color[1],
                                    self.crt_settings.bg_color[2]
                                );
                                if ui.color_edit_button_srgba(&mut bg_color).changed() {
                                    self.crt_settings.bg_color = [bg_color.r(), bg_color.g(), bg_color.b()];
                                    self.apply_crt_filter();
                                }
                            });
                            ui.label("Background Opacity:");
                            let mut opacity_f32 = self.crt_settings.bg_opacity as f32;
                            if ui.add(egui::Slider::new(&mut opacity_f32, 0.0..=255.0).text("alpha")).changed() {
                                self.crt_settings.bg_opacity = opacity_f32 as u8;
                                self.apply_crt_filter();
                            }
                        });
                    }
                    ActiveFilter::None => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(50.0);
                            ui.heading("No Filter Selected");
                            ui.add_space(10.0);
                            ui.label("Select a filter from the");
                            ui.label("Filters menu to begin");
                        });
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.input_image.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 2.0 - 50.0);
                    ui.heading("📸 Open an image to begin");
                    ui.label("File → Open or drag and drop");
                    ui.label("Supported: PNG, JPG, BMP, GIF, WebP");
                });
            } else {
                egui::ScrollArea::both().id_salt("preview_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    if self.active_filter == ActiveFilter::None {
                        if self.cached_original.is_none() {
                            if let Some(input_image) = &self.input_image {
                                let (img_w, img_h) = input_image.dimensions();
                                let max_preview = 2048;
                                let preview_img = if img_w > max_preview || img_h > max_preview {
                                    input_image.resize(max_preview, max_preview, image::imageops::FilterType::Triangle)
                                } else {
                                    input_image.clone()
                                };
                                let rgba = preview_img.to_rgba8();
                                let size = [preview_img.width() as usize, preview_img.height() as usize];
                                let pixels = rgba.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                self.cached_original = Some(ui.ctx().load_texture("original_image", color_image, egui::TextureOptions::LINEAR));
                            }
                        }
                        if let Some(texture) = &self.cached_original {
                            let texture_size = texture.size_vec2();
                            let display_size = texture_size * self.zoom_level;
                            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                        }
                    } else if self.active_filter == ActiveFilter::Dither {
                        if self.cached_dither.is_none() {
                            if let Some(dithered) = &self.dithered_image {
                                let size = [dithered.width() as usize, dithered.height() as usize];
                                let pixels = dithered.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                self.cached_dither = Some(ui.ctx().load_texture("dithered_image", color_image, egui::TextureOptions::NEAREST));
                            }
                        }
                        if let Some(texture) = &self.cached_dither {
                            let texture_size = texture.size_vec2();
                            let display_size = texture_size * self.zoom_level;
                            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                        }
                    } else if self.active_filter == ActiveFilter::Fisheye {
                        if self.cached_fisheye.is_none() {
                            if let Some(fisheye) = &self.fisheye_image {
                                let size = [fisheye.width() as usize, fisheye.height() as usize];
                                let pixels = fisheye.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                self.cached_fisheye = Some(ui.ctx().load_texture("fisheye_image", color_image, egui::TextureOptions::LINEAR));
                            }
                        }
                        if let Some(texture) = &self.cached_fisheye {
                            let available_size = ui.available_size();
                            let texture_size = texture.size_vec2();
                            let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(2.0).max(0.1);
                            let display_size = texture_size * scale;
                            let (rect, response) = ui.allocate_exact_size(display_size, egui::Sense::click_and_drag());
                            ui.put(rect, egui::Image::new(egui::ImageSource::Texture(egui::load::SizedTexture::new(texture.id(), display_size))));
                            let center_x = rect.min.x + display_size.x * self.fisheye_settings.center_x;
                            let center_y = rect.min.y + display_size.y * self.fisheye_settings.center_y;
                            let center_pos = egui::pos2(center_x, center_y);
                            let painter = ui.painter();
                            let cross_size = 20.0;
                            let cross_color = egui::Color32::BLACK;
                            painter.line_segment([egui::pos2(center_pos.x - cross_size, center_pos.y), egui::pos2(center_pos.x + cross_size, center_pos.y)], egui::Stroke::new(2.0, cross_color));
                            painter.line_segment([egui::pos2(center_pos.x, center_pos.y - cross_size), egui::pos2(center_pos.x, center_pos.y + cross_size)], egui::Stroke::new(2.0, cross_color));
                            painter.circle_stroke(center_pos, 5.0, egui::Stroke::new(2.0, cross_color));
                            if response.dragged() || response.clicked() {
                                if let Some(mouse_pos) = response.interact_pointer_pos() {
                                    let new_x = ((mouse_pos.x - rect.min.x) / display_size.x).clamp(0.0, 1.0);
                                    let new_y = ((mouse_pos.y - rect.min.y) / display_size.y).clamp(0.0, 1.0);
                                    if new_x != self.fisheye_settings.center_x || new_y != self.fisheye_settings.center_y {
                                        self.fisheye_settings.center_x = new_x;
                                        self.fisheye_settings.center_y = new_y;
                                        self.apply_fisheye_filter();
                                    }
                                }
                            }
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                            }
                        }
                    } else if self.active_filter == ActiveFilter::Crt {
                        if self.cached_crt.is_none() {
                            if let Some(crt) = &self.crt_image {
                                let size = [crt.width() as usize, crt.height() as usize];
                                let pixels = crt.as_flat_samples();
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                self.cached_crt = Some(ui.ctx().load_texture("crt_image", color_image, egui::TextureOptions::LINEAR));
                            }
                        }
                        if let Some(texture) = &self.cached_crt {
                            let available_size = ui.available_size();
                            let texture_size = texture.size_vec2();
                            let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(2.0).max(0.1);
                            let display_size = texture_size * scale;
                            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                        }
                    } else if self.active_filter == ActiveFilter::Ascii && !self.colored_ascii.is_empty() {
                        let preview_font_size = 8.0;
                        let current_settings = (preview_font_size, self.settings.use_colors);
                        let needs_regenerate = self.cached_preview.is_none() || self.last_preview_settings != Some(current_settings);
                        if needs_regenerate {
                            match Self::render_ascii_to_image(&self.colored_ascii, preview_font_size, self.settings.use_colors) {
                                Ok(img) => {
                                    let size = [img.width() as usize, img.height() as usize];
                                    let pixels = img.as_flat_samples();
                                    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                                    self.cached_preview = Some(ui.ctx().load_texture("ascii_rendered", color_image, egui::TextureOptions::NEAREST));
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
                            let scale = (available_size.x / texture_size.x).min(available_size.y / texture_size.y).min(2.0).max(0.1);
                            let display_size = texture_size * scale;
                            ui.image(egui::load::SizedTexture::new(texture.id(), display_size));
                        }
                    }
                });
            }
        });

        if self.active_filter == ActiveFilter::Ascii && !self.colored_ascii.is_empty() && !self.processing {
            let char_width = self.colored_ascii[0].len();
            let char_height = self.colored_ascii.len();
            let char_pixel_width = self.settings.font_size * 0.6;
            let char_pixel_height = self.settings.font_size * 1.2;
            let out_width = (char_width as f32 * char_pixel_width).ceil() as u32;
            let out_height = (char_height as f32 * char_pixel_height).ceil() as u32;
            egui::Window::new("info_overlay").anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-10.0, -10.0)).title_bar(false).resizable(false).show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("ASCII: {}×{} chars", char_width, char_height)).color(egui::Color32::WHITE).size(13.0));
                ui.label(egui::RichText::new(format!("Output: {}×{} px", out_width, out_height)).color(egui::Color32::WHITE).size(13.0));
            });
        }

        if self.processing || self.file_dialog_receiver.is_some() || self.save_dialog_receiver.is_some() || self.pending_update {
            ctx.request_repaint();
        }
    }
}