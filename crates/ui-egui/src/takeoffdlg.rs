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
    /// Whether the file was typed or chosen: until it is, it follows the drawing's folder.
    pub path_edited: bool,
    pub takeoff: Option<Takeoff>,
    pub message: Option<String>,
    pub error: bool,
    pub raise: bool,
}

/// Open the dialog on the quantities of the drawing as it is now; open already, count again.
pub fn open(app: &mut CadApp) {
    let mut st = app.takeoff_dlg.take().unwrap_or_else(|| TakeoffDlgState { path: "metre.xlsx".into(), ..Default::default() });
    if !st.path_edited {
        st.path = default_path(app, &st.path);
    }
    st.raise = true;
    st.message = None;
    app.takeoff_dlg = Some(st);
    refresh(app);
    app.ui.dialog = Some("takeoff".into());
}

/// The file the quantities go to when none was chosen: the name kept (`metre.xlsx` at first), in
/// the folder of the drawing that is open; just the name for a drawing not saved yet.
fn default_path(app: &CadApp, current: &str) -> String {
    let name = std::path::Path::new(current).file_name().map_or_else(|| "metre.xlsx".to_string(), |f| f.to_string_lossy().into_owned());
    let folder = app
        .session
        .state()
        .ok()
        .and_then(|st| st.path.clone())
        .and_then(|p| std::path::Path::new(&p).parent().map(std::path::Path::to_path_buf))
        .filter(|d| !d.as_os_str().is_empty());
    folder.map_or(name.clone(), |d| d.join(&name).display().to_string())
}

/// Choose the file with the system's dialog, which opens in the file's folder.
fn browse(app: &mut CadApp) {
    let Some(path) = app.takeoff_dlg.as_ref().map(|s| s.path.clone()) else { return };
    let asked = app.pick_then(crate::Pick::Save(path), |app, chosen| {
        if let Some(st) = app.takeoff_dlg.as_mut() {
            (st.path, st.path_edited) = (chosen, true);
        }
        Ok(serde_json::Value::Null)
    });
    if asked.is_none()
        && let Some(st) = app.takeoff_dlg.as_mut()
    {
        (st.message, st.error) = (Some(crate::tl!("There is no file dialog here: type the file name.").to_string()), true);
    }
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
    let r = app.run("hvactakeoff", json!({ "level": level, "path": path, "lang": crate::i18n::current_language() }));
    if let Some(st) = app.takeoff_dlg.as_mut() {
        match r {
            Ok(v) => (st.message, st.error) = (Some(crate::tf!("Written: {path}", path = v["path"].as_str().unwrap_or(&path))), false),
            Err(e) => (st.message, st.error) = (Some(e), true),
        }
    }
}

/// Write the Excel workbook (the file's extension becomes .xlsx), with the calculation note.
pub fn export_excel(app: &mut CadApp) {
    if let Some(st) = app.takeoff_dlg.as_mut() {
        st.path = std::path::Path::new(&st.path).with_extension("xlsx").display().to_string();
    }
    export(app);
}

/// Write the `;`-separated file (the file's extension becomes .csv).
pub fn export_csv(app: &mut CadApp) {
    if let Some(st) = app.takeoff_dlg.as_mut() {
        st.path = std::path::Path::new(&st.path).with_extension("csv").display().to_string();
    }
    export(app);
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
    let (mut refresh_clicked, mut excel_clicked, mut csv_clicked, mut browse_clicked) = (false, false, false, false);
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
            let typed = ui.add(egui::TextEdit::singleline(&mut st.path).desired_width(300.0)).labelled_by(label.id);
            st.path_edited |= typed.changed();
            if ui.button(crate::tl!("Browse…")).clicked() {
                browse_clicked = true;
            }
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
    if excel_clicked {
        export_excel(app);
    }
    if csv_clicked {
        export_csv(app);
    }
    if browse_clicked {
        browse(app);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::Services;

    fn app_with(pick: Option<Box<dyn Fn(&str) -> Option<String>>>) -> CadApp {
        CadApp::new(cadcraft_engine::Session::new(), Services { pick_save: pick, ..Services::default() })
    }

    fn saved_as(app: &mut CadApp, path: &str) {
        if let Ok(st) = app.session.state_mut() {
            st.path = Some(path.into());
        }
    }

    #[test]
    fn the_file_starts_in_the_folder_of_the_open_drawing_until_it_is_chosen() {
        let mut app = app_with(None);
        open(&mut app);
        assert_eq!(app.takeoff_dlg.as_ref().map(|s| s.path.as_str()), Some("metre.xlsx"), "an unsaved drawing: just the name");
        saved_as(&mut app, "/projets/lycee/plan.dwg");
        open(&mut app);
        let dir = std::path::Path::new("/projets/lycee").join("metre.xlsx").display().to_string();
        assert_eq!(app.takeoff_dlg.as_ref().map(|s| s.path.clone()), Some(dir), "the drawing's folder");
        // Another drawing, another folder, while the file was not chosen.
        saved_as(&mut app, "/autre/dossier/rdc.dxf");
        open(&mut app);
        assert_eq!(
            app.takeoff_dlg.as_ref().map(|s| s.path.clone()),
            Some(std::path::Path::new("/autre/dossier").join("metre.xlsx").display().to_string())
        );
        // Typed: kept as it is.
        if let Some(st) = app.takeoff_dlg.as_mut() {
            (st.path, st.path_edited) = ("/ailleurs/mon-metre.xlsx".into(), true);
        }
        saved_as(&mut app, "/projets/lycee/plan.dwg");
        open(&mut app);
        assert_eq!(app.takeoff_dlg.as_ref().map(|s| s.path.as_str()), Some("/ailleurs/mon-metre.xlsx"));
    }

    #[test]
    fn browse_asks_for_the_file_in_its_folder_and_keeps_the_answer() {
        let asked = Rc::new(RefCell::new(Vec::<String>::new()));
        let seen = asked.clone();
        let mut app = app_with(Some(Box::new(move |name: &str| {
            seen.borrow_mut().push(name.to_string());
            Some("/choisi/metre-rdc.xlsx".into())
        })));
        saved_as(&mut app, "/projets/lycee/plan.dwg");
        open(&mut app);
        browse(&mut app);
        let first = std::path::Path::new("/projets/lycee").join("metre.xlsx").display().to_string();
        assert_eq!(*asked.borrow(), [first], "the dialog is given the file with its folder");
        let st = app.takeoff_dlg.as_ref().expect("the dialog is open");
        assert_eq!(st.path, "/choisi/metre-rdc.xlsx");
        assert!(st.path_edited, "a chosen file stays when the dialog opens again");
        assert!(!st.error);
    }

    #[test]
    fn without_a_file_dialog_browse_says_to_type_the_name() {
        let mut app = app_with(None);
        open(&mut app);
        browse(&mut app);
        let st = app.takeoff_dlg.as_ref().expect("the dialog is open");
        assert!(st.error && st.message.is_some());
        assert_eq!(st.path, "metre.xlsx");
    }
}
