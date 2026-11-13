//! The HVAC Quantities dialog (HVACTAKEOFF typed with no parameters, or the HVAC tab): the quantities
//! of the drawing's level, by kind of object, as tables, and a button that writes them as a spreadsheet.
//! Size the ducts first: the sections are the ones sizing wrote.

use cadcraft_doc::Takeoff;
use egui::{RichText, vec2};
use serde_json::json;

use crate::CadApp;
use crate::theme::Tokens;

/// The dialog while open.
#[derive(Clone, Debug, Default)]
pub struct TakeoffDlgState {
    /// The name of the floor, written on the result and in the file.
    pub level: String,
    /// Where the spreadsheet is written.
    pub path: String,
    pub takeoff: Option<Takeoff>,
    pub message: Option<String>,
    pub error: bool,
    pub raise: bool,
}

/// Open the dialog on the quantities of the drawing as it is now; open already, count again.
pub fn open(app: &mut CadApp) {
    let mut st = app.takeoff_dlg.take().unwrap_or_else(|| TakeoffDlgState { path: "metre.csv".into(), ..Default::default() });
    st.raise = true;
    st.message = None;
    app.takeoff_dlg = Some(st);
    refresh(app);
    app.ui.dialog = Some("takeoff".into());
}

/// Count again, after the drawing changed.
pub fn refresh(app: &mut CadApp) {
    let level = app.takeoff_dlg.as_ref().map(|s| s.level.clone()).unwrap_or_default();
    let counted = app.session.doc().map_err(|e| e.to_string()).and_then(|d| d.takeoff(&level).map_err(|e| e.to_string()));
    if let Some(st) = app.takeoff_dlg.as_mut() {
        match counted {
            Ok(t) => (st.takeoff, st.error) = (Some(t), false),
            Err(e) => (st.message, st.error) = (Some(e), true),
        }
    }
}

/// Write the spreadsheet through HVACTAKEOFF, and say where.
pub fn export(app: &mut CadApp) {
    let Some((level, path)) = app.takeoff_dlg.as_ref().map(|s| (s.level.clone(), s.path.clone())) else { return };
    let r = app.run("hvactakeoff", json!({ "level": level, "path": path }));
    if let Some(st) = app.takeoff_dlg.as_mut() {
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

/// One table: a title, its headings and its rows; nothing when there are no rows.
fn table(ui: &mut egui::Ui, id: &str, title: &str, heads: &[&str], rows: Vec<Vec<String>>) {
    if rows.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(RichText::new(crate::i18n::t(title)).strong()).id_salt(id).default_open(true).show(ui, |ui| {
        egui::Grid::new(("takeoff_grid", id)).striped(true).num_columns(heads.len()).show(ui, |ui| {
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
    if app.takeoff_dlg.is_none() {
        open(app);
    }
    let warn = Tokens::get().warn;
    let (mut refresh_clicked, mut export_clicked) = (false, false);
    let Some(st) = app.takeoff_dlg.as_mut() else { return };
    let id = egui::Id::unique("takeoff_dialog");
    if std::mem::take(&mut st.raise) {
        ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, id));
    }
    egui::Window::new(crate::tl!("HVAC Quantities")).id(id).open(open_flag).collapsible(false).default_size(vec2(760.0, 560.0)).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let label = ui.label(crate::tl!("Level"));
            ui.add(egui::TextEdit::singleline(&mut st.level).desired_width(120.0)).labelled_by(label.id);
            if ui.button(crate::tl!("Count again")).clicked() {
                refresh_clicked = true;
            }
        });
        ui.horizontal(|ui| {
            let label = ui.label(crate::tl!("File"));
            ui.add(egui::TextEdit::singleline(&mut st.path).desired_width(360.0)).labelled_by(label.id);
            if ui.button(crate::tl!("Export spreadsheet")).clicked() {
                export_clicked = true;
            }
        });
        if let Some(m) = &st.message {
            ui.label(if st.error { RichText::new(m).color(warn) } else { RichText::new(m) });
        }
        ui.label(RichText::new(crate::tl!("Size the ducts first: the sections are the ones sizing wrote.")).small());
        ui.separator();
        let Some(t) = &st.takeoff else { return };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let air = |a: cadcraft_doc::AirDirection| {
                crate::i18n::t(match a {
                    cadcraft_doc::AirDirection::Supply => "Supply",
                    cadcraft_doc::AirDirection::Extract => "Extract",
                    cadcraft_doc::AirDirection::Fresh => "Fresh air",
                    cadcraft_doc::AirDirection::Exhaust => "Exhaust air",
                })
            };
            let mut ducts: Vec<Vec<String>> = t
                .ducts
                .iter()
                .map(|d| {
                    vec![
                        air(d.air).to_string(),
                        crate::i18n::t(if d.flexible { "Flexible" } else { "Rigid" }).to_string(),
                        d.section.clone(),
                        opt(d.altitude_m),
                        d.count.to_string(),
                        num(d.length_m),
                        num(d.area_m2),
                    ]
                })
                .collect();
            if !ducts.is_empty() {
                ducts.push(vec![
                    crate::i18n::t("Total").into(),
                    String::new(),
                    String::new(),
                    String::new(),
                    t.totals.duct_count.to_string(),
                    num(t.totals.duct_length_m),
                    num(t.totals.duct_area_m2),
                ]);
            }
            table(ui, "ducts", "Ducts", &["Air", "Material", "Section", "Altitude (m)", "Count", "Length (m)", "Area (m²)"], ducts);
            let counts = |list: &[cadcraft_doc::CountQuantity]| -> Vec<Vec<String>> {
                list.iter().map(|c| vec![crate::i18n::t(&c.name).to_string(), c.size.clone(), c.count.to_string(), opt(c.power)]).collect()
            };
            table(ui, "fittings", "Fittings", &["Name", "Size", "Count", "Power"], counts(&t.fittings));
            table(ui, "accessories", "Duct Accessories", &["Name", "Size", "Count", "Power"], counts(&t.accessories));
            table(
                ui,
                "terminals",
                "Terminals",
                &["Model", "Air", "Count", "Flow (m³/h)"],
                t.terminals.iter().map(|q| vec![q.model.clone(), air(q.air).to_string(), q.count.to_string(), num(q.flow_m3h)]).collect(),
            );
            table(
                ui,
                "units",
                "Units",
                &["Name", "Kind", "Supply", "Extract", "Fresh air", "Exhaust", "Width (mm)", "Depth (mm)", "Height (mm)"],
                t.units
                    .iter()
                    .map(|u| {
                        vec![
                            u.name.clone(),
                            u.kind.clone(),
                            opt(u.supply_m3h),
                            opt(u.extract_m3h),
                            opt(u.fresh_m3h),
                            opt(u.exhaust_m3h),
                            num(u.width_mm),
                            num(u.depth_mm),
                            opt(u.height_mm),
                        ]
                    })
                    .collect(),
            );
            table(
                ui,
                "risers",
                "Risers",
                &["Name", "Air", "Way", "Size", "Flow (m³/h)", "From other floors (m³/h)"],
                t.risers
                    .iter()
                    .map(|r| {
                        vec![
                            r.name.clone(),
                            air(r.air).to_string(),
                            crate::i18n::t(if r.up { "Goes up" } else { "Goes down" }).to_string(),
                            r.size.clone(),
                            opt(r.flow_m3h),
                            opt(r.incoming_m3h),
                        ]
                    })
                    .collect(),
            );
            table(ui, "equipment", "Equipment", &["Name", "Size", "Count", "Power"], counts(&t.equipment));
            table(
                ui,
                "spaces",
                "Spaces",
                &[
                    "Name",
                    "Area (m²)",
                    "Needed (m³/h)",
                    "Supply terminals",
                    "Supply (m³/h)",
                    "Extract terminals",
                    "Extract (m³/h)",
                    "Duct inside (m)",
                ],
                t.spaces
                    .iter()
                    .map(|s| {
                        vec![
                            s.name.clone(),
                            num(s.area_m2),
                            opt(s.needed_m3h),
                            s.supply_terminals.to_string(),
                            num(s.supply_m3h),
                            s.extract_terminals.to_string(),
                            num(s.extract_m3h),
                            num(s.duct_m),
                        ]
                    })
                    .collect(),
            );
        });
    });
    if refresh_clicked {
        refresh(app);
    }
    if export_clicked {
        export(app);
    }
}
