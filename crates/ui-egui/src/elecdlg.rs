//! The Electrical Quantities window (ELECTAKEOFF typed with no parameters, or the Electricity tab): the circuits of the drawing with what is wrong with each,
//! the cables, the devices and the loads of the HVAC equipment, with a price for each line, and a button that writes them as a spreadsheet for the report of the
//! initial verification.

use cadcraft_doc::ElecTakeoff;
use egui::{RichText, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// The window while open.
#[derive(Clone, Debug, Default)]
pub struct ElecDlgState {
    pub level: String,
    pub path: String,
    pub takeoff: Option<ElecTakeoff>,
    pub message: Option<String>,
    pub error: bool,
    pub raise: bool,
}

/// Open the window on the quantities of the drawing as it is now; open already, count again.
pub fn open(app: &mut CadApp) {
    let mut st = app.elec_dlg.take().unwrap_or_else(|| ElecDlgState { path: "electricite.xlsx".into(), ..Default::default() });
    st.raise = true;
    st.message = None;
    app.elec_dlg = Some(st);
    refresh(app);
    app.ui.dialog = Some("elecreport".into());
}

/// Count again, after the drawing changed.
pub fn refresh(app: &mut CadApp) {
    let level = app.elec_dlg.as_ref().map(|s| s.level.clone()).unwrap_or_default();
    let counted = app.session.doc().map_err(|e| e.to_string()).and_then(|d| d.elec_takeoff(&level, &Default::default()).map_err(|e| e.to_string()));
    if let Some(st) = app.elec_dlg.as_mut() {
        match counted {
            Ok(t) => (st.takeoff, st.error) = (Some(t), false),
            Err(e) => (st.message, st.error) = (Some(e), true),
        }
    }
}

/// Write the file through ELECTAKEOFF, with the prices, and say where.
fn export(app: &mut CadApp, extension: &str) {
    let Some((level, path)) = app.elec_dlg.as_ref().map(|s| (s.level.clone(), s.path.clone())) else { return };
    let path = std::path::Path::new(&path).with_extension(extension).display().to_string();
    let mut params = json!({ "level": level, "path": path });
    if !app.takeoff_store.settings.prices.is_empty() {
        params["prices"] = json!(app.takeoff_store.settings.prices);
    }
    let r = app.run("electakeoff", params);
    if let Some(st) = app.elec_dlg.as_mut() {
        st.path = path.clone();
        match r {
            Ok(v) => (st.message, st.error) = (Some(crate::tf!("Written: {path}", path = v["path"].as_str().unwrap_or(&path))), false),
            Err(e) => (st.message, st.error) = (Some(e), true),
        }
    }
}

fn num(v: f64) -> String {
    format!("{v:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
}

fn opt(v: Option<f64>) -> String {
    v.map_or("—".to_string(), num)
}

fn table(ui: &mut egui::Ui, id: &str, title: &str, heads: &[&str], rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(RichText::new(crate::i18n::t(title)).strong()).id_salt(id).default_open(true).show(ui, |ui| {
        egui::Grid::new(("elec_grid", id)).striped(true).num_columns(heads.len()).show(ui, |ui| {
            for h in heads {
                ui.strong(crate::i18n::t(h));
            }
            ui.end_row();
            for r in rows {
                for c in r {
                    ui.label(c);
                }
                ui.end_row();
            }
        });
    });
}

pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open_flag: &mut bool) {
    if app.elec_dlg.is_none() {
        open(app);
    }
    let warn = Tokens::get().warn;
    let (mut refresh_clicked, mut size_clicked, mut excel_clicked, mut csv_clicked) = (false, false, false, false);
    // The cost of the lines, with the prices of the store (shared with the HVAC quantities, whose keys are others').
    let mut shown = app.elec_dlg.as_ref().and_then(|s| s.takeoff.clone());
    if let Some(t) = shown.as_mut() {
        t.costing = Some(t.priced(&app.takeoff_store.settings));
    }
    let settings = &mut app.takeoff_store.settings;
    let Some(st) = app.elec_dlg.as_mut() else { return };
    let id = egui::Id::unique("elec_dialog");
    if std::mem::take(&mut st.raise) {
        ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, id));
    }
    egui::Window::new(crate::tl!("Electrical Quantities")).id(id).open(open_flag).collapsible(false).default_size(vec2(820.0, 560.0)).show(
        ctx,
        |ui| {
            ui.horizontal(|ui| {
                let label = ui.label(crate::tl!("Level"));
                ui.add(egui::TextEdit::singleline(&mut st.level).desired_width(100.0)).labelled_by(label.id);
                if ui.button(crate::tl!("Count again")).clicked() {
                    refresh_clicked = true;
                }
                if ui.button(crate::tl!("Size Circuits")).clicked() {
                    size_clicked = true;
                }
                let label = ui.label(crate::tl!("File"));
                ui.add(egui::TextEdit::singleline(&mut st.path).desired_width(220.0)).labelled_by(label.id);
                if ui.button(crate::tl!("Export Excel")).clicked() {
                    excel_clicked = true;
                }
                if ui.button(crate::tl!("Export CSV")).clicked() {
                    csv_clicked = true;
                }
            });
            if let Some(m) = &st.message {
                ui.label(if st.error { RichText::new(m).color(warn) } else { RichText::new(m) });
            }
            ui.separator();
            let Some(t) = &shown else { return };
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.label(crate::tf!(
                    "Cables: {m} m, installed power: {kw} kW, circuits: {n}",
                    m = num(t.cable_length_m),
                    kw = num(t.installed_power_w / 1000.0),
                    n = t.circuits.len()
                ));
                egui::CollapsingHeader::new(RichText::new(crate::tl!("Circuits")).strong()).id_salt("elec_circuits").default_open(true).show(
                    ui,
                    |ui| {
                        egui::Grid::new("elec_circuits_grid").striped(true).num_columns(12).show(ui, |ui| {
                            for h in [
                                "Circuit",
                                "Board",
                                "Power (W)",
                                "Current (A)",
                                "Breaker (A)",
                                "Length (m)",
                                "Section drawn (mm²)",
                                "Section required (mm²)",
                                "Voltage drop (%)",
                                "Isc max (kA)",
                                "Isc min (A)",
                                "Board / breaker",
                            ] {
                                ui.strong(crate::i18n::t(h));
                            }
                            ui.end_row();
                            for c in &t.circuits {
                                let name = if c.ok() { RichText::new(&c.name) } else { RichText::new(&c.name).color(warn) };
                                ui.label(name).on_hover_text(c.warnings.join("\n"));
                                ui.label(&c.panel);
                                for v in [
                                    num(c.power_w),
                                    num(c.current_a),
                                    opt(c.rating_a),
                                    num(c.cable_length_m),
                                    opt(c.section_mm2),
                                    opt(c.required_section_mm2),
                                    opt(c.drop_percent),
                                    opt(c.ik_max_ka),
                                    opt(c.ik_min_a),
                                    opt(c.selectivity_ratio),
                                ] {
                                    ui.label(v);
                                }
                                ui.end_row();
                            }
                        });
                        for c in t.circuits.iter().filter(|c| !c.ok()) {
                            for w in &c.warnings {
                                ui.label(RichText::new(format!("{}: {w}", c.name)).small().color(warn));
                            }
                        }
                    },
                );
                table(
                    ui,
                    "cables",
                    "Cables",
                    &["Size", "Use", "Count", "Length (m)"],
                    t.cables.iter().map(|q| vec![q.size.clone(), q.usage.clone(), q.count.to_string(), num(q.length_m)]).collect(),
                );
                if !t.subnetworks.is_empty() {
                    table(
                        ui,
                        "subnetworks",
                        "Sub-networks",
                        &["Circuit", "Sub-network", "Sizes", "Count", "Length (m)"],
                        t.subnetworks
                            .iter()
                            .map(|q| vec![q.network.clone(), q.name.clone(), q.sizes.join(" + "), q.count.to_string(), num(q.length_m)])
                            .collect(),
                    );
                }
                table(
                    ui,
                    "devices",
                    "Devices",
                    &["Device", "Count", "Power (W)", "Flux (lm)"],
                    t.devices
                        .iter()
                        .map(|q| vec![crate::i18n::t(&q.name).to_string(), q.count.to_string(), num(q.power_w), num(q.flux_lm)])
                        .collect(),
                );
                table(
                    ui,
                    "hvac",
                    "HVAC loads",
                    &["Equipment", "Type", "Electrical power (kW)", "Circuit"],
                    t.hvac.iter().map(|h| vec![h.name.clone(), h.kind.clone(), opt(h.electric_kw), h.circuit.clone()]).collect(),
                );
                if let Some(costing) = &t.costing {
                    egui::CollapsingHeader::new(RichText::new(crate::tl!("Prices and cost")).strong())
                        .id_salt("elec_prices")
                        .default_open(false)
                        .show(ui, |ui| {
                            egui::Grid::new("elec_prices_grid").striped(true).num_columns(5).show(ui, |ui| {
                                for h in ["Group", "Label", "Quantity", "Unit price", "Total"] {
                                    ui.strong(crate::i18n::t(h));
                                }
                                ui.end_row();
                                for l in &costing.lines {
                                    ui.label(crate::i18n::t(&l.group));
                                    ui.label(&l.label);
                                    ui.label(format!("{} {}", num(l.quantity), crate::i18n::t(&l.unit)));
                                    let mut price = l.unit_price.unwrap_or(0.0);
                                    if ui.add(egui::DragValue::new(&mut price).speed(0.1).range(0.0..=1.0e6).suffix(" €")).changed() {
                                        if price > 0.0 {
                                            settings.prices.insert(l.key.clone(), price);
                                        } else {
                                            settings.prices.remove(&l.key);
                                        }
                                    }
                                    ui.label(l.total.map_or("—".to_string(), |x| format!("{} €", num(x))));
                                    ui.end_row();
                                }
                            });
                            ui.label(
                                RichText::new(crate::tf!(
                                    "Total: {total} € ({n} lines without a price)",
                                    total = num(costing.total),
                                    n = costing.unpriced
                                ))
                                .strong(),
                            );
                        });
                }
            });
        },
    );
    if size_clicked {
        let r = app.run("circuitsize", json!({}));
        if let Some(st) = app.elec_dlg.as_mut() {
            match r {
                Ok(v) => {
                    (st.message, st.error) = (Some(crate::tf!("Circuits sized, to correct: {n}", n = v["problems"].as_u64().unwrap_or(0))), false)
                }
                Err(e) => (st.message, st.error) = (Some(e), true),
            }
        }
        refresh(app);
    }
    if refresh_clicked {
        refresh(app);
    }
    if excel_clicked {
        export(app, "xlsx");
    }
    if csv_clicked {
        export(app, "csv");
    }
}
