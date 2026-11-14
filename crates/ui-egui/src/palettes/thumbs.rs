//! The small pictures of the Architecture and HVAC tabs, drawn in code in the colours of the
//! Drafting and Modeling icons (`Tokens::icon`, `icon_accent`, `icon_point`). Openings are their
//! own plan symbol, scaled down; the rest are plan or elevation sketches on a 24 x 24 grid.

use cadcraft_doc::{AccessoryKind, AhuKind, EntityKind, EquipmentKind, OpeningKind, OpeningParams, OpeningStyle, PipeLeg, PipeService, WallUnits};
use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};

use crate::theme::Tokens;

/// The square the picture goes in, centred on `r` (the tab's thumbnail slot).
fn square(r: Rect) -> Rect {
    Rect::from_center_size(r.center(), vec2(26.0, 26.0))
}

/// A pen on a 24 x 24 grid.
struct Th<'a> {
    p: &'a Painter,
    r: Rect,
    base: Color32,
    acc: Color32,
    pt: Color32,
}

impl<'a> Th<'a> {
    fn new(p: &'a Painter, r: Rect) -> Self {
        let t = Tokens::get();
        Th { p, r: square(r), base: t.icon, acc: t.icon_accent, pt: t.icon_point }
    }
    fn at(&self, x: f32, y: f32) -> Pos2 {
        let s = self.r.width() / 24.0;
        pos2(self.r.min.x + x * s, self.r.min.y + y * s)
    }
    fn st(&self, c: Color32) -> Stroke {
        Stroke::new(1.3, c)
    }
    fn l(&self, a: (f32, f32), b: (f32, f32), c: Color32) {
        self.p.line_segment([self.at(a.0, a.1), self.at(b.0, b.1)], self.st(c));
    }
    fn poly(&self, pts: &[(f32, f32)], closed: bool, c: Color32) {
        let mut v: Vec<Pos2> = pts.iter().map(|(x, y)| self.at(*x, *y)).collect();
        if closed && let Some(f) = v.first().copied() {
            v.push(f);
        }
        self.p.line(v, self.st(c));
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color32) {
        self.poly(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], true, c);
    }
    fn circle(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.p.circle_stroke(self.at(x, y), r * self.r.width() / 24.0, self.st(c));
    }
    fn dot(&self, x: f32, y: f32) {
        self.p.circle_filled(self.at(x, y), 1.5, self.pt);
    }
    /// A fan: a circle with a cross.
    fn fan(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.circle(x, y, r, c);
        let d = r * 0.7;
        self.l((x - d, y - d), (x + d, y + d), c);
        self.l((x - d, y + d), (x + d, y - d), c);
    }
    /// A dashed line.
    fn dashed(&self, a: (f32, f32), b: (f32, f32), c: Color32) {
        let n = 6;
        for i in 0..n {
            let (t0, t1) = (i as f32 / n as f32, (i as f32 + 0.55) / n as f32);
            let at = |t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            self.l(at(t0), at(t1), c);
        }
    }
    /// Two parallel lines of a duct run from `x0` to `x1`, `y` its middle.
    fn duct(&self, x0: f32, x1: f32, y: f32, c: Color32) {
        self.l((x0, y - 3.0), (x1, y - 3.0), c);
        self.l((x0, y + 3.0), (x1, y + 3.0), c);
    }
    fn arrow(&self, from: (f32, f32), to: (f32, f32), c: Color32) {
        self.l(from, to, c);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let n = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / n, dy / n);
        self.l(to, (to.0 - ux * 3.0 - uy * 2.0, to.1 - uy * 3.0 + ux * 2.0), c);
        self.l(to, (to.0 - ux * 3.0 + uy * 2.0, to.1 - uy * 3.0 - ux * 2.0), c);
    }
    fn pencil(&self) {
        self.l((13.0, 21.0), (21.0, 13.0), self.pt);
        self.l((13.0, 21.0), (12.0, 22.0), self.pt);
    }
}

/// The plan symbol of an opening, as it is drawn in a wall, scaled to the slot.
pub(super) fn paint_opening(p: &Painter, r: Rect, kind: OpeningKind, style: OpeningStyle, size: (f64, f64, f64)) {
    let t = Tokens::get();
    let mut o = OpeningParams::new(kind, WallUnits::default());
    (o.style, o.width_mm, o.height_mm, o.sill_mm) = (style, size.0, size.1, size.2);
    let Ok(pieces) = o.generate() else { return };
    // The wall either side, a quarter of the opening's width long.
    let hw = o.width_mm / 2.0;
    let stub = hw * 0.8;
    let mut pts: Vec<(f64, f64)> = vec![(-hw - stub, -100.0), (hw + stub, 100.0)];
    let mut lines: Vec<([(f64, f64); 2], bool)> = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let jamb = i < 2;
        match &piece.kind {
            EntityKind::Line(l) => {
                lines.push(([(l.a.x, l.a.y), (l.b.x, l.b.y)], jamb));
                pts.extend([(l.a.x, l.a.y), (l.b.x, l.b.y)]);
            }
            EntityKind::LwPolyline(w) => {
                let v: Vec<(f64, f64)> = w.vertices.iter().map(|q| (q.p.x, q.p.y)).collect();
                for k in 0..v.len() {
                    let (a, b) = (v[k], v[(k + 1) % v.len()]);
                    if w.closed || k + 1 < v.len() {
                        lines.push(([a, b], false));
                    }
                }
                pts.extend(v);
            }
            EntityKind::Arc(a) => {
                let n = 12;
                let arc: Vec<(f64, f64)> = (0..=n)
                    .map(|i| {
                        let ang = a.start + (a.end - a.start) * f64::from(i) / f64::from(n);
                        (a.center.x + a.radius * ang.cos(), a.center.y + a.radius * ang.sin())
                    })
                    .collect();
                for w in arc.windows(2) {
                    lines.push(([w[0], w[1]], false));
                }
                pts.extend(arc);
            }
            EntityKind::Circle(c) => {
                let arc: Vec<(f64, f64)> = (0..=24)
                    .map(|i| {
                        let ang = f64::from(i) * std::f64::consts::TAU / 24.0;
                        (c.center.x + c.radius * ang.cos(), c.center.y + c.radius * ang.sin())
                    })
                    .collect();
                for w in arc.windows(2) {
                    lines.push(([w[0], w[1]], false));
                }
                pts.extend(arc);
            }
            _ => {}
        }
    }
    let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |m, q| (m.0.min(q.0), m.1.max(q.0)));
    let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |m, q| (m.0.min(q.1), m.1.max(q.1)));
    let sq = square(r).shrink(1.5);
    let scale = (f64::from(sq.width()) / (x1 - x0).max(1.0)).min(f64::from(sq.height()) / (y1 - y0).max(1.0));
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    // The plan's y runs up, the screen's down.
    let to = |q: (f64, f64)| pos2(sq.center().x + ((q.0 - cx) * scale) as f32, sq.center().y - ((q.1 - cy) * scale) as f32);
    let (wall, line) = (Stroke::new(1.6, t.icon), Stroke::new(1.2, t.icon_accent));
    // The wall: its two faces, either side of the opening.
    for (a, b) in [(-hw - stub, -hw), (hw, hw + stub)] {
        p.line_segment([to((a, 100.0)), to((b, 100.0))], wall);
        p.line_segment([to((a, -100.0)), to((b, -100.0))], wall);
    }
    for (l, jamb) in lines {
        p.line_segment([to(l[0]), to(l[1])], if jamb { wall } else { line });
    }
}

/// A wall seen in plan, with a pencil: Edit Wall.
pub(super) fn paint_edit_wall(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(2.0, 6.0, 16.0, 5.0, t.base);
    t.l((6.0, 6.0), (6.0, 11.0), t.base);
    t.pencil();
}

/// Layers side by side: the wall styles.
pub(super) fn paint_wall_styles(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(3.0, 5.0, 18.0, 14.0, t.base);
    t.l((8.0, 5.0), (8.0, 19.0), t.acc);
    t.l((13.0, 5.0), (13.0, 19.0), t.acc);
    t.l((16.5, 5.0), (16.5, 19.0), t.acc);
}

/// Two walls meeting in an L: Wall Junctions.
pub(super) fn paint_junctions(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.poly(&[(3.0, 4.0), (8.0, 4.0), (8.0, 16.0), (21.0, 16.0), (21.0, 21.0), (3.0, 21.0)], true, t.base);
    t.dot(8.0, 16.0);
}

/// A space with its points of distribution.
pub(super) fn paint_distribute(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(3.0, 4.0, 18.0, 16.0, t.base);
    for (x, y) in [(8.0, 9.0), (16.0, 9.0), (8.0, 15.0), (16.0, 15.0)] {
        t.circle(x, y, 2.2, t.acc);
    }
}

/// An air terminal with a pencil: Edit Air Terminal.
pub(super) fn paint_edit_terminal(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(2.0, 3.0, 13.0, 13.0, t.base);
    t.circle(8.5, 9.5, 3.5, t.acc);
    t.pencil();
}

/// A space outline with a pencil: Edit Space.
pub(super) fn paint_edit_space(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(2.0, 3.0, 13.0, 13.0, t.base);
    t.l((5.0, 8.0), (12.0, 8.0), t.acc);
    t.l((6.0, 11.0), (11.0, 11.0), t.acc);
    t.pencil();
}

/// An air handling unit.
pub(super) fn paint_ahu(p: &Painter, r: Rect, kind: AhuKind) {
    let t = Th::new(p, r);
    match kind {
        AhuKind::DoubleFlow => {
            t.rect(4.0, 7.0, 16.0, 10.0, t.base);
            t.l((4.0, 7.0), (20.0, 17.0), t.acc);
            t.l((4.0, 17.0), (20.0, 7.0), t.acc);
            for (x, y) in [(2.0, 9.5), (2.0, 14.5), (22.0, 9.5), (22.0, 14.5)] {
                t.l((x, y), (if x < 12.0 { 4.0 } else { 20.0 }, y), t.base);
            }
        }
        AhuKind::SupplyUnit => {
            t.rect(3.0, 7.0, 14.0, 10.0, t.base);
            t.arrow((6.0, 12.0), (14.0, 12.0), t.acc);
            t.duct(17.0, 23.0, 12.0, t.base);
        }
        AhuKind::ExtractUnit => {
            t.rect(7.0, 7.0, 14.0, 10.0, t.base);
            t.arrow((18.0, 12.0), (10.0, 12.0), t.acc);
            t.duct(1.0, 7.0, 12.0, t.base);
        }
        AhuKind::VmcBox => {
            t.rect(5.0, 6.0, 14.0, 12.0, t.base);
            t.fan(12.0, 12.0, 3.5, t.acc);
            t.l((2.0, 9.0), (5.0, 9.0), t.base);
            t.l((19.0, 15.0), (22.0, 15.0), t.base);
        }
        AhuKind::StackedDoubleFlow => {
            t.rect(3.0, 3.0, 18.0, 8.0, t.base);
            t.rect(3.0, 13.0, 18.0, 8.0, t.base);
            t.l((3.0, 3.0), (21.0, 11.0), t.acc);
            t.l((3.0, 21.0), (21.0, 13.0), t.acc);
        }
    }
}

/// A riser: the outline of a shaft with the direction of the air.
pub(super) fn paint_riser(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(5.0, 5.0, 14.0, 14.0, t.base);
    t.l((5.0, 5.0), (19.0, 19.0), t.acc);
    t.l((5.0, 19.0), (19.0, 5.0), t.acc);
    t.dot(12.0, 12.0);
}

/// A duct run with a bend, seen from above.
pub(super) fn paint_duct(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.poly(&[(2.0, 12.0), (15.0, 12.0), (15.0, 2.0)], false, t.base);
    t.poly(&[(2.0, 18.0), (21.0, 18.0), (21.0, 2.0)], false, t.base);
    t.dot(2.0, 15.0);
}

/// A zone where ducts have less height: a dashed outline with its height.
pub(super) fn paint_zone(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.dashed((3.0, 5.0), (21.0, 5.0), t.acc);
    t.dashed((21.0, 5.0), (21.0, 19.0), t.acc);
    t.dashed((21.0, 19.0), (3.0, 19.0), t.acc);
    t.dashed((3.0, 19.0), (3.0, 5.0), t.acc);
    t.arrow((12.0, 12.0), (12.0, 6.5), t.base);
    t.arrow((12.0, 12.0), (12.0, 17.5), t.base);
}

/// A terminal joined to a duct by a flexible run.
pub(super) fn paint_connect(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(2.0, 9.0, 6.0, 6.0, t.base);
    t.l((2.0, 9.0), (8.0, 15.0), t.base);
    t.l((2.0, 15.0), (8.0, 9.0), t.base);
    t.poly(&[(8.0, 12.0), (11.0, 9.0), (14.0, 15.0), (17.0, 12.0)], false, t.acc);
    t.l((17.0, 2.0), (17.0, 22.0), t.base);
    t.l((22.0, 2.0), (22.0, 22.0), t.base);
}

/// A duct with its size written on it.
pub(super) fn paint_size(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.duct(1.0, 23.0, 14.0, t.base);
    t.p.text(t.at(12.0, 6.0), egui::Align2::CENTER_CENTER, "Ø", egui::FontId::proportional(10.0), t.acc);
}

/// Two ducts that cross, one broken where it passes under.
pub(super) fn paint_crossing(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.duct(1.0, 23.0, 12.0, t.base);
    for x in [9.0, 15.0] {
        t.l((x, 1.0), (x, 6.0), t.acc);
        t.l((x, 18.0), (x, 23.0), t.acc);
    }
}

/// A damper, fire damper, silencer or flexible connector in a duct.
pub(super) fn paint_accessory(p: &Painter, r: Rect, kind: AccessoryKind) {
    let t = Th::new(p, r);
    match kind {
        AccessoryKind::Damper => {
            t.duct(1.0, 23.0, 12.0, t.base);
            t.l((9.0, 17.0), (15.0, 7.0), t.acc);
        }
        AccessoryKind::FireDamper => {
            t.duct(1.0, 23.0, 12.0, t.base);
            t.l((9.0, 17.0), (15.0, 7.0), t.acc);
            t.rect(10.5, 10.5, 3.0, 3.0, t.pt);
        }
        AccessoryKind::Silencer => {
            t.l((1.0, 9.0), (6.0, 9.0), t.base);
            t.l((1.0, 15.0), (6.0, 15.0), t.base);
            t.l((18.0, 9.0), (23.0, 9.0), t.base);
            t.l((18.0, 15.0), (23.0, 15.0), t.base);
            t.rect(6.0, 5.0, 12.0, 14.0, t.base);
            for y in [9.0, 12.0, 15.0] {
                t.l((8.0, y), (16.0, y), t.acc);
            }
        }
        AccessoryKind::FlexConnector => {
            t.l((1.0, 9.0), (7.0, 9.0), t.base);
            t.l((1.0, 15.0), (7.0, 15.0), t.base);
            t.l((17.0, 9.0), (23.0, 9.0), t.base);
            t.l((17.0, 15.0), (23.0, 15.0), t.base);
            t.poly(&[(7.0, 9.0), (9.0, 7.0), (11.0, 11.0), (13.0, 7.0), (15.0, 11.0), (17.0, 9.0)], false, t.acc);
            t.poly(&[(7.0, 15.0), (9.0, 13.0), (11.0, 17.0), (13.0, 13.0), (15.0, 17.0), (17.0, 15.0)], false, t.acc);
        }
    }
}

/// An item of HVAC equipment, in plan or elevation.
pub(super) fn paint_equipment(p: &Painter, r: Rect, kind: EquipmentKind) {
    let t = Th::new(p, r);
    match kind {
        EquipmentKind::Chiller => {
            t.rect(2.0, 6.0, 20.0, 12.0, t.base);
            t.fan(8.0, 12.0, 3.5, t.acc);
            t.fan(16.0, 12.0, 3.5, t.acc);
        }
        EquipmentKind::VrfOutdoor => {
            t.rect(4.0, 4.0, 16.0, 16.0, t.base);
            t.fan(12.0, 12.0, 6.0, t.acc);
        }
        EquipmentKind::SplitOutdoor => {
            t.rect(4.0, 7.0, 16.0, 10.0, t.base);
            t.fan(12.0, 12.0, 3.5, t.acc);
            t.l((4.0, 17.0), (20.0, 17.0), t.base);
        }
        EquipmentKind::Cassette => {
            t.rect(4.0, 4.0, 16.0, 16.0, t.base);
            t.rect(8.0, 8.0, 8.0, 8.0, t.acc);
            for (x, y) in [(4.0, 4.0), (20.0, 4.0), (4.0, 20.0), (20.0, 20.0)] {
                t.l((x, y), (12.0 + (x - 12.0) * 0.5, 12.0 + (y - 12.0) * 0.5), t.base);
            }
        }
        EquipmentKind::Ducted => {
            t.rect(2.0, 7.0, 12.0, 10.0, t.base);
            t.fan(8.0, 12.0, 3.0, t.acc);
            t.duct(14.0, 23.0, 12.0, t.base);
        }
        EquipmentKind::WallUnit => {
            t.rect(2.0, 4.0, 20.0, 8.0, t.base);
            for x in [7.0, 12.0, 17.0] {
                t.arrow((x, 14.0), (x, 21.0), t.acc);
            }
        }
        EquipmentKind::CeilingUnit => {
            t.rect(2.0, 9.0, 20.0, 6.0, t.base);
            t.arrow((8.0, 17.0), (8.0, 22.0), t.acc);
            t.arrow((16.0, 17.0), (16.0, 22.0), t.acc);
            t.l((2.0, 5.0), (22.0, 5.0), t.base);
        }
        EquipmentKind::FloorUnit => {
            t.rect(6.0, 2.0, 12.0, 20.0, t.base);
            for y in [14.0, 17.0, 20.0] {
                t.l((8.0, y), (16.0, y), t.acc);
            }
        }
    }
}

/// A quantities table.
pub(super) fn paint_quantities(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(3.0, 4.0, 18.0, 16.0, t.base);
    t.l((3.0, 9.0), (21.0, 9.0), t.acc);
    t.l((3.0, 14.0), (21.0, 14.0), t.acc);
    t.l((10.0, 4.0), (10.0, 20.0), t.acc);
}

/// A wall seen straight on: its outline with a door and a window.
pub(super) fn paint_elevation(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.rect(2.0, 5.0, 20.0, 14.0, t.base);
    t.rect(5.0, 10.0, 5.0, 9.0, t.acc);
    t.rect(13.0, 9.0, 6.0, 5.0, t.acc);
    t.l((16.0, 9.0), (16.0, 14.0), t.acc);
}

/// A block turned into an opening: a door symbol and an arrow to the opening in the wall.
pub(super) fn paint_convert(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.l((2.0, 9.0), (8.0, 9.0), t.base);
    t.l((2.0, 15.0), (8.0, 15.0), t.base);
    t.l((8.0, 15.0), (8.0, 5.0), t.pt);
    t.arrow((11.0, 12.0), (15.0, 12.0), t.acc);
    t.l((16.0, 9.0), (22.0, 9.0), t.base);
    t.l((16.0, 15.0), (22.0, 15.0), t.base);
    t.l((17.0, 15.0), (17.0, 9.0), t.acc);
    t.l((21.0, 15.0), (21.0, 9.0), t.acc);
}

/// The colour of a pipe service as its layer is drawn.
fn pipe_color(layer: &str, fallback: Color32) -> Color32 {
    match cadcraft_doc::pipe_layer_color(layer) {
        Some(cadcraft_color::Color::True(rgb)) => Color32::from_rgb(rgb.0, rgb.1, rgb.2),
        _ => fallback,
    }
}

/// A pipe bend in the colours of its service: the two sides, and the band along the axis for the services that have one.
pub(super) fn paint_pipe(p: &Painter, r: Rect, service: PipeService, leg: PipeLeg) {
    let t = Th::new(p, r);
    let layer = cadcraft_doc::pipe_layer_of(service, leg);
    let side = pipe_color(layer, t.base);
    t.poly(&[(3.0, 20.0), (3.0, 7.0), (21.0, 7.0)], false, side);
    t.poly(&[(9.0, 20.0), (9.0, 13.0), (21.0, 13.0)], false, side);
    match service {
        PipeService::ChilledWater => t.poly(&[(6.0, 20.0), (6.0, 10.0), (21.0, 10.0)], false, t.base),
        PipeService::Condensate => t.poly(&[(6.0, 20.0), (6.0, 10.0), (21.0, 10.0)], false, pipe_color("M-PIPE-BAND-YELLOW", t.pt)),
        _ => {}
    }
}

/// A pipe with a pencil: Edit Pipe.
pub(super) fn paint_edit_pipe(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.poly(&[(2.0, 17.0), (2.0, 6.0), (15.0, 6.0)], false, t.acc);
    t.poly(&[(6.0, 17.0), (6.0, 10.0), (15.0, 10.0)], false, t.acc);
    t.pencil();
}

/// A pipe with its size written on it: Size Pipes.
pub(super) fn paint_size_pipes(p: &Painter, r: Rect) {
    let t = Th::new(p, r);
    t.l((2.0, 8.0), (22.0, 8.0), t.acc);
    t.l((2.0, 16.0), (22.0, 16.0), t.acc);
    t.p.text(t.at(12.0, 12.0), egui::Align2::CENTER_CENTER, "DN", egui::FontId::proportional(8.0), t.pt);
}
