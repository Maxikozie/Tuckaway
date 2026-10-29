//! Dark theme.

use egui::{Color32, CornerRadius, Stroke};

pub const BG: Color32 = Color32::from_rgb(0x15, 0x17, 0x1c);
pub const SURFACE_1: Color32 = Color32::from_rgb(0x1c, 0x1f, 0x26);
pub const SURFACE_2: Color32 = Color32::from_rgb(0x24, 0x28, 0x31);
pub const SURFACE_3: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3e);
pub const OUTLINE: Color32 = Color32::from_rgb(0x4a, 0x51, 0x60);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe8, 0xec);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x95, 0x9c, 0xaa);
pub const ACCENT: Color32 = Color32::from_rgb(0x4f, 0x9d, 0xff);
pub const OK: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const WARN: Color32 = Color32::from_rgb(0xf2, 0xb3, 0x4c);
pub const ERR: Color32 = Color32::from_rgb(0xef, 0x5f, 0x5f);

/// Add Segoe UI (and its symbol companion) from the Windows fonts folder as
/// fallbacks, so arrows and layout-specific key labels always have glyphs.
fn add_system_fallback_fonts(ctx: &egui::Context) {
    let dir = std::env::var_os("WINDIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| r"C:\Windows".into())
        .join("Fonts");
    let mut fonts = egui::FontDefinitions::default();
    let mut added = false;
    for file in ["segoeui.ttf", "seguisym.ttf"] {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            fonts.font_data.insert(
                file.into(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(file.into());
            }
            added = true;
        }
    }
    if added {
        ctx.set_fonts(fonts);
    }
}

pub fn apply(ctx: &egui::Context) {
    add_system_fallback_fonts(ctx);
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        let v = &mut s.visuals;
        v.panel_fill = BG;
        v.window_fill = SURFACE_1;
        v.extreme_bg_color = Color32::from_rgb(0x10, 0x12, 0x16);
        v.faint_bg_color = SURFACE_1;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = ACCENT.gamma_multiply(0.6);
        v.selection.stroke = Stroke::new(1.0, TEXT);
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = CornerRadius::same(10);
        for w in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(6);
        }
        v.widgets.inactive.weak_bg_fill = SURFACE_2;
        v.widgets.inactive.bg_fill = SURFACE_2;
        v.widgets.hovered.weak_bg_fill = SURFACE_3;
        v.widgets.hovered.bg_fill = SURFACE_3;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, SURFACE_3);
    });
}
