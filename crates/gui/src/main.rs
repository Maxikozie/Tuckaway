#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod keycapture;
mod schematic;
mod theme;
mod worker;

#[cfg(test)]
mod ui_tests;

fn main() -> anyhow::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MouseForge")
            .with_inner_size([1060.0, 700.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "MouseForge",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
