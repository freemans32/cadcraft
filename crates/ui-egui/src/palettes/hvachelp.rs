//! The "i" beside an HVAC tool: a window with a drawing of what the tool does (seen from above, as
//! on the plan) and the steps to follow. The drawings are made here, from lines and arcs.

use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};

use crate::theme::Tokens;

/// One tool's help: its English name (the tool's label), the steps, and its drawing.
struct Help {
    id: &'static str,
    steps: &'static [&'static str],
    figure: fn(&Painter, Rect),
}

const OPEN: &str = "hvac_help_open";

const HELPS: &[Help] = &[
    Help {
        id: "Riser",
        steps: &[
            "Start the tool and click the corners of the shaft, then press Enter.",
            "Type the flow that comes from the other floors in Properties.",
            "Size Ducts gives the riser its section; the arrow shows if the air goes up or down.",
        ],
        figure: riser,
    },
    Help {
        id: "Duct",
        steps: &[
            "Click the points of the run: one duct is drawn per segment.",
            "Start on a unit's collar, end on a terminal; start on another duct to make a tee.",
            "Leave the direction on auto: Size Ducts finds supply or extract from the terminals the run reaches.",
        ],
        figure: duct,
    },
    Help {
        id: "Height Zone",
        steps: &[
            "Click the corners of the outline where ducts have less height (a beam, a low ceiling), seen from above.",
            "Type the height available for the ducts, in mm. An altitude is optional.",
            "Size Ducts cuts the ducts at the edges and sizes them flat inside the zone.",
        ],
        figure: zone,
    },
    Help {
        id: "Connect Terminals",
        steps: &[
            "No selection needed: every terminal that no duct reaches is taken.",
            "A flexible duct is drawn from each one to the nearest duct, 3 m at most.",
            "Terminals too far from any duct are listed and left alone.",
        ],
        figure: connect,
    },
    Help {
        id: "Size Ducts",
        steps: &[
            "Run it when the ducts and terminals are drawn; it reads each network from its unit or riser.",
            "Each duct gets its section, and its flow and velocity are written on it.",
            "Run it again after any change: sections and flows are recalculated.",
        ],
        figure: size,
    },
    Help {
        id: "Duct Crossing",
        steps: &[
            "Click the two ducts that cross.",
            "Choose which one passes under or over the other.",
            "It is drawn broken, or with four 45° elbows when the duct has to go round the other.",
        ],
        figure: crossing,
    },
    Help {
        id: "HVAC Quantities",
        steps: &[
            "Draw one drawing per level, then open this tool on the level.",
            "The tables count ducts by section, fittings, terminals, units, risers and equipment.",
            "The spreadsheet button saves them for Excel; each level makes its own file.",
        ],
        figure: quantities,
    },
];

/// Asks for the help of the tool whose label is `id` to open.
pub(super) fn request(ctx: &egui::Context, id: &'static str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(OPEN), id));
}

/// Shows the window asked for, if any.
pub(super) fn show(ctx: &egui::Context) {
    let Some(id) = ctx.data(|d| d.get_temp::<&'static str>(egui::Id::unique(OPEN))) else { return };
    let Some(help) = HELPS.iter().find(|h| h.id == id) else { return };
    let mut open = true;
    egui::Window::new(crate::i18n::t(help.id)).id(egui::Id::unique("hvac_help_window")).open(&mut open).collapsible(false).resizable(false).show(
        ctx,
        |ui| {
            ui.set_width(440.0);
            let (r, _) = ui.allocate_exact_size(vec2(440.0, 170.0), egui::Sense::hover());
            let t = Tokens::get();
            ui.painter().rect_filled(r, 4.0, t.chrome_dark);
            (help.figure)(&ui.painter().with_clip_rect(r), r.shrink(10.0));
            ui.add_space(8.0);
            for (i, step) in help.steps.iter().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{}.", i + 1));
                    ui.label(crate::i18n::t(step));
                });
            }
        },
    );
    if !open {
        ctx.data_mut(|d| d.remove::<&'static str>(egui::Id::unique(OPEN)));
    }
}

const DUCT: Color32 = Color32::from_rgb(0x2f, 0x86, 0xe8);
const DUCT_FILL: Color32 = Color32::from_rgb(0x1e, 0x33, 0x50);
const ZONE: Color32 = Color32::from_rgb(0xf0, 0xa1, 0x5f);
const GREY: Color32 = Color32::from_gray(150);

fn stroke() -> Stroke {
    Stroke::new(1.4, DUCT)
}

/// A horizontal duct from `x0` to `x1`, `w` wide, centred on `y`.
fn hduct(p: &Painter, x0: f32, x1: f32, y: f32, w: f32) {
    let r = Rect::from_min_max(pos2(x0, y - w / 2.0), pos2(x1, y + w / 2.0));
    p.rect_filled(r, 0.0, DUCT_FILL);
    p.rect_stroke(r, 0.0, stroke(), egui::StrokeKind::Inside);
}

fn vduct(p: &Painter, x: f32, y0: f32, y1: f32, w: f32) {
    let r = Rect::from_min_max(pos2(x - w / 2.0, y0), pos2(x + w / 2.0, y1));
    p.rect_filled(r, 0.0, DUCT_FILL);
    p.rect_stroke(r, 0.0, stroke(), egui::StrokeKind::Inside);
}

fn text(p: &Painter, at: egui::Pos2, s: &str, color: Color32) {
    p.text(at, egui::Align2::LEFT_CENTER, s, egui::FontId::proportional(11.0), color);
}

/// A terminal: a square with a cross.
fn terminal(p: &Painter, c: egui::Pos2) {
    let r = Rect::from_center_size(c, vec2(16.0, 16.0));
    p.rect_stroke(r, 0.0, Stroke::new(1.2, Color32::WHITE), egui::StrokeKind::Inside);
    p.line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.0, Color32::WHITE));
    p.line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.0, Color32::WHITE));
}

/// Diagonal hatching of `r`.
fn hatch(p: &Painter, r: Rect) {
    let s = Stroke::new(1.0, ZONE.gamma_multiply(0.7));
    let mut k = -r.height();
    while k < r.width() {
        let a = pos2(r.left() + k, r.bottom());
        let b = pos2(r.left() + k + r.height(), r.top());
        // Clip the line to the rectangle by the painter's own clip rect.
        p.with_clip_rect(r).line_segment([a, b], s);
        k += 9.0;
    }
}

fn riser(p: &Painter, r: Rect) {
    let c = r.center();
    let shaft = Rect::from_center_size(c, vec2(90.0, 60.0));
    p.rect_filled(shaft, 0.0, DUCT_FILL);
    p.rect_stroke(shaft, 0.0, stroke(), egui::StrokeKind::Inside);
    p.circle_stroke(c, 16.0, stroke());
    p.line_segment([c, c + vec2(70.0, -45.0)], Stroke::new(1.0, DUCT));
    text(p, c + vec2(72.0, -45.0), "Riser 1  Ø500  7000 m³/h", DUCT);
    p.arrow(c + vec2(-30.0, 28.0), vec2(0.0, -20.0), Stroke::new(1.2, DUCT));
}

fn duct(p: &Painter, r: Rect) {
    let y = r.center().y;
    hduct(p, r.left() + 40.0, r.right() - 60.0, y, 22.0);
    vduct(p, r.center().x, r.top() + 10.0, y, 16.0);
    let unit = Rect::from_center_size(pos2(r.left() + 20.0, y), vec2(40.0, 50.0));
    p.rect_stroke(unit, 0.0, Stroke::new(1.2, GREY), egui::StrokeKind::Inside);
    text(p, unit.center() - vec2(10.0, 0.0), "CTA", GREY);
    terminal(p, pos2(r.right() - 40.0, y));
    terminal(p, pos2(r.center().x, r.top() + 10.0));
    text(p, pos2(r.left() + 60.0, y - 24.0), "1", DUCT);
    text(p, pos2(r.center().x + 14.0, y - 30.0), "2", DUCT);
}

fn zone(p: &Painter, r: Rect) {
    let y = r.center().y;
    let z = Rect::from_min_max(pos2(r.center().x - 60.0, r.top()), pos2(r.center().x + 60.0, r.bottom()));
    p.rect_filled(z, 0.0, ZONE.gamma_multiply(0.18));
    hatch(p, z);
    p.rect_stroke(z, 0.0, Stroke::new(1.6, ZONE), egui::StrokeKind::Inside);
    hduct(p, r.left(), z.left(), y, 30.0);
    hduct(p, z.left(), z.right(), y, 18.0);
    hduct(p, z.right(), r.right(), y, 30.0);
    // The labels sit on a patch of the background, over the hatching.
    let patch = |at: egui::Pos2| p.rect_filled(Rect::from_min_size(at, vec2(66.0, 16.0)), 2.0, Tokens::get().chrome_dark);
    patch(pos2(z.left() + 4.0, r.top() + 4.0));
    text(p, pos2(z.left() + 8.0, r.top() + 12.0), "H 300 mm", ZONE);
    patch(pos2(z.left() + 4.0, y + 16.0));
    text(p, pos2(z.left() + 8.0, y + 24.0), "600×200", DUCT);
    text(p, pos2(r.left() + 6.0, y - 24.0), "Ø400", DUCT);
    text(p, pos2(r.right() - 44.0, y - 24.0), "Ø400", DUCT);
}

fn connect(p: &Painter, r: Rect) {
    let y = r.center().y + 20.0;
    hduct(p, r.left(), r.right(), y, 20.0);
    let t = pos2(r.center().x, r.top() + 22.0);
    terminal(p, t);
    // The flexible duct: a dashed line from the terminal to the duct.
    let mut yy = t.y + 10.0;
    while yy < y - 10.0 {
        p.line_segment([pos2(t.x, yy), pos2(t.x, (yy + 5.0).min(y - 10.0))], Stroke::new(1.6, DUCT));
        yy += 9.0;
    }
    text(p, t + vec2(16.0, 6.0), "flexible", DUCT);
}

fn size(p: &Painter, r: Rect) {
    let y = r.center().y;
    hduct(p, r.left(), r.center().x + 20.0, y, 26.0);
    hduct(p, r.center().x + 20.0, r.right(), y, 16.0);
    vduct(p, r.center().x - 40.0, r.top() + 12.0, y - 13.0, 14.0);
    text(p, pos2(r.left() + 4.0, y + 24.0), "Ø315  1400 m³/h", DUCT);
    text(p, pos2(r.center().x + 26.0, y + 22.0), "Ø200  400 m³/h", DUCT);
    text(p, pos2(r.center().x - 30.0, r.top() + 12.0), "Ø200  350 m³/h", DUCT);
    terminal(p, pos2(r.right() - 10.0, y));
}

fn crossing(p: &Painter, r: Rect) {
    let c = r.center();
    vduct(p, c.x, r.top(), r.bottom(), 36.0);
    hduct(p, r.left(), c.x - 70.0, c.y, 22.0);
    hduct(p, c.x + 70.0, r.right(), c.y, 22.0);
    // Four 45° elbows, each a shallow arc bulging toward the middle of the jog.
    for (i, dx) in [-70.0_f32, -34.0, 34.0, 70.0].into_iter().enumerate() {
        let x = c.x + dx;
        let bulge = if i < 2 { 5.0 } else { -5.0 };
        let pts: Vec<_> = (0..=8)
            .map(|k| {
                let t = k as f32 / 8.0;
                let off = bulge * (1.0 - (2.0 * t - 1.0).powi(2));
                pos2(x + off, c.y - 11.0 + 22.0 * t)
            })
            .collect();
        p.add(egui::Shape::line(pts, stroke()));
    }
    p.line_segment([pos2(c.x - 34.0, c.y - 11.0), pos2(c.x + 34.0, c.y - 11.0)], stroke());
    p.line_segment([pos2(c.x - 34.0, c.y + 11.0), pos2(c.x + 34.0, c.y + 11.0)], stroke());
    text(p, pos2(c.x + 24.0, r.top() + 14.0), "-458", DUCT);
}

fn quantities(p: &Painter, r: Rect) {
    let rows = [("Ø200", "12", "34,5 m"), ("Ø315", "5", "18,2 m"), ("600×200", "2", "6,0 m")];
    let x = r.left() + 20.0;
    text(p, pos2(x, r.top() + 14.0), "Section", GREY);
    text(p, pos2(x + 120.0, r.top() + 14.0), "Count", GREY);
    text(p, pos2(x + 200.0, r.top() + 14.0), "Length", GREY);
    for (i, (a, b, c)) in rows.into_iter().enumerate() {
        let y = r.top() + 40.0 + 28.0 * i as f32;
        p.hline(r.x_range(), y - 14.0, Stroke::new(1.0, Color32::from_gray(70)));
        text(p, pos2(x, y), a, Color32::WHITE);
        text(p, pos2(x + 120.0, y), b, Color32::WHITE);
        text(p, pos2(x + 200.0, y), c, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn every_tool_has_a_help_window_that_draws_and_closes() {
        assert_eq!(HELPS.len(), 7);
        for help in HELPS {
            let mut h = Harness::builder().with_size(vec2(700.0, 500.0)).build_ui(|ui| {
                show(ui.ctx());
            });
            request(&h.ctx, help.id);
            h.run_steps(3);
            assert!(h.query_by_label(help.steps[0]).is_some(), "{}: the first step is shown", help.id);
            // Its window is gone once closed.
            h.ctx.data_mut(|d| d.remove::<&'static str>(egui::Id::unique(OPEN)));
            h.run_steps(2);
            assert!(h.query_by_label(help.steps[0]).is_none(), "{}: closed", help.id);
        }
    }
}

#[cfg(test)]
mod shots {
    use super::*;
    use egui_kittest::Harness;

    /// Writes each drawing to the directory in CADCRAFT_HELP_SHOTS, when it is set (a look at the real rendering).
    #[test]
    fn drawings_can_be_saved_to_look_at() {
        let Some(dir) = std::env::var_os("CADCRAFT_HELP_SHOTS") else { return };
        for help in HELPS {
            let mut h = Harness::builder().with_size(vec2(480.0, 380.0)).build_ui(|ui| {
                show(ui.ctx());
            });
            request(&h.ctx, help.id);
            h.run_steps(3);
            if let Ok(img) = h.render() {
                let _ = img.save(std::path::Path::new(&dir).join(format!("{}.png", help.id.replace(' ', "_"))));
            }
        }
    }
}
