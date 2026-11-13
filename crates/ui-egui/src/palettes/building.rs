//! The Tool Sets tabs for building objects: Architecture (wall styles to draw with, wall
//! editing) and HVAC (spaces, air terminals). Their content comes from the drawing (its wall styles), so they are drawn here
//! rather than from a fixed list of tools.

use cadcraft_color::Color;
use cadcraft_doc::{AccessoryKind, AhuKind, AirDirection, ComponentFunction, OpeningKind, TerminalKind};
use egui::{Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::theme::Tokens;

/// The building tabs, after Drafting and Modeling.
pub(super) const TABS: &[&str] = &["Architecture", "HVAC"];

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
    match tab {
        "Architecture" => architecture(app, ui),
        "HVAC" => hvac(app, ui),
        _ => {}
    }
}

/// A small picture of air terminal `kind` in `r`, as it is drawn in plan.
fn paint_terminal(p: &egui::Painter, r: Rect, kind: TerminalKind) {
    let t = Tokens::get();
    let s = Stroke::new(1.2, t.text);
    let c = r.center();
    let h = r.height() / 2.0;
    match kind {
        TerminalKind::SquareCeiling => {
            let q = Rect::from_center_size(c, vec2(2.0 * h, 2.0 * h));
            p.rect_stroke(q, 0.0, s, egui::StrokeKind::Inside);
            p.circle_stroke(c, h * 0.35, s);
            for corner in [q.left_top(), q.right_top(), q.left_bottom(), q.right_bottom()] {
                p.line_segment([c + (corner - c) * 0.4, corner], s);
            }
        }
        TerminalKind::Round => {
            p.circle_stroke(c, h, s);
            p.circle_stroke(c, h * 0.45, s);
        }
        TerminalKind::Rectangular => {
            let q = Rect::from_center_size(c, vec2(r.width(), h));
            p.rect_stroke(q, 0.0, s, egui::StrokeKind::Inside);
            for k in [-0.25f32, 0.0, 0.25] {
                p.line_segment([pos2(q.left() + 2.0, c.y + k * h), pos2(q.right() - 2.0, c.y + k * h)], s);
            }
        }
        TerminalKind::Linear => {
            let q = Rect::from_center_size(c, vec2(r.width(), h * 0.6));
            p.rect_stroke(q, 0.0, s, egui::StrokeKind::Inside);
            p.line_segment([pos2(q.left(), c.y), pos2(q.right(), c.y)], s);
        }
        TerminalKind::Louvre => {
            let q = Rect::from_center_size(c, vec2(2.0 * h, 1.4 * h));
            p.rect_stroke(q, 0.0, s, egui::StrokeKind::Inside);
            for k in [-0.5, 0.0, 0.5] {
                p.line_segment([pos2(q.left() + 3.0, c.y + k * q.height() * 0.8), pos2(q.right() - 3.0, c.y + k * q.height() * 0.8 - 2.0)], s);
            }
        }
        TerminalKind::Extract => {
            p.circle_stroke(c, h, s);
            let d = h * 0.7;
            p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], s);
            p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], s);
        }
    }
}

/// A space outline with lines for its label.
fn paint_space(p: &egui::Painter, r: Rect) {
    let t = Tokens::get();
    let s = Stroke::new(1.2, t.text);
    p.rect_stroke(r, 0.0, s, egui::StrokeKind::Inside);
    p.line_segment([pos2(r.left() + 6.0, r.center().y - 2.0), pos2(r.right() - 6.0, r.center().y - 2.0)], Stroke::new(1.0, t.text_dim));
    p.line_segment([pos2(r.left() + 9.0, r.center().y + 3.0), pos2(r.right() - 9.0, r.center().y + 3.0)], Stroke::new(1.0, t.text_dim));
}

/// The label of a direction named in a sizing report.
fn air_label(name: &str) -> &'static str {
    match name {
        "extract" => "Extract",
        "fresh" => "Fresh air",
        "exhaust" => "Exhaust air",
        _ => "Supply",
    }
}

fn hvac(app: &mut CadApp, ui: &mut egui::Ui) {
    let current = app.session.last_used.air_terminal.kind;
    let app_air = app.session.last_used.air_terminal.air;
    let mut outdoor: Option<AirDirection> = None;
    let mut start: Option<&'static str> = None;
    let mut terminal: Option<TerminalKind> = None;
    let mut distribute = false;
    let mut size = false;
    let mut connect = false;
    let unit = app.session.last_used.ahu.kind;
    let mut chosen_unit: Option<AhuKind> = None;
    let accessory = app.session.last_used.accessory.kind;
    let mut chosen_accessory: Option<AccessoryKind> = None;
    let equipment = app.session.last_used.equipment.kind;
    let mut chosen_equipment: Option<cadcraft_doc::EquipmentKind> = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        header(ui, "Spaces");
        if row_button(ui, crate::i18n::t("Space"), false, paint_space) {
            start = Some("spaceadd");
        }
        if row_button(ui, crate::i18n::t("Edit Space"), false, |_, _| {}) {
            start = Some("spaceedit");
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Air Terminals");
        for (kind, label) in [
            (TerminalKind::SquareCeiling, "Square ceiling diffuser"),
            (TerminalKind::Round, "Round diffuser"),
            (TerminalKind::Rectangular, "Rectangular grille"),
            (TerminalKind::Linear, "Slot diffuser"),
            (TerminalKind::Extract, "Extract valve"),
        ] {
            if row_button(ui, crate::i18n::t(label), kind == current, |p, r| paint_terminal(p, r, kind)) {
                terminal = Some(kind);
            }
        }
        for (air, label) in [(AirDirection::Fresh, "Fresh air louvre"), (AirDirection::Exhaust, "Exhaust louvre")] {
            if row_button(ui, crate::i18n::t(label), current == TerminalKind::Louvre && app_air == air, |p, r| {
                paint_terminal(p, r, TerminalKind::Louvre)
            }) {
                outdoor = Some(air);
            }
        }
        if row_button(ui, crate::i18n::t("Distribute in a Space"), false, |_, _| {}) {
            distribute = true;
        }
        if row_button(ui, crate::i18n::t("Edit Air Terminal"), false, |_, _| {}) {
            start = Some("airterminaledit");
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Ducts");
        for (kind, label) in [
            (AhuKind::DoubleFlow, "Double-flow unit"),
            (AhuKind::SupplyUnit, "Supply unit"),
            (AhuKind::ExtractUnit, "Extract unit"),
            (AhuKind::VmcBox, "Ventilation box"),
            (AhuKind::StackedDoubleFlow, "Double-flow unit, vertical"),
        ] {
            if row_button(ui, crate::i18n::t(label), kind == unit, |_, _| {}) {
                chosen_unit = Some(kind);
            }
        }
        if row_button(ui, crate::i18n::t("Duct"), false, |_, _| {}) {
            start = Some("ductadd");
        }
        if row_button(ui, crate::i18n::t("Height Zone"), false, |_, _| {}) {
            start = Some("zoneadd");
        }
        if row_button(ui, crate::i18n::t("Riser"), false, |_, _| {}) {
            start = Some("riseradd");
        }
        if row_button(ui, crate::i18n::t("Duct Crossing"), false, |_, _| {}) {
            start = Some("ductcross");
        }
        if row_button(ui, crate::i18n::t("HVAC Quantities"), false, |_, _| {}) {
            start = Some("hvactakeoff");
        }
        if row_button(ui, crate::i18n::t("Connect Terminals"), false, |_, _| {}) {
            connect = true;
        }
        if row_button(ui, crate::i18n::t("Size Ducts"), false, |_, _| {}) {
            size = true;
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Duct Accessories");
        for (kind, label) in [
            (AccessoryKind::Damper, "Damper"),
            (AccessoryKind::FireDamper, "Fire damper"),
            (AccessoryKind::Silencer, "Silencer"),
            (AccessoryKind::FlexConnector, "Flexible connector"),
        ] {
            if row_button(ui, crate::i18n::t(label), kind == accessory, |_, _| {}) {
                chosen_accessory = Some(kind);
            }
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Equipment");
        for kind in cadcraft_doc::EquipmentKind::ALL {
            if row_button(ui, crate::i18n::t(equipment_kind_label(kind)), kind == equipment, |_, _| {}) {
                chosen_equipment = Some(kind);
            }
        }
    });
    if let Some(kind) = terminal {
        // The type clicked becomes AIRTERMINALADD's, at the smallest size for the flow.
        let last = &mut app.session.last_used.air_terminal;
        last.kind = kind;
        last.model.clear();
        last.air = if kind == TerminalKind::Extract { AirDirection::Extract } else { AirDirection::Supply };
        app.cmd.buffer.clear();
        app.start("airterminaladd");
    } else if let Some(air) = outdoor {
        // A louvre on the outside: fresh air comes in through it, exhaust air goes out.
        let last = &mut app.session.last_used.air_terminal;
        (last.kind, last.air) = (TerminalKind::Louvre, air);
        last.model.clear();
        app.cmd.buffer.clear();
        app.start("airterminaladd");
    } else if distribute {
        app.cmd.buffer.clear();
        app.start("airterminaladd");
        app.cmdline("Distribute");
    } else if let Some(kind) = chosen_unit {
        // The type clicked becomes AHUADD's, then it starts with it.
        app.session.last_used.ahu.kind = kind;
        app.session.last_used.ahu.size = None;
        app.cmd.buffer.clear();
        app.start("ahuadd");
    } else if let Some(kind) = chosen_equipment {
        // The kind clicked becomes EQUIPMENTADD's, at its first model, then it starts with it.
        app.session.last_used.equipment = cadcraft_engine::cmd::EquipmentLastUsed { kind, model: String::new() };
        app.cmd.buffer.clear();
        app.start("equipmentadd");
    } else if let Some(kind) = chosen_accessory {
        // The type clicked becomes DUCTACCESSORYADD's, then it starts with it.
        app.session.last_used.accessory.kind = kind;
        app.cmd.buffer.clear();
        app.start("ductaccessoryadd");
    } else if connect {
        // DUCTCONNECT asks nothing: it runs at once and says what it did.
        match app.run("ductconnect", serde_json::json!({})) {
            Ok(r) => {
                let n = r["created"].as_array().map_or(0, Vec::len);
                app.session.echo(crate::tf!("Terminals connected: {n}", n = n));
                for k in r["skipped"].as_array().into_iter().flatten() {
                    app.session.echo(format!("{}: {}", k["terminal"].as_str().unwrap_or(""), k["reason"].as_str().unwrap_or("")));
                }
            }
            Err(e) => app.session.echo(e.to_string()),
        }
    } else if size {
        // DUCTSIZE asks nothing: it runs at once and says what it did.
        match app.run("ductsize", serde_json::json!({})) {
            Ok(r) => {
                for net in r["networks"].as_array().into_iter().flatten() {
                    let (flow, pa) = (net["totalFlow"].as_f64().unwrap_or(0.0), net["requiredPressure"].as_f64().unwrap_or(0.0));
                    let air = crate::i18n::t(air_label(net["air"].as_str().unwrap_or("")));
                    app.session.echo(crate::tf!(
                        "Ducts sized, {air}: {flow} m³/h, {pa} Pa",
                        air = air,
                        flow = format!("{flow:.0}"),
                        pa = format!("{pa:.0}")
                    ));
                    for w in net["warnings"].as_array().into_iter().flatten().filter_map(|w| w.as_str()) {
                        app.session.echo(w.to_string());
                    }
                }
                app.session.echo(crate::tf!(
                    "Ducts reached from a unit: {reached} of {ducts}",
                    reached = r["reached"].as_u64().unwrap_or(0),
                    ducts = r["ducts"].as_u64().unwrap_or(0)
                ));
                for b in r["balance"].as_array().into_iter().flatten() {
                    app.session.echo(crate::tf!(
                        "Unbalanced unit: {a} {fa} m³/h against {b} {fb} m³/h",
                        a = crate::i18n::t(air_label(b["left"].as_str().unwrap_or(""))),
                        fa = format!("{:.0}", b["leftFlow"].as_f64().unwrap_or(0.0)),
                        b = crate::i18n::t(air_label(b["right"].as_str().unwrap_or(""))),
                        fb = format!("{:.0}", b["rightFlow"].as_f64().unwrap_or(0.0))
                    ));
                }
                for sp in r["spaces"].as_array().into_iter().flatten() {
                    app.session.echo(crate::tf!(
                        "{name} ({air}): the terminals carry {supplied} m³/h, the space needs {needed} m³/h",
                        name = sp["space"].as_str().unwrap_or(""),
                        air = if sp["air"] == "extract" { crate::i18n::t("Extract") } else { crate::i18n::t("Supply") },
                        supplied = format!("{:.0}", sp["supplied"].as_f64().unwrap_or(0.0)),
                        needed = format!("{:.0}", sp["needed"].as_f64().unwrap_or(0.0))
                    ));
                }
            }
            Err(e) => app.session.echo(e.to_string()),
        }
    } else if let Some(cmd) = start {
        app.cmd.buffer.clear();
        app.start(cmd);
    }
}

fn architecture(app: &mut CadApp, ui: &mut egui::Ui) {
    let styles = app.session.doc().map(|d| d.available_wall_styles()).unwrap_or_default();
    let current = app.session.last_used.wall_style.clone();
    let mut draw: Option<String> = None;
    let mut start: Option<&'static str> = None;
    let mut cleanup = false;
    let opening = app.session.last_used.opening.kind;
    let mut chosen_opening: Option<OpeningKind> = None;
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
        header(ui, "Openings");
        for (kind, label) in
            [(OpeningKind::Door, "Door"), (OpeningKind::DoubleDoor, "Double door"), (OpeningKind::Window, "Window"), (OpeningKind::Bay, "Bay")]
        {
            if row_button(ui, crate::i18n::t(label), kind == opening, |_, _| {}) {
                chosen_opening = Some(kind);
            }
        }
        ui.add_space(6.0);
        separator(ui);
        header(ui, "Tools");
        if row_button(ui, crate::i18n::t("Edit Wall"), false, |_, _| {}) {
            start = Some("walledit");
        }
        if row_button(ui, crate::i18n::t("Wall Styles"), false, |_, _| {}) {
            start = Some("wallstyleedit");
        }
        if row_button(ui, crate::i18n::t("Wall Junctions"), false, |_, _| {}) {
            cleanup = true;
        }
    });
    if let Some(name) = draw {
        // The style clicked becomes WALLADD's style, then WALLADD starts with it.
        app.session.last_used.wall_style = name;
        app.cmd.buffer.clear();
        app.start("walladd");
    } else if let Some(kind) = chosen_opening {
        // The type clicked becomes OPENINGADD's, at its own size, then it starts with it.
        let last = &mut app.session.last_used.opening;
        (last.kind, last.size) = (kind, None);
        app.cmd.buffer.clear();
        app.start("openingadd");
    } else if let Some(cmd) = start {
        app.cmd.buffer.clear();
        app.start(cmd);
    } else if cleanup {
        // WALLCLEANUP asks nothing: it runs at once and says what it did.
        match app.run("wallcleanup", serde_json::json!({})) {
            Ok(r) => {
                let n = r["regenerated"].as_u64().unwrap_or(0);
                app.session.echo(crate::tf!("Walls joined: {n} redrawn", n = n));
            }
            Err(e) => app.session.echo(e.to_string()),
        }
    }
}

/// The label of an equipment kind in the Tool Sets and in Properties.
pub(super) fn equipment_kind_label(k: cadcraft_doc::EquipmentKind) -> &'static str {
    use cadcraft_doc::EquipmentKind::{Cassette, Chiller, Ducted, FloorUnit, SplitOutdoor, VrfOutdoor, WallUnit};
    match k {
        Chiller => "Chiller",
        VrfOutdoor => "VRV outdoor unit",
        SplitOutdoor => "Split outdoor unit",
        Cassette => "Cassette",
        Ducted => "Ducted unit",
        WallUnit => "Wall unit",
        cadcraft_doc::EquipmentKind::CeilingUnit => "Ceiling unit",
        FloorUnit => "Floor unit",
    }
}
