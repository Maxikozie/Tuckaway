//! A simple, original top-view schematic of an 11-button mouse with clickable
//! hotspots per button slot. Coordinates are normalised to the drawing area.

use egui::{pos2, vec2, Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};

use crate::theme;

// Body: superellipse centred at (CX, CY), half sizes A × B, narrower at the front.
const CX: f32 = 0.60;
const CY: f32 = 0.53;
const A: f32 = 0.30;
const B: f32 = 0.45;
const EXP: f32 = 3.2;
/// Normalised y (−1 = front) where the main buttons end.
const SPLIT: f32 = -0.30;
/// Half width of the channel between the main buttons.
const CHANNEL: f32 = 0.035;

fn half_width(ny: f32) -> f32 {
    let taper = 0.86 + 0.14 * ny;
    A * (1.0 - ny.abs().powf(EXP)).max(0.0).powf(1.0 / EXP) * taper
}

fn y_of(ny: f32) -> f32 {
    CY + B * ny
}

#[derive(Clone, Copy)]
enum Look {
    /// Left/right main button: polygon clipped to the body outline.
    Main {
        left: bool,
    },
    Wheel,
    Arrow {
        left: bool,
    },
    Chip(&'static str),
}

struct Hotspot {
    slot: usize,
    rect: [f32; 4],
    look: Look,
}

const fn hs(slot: usize, rect: [f32; 4], look: Look) -> Hotspot {
    Hotspot { slot, rect, look }
}

/// Slot numbers follow `hidpp::layout::SLOTS_046D_C08B`.
const HOTSPOTS: &[Hotspot] = &[
    hs(
        0,
        [0.36, 0.09, CX - CHANNEL, 0.395],
        Look::Main { left: true },
    ),
    hs(
        1,
        [CX + CHANNEL, 0.09, 0.84, 0.395],
        Look::Main { left: false },
    ),
    hs(2, [CX - 0.026, 0.13, CX + 0.026, 0.285], Look::Wheel),
    hs(8, [CX - 0.03, 0.31, CX + 0.03, 0.355], Look::Chip("G9")),
    hs(
        10,
        [CX - 0.135, 0.005, CX - 0.045, 0.055],
        Look::Arrow { left: true },
    ),
    hs(
        9,
        [CX + 0.045, 0.005, CX + 0.135, 0.055],
        Look::Arrow { left: false },
    ),
    hs(7, [0.24, 0.13, 0.315, 0.205], Look::Chip("G8")),
    hs(6, [0.24, 0.22, 0.315, 0.295], Look::Chip("G7")),
    hs(5, [0.05, 0.40, 0.19, 0.47], Look::Chip("G6")),
    hs(4, [0.05, 0.495, 0.19, 0.555], Look::Chip("G5")),
    hs(3, [0.05, 0.57, 0.19, 0.63], Look::Chip("G4")),
];

/// Draws the schematic. Returns the slot that was clicked, if any.
/// `describe(slot)` gives the hover text.
pub fn show(
    ui: &mut Ui,
    max_height: f32,
    slots: usize,
    selected: Option<usize>,
    describe: &dyn Fn(usize) -> String,
) -> Option<usize> {
    let avail = ui.available_size();
    let h = max_height.clamp(300.0, 540.0);
    let w = (h * 0.72).min(avail.x);
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let p = ui.painter_at(rect);
    let at = |x: f32, y: f32| {
        pos2(
            rect.left() + x * rect.width(),
            rect.top() + y * rect.height(),
        )
    };
    let r = |v: [f32; 4]| Rect::from_min_max(at(v[0], v[1]), at(v[2], v[3]));
    let line = Stroke::new(1.5, theme::OUTLINE);

    // Body outline.
    let n = 120;
    let outline: Vec<Pos2> = (0..n)
        .map(|i| {
            let t = i as f32 / n as f32 * std::f32::consts::TAU;
            let ny = t.sin();
            let side = t.cos().signum();
            at(CX + side * half_width(ny), y_of(ny))
        })
        .collect();
    p.add(Shape::convex_polygon(outline, theme::SURFACE_2, line));

    // Thumb side panel.
    p.rect(
        r([0.02, 0.37, 0.22, 0.66]),
        CornerRadius::same(14),
        theme::SURFACE_1,
        Stroke::new(1.0, theme::OUTLINE),
        StrokeKind::Inside,
    );
    p.text(
        at(0.12, 0.675),
        egui::Align2::CENTER_TOP,
        "thumb side",
        egui::FontId::proportional(11.0),
        theme::TEXT_DIM,
    );

    let main_poly = |left: bool| -> Vec<Pos2> {
        let side = if left { -1.0 } else { 1.0 };
        let mut pts = vec![at(CX + side * CHANNEL, y_of(SPLIT))];
        let steps = 24;
        for i in 0..=steps {
            let ny = SPLIT - (SPLIT + 0.985) * i as f32 / steps as f32;
            pts.push(at(CX + side * half_width(ny), y_of(ny)));
        }
        pts.push(at(CX + side * CHANNEL, y_of(-0.985)));
        pts
    };

    let mut clicked = None;
    for hsp in HOTSPOTS.iter().filter(|h| h.slot < slots) {
        let hr = r(hsp.rect);
        let id = ui.scope_id().with(("hotspot", hsp.slot));
        let resp = ui
            .interact(hr, id, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let is_sel = selected == Some(hsp.slot);
        let hot = if is_sel {
            Some(theme::ACCENT.gamma_multiply(0.55))
        } else if resp.hovered() {
            Some(theme::ACCENT.gamma_multiply(0.22))
        } else {
            None
        };
        let stroke = if is_sel {
            Stroke::new(2.0, theme::ACCENT)
        } else {
            Stroke::new(1.0, theme::OUTLINE)
        };
        match hsp.look {
            Look::Main { left } => {
                let pts = main_poly(left);
                let fill = hot.unwrap_or(Color32::TRANSPARENT);
                p.add(Shape::convex_polygon(pts, fill, stroke));
                let c = at(if left { CX - 0.13 } else { CX + 0.13 }, 0.27);
                p.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    if left { "L" } else { "R" },
                    egui::FontId::proportional(14.0),
                    theme::TEXT_DIM,
                );
            }
            Look::Wheel => {
                p.rect(
                    hr,
                    CornerRadius::same(10),
                    hot.unwrap_or(theme::SURFACE_3),
                    stroke,
                    StrokeKind::Inside,
                );
                for k in 1..6 {
                    let y = hr.top() + hr.height() * k as f32 / 6.0;
                    p.line_segment(
                        [pos2(hr.left() + 4.0, y), pos2(hr.right() - 4.0, y)],
                        Stroke::new(1.0, theme::OUTLINE),
                    );
                }
            }
            Look::Arrow { left } => {
                p.rect(
                    hr,
                    CornerRadius::same(6),
                    hot.unwrap_or(theme::SURFACE_3),
                    stroke,
                    StrokeKind::Inside,
                );
                let c = hr.center();
                let s = hr.height() * 0.28;
                let d = if left { -1.0 } else { 1.0 };
                let tri = vec![
                    pos2(c.x + d * s, c.y),
                    pos2(c.x - d * s * 0.8, c.y - s),
                    pos2(c.x - d * s * 0.8, c.y + s),
                ];
                p.add(Shape::convex_polygon(tri, theme::TEXT, Stroke::NONE));
            }
            Look::Chip(tag) => {
                p.rect(
                    hr,
                    CornerRadius::same(7),
                    hot.unwrap_or(theme::SURFACE_3),
                    stroke,
                    StrokeKind::Inside,
                );
                p.text(
                    hr.center(),
                    egui::Align2::CENTER_CENTER,
                    tag,
                    egui::FontId::proportional(11.0),
                    theme::TEXT,
                );
            }
        }
        if resp.clicked() {
            clicked = Some(hsp.slot);
        }
        let text = describe(hsp.slot);
        resp.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, true, format!("mouse {text}"))
        });
        resp.on_hover_text(text);
    }
    clicked
}
