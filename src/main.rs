mod asciiconverter;
mod ditherconverter;
mod gui;

use gui::AsciiArtApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 1000.0])
            .with_min_inner_size([1000.0, 700.0])
            .with_title("Artsify"),
        ..Default::default()
    };

    eframe::run_native(
        "Artsify",
        options,
        Box::new(|cc| Ok(Box::new(AsciiArtApp::new(cc)))),
    )
}