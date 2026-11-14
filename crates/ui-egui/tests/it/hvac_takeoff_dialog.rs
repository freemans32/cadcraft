//! The HVAC Quantities dialog opens from the command with no parameters, shows the quantities of the
//! level as tables and writes the spreadsheet.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, dialogs, takeoffdlg, theme};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

#[test]
fn the_quantities_dialog_shows_the_tables_and_exports_the_spreadsheet() {
    let mut app = CadApp::new(Session::new(), Services::default());
    *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
    app.run("ahuadd", json!({"at": [0, 0]})).unwrap();
    app.run("ductadd", json!({"points": [[800, -300], [4000, -300]], "diameter": 200, "air": "supply"})).unwrap();
    app.run("airterminaladd", json!({"at": [4000, -300], "flow": 200, "air": "supply"})).unwrap();
    // JSON calls keep the JSON form; typed, the command opens the dialog.
    app.run("hvactakeoff", json!({})).unwrap();
    assert!(app.ui.dialog.is_none());
    app.start("hvactakeoff");
    assert_eq!(app.ui.dialog.as_deref(), Some("takeoff"));
    let t = app.takeoff_dlg.as_ref().unwrap().takeoff.clone().unwrap();
    assert_eq!((t.totals.duct_count, t.totals.terminals), (1, 1));
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            theme::apply(ui.ctx(), egui::Theme::Dark, false);
            dialogs::show(app, ui.ctx());
        },
        app,
    );
    h.run_steps(4);
    assert!(h.query_by_label("Ø200").is_some(), "the section of the duct is in the table");
    assert!(h.query_by_label("CTA1").is_some(), "the unit is listed");
    // Export: the file is written, and its place said.
    let path = std::env::temp_dir().join(format!("cadcraft-takeoff-dialog-{}.csv", std::process::id()));
    h.state_mut().takeoff_dlg.as_mut().unwrap().path = path.to_str().unwrap().to_string();
    takeoffdlg::export(h.state_mut());
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert!(text.contains("Ø200"), "{text}");
    assert!(!h.state().takeoff_dlg.as_ref().unwrap().error, "{:?}", h.state().takeoff_dlg.as_ref().unwrap().message);
    // The Excel button writes a workbook (the extension becomes .xlsx) with the calculation note.
    let xlsx = path.with_extension("xlsx");
    takeoffdlg::export_excel(h.state_mut());
    let bytes = std::fs::read(&xlsx).unwrap();
    std::fs::remove_file(&xlsx).ok();
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    assert!(String::from_utf8_lossy(&bytes).contains("Calculation note"));
    assert!(!h.state().takeoff_dlg.as_ref().unwrap().error);
}
