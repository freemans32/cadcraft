//! The Wall Styles dialog (WALLSTYLEEDIT typed with no parameters, or the Architecture tab): the
//! drawing's wall styles and the standard ones in a list, and for the one selected its layers, side
//! by side from one face to the other: name, thickness, function, hatch, priority. Apply sends the
//! edited style to WALLSTYLEEDIT, which redraws the walls that use it; New copies the selected style
//! under a new name. The dialog edits a copy and never touches the drawing until Apply.

use cadcraft_doc::{ComponentFunction, WallComponent, WallStyle};
use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::CadApp;
use crate::theme::Tokens;

/// The dialog while open.
#[derive(Clone, Debug, Default)]
pub struct WallStyleDlgState {
    /// The style the list has selected, by name.
    pub selected: String,
    /// The selected style as edited, and as it was when loaded.
    pub draft: Option<WallStyle>,
    pub loaded: Option<WallStyle>,
    /// The name field of New, while it is shown.
    pub new_name: Option<String>,
    pub error: Option<String>,
    /// WALLSTYLEEDIT typed again while open: bring the window to the front on the next frame.
    pub raise: bool,
}

const FUNCTIONS: [(ComponentFunction, &str); 5] = [
    (ComponentFunction::Structure, "Structure"),
    (ComponentFunction::Insulation, "Insulation"),
    (ComponentFunction::Finish, "Finish"),
    (ComponentFunction::Membrane, "Membrane"),
    (ComponentFunction::AirGap, "Air gap"),
];

/// Open the dialog; open already, bring it forward.
pub fn open(app: &mut CadApp) {
    if app.ui.dialog.as_deref() == Some("wallstyles")
        && let Some(st) = app.wall_style_dlg.as_mut()
    {
        st.raise = true;
        return;
    }
    app.wall_style_dlg = Some(WallStyleDlgState::default());
    app.ui.dialog = Some("wallstyles".into());
}

fn styles(app: &CadApp) -> Vec<WallStyle> {
    app.session.doc().map(|d| d.available_wall_styles()).unwrap_or_default()
}

/// Whether the drawing holds a style of its own named `name` (a standard style is not deleted).
fn in_drawing(app: &CadApp, name: &str) -> bool {
    app.session.doc().is_ok_and(|d| d.wall_style(name).is_some())
}

fn load(st: &mut WallStyleDlgState, style: &WallStyle) {
    st.selected = style.name.clone();
    st.draft = Some(style.clone());
    st.loaded = Some(style.clone());
    st.error = None;
}

/// The parameters of the WALLSTYLEEDIT call that gives the style its edited make-up.
fn call(style: &WallStyle) -> Value {
    json!({
        "name": style.name,
        "description": style.description,
        "components": style.components,
        "defaultHeight": style.default_height,
    })
}

/// Send the edited style: a new name makes a copy of a standard style and renames one of the drawing's.
pub fn apply(app: &mut CadApp) {
    let Some(st) = app.wall_style_dlg.as_ref() else { return };
    let (Some(draft), Some(loaded)) = (st.draft.clone(), st.loaded.clone()) else { return };
    let mut params = call(&draft);
    let renamed = !draft.name.eq_ignore_ascii_case(&loaded.name);
    let result = if renamed && in_drawing(app, &loaded.name) {
        app.run("wallstyleedit", json!({ "name": loaded.name, "rename": draft.name })).and_then(|_| app.run("wallstyleedit", params))
    } else {
        if renamed {
            params["copyFrom"] = json!(loaded.name);
        }
        app.run("wallstyleedit", params)
    };
    let fresh = styles(app).into_iter().find(|s| s.name.eq_ignore_ascii_case(&draft.name));
    let Some(st) = app.wall_style_dlg.as_mut() else { return };
    match result {
        Ok(_) => {
            st.error = None;
            if let Some(s) = fresh {
                load(st, &s);
            }
        }
        Err(e) => st.error = Some(e),
    }
}

/// New style `name`, a copy of the selected one.
pub fn create(app: &mut CadApp, name: &str) {
    let Some(from) = app.wall_style_dlg.as_ref().map(|s| s.selected.clone()).filter(|s| !s.is_empty()) else { return };
    let result = app.run("wallstyleedit", json!({ "name": name.trim(), "copyFrom": from }));
    let fresh = styles(app).into_iter().find(|s| s.name.eq_ignore_ascii_case(name.trim()));
    let Some(st) = app.wall_style_dlg.as_mut() else { return };
    match result {
        Ok(_) => {
            st.new_name = None;
            if let Some(s) = fresh {
                load(st, &s);
            }
            st.error = None;
        }
        Err(e) => st.error = Some(e),
    }
}

pub fn delete(app: &mut CadApp) {
    let Some(name) = app.wall_style_dlg.as_ref().map(|s| s.selected.clone()).filter(|s| !s.is_empty()) else { return };
    let result = app.run("wallstyleedit", json!({ "delete": name }));
    let first = styles(app).into_iter().next();
    let Some(st) = app.wall_style_dlg.as_mut() else { return };
    match result {
        Ok(_) => {
            st.draft = None;
            st.loaded = None;
            st.selected.clear();
            if let Some(s) = first {
                load(st, &s);
            }
        }
        Err(e) => st.error = Some(e),
    }
}

fn function_label(f: ComponentFunction) -> &'static str {
    FUNCTIONS.iter().find(|x| x.0 == f).map_or("Structure", |x| x.1)
}

/// The layers of the style as a table; true when a layer was added, moved or removed.
fn layers(ui: &mut egui::Ui, style: &mut WallStyle, patterns: &[&'static str]) {
    let mut remove = None;
    let mut swap = None;
    let n = style.components.len();
    egui::ScrollArea::both().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("wallstyle_layers").striped(true).num_columns(10).show(ui, |ui| {
            let mut head = Vec::new();
            for h in ["", "Layer", "Thickness (mm)", "Function", "Hatch", "Scale", "Angle", "Priority", "", ""] {
                head.push(ui.strong(if h.is_empty() { "" } else { crate::i18n::t(h) }).id);
            }
            ui.end_row();
            for (i, c) in style.components.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new("^").small()).on_hover_text(crate::i18n::t("Move up")).clicked() {
                        swap = Some((i, i - 1));
                    }
                    if ui.add_enabled(i + 1 < n, egui::Button::new("v").small()).on_hover_text(crate::i18n::t("Move down")).clicked() {
                        swap = Some((i, i + 1));
                    }
                });
                ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(150.0)).labelled_by(head[1]);
                let membrane = c.function == ComponentFunction::Membrane;
                if membrane {
                    c.thickness = 0.0;
                }
                ui.add_enabled(!membrane, egui::DragValue::new(&mut c.thickness).speed(1.0).range(0.0..=5000.0)).labelled_by(head[2]);
                let combo = craft_ui::ComboBox::from_id_salt(("wallstyle_function", i))
                    .selected_text(crate::i18n::t(function_label(c.function)))
                    .width(110.0)
                    .show_ui(ui, |ui| {
                        for (f, label) in FUNCTIONS {
                            ui.selectable_value(&mut c.function, f, crate::i18n::t(label));
                        }
                    });
                combo.response.labelled_by(head[3]);
                let shown = if c.pattern.is_empty() { crate::i18n::t("None").to_string() } else { c.pattern.clone() };
                let combo = craft_ui::ComboBox::from_id_salt(("wallstyle_pattern", i)).selected_text(shown).width(110.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.pattern, String::new(), crate::i18n::t("None"));
                    ui.selectable_value(&mut c.pattern, "SOLID".to_string(), "SOLID");
                    for p in patterns {
                        ui.selectable_value(&mut c.pattern, (*p).to_string(), *p);
                    }
                });
                combo.response.labelled_by(head[4]);
                ui.add(egui::DragValue::new(&mut c.pattern_scale).speed(0.1).range(0.01..=1000.0)).labelled_by(head[5]);
                ui.add(egui::DragValue::new(&mut c.pattern_angle).speed(1.0).range(-360.0..=360.0)).labelled_by(head[6]);
                ui.add(egui::DragValue::new(&mut c.priority).speed(1.0).range(0..=1000)).labelled_by(head[7]);
                if ui.small_button("x").on_hover_text(crate::i18n::t("Remove layer")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    });
    if let Some((a, b)) = swap {
        style.components.swap(a, b);
    }
    if let Some(i) = remove {
        style.components.remove(i);
    }
}

pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open_flag: &mut bool) {
    if app.wall_style_dlg.is_none() {
        open(app);
    }
    let all = styles(app);
    // A first look selects the first style.
    if let Some(st) = app.wall_style_dlg.as_mut()
        && st.draft.is_none()
        && let Some(first) = all.first()
    {
        load(st, first);
    }
    let own: Vec<bool> = all.iter().map(|s| in_drawing(app, &s.name)).collect();
    let patterns: Vec<&'static str> = cadcraft_doc::library::standard_patterns().into_iter().map(|p| p.name).collect();
    let warn = Tokens::get().warn;
    let (mut apply_clicked, mut create_name, mut delete_clicked, mut close, mut cancel_new) = (false, None::<String>, false, false, false);
    let Some(st) = app.wall_style_dlg.as_mut() else { return };
    let id = egui::Id::unique("wallstyles_dialog");
    if std::mem::take(&mut st.raise) {
        ctx.move_to_top(egui::LayerId::new(egui::Order::Middle, id));
    }
    egui::Window::new(crate::tl!("Wall Styles")).id(id).open(open_flag).collapsible(false).default_size(vec2(900.0, 520.0)).show(ctx, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui.strong(crate::tl!("Styles"));
                egui::ScrollArea::vertical().max_height(300.0).id_salt("wallstyle_list").show(ui, |ui| {
                    for (s, mine) in all.iter().zip(&own) {
                        let text = if *mine { RichText::new(&s.name) } else { RichText::new(&s.name).weak() };
                        if ui.selectable_label(st.selected.eq_ignore_ascii_case(&s.name), text).clicked() {
                            load(st, s);
                        }
                    }
                });
                ui.add_space(6.0);
                match st.new_name.as_mut() {
                    Some(name) => {
                        ui.label(crate::tl!("Name of the new style"));
                        let r = ui.text_edit_singleline(name);
                        ui.horizontal(|ui| {
                            let ok = ui.add_enabled(!name.trim().is_empty(), egui::Button::new(crate::tl!("Create"))).clicked()
                                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty());
                            if ok {
                                create_name = Some(name.clone());
                            }
                            cancel_new = ui.button(crate::tl!("Cancel")).clicked();
                        });
                    }
                    None => {
                        if ui.button(crate::tl!("New style (copy of the selected one)")).clicked() {
                            st.new_name = Some(String::new());
                        }
                        let can_delete = own.iter().zip(&all).any(|(m, s)| *m && s.name.eq_ignore_ascii_case(&st.selected));
                        if ui.add_enabled(can_delete, egui::Button::new(crate::tl!("Delete style"))).clicked() {
                            delete_clicked = true;
                        }
                    }
                }
            });
            if cancel_new {
                st.new_name = None;
            }
            ui.separator();
            ui.vertical(|ui| {
                let standard = !own.iter().zip(&all).any(|(m, s)| *m && s.name.eq_ignore_ascii_case(&st.selected));
                let modified = st.draft != st.loaded;
                if let Some(d) = st.draft.as_mut() {
                    egui::Grid::new("wallstyle_head").num_columns(2).show(ui, |ui| {
                        let l = ui.label(crate::tl!("Name"));
                        ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(260.0)).labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label(crate::tl!("Description"));
                        ui.add(egui::TextEdit::singleline(&mut d.description).desired_width(260.0)).labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label(crate::tl!("Default height (mm)"));
                        ui.add(egui::DragValue::new(&mut d.default_height).speed(10.0).range(100.0..=50_000.0)).labelled_by(l.id);
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    ui.strong(crate::tl!("Layers, from one face to the other"));
                    layers(ui, d, &patterns);
                    if ui.button(crate::tl!("Add layer")).clicked() {
                        d.components.push(WallComponent::new("Layer", 100.0, ComponentFunction::Structure));
                    }
                    ui.label(format!("{}: {:.0} mm", crate::tl!("Total thickness"), d.thickness()));
                    if standard {
                        ui.label(RichText::new(crate::tl!("A standard style: Apply keeps a copy of it in the drawing.")).small());
                    }
                }
                if let Some(e) = &st.error {
                    ui.label(RichText::new(e).color(warn));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(modified, egui::Button::new(crate::tl!("Apply"))).clicked() {
                        apply_clicked = true;
                    }
                    if modified {
                        ui.label(RichText::new(crate::tl!("Modified")).color(warn));
                    }
                    if ui.button(crate::tl!("Close")).clicked() {
                        close = true;
                    }
                });
            });
        });
    });
    if let Some(n) = create_name {
        create(app, &n);
    }
    if delete_clicked {
        delete(app);
    }
    if apply_clicked {
        apply(app);
    }
    if close {
        *open_flag = false;
    }
}
