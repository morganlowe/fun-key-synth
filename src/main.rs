fn main() -> eframe::Result {
    // Keep a Tokio runtime active on the main thread so xdg-portal file dialogs (rfd/zbus)
    // always have a reactor when opening Load/Save/Export dialogs on Linux.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to start Tokio runtime");
    let _guard = rt.enter();

    let icon = egui::IconData {
        rgba: include_bytes!("icon_64.rgba").to_vec(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 820.0])
            .with_min_inner_size([1180.0, 620.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Fun Key Synth",
        options,
        Box::new(|cc| Ok(Box::new(fun_key_synth::ui::FunKeySynthApp::new(cc)))),
    )
}
