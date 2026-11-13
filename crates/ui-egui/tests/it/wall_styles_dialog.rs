//! The Wall Styles dialog: opened by WALLSTYLEEDIT with no parameters, it lists the styles, makes a new
//! one from the selected, applies the layers as edited (a rename of a style of the drawing renames it,
//! of a standard one copies it) and deletes the drawing's own styles.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, dialogs, theme, wallstyledlg};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use serde_json::json;

fn app() -> CadApp {
    let mut app = CadApp::new(Session::new(), Services::default());
    *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
    app
}

fn draft(app: &CadApp) -> cadcraft_doc::WallStyle {
    app.wall_style_dlg.as_ref().unwrap().draft.clone().unwrap()
}

/// The dialog opened and drawn once, so the first style is loaded.
fn shown(app: &mut CadApp) {
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            theme::apply(ui.ctx(), egui::Theme::Dark, false);
            dialogs::show(app, ui.ctx());
        },
        std::mem::replace(app, CadApp::new(Session::new(), Services::default())),
    );
    h.run_steps(4);
    *app = std::mem::replace(h.state_mut(), CadApp::new(Session::new(), Services::default()));
}

#[test]
fn the_dialog_opens_from_the_command_and_lists_the_styles_with_the_layers_of_the_first() {
    let mut a = app();
    a.start("wallstyleedit");
    assert_eq!(a.ui.dialog.as_deref(), Some("wallstyles"));
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            theme::apply(ui.ctx(), egui::Theme::Dark, false);
            dialogs::show(app, ui.ctx());
        },
        a,
    );
    h.run_steps(4);
    assert!(h.query_by_label("Concrete 200").is_some(), "the standard styles are listed");
    assert!(h.query_by_label_contains("Add layer").is_some(), "the layers can be added");
    // JSON calls keep the JSON form and do not open the dialog.
    let mut b = app();
    b.run("wallstyleedit", json!({})).unwrap();
    assert!(b.ui.dialog.is_none());
}

#[test]
fn a_new_style_copies_the_selected_one_and_the_layers_as_edited_are_applied_to_the_drawing() {
    let mut a = app();
    wallstyledlg::open(&mut a);
    shown(&mut a);
    let first = draft(&a);
    wallstyledlg::create(&mut a, "Mon mur");
    assert_eq!(draft(&a).name, "Mon mur");
    assert_eq!(draft(&a).components, first.components, "a copy of the selected style");
    assert!(a.session.doc().unwrap().wall_style("Mon mur").is_some());
    // The first layer made thicker, a layer added, then applied.
    let st = a.wall_style_dlg.as_mut().unwrap();
    st.draft.as_mut().unwrap().components[0].thickness = 250.0;
    st.draft.as_mut().unwrap().components.push(cadcraft_doc::WallComponent::new("BA13", 13.0, cadcraft_doc::ComponentFunction::Finish));
    wallstyledlg::apply(&mut a);
    assert!(a.wall_style_dlg.as_ref().unwrap().error.is_none(), "{:?}", a.wall_style_dlg.as_ref().unwrap().error);
    let saved = a.session.doc().unwrap().wall_style("Mon mur").unwrap().clone();
    assert_eq!((saved.components[0].thickness, saved.components.len()), (250.0, first.components.len() + 1));
    assert_eq!(a.wall_style_dlg.as_ref().unwrap().draft, a.wall_style_dlg.as_ref().unwrap().loaded, "nothing left to apply");
    // A wall drawn with it is redrawn when the style changes again.
    let w = a.run("walladd", json!({"points": [[0, 0], [4000, 0]], "style": "Mon mur"})).unwrap();
    let before =
        cadcraft_doc::wall_of(a.session.doc().unwrap().entity(cadcraft_doc::Handle::parse_hex(w["handle"].as_str().unwrap()).unwrap()).unwrap())
            .unwrap();
    assert_eq!(before.style.components[0].thickness, 250.0);
    a.wall_style_dlg.as_mut().unwrap().draft.as_mut().unwrap().components[0].thickness = 300.0;
    wallstyledlg::apply(&mut a);
    let after =
        cadcraft_doc::wall_of(a.session.doc().unwrap().entity(cadcraft_doc::Handle::parse_hex(w["handle"].as_str().unwrap()).unwrap()).unwrap())
            .unwrap();
    assert_eq!(after.style.components[0].thickness, 300.0, "the wall follows its style");
}

#[test]
fn renaming_a_style_of_the_drawing_renames_it_a_standard_one_is_copied_and_the_drawings_own_are_deleted() {
    let mut a = app();
    wallstyledlg::open(&mut a);
    shown(&mut a);
    let standard = draft(&a).name;
    // A standard style under another name: a copy, the standard one stays.
    a.wall_style_dlg.as_mut().unwrap().draft.as_mut().unwrap().name = "Copie".into();
    wallstyledlg::apply(&mut a);
    assert!(a.wall_style_dlg.as_ref().unwrap().error.is_none(), "{:?}", a.wall_style_dlg.as_ref().unwrap().error);
    assert!(a.session.doc().unwrap().wall_style("Copie").is_some());
    assert!(a.session.doc().unwrap().available_wall_styles().iter().any(|s| s.name == standard));
    // A style of the drawing under another name: renamed.
    a.wall_style_dlg.as_mut().unwrap().draft.as_mut().unwrap().name = "Renomme".into();
    wallstyledlg::apply(&mut a);
    assert!(a.session.doc().unwrap().wall_style("Copie").is_none() && a.session.doc().unwrap().wall_style("Renomme").is_some());
    // An error is shown, not applied: no layers.
    a.wall_style_dlg.as_mut().unwrap().draft.as_mut().unwrap().components.clear();
    wallstyledlg::apply(&mut a);
    assert!(a.wall_style_dlg.as_ref().unwrap().error.is_some());
    // Deleted.
    wallstyledlg::delete(&mut a);
    assert!(a.session.doc().unwrap().wall_style("Renomme").is_none());
}
