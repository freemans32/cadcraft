//! Properties of the parametric objects drawn as block references (walls): when the selection
//! is only such objects, a group of their own parameters, each change running the object's edit
//! command so the object is regenerated, as the command line would.

use cadcraft_doc::{Drawing, Handle, WallJustification, wall_of};
use egui::{Sense, pos2, vec2};
use serde_json::{Value, json};

use super::{edit_field, number, pick_menu, prop_row, read_only};
use crate::theme::Tokens;

/// The command a change runs, with its parameters.
pub(super) type Action = Option<(&'static str, Value)>;

fn group(ui: &mut egui::Ui, title: &str) {
    let t = Tokens::get();
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    ui.painter().rect_filled(r.shrink2(vec2(6.0, 1.0)), 2.0, t.chrome_dark);
    ui.painter().text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(title), crate::theme::small(), t.text_dim);
}

/// `value` when every item gives the same, else "*VARIES*".
fn shared<T: PartialEq + Clone>(items: &[T]) -> Option<T> {
    let first = items.first()?;
    items.iter().all(|x| x == first).then(|| first.clone())
}

const VARIES: &str = "*VARIES*";

/// The parametric objects' group of the Properties palette, when every selected object is one
/// of a single kind; returns the command a change asks for.
pub(super) fn section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], len: &dyn Fn(f64) -> String, bad: &mut Option<String>) -> Action {
    let walls: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| wall_of(e))).collect();
    if !walls.is_empty() && walls.len() == sel.len() {
        return wall_section(ui, d, sel, &walls, len, bad);
    }
    None
}

/// Whether object `h` is a parametric object with a Properties group of its own.
pub(crate) fn is_parametric(d: &Drawing, h: Handle) -> bool {
    d.entity(h).is_some_and(|e| wall_of(e).is_some())
}

fn handles(sel: &[Handle]) -> Vec<String> {
    sel.iter().map(|h| h.hex()).collect()
}

fn wall_section(
    ui: &mut egui::Ui,
    d: &Drawing,
    sel: &[Handle],
    walls: &[cadcraft_doc::WallParams],
    len: &dyn Fn(f64) -> String,
    bad: &mut Option<String>,
) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Wall");
    let style = shared(&walls.iter().map(|w| w.style.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    let styles: Vec<String> = d.available_wall_styles().into_iter().map(|s| s.name).collect();
    prop_row(ui, "Style", |ui| {
        if let Some(n) = pick_menu(ui, &style, styles) {
            action = Some(("walledit", json!({ "handles": ids, "style": n })));
        }
    });
    let just = shared(&walls.iter().map(|w| w.justification).collect::<Vec<_>>());
    let shown = just.map_or(VARIES, WallJustification::name);
    prop_row(ui, "Justification", |ui| {
        let items = [WallJustification::Left, WallJustification::Center, WallJustification::Right];
        let mut out = None;
        ui.menu_button(crate::i18n::t(shown), |ui| {
            for j in items {
                if ui.button(crate::i18n::t(j.name())).clicked() {
                    out = Some(j);
                    ui.close();
                }
            }
        });
        if let Some(j) = out {
            action = Some(("walledit", json!({ "handles": ids, "justification": j.name() })));
        }
    });
    let height = shared(&walls.iter().map(|w| format!("{:.0}", w.height)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height (mm)", |ui| {
        if let Some(h) = number(edit_field(ui, ui.scope_id().with(("wall_height", &ids)), height), bad) {
            action = Some(("walledit", json!({ "handles": ids, "height": h })));
        }
    });
    let thickness = shared(&walls.iter().map(|w| format!("{:.0} mm", w.style.thickness())).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Thickness", |ui| read_only(ui, &thickness));
    if let [w] = walls {
        let mut length = 0.0;
        let n = w.path.len();
        for i in 0..n {
            let next = if i + 1 < n {
                i + 1
            } else if w.closed {
                0
            } else {
                break;
            };
            if let (Some(a), Some(b)) = (w.path.get(i), w.path.get(next)) {
                length += (*b - *a).len();
            }
        }
        prop_row(ui, "Length", |ui| read_only(ui, &len(length)));
    }
    action
}

#[cfg(test)]
mod tests {
    use cadcraft_engine::Session;
    use egui::vec2;
    use egui_kittest::kittest::Queryable;
    use serde_json::json;

    use crate::{CadApp, Services};

    fn palette(app: CadApp) -> egui_kittest::Harness<'static, CadApp> {
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(400.0, 1400.0))
            .build_ui_state(|ui, app: &mut CadApp| super::super::properties_section(app, ui), app);
        h.run_steps(3);
        h
    }

    #[test]
    fn a_selected_wall_shows_its_style_and_a_new_height_regenerates_it() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("walladd", json!({ "points": [[0, 0], [4000, 0]], "height": 2500 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        let style = cadcraft_doc::wall_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap().style.name;
        assert!(hn.query_by_label(&style).is_some(), "the style menu shows the wall's style");
        // The wall's group comes before the block reference's Geometry group.
        let field = hn.get_all_by_value("2500").next().unwrap();
        field.focus();
        hn.run_steps(2);
        hn.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        hn.run_steps(2);
        hn.get_all_by_value("2500").next().unwrap().type_text("2700");
        hn.run_steps(2);
        hn.key_press(egui::Key::Enter);
        hn.run_steps(3);
        let w = cadcraft_doc::wall_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(w.height, 2700.0);
    }
}
