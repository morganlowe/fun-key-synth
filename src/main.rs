fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 820.0])
            .with_min_inner_size([1180.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Fun Key Synth",
        options,
        Box::new(|cc| Ok(Box::new(fun_key_synth::ui::FunKeySynthApp::new(cc)))),
    )
}
