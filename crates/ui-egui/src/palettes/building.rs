//! The Tool Sets tabs for building objects: Architecture (wall styles to draw with, wall
//! editing). Their content comes from the drawing (its wall styles), so they are drawn here
//! rather than from a fixed list of tools.

use cadcraft_color::Color;
use cadcraft_doc::ComponentFunction;
use egui::{Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::theme::Tokens;

/// The building tabs, after Drafting and Modeling.
pub(super) const TABS: &[&str] = &["Architecture"];

fn header(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(
        pos2(r.left() + 10.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        crate::i18n::t(title),
        egui::FontId::proportional(12.5),
        t.text,
    );
}

fn separator(ui: &mut egui::Ui) {
    let t = Tokens::get();
    let (sr, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(sr.x_range().shrink(6.0), sr.center().y, Stroke::new(1.0, t.border));
}

/// A full-width button with a small picture drawn by `paint` on its left; true when clicked.
fn row_button(ui: &mut egui::Ui, label: &str, selected: bool, paint: impl FnOnce(&egui::Painter, Rect)) -> bool {
    let t = Tokens::get();
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
    let r = r.shrink2(vec2(6.0, 2.0));
    let p = ui.painter();
    if selected || resp.hovered() {
        p.rect_filled(r, 4.0, if selected { t.tab_active } else { t.chrome_dark });
    }
    paint(p, Rect::from_min_size(pos2(r.left() + 6.0, r.center().y - 8.0), vec2(30.0, 16.0)));
    p.text(pos2(r.left() + 44.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), t.text);
    crate::icons::describe_control(ui, &resp, label, Some(selected), false);
    resp.clicked()
}

/// A wall style's components side by side, as wide as they are thick.
fn paint_style(p: &egui::Painter, r: Rect, st: &cadcraft_doc::WallStyle) {
    let total = st.thickness().max(1e-9);
    let mut x = r.left();
    for c in &st.components {
        let w = (c.thickness / total) as f32 * r.width();
        // A component with its own colour shows it; one on the layer's colour a shade for what
        // it does, so a wall's make-up reads at a glance.
        let fill = match c.color {
            Color::ByLayer | Color::ByBlock => match c.function {
                ComponentFunction::Structure => egui::Color32::from_rgb(176, 122, 98),
                ComponentFunction::Insulation => egui::Color32::from_rgb(233, 198, 74),
                ComponentFunction::Finish => egui::Color32::from_gray(225),
                ComponentFunction::Membrane => egui::Color32::from_rgb(91, 156, 255),
                ComponentFunction::AirGap => egui::Color32::from_gray(45),
            },
            other => {
                let rgb = other.resolve(Color::Index(8), Color::Index(8));
                egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2)
            }
        };
        p.rect_filled(Rect::from_min_size(pos2(x, r.top()), vec2(w.max(1.0), r.height())), 0.0, fill);
        x += w;
    }
    p.rect_stroke(r, 0.0, Stroke::new(1.0, egui::Color32::from_gray(40)), egui::StrokeKind::Inside);
}

/// The body of building tab `tab`.
pub(super) fn body(app: &mut CadApp, ui: &mut egui::Ui, tab: &str) {
    if tab == "Architecture" {
        architecture(app, ui);
    }
}

fn architecture(app: &mut CadApp, ui: &mut egui::Ui) {
    let styles = app.session.doc().map(|d| d.available_wall_styles()).unwrap_or_default();
    let current = app.session.last_used.wall_style.clone();
    let mut draw: Option<String> = None;
    let mut start: Option<&'static str> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        header(ui, "Walls");
        for (i, st) in styles.iter().enumerate() {
            let selected = st.name.eq_ignore_ascii_case(&current) || (current.is_empty() && i == 0);
            if row_button(ui, &st.name, selected, |p, r| paint_style(p, r, st)) {
                draw = Some(st.name.clone());
            }
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Tools");
        if row_button(ui, crate::i18n::t("Edit Wall"), false, |_, _| {}) {
            start = Some("walledit");
        }
    });
    if let Some(name) = draw {
        // The style clicked becomes WALLADD's style, then WALLADD starts with it.
        app.session.last_used.wall_style = name;
        app.cmd.buffer.clear();
        app.start("walladd");
    } else if let Some(cmd) = start {
        app.cmd.buffer.clear();
        app.start(cmd);
    }
}
