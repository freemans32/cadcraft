//! Properties of the parametric objects drawn as block references (walls, spaces, air
//! terminals): when the selection is only such objects of one kind, a group of their own parameters, each change running the object's edit
//! command so the object is regenerated, as the command line would.

use cadcraft_doc::{
    AccessoryKind, AccessoryParams, AhuKind, AhuParams, AirDirection, Drawing, DuctKind, DuctParams, DuctShape, Handle, OpeningKind, OpeningParams,
    RiserParams, SpaceParams, TerminalKind, TerminalParams, WallJustification, ZoneParams, accessory_of, ahu_of, air_terminal_of, duct_of,
    opening_of, riser_of, space_of, wall_of, zone_of,
};
use cadcraft_hvac::RoomUse;
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
    let spaces: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| space_of(e))).collect();
    if !spaces.is_empty() && spaces.len() == sel.len() {
        return space_section(ui, d, sel, &spaces, bad);
    }
    let terminals: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| air_terminal_of(e))).collect();
    if !terminals.is_empty() && terminals.len() == sel.len() {
        return terminal_section(ui, d, sel, &terminals, bad);
    }
    let openings: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| opening_of(e))).collect();
    if !openings.is_empty() && openings.len() == sel.len() {
        return opening_section(ui, d, sel, &openings, bad);
    }
    let zones: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| zone_of(e))).collect();
    if !zones.is_empty() && zones.len() == sel.len() {
        return zone_section(ui, sel, &zones, bad);
    }
    let risers: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| riser_of(e))).collect();
    if !risers.is_empty() && risers.len() == sel.len() {
        return riser_section(ui, sel, &risers, bad);
    }
    let units: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| ahu_of(e))).collect();
    if !units.is_empty() && units.len() == sel.len() {
        return ahu_section(ui, sel, &units, bad);
    }
    let accessories: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| accessory_of(e))).collect();
    if !accessories.is_empty() && accessories.len() == sel.len() {
        return accessory_section(ui, sel, &accessories, bad);
    }
    let ducts: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| duct_of(e))).collect();
    if !ducts.is_empty() && ducts.len() == sel.len() {
        return duct_section(ui, sel, &ducts, len, bad);
    }
    None
}

/// Whether object `h` is a parametric object with a Properties group of its own.
pub(crate) fn is_parametric(d: &Drawing, h: Handle) -> bool {
    d.entity(h).is_some_and(|e| {
        wall_of(e).is_some()
            || space_of(e).is_some()
            || air_terminal_of(e).is_some()
            || duct_of(e).is_some()
            || accessory_of(e).is_some()
            || riser_of(e).is_some()
    })
}

fn handles(sel: &[Handle]) -> Vec<String> {
    sel.iter().map(|h| h.hex()).collect()
}

/// Whether two walls have the same layers (names and which are membranes), so their thicknesses can be listed together.
fn same_layers(a: &cadcraft_doc::WallParams, b: &cadcraft_doc::WallParams) -> bool {
    a.style.components.len() == b.style.components.len()
        && a.style.components.iter().zip(&b.style.components).all(|(x, y)| x.name == y.name && (x.thickness > 0.0) == (y.thickness > 0.0))
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
    // The layers, each with its own thickness: a change is for these walls only, the button gives
    // the whole style the thicknesses shown. Walls of different layers are not listed together.
    if let Some(first) = walls.first().filter(|f| walls.iter().all(|w| same_layers(w, f))) {
        let mut current: Vec<Value> = Vec::new();
        for (i, c) in first.style.components.iter().enumerate().filter(|(_, c)| c.thickness > 0.0) {
            let value = shared(&walls.iter().map(|w| format!("{:.0}", w.style.components.get(i).map_or(0.0, |x| x.thickness))).collect::<Vec<_>>())
                .unwrap_or_else(|| VARIES.into());
            if let Ok(t) = value.parse::<f64>() {
                current.push(json!({ "index": i, "thickness": t }));
            }
            prop_row(ui, &c.name, |ui| {
                if let Some(t) = number(edit_field(ui, ui.scope_id().with(("wall_layer", &ids, i)), value), bad) {
                    action = Some(("walledit", json!({ "handles": ids, "layers": [{ "index": i, "thickness": t }] })));
                }
            });
        }
        if !current.is_empty() {
            prop_row(ui, "Style", |ui| {
                if ui.button(crate::i18n::t("Apply to the whole style")).clicked() {
                    action = Some(("walledit", json!({ "handles": ids, "layers": current, "applyToStyle": true })));
                }
            });
        }
    }
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

// =====================================================================================
// Spaces
// =====================================================================================

/// The uses a space menu offers: the SPACEEDIT name, and the label shown.
const USES: &[(&str, &str)] = &[
    ("office", "Office (25 m³/h per occupant)"),
    ("meeting", "Meeting, sales, dining (30 m³/h per occupant)"),
    ("lightWorkshop", "Light workshop (45 m³/h per occupant)"),
    ("workshop", "Other rooms (60 m³/h per occupant)"),
    ("toilet", "Toilet"),
    ("shower", "Shower room"),
    ("showerToilet", "Shower room with toilet"),
    ("sanitary", "Grouped sanitary room"),
    ("washbasins", "Grouped washbasins"),
];

fn use_key(r: RoomUse) -> &'static str {
    match r {
        RoomUse::Office => "office",
        RoomUse::DiningSalesMeeting => "meeting",
        RoomUse::LightWorkshop => "lightWorkshop",
        RoomUse::OtherWorkshop => "workshop",
        RoomUse::Toilet => "toilet",
        RoomUse::Shower => "shower",
        RoomUse::ShowerWithToilet => "showerToilet",
        RoomUse::GroupedSanitary { .. } => "sanitary",
        RoomUse::GroupedWashbasins { .. } => "washbasins",
    }
}

fn use_label(key: &str) -> &'static str {
    USES.iter().find(|(k, _)| *k == key).map_or(VARIES, |(_, l)| l)
}

/// A menu of `items` (value, label) showing `current`; returns the value clicked.
fn choice(ui: &mut egui::Ui, current: &str, items: &[(&'static str, &'static str)]) -> Option<&'static str> {
    choice_text(ui, crate::i18n::t(current), items)
}

/// [`choice`] with the shown text already in the interface language.
fn choice_text(ui: &mut egui::Ui, shown: impl Into<egui::WidgetText>, items: &[(&'static str, &'static str)]) -> Option<&'static str> {
    let mut out = None;
    ui.menu_button(shown, |ui| {
        for (value, label) in items {
            if ui.button(crate::i18n::t(label)).clicked() {
                out = Some(*value);
                ui.close();
            }
        }
    });
    out
}

/// The supply flow of the terminals inside space `h`, and how many there are.
fn terminals_in(d: &Drawing, h: Handle) -> Option<(usize, f64)> {
    let (_, outline) = d.space_outline(h)?;
    let mut n = 0;
    let mut flow = 0.0;
    for e in d.model.iter() {
        let (Some(t), cadcraft_doc::EntityKind::Insert(i)) = (air_terminal_of(e), &e.kind) else { continue };
        if t.air == AirDirection::Supply && cadcraft_geom::point_in_polygon(&outline, cadcraft_geom::Vec2::new(i.insert.x, i.insert.y)) {
            n += 1;
            flow += t.flow_m3h;
        }
    }
    Some((n, flow))
}

fn space_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], spaces: &[SpaceParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("spaceedit", json!({ "handles": ids, key: value })));
    group(ui, "Space");
    let name = shared(&spaces.iter().map(|p| p.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("space_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let key = shared(&spaces.iter().map(|p| use_key(p.room)).collect::<Vec<_>>());
    prop_row(ui, "Use", |ui| {
        if let Some(k) = choice(ui, key.map_or(VARIES, use_label), USES) {
            action = edit("use", json!(k));
        }
    });
    let per_occupant = spaces.iter().all(|p| p.room.per_occupant().is_some());
    if per_occupant {
        let occ = shared(&spaces.iter().map(|p| p.occupants.to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Occupants", |ui| {
            if let Some(n) = number(edit_field(ui, ui.scope_id().with(("space_occ", &ids)), occ), bad) {
                action = edit("occupants", json!(n.round().max(0.0)));
            }
        });
    }
    let fixtures = |p: &SpaceParams| match p.room {
        RoomUse::GroupedSanitary { fixtures } | RoomUse::GroupedWashbasins { fixtures } => Some(fixtures),
        _ => None,
    };
    if spaces.iter().all(|p| fixtures(p).is_some()) {
        let f = shared(&spaces.iter().map(|p| fixtures(p).unwrap_or(0).to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Fixtures", |ui| {
            if let Some(n) = number(edit_field(ui, ui.scope_id().with(("space_fix", &ids)), f), bad) {
                action = edit("fixtures", json!(n.round().max(1.0)));
            }
        });
    }
    if spaces.iter().all(|p| matches!(p.room, RoomUse::Toilet | RoomUse::Shower | RoomUse::ShowerWithToilet)) {
        let col = shared(&spaces.iter().map(|p| p.collective).collect::<Vec<_>>());
        prop_row(ui, "Collective use", |ui| {
            let shown = col.map_or(VARIES, |c| if c { "Yes" } else { "No" });
            if let Some(v) = choice(ui, shown, &[("yes", "Yes"), ("no", "No")]) {
                action = edit("collective", json!(v == "yes"));
            }
        });
    }
    group(ui, "Flow");
    // The flow the space gets: typing one sets it by hand, 0 or nothing goes back to the
    // labour code's.
    let flow =
        shared(&spaces.iter().map(|p| p.flow().map(|f| format!("{f:.0}")).unwrap_or_default()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Flow (m³/h)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("space_flow", &ids)), flow) {
            if t.trim().is_empty() || t.trim() == "0" {
                action = edit("flow", Value::Null);
            } else if let Some(f) = number(Some(t), bad) {
                action = edit("flow", json!(f));
            }
        }
    });
    // The height available above the false ceiling for ducts: empty or 0 leaves it to the unit's.
    let void = shared(&spaces.iter().map(|p| p.void_height_mm.map(|v| format!("{v:.0}")).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height available (mm)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("space_void", &ids)), void) {
            if t.trim().is_empty() || t.trim() == "0" {
                action = edit("voidHeight", Value::Null);
            } else if let Some(v) = number(Some(t), bad) {
                action = edit("voidHeight", json!(v));
            }
        }
    });
    let origin = shared(&spaces.iter().map(|p| p.flow_m3h.is_some()).collect::<Vec<_>>());
    let origin = origin.map_or(VARIES, |set| if set { "Set by hand" } else { "Labour code" });
    prop_row(ui, "Flow from", |ui| read_only(ui, origin));
    if let ([p], [h]) = (spaces, sel) {
        prop_row(ui, "Area", |ui| read_only(ui, &format!("{:.1} m²", p.area_m2())));
        if let Some((n, supplied)) = terminals_in(d, *h) {
            group(ui, "Air terminals in the space");
            prop_row(ui, "Supply terminals", |ui| read_only(ui, &n.to_string()));
            let need = p.flow().unwrap_or(0.0);
            prop_row(ui, "Supplied / needed", |ui| read_only(ui, &format!("{supplied:.0} / {need:.0} m³/h")));
        }
    }
    action
}

// =====================================================================================
// Air terminals
// =====================================================================================

const KINDS: &[(&str, &str)] = &[
    ("louvre", "Louvre"),
    ("squareCeiling", "Square ceiling diffuser"),
    ("round", "Round diffuser"),
    ("rectangular", "Rectangular grille"),
    ("linear", "Slot diffuser"),
    ("extract", "Extract valve"),
];

fn kind_label(k: TerminalKind) -> &'static str {
    KINDS.iter().find(|(n, _)| *n == k.name()).map_or(VARIES, |(_, l)| l)
}

fn terminal_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], ts: &[TerminalParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("airterminaledit", json!({ "handles": ids, key: value })));
    group(ui, "Air Terminal");
    let kind = shared(&ts.iter().map(|t| t.model.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        if let Some(k) = choice(ui, kind.map_or(VARIES, kind_label), KINDS) {
            action = edit("type", json!(k));
        }
    });
    let model = shared(&ts.iter().map(|t| t.model.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    let models: Vec<String> = std::iter::once("auto".to_string())
        .chain(d.available_air_terminal_models().into_iter().filter(|m| kind.is_none_or(|k| m.kind == k)).map(|m| m.name))
        .collect();
    prop_row(ui, "Model", |ui| {
        if let Some(m) = pick_menu(ui, &model, models) {
            action = edit("model", json!(m));
        }
    });
    let flow = shared(&ts.iter().map(|t| format!("{:.0}", t.flow_m3h)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Flow (m³/h)", |ui| {
        if let Some(f) = number(edit_field(ui, ui.scope_id().with(("term_flow", &ids)), flow), bad) {
            action = edit("flow", json!(f));
        }
    });
    let air = shared(&ts.iter().map(|t| t.air).collect::<Vec<_>>());
    prop_row(ui, "Air", |ui| {
        let shown = air.map_or(VARIES, |a| match a {
            AirDirection::Supply => "Supply",
            AirDirection::Extract => "Extract",
            AirDirection::Fresh => "Fresh air",
            AirDirection::Exhaust => "Exhaust air",
        });
        let items = [("supply", "Supply"), ("extract", "Extract"), ("fresh", "Fresh air"), ("exhaust", "Exhaust air")];
        if let Some(a) = choice(ui, shown, &items) {
            action = edit("air", json!(a));
        }
    });
    if let Some(m) = shared(&ts.iter().map(|t| t.model.clone()).collect::<Vec<_>>()) {
        group(ui, "Model");
        let face = if m.kind == TerminalKind::Round || m.kind == TerminalKind::Extract {
            format!("Ø {:.0} mm", m.width_mm)
        } else {
            format!("{:.0} × {:.0} mm", m.width_mm, m.depth_mm)
        };
        prop_row(ui, "Face", |ui| read_only(ui, &face));
        prop_row(ui, "Duct connection", |ui| read_only(ui, &format!("Ø {:.0}", m.neck_mm)));
        prop_row(ui, "Largest flow", |ui| read_only(ui, &format!("{:.0} m³/h", m.max_flow_m3h)));
        prop_row(ui, "Pressure drop", |ui| read_only(ui, &format!("{:.0} Pa", m.pressure_drop_pa)));
        if ts.iter().any(|t| t.flow_m3h > m.max_flow_m3h) {
            prop_row(ui, "Check", |ui| read_only(ui, "Flow above the model's largest"));
        }
    }
    action
}

// =====================================================================================
// Openings
// =====================================================================================

fn opening_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], openings: &[OpeningParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("openingedit", json!({ "handles": ids, key: value })));
    group(ui, "Opening");
    let kind = shared(&openings.iter().map(|o| o.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let label = |k: OpeningKind| match k {
            OpeningKind::Door => "Door",
            OpeningKind::DoubleDoor => "Double door",
            OpeningKind::Window => "Window",
            OpeningKind::Bay => "Bay",
        };
        let items = [("door", "Door"), ("doubleDoor", "Double door"), ("window", "Window"), ("bay", "Bay")];
        if let Some(k) = choice(ui, kind.map_or(VARIES, label), &items) {
            action = edit("type", json!(k));
        }
    });
    for (key, label, get) in [
        ("width", "Width (mm)", (|o: &OpeningParams| o.width_mm) as fn(&OpeningParams) -> f64),
        ("height", "Height (mm)", |o| o.height_mm),
        ("sill", "Sill (mm)", |o| o.sill_mm),
    ] {
        let value = shared(&openings.iter().map(|o| format!("{:.0}", get(o))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(v) = number(edit_field(ui, ui.scope_id().with(("opening", key, &ids)), value), bad) {
                action = edit(key, json!(v));
            }
        });
    }
    // Distances to the edge of the opening, from each end of the wall segment it is in: typing one moves it.
    if let ([o], [h]) = (openings, sel)
        && let Some((_, hit)) = d.opening_host(*h)
    {
        let mm = o.units.mm_per_unit;
        let (from_start, from_end) = ((hit.along - o.width() / 2.0) * mm, (hit.length - hit.along - o.width() / 2.0) * mm);
        for (from, label, value) in [("start", "From the start (mm)", from_start), ("end", "From the end (mm)", from_end)] {
            prop_row(ui, label, |ui| {
                if let Some(v) = number(edit_field(ui, ui.scope_id().with(("opening_distance", from, &ids)), format!("{value:.0}")), bad) {
                    action = Some(("openingedit", json!({ "handles": ids, "distance": v, "from": from })));
                }
            });
        }
    }
    if openings.iter().any(|o| matches!(o.kind, OpeningKind::Door | OpeningKind::DoubleDoor)) {
        let side = shared(&openings.iter().map(|o| o.flip_side).collect::<Vec<_>>());
        prop_row(ui, "Swing", |ui| {
            let shown = side.map_or(VARIES, |f| if f { "Right" } else { "Left" });
            if let Some(v) = choice(ui, shown, &[("left", "Left"), ("right", "Right")]) {
                action = edit("flipSide", json!(v == "right"));
            }
        });
        let hand = shared(&openings.iter().map(|o| o.flip_hand).collect::<Vec<_>>());
        prop_row(ui, "Hinge", |ui| {
            let shown = hand.map_or(VARIES, |f| if f { "End" } else { "Start" });
            if let Some(v) = choice(ui, shown, &[("start", "Start"), ("end", "End")]) {
                action = edit("flipHand", json!(v == "end"));
            }
        });
    }
    action
}

// =====================================================================================
// Height zones
// =====================================================================================

fn zone_section(ui: &mut egui::Ui, sel: &[Handle], zones: &[ZoneParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("zoneedit", json!({ "handles": ids, key: value })));
    group(ui, "Height Zone");
    let name = shared(&zones.iter().map(|z| z.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("zone_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let height = shared(&zones.iter().map(|z| format!("{:.0}", z.void_height_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height available (mm)", |ui| {
        if let Some(v) = number(edit_field(ui, ui.scope_id().with(("zone_height", &ids)), height), bad) {
            action = edit("voidHeight", json!(v));
        }
    });
    // The altitude of the underside of the ducts, in metres as plans write it: empty or 0 for none.
    let altitude = shared(&zones.iter().map(|z| z.bottom_mm.map(|b| format!("{}", b / 1000.0)).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Altitude of the underside (m)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("zone_altitude", &ids)), altitude) {
            if t.trim().is_empty() || t.trim() == "0" {
                action = edit("altitude", Value::Null);
            } else if let Some(m) = number(Some(t), bad) {
                action = edit("altitude", json!(m));
            }
        }
    });
    action
}

// =====================================================================================
// Risers
// =====================================================================================

fn riser_section(ui: &mut egui::Ui, sel: &[Handle], risers: &[RiserParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("riseredit", json!({ "handles": ids, key: value })));
    group(ui, "Riser");
    let name = shared(&risers.iter().map(|r| r.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("riser_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let air = shared(&risers.iter().map(|r| r.air).collect::<Vec<_>>());
    prop_row(ui, "Air", |ui| {
        let shown = air.map_or(VARIES, |a| match a {
            AirDirection::Supply => "Supply",
            AirDirection::Extract => "Extract",
            AirDirection::Fresh => "Fresh air",
            AirDirection::Exhaust => "Exhaust air",
        });
        let items = [("supply", "Supply"), ("extract", "Extract"), ("fresh", "Fresh air"), ("exhaust", "Exhaust air")];
        if let Some(a) = choice(ui, shown, &items) {
            action = edit("air", json!(a));
        }
    });
    let up = shared(&risers.iter().map(|r| r.up).collect::<Vec<_>>());
    prop_row(ui, "Direction", |ui| {
        let shown = up.map_or(VARIES, |u| if u { "Goes up" } else { "Goes down" });
        if let Some(d) = choice(ui, shown, &[("up", "Goes up"), ("down", "Goes down")]) {
            action = edit("direction", json!(d));
        }
    });
    // The shaft (trémie): a rectangle around the same centre.
    for (label, key, pick) in [("Shaft width (mm)", "width", 0), ("Shaft depth (mm)", "depth", 1)] {
        let v = shared(&risers.iter().map(|r| format!("{:.0}", if pick == 0 { r.shaft_mm().0 } else { r.shaft_mm().1 })).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(n) = number(edit_field(ui, ui.scope_id().with(("riser_shaft", key, &ids)), v), bad) {
                action = edit(key, json!(n));
            }
        });
    }
    // The size: 200 for a diameter, 400x250 for a rectangle, empty for none.
    let size = shared(&risers.iter().map(|r| r.size.map_or(String::new(), |s| s.label().replace('Ø', "").replace('×', "x"))).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Size (mm)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_size", &ids)), size).filter(|t| t != VARIES) {
            action = edit("size", json!(if t.trim().is_empty() { "auto" } else { t.trim() }));
        }
    });
    // The flows, empty for none: the one from the other floors, and the one carried here.
    for (label, key) in [("Flow from other floors (m³/h)", "incomingFlow"), ("Flow (m³/h)", "flow")] {
        let pick = |r: &RiserParams| if key == "flow" { r.flow_m3h } else { r.incoming_flow_m3h };
        let v =
            shared(&risers.iter().map(|r| pick(r).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_flow", key, &ids)), v).filter(|t| t != VARIES) {
                if t.trim().is_empty() {
                    action = edit(key, Value::Null);
                } else if let Some(f) = number(Some(t), bad) {
                    action = edit(key, json!(f));
                }
            }
        });
    }
    action
}

// =====================================================================================
// Air handling units
// =====================================================================================

fn ahu_section(ui: &mut egui::Ui, sel: &[Handle], units: &[AhuParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("ahuedit", json!({ "handles": ids, key: value })));
    group(ui, "Air Handling Unit");
    let kind = shared(&units.iter().map(|p| p.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let label = |k: AhuKind| match k {
            AhuKind::DoubleFlow => "Double-flow unit",
            AhuKind::SupplyUnit => "Supply unit",
            AhuKind::ExtractUnit => "Extract unit",
            AhuKind::VmcBox => "Ventilation box",
            AhuKind::StackedDoubleFlow => "Compact double-flow unit",
        };
        let items = [
            ("doubleFlow", "Double-flow unit"),
            ("supplyUnit", "Supply unit"),
            ("extractUnit", "Extract unit"),
            ("vmcBox", "Ventilation box"),
            ("stackedDoubleFlow", "Compact double-flow unit"),
        ];
        if let Some(k) = choice(ui, kind.map_or(VARIES, label), &items) {
            action = edit("type", json!(k));
        }
    });
    let name = shared(&units.iter().map(|p| p.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("ahu_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    // The casing: width x depth on the plan and the height; the make and model and the weight go on its spec block.
    for (label, key, get) in [
        ("Width (mm)", "width", (|u: &AhuParams| Some(u.width_mm)) as fn(&AhuParams) -> Option<f64>),
        ("Depth (mm)", "depth", |u| Some(u.depth_mm)),
        ("Height (mm)", "height", |u| u.height_mm),
        ("Weight (kg)", "weight", |u| u.weight_kg),
    ] {
        let v = shared(&units.iter().map(|u| get(u).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_size", key, &ids)), v).filter(|t| t != VARIES) {
                if t.trim().is_empty() && (key == "height" || key == "weight") {
                    action = edit(key, Value::Null);
                } else if let Some(n) = number(Some(t), bad) {
                    action = edit(key, json!(n));
                }
            }
        });
    }
    let model = shared(&units.iter().map(|p| p.model.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Make and model", |ui| {
        if let Some(m) = edit_field(ui, ui.scope_id().with(("ahu_model", &ids)), model).filter(|m| m != VARIES) {
            action = edit("model", json!(m));
        }
    });
    if let [u] = units {
        let flows = |f: Option<f64>, p: Option<f64>| match (f, p) {
            (Some(f), Some(p)) => format!("{f:.0} m³/h, {p:.0} Pa"),
            (Some(f), None) => format!("{f:.0} m³/h"),
            _ => "—".to_string(),
        };
        prop_row(ui, "Supply", |ui| read_only(ui, &flows(u.flow_m3h, u.pressure_pa)));
        prop_row(ui, "Extract", |ui| read_only(ui, &flows(u.extract_flow_m3h, u.extract_pressure_pa)));
        // The direction of the air at each collar: the ducts that start there take it.
        let roles = u.roles();
        let items = [("supply", "Supply"), ("extract", "Extract"), ("fresh", "Fresh air"), ("exhaust", "Exhaust air")];
        for (k, name) in u.kind.port_names().iter().enumerate() {
            let Some(role) = roles.get(k).copied() else { continue };
            // The orifice of the collar: its shape (none, round, rectangular) and its size; the duct that
            // starts here takes it.
            let size = u.port_sizes.get(k).copied().flatten();
            let text = size.map_or(String::new(), |s| s.label().replace('Ø', "").replace('×', "x"));
            let with = |value: Value| -> Value {
                let mut sizes: Vec<Value> = (0..u.kind.port_names().len())
                    .map(|j| u.port_sizes.get(j).copied().flatten().map_or(Value::Null, |s| json!(s.label().replace('Ø', "").replace('×', "x"))))
                    .collect();
                if let Some(slot) = sizes.get_mut(k) {
                    *slot = value;
                }
                json!(sizes)
            };
            prop_row(ui, &format!("{} ({})", crate::i18n::t(name), crate::i18n::t("section")), |ui| {
                let shown = match size {
                    None => crate::i18n::t("None"),
                    Some(cadcraft_doc::RiserSize::Round { .. }) => crate::i18n::t("Round"),
                    Some(cadcraft_doc::RiserSize::Rectangular { .. }) => crate::i18n::t("Rectangular"),
                };
                let (w, round) = size.map_or((250.0, true), |s| (s.width_mm(), matches!(s, cadcraft_doc::RiserSize::Round { .. })));
                match choice_text(ui, shown, &[("none", "None"), ("round", "Round"), ("rectangular", "Rectangular")]) {
                    Some("none") => action = edit("portSizes", with(Value::Null)),
                    Some("round") if !(size.is_some() && round) => action = edit("portSizes", with(json!(w))),
                    Some("rectangular") if !matches!(size, Some(cadcraft_doc::RiserSize::Rectangular { .. })) => {
                        let h = ((w * 0.75 / 50.0).round() * 50.0).max(100.0);
                        action = edit("portSizes", with(json!(format!("{w:.0}x{h:.0}"))));
                    }
                    _ => {}
                }
            });
            if size.is_some() {
                prop_row(ui, &format!("{} ({})", crate::i18n::t(name), crate::i18n::t("size")), |ui| {
                    if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_port_size", k, &ids)), text).filter(|t| t != VARIES) {
                        action = edit("portSizes", with(if t.trim().is_empty() { Value::Null } else { json!(t.trim()) }));
                    }
                });
            }
            prop_row(ui, name, |ui| {
                if let Some(c) = choice(ui, items.iter().find(|i| AirDirection::parse(i.0) == Some(role)).map_or("", |i| i.1), &items)
                    && let Some(new) = AirDirection::parse(c)
                {
                    let mut next = roles.clone();
                    if let Some(slot) = next.get_mut(k) {
                        *slot = new;
                    }
                    action = edit("ports", json!(next.iter().map(|r| r.name()).collect::<Vec<_>>()));
                }
            });
        }
    }
    action
}

// =====================================================================================
// Ducts
// =====================================================================================

fn duct_kind_label(k: DuctKind) -> &'static str {
    match k {
        DuctKind::Auto => "Auto",
        DuctKind::Main => "Main",
        DuctKind::Branch => "Branch",
        DuctKind::Connection => "Connection",
    }
}

fn duct_section(ui: &mut egui::Ui, sel: &[Handle], ds: &[DuctParams], len: &dyn Fn(f64) -> String, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("ductedit", json!({ "handles": ids, key: value })));
    group(ui, "Duct");
    let imposed = shared(&ds.iter().map(|p| p.imposed).collect::<Vec<_>>());
    prop_row(ui, "Size set by", |ui| {
        let shown = imposed.map_or(VARIES, |i| if i { "User" } else { "Sizing" });
        // Giving the size back to sizing keeps it until the next DUCTSIZE.
        if let Some(c) = choice(ui, shown, &[("auto", "Sizing"), ("keep", "User")]) {
            if c == "auto" {
                action = edit("diameter", json!("auto"));
            } else if let [p] = ds {
                action = match p.shape {
                    DuctShape::Round => edit("diameter", json!(p.diameter_mm)),
                    DuctShape::Rectangular => Some(("ductedit", json!({ "handles": ids, "width": p.width_mm, "height": p.height_mm }))),
                };
            }
        }
    });
    if let [p] = ds {
        // The section is chosen here: round, or rectangular (its sides start from the diameter), as sizing would size it.
        prop_row(ui, "Section", |ui| {
            let items = [("round", "Round"), ("rectangular", "Rectangular")];
            let shown = format!("{} ({})", crate::i18n::t(if p.shape == DuctShape::Round { "Round" } else { "Rectangular" }), p.size_label());
            match choice_text(ui, shown, &items) {
                Some("round") if p.shape != DuctShape::Round => {
                    action = edit("diameter", json!(p.diameter_mm));
                }
                Some("rectangular") if p.shape != DuctShape::Rectangular => {
                    let w = p.diameter_mm;
                    let h = ((w * 0.75 / 50.0).round() * 50.0).max(100.0);
                    action = Some(("ductedit", json!({ "handles": ids, "width": w, "height": h })));
                }
                _ => {}
            }
        });
    }
    // A diameter typed makes the duct round, a width or a height rectangular (the missing side is
    // the other one, else the diameter).
    let diameter =
        shared(&ds.iter().map(|p| if p.shape == DuctShape::Round { format!("{:.0}", p.diameter_mm) } else { String::new() }).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Diameter (mm)", |ui| {
        if let Some(mm) = number(edit_field(ui, ui.scope_id().with(("duct_diameter", &ids)), diameter), bad) {
            action = edit("diameter", json!(mm));
        }
    });
    let side = |f: fn(&DuctParams) -> f64| {
        shared(&ds.iter().map(|p| if p.shape == DuctShape::Rectangular { format!("{:.0}", f(p)) } else { String::new() }).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into())
    };
    let (width, height) = (side(|p| p.width_mm), side(|p| p.height_mm));
    prop_row(ui, "Width (mm)", |ui| {
        if let (Some(w), [p]) = (number(edit_field(ui, ui.scope_id().with(("duct_width", &ids)), width), bad), ds) {
            let h = if p.height_mm > 0.0 { p.height_mm } else { p.diameter_mm };
            action = Some(("ductedit", json!({ "handles": ids, "width": w, "height": h })));
        }
    });
    prop_row(ui, "Height (mm)", |ui| {
        if let (Some(h), [p]) = (number(edit_field(ui, ui.scope_id().with(("duct_height", &ids)), height), bad), ds) {
            let w = if p.width_mm > 0.0 { p.width_mm } else { p.diameter_mm };
            action = Some(("ductedit", json!({ "handles": ids, "width": w, "height": h })));
        }
    });
    let kind = shared(&ds.iter().map(|p| p.kind).collect::<Vec<_>>());
    prop_row(ui, "Kind", |ui| {
        let items = [("auto", "Auto"), ("main", "Main"), ("branch", "Branch"), ("connection", "Connection")];
        // On Auto, the type sizing gave the run is shown beside it, once it has been sized.
        let found = match (kind, ds) {
            (Some(DuctKind::Auto), [p]) => p.kind_found,
            _ => None,
        };
        let shown = match (kind, found) {
            (Some(DuctKind::Auto), Some(f)) => format!("{} ({})", crate::i18n::t("Auto"), crate::i18n::t(duct_kind_label(f))),
            (k, _) => crate::i18n::t(k.map_or(VARIES, duct_kind_label)).to_string(),
        };
        if let Some(k) = choice_text(ui, shown, &items) {
            action = edit("kind", json!(k));
        }
    });
    let flexible = shared(&ds.iter().map(|p| p.flexible).collect::<Vec<_>>());
    prop_row(ui, "Material", |ui| {
        let shown = flexible.map_or(VARIES, |f| if f { "Flexible" } else { "Rigid" });
        if let Some(m) = choice(ui, shown, &[("rigid", "Rigid"), ("flexible", "Flexible")]) {
            action = edit("flexible", json!(m == "flexible"));
        }
    });
    // The direction of a duct is its own when the user fixed it, else the one sizing found.
    let air = shared(&ds.iter().map(|p| (p.air_set, p.air)).collect::<Vec<_>>());
    prop_row(ui, "Air", |ui| {
        let name = |a: AirDirection| match a {
            AirDirection::Supply => "Supply",
            AirDirection::Extract => "Extract",
            AirDirection::Fresh => "Fresh air",
            AirDirection::Exhaust => "Exhaust air",
        };
        // On Auto, the direction sizing found is shown beside it.
        let shown = match air {
            None => crate::i18n::t(VARIES).to_string(),
            Some((false, a)) if ds.iter().all(|p| p.flow_m3h.is_some()) => format!("{} ({})", crate::i18n::t("Auto"), crate::i18n::t(name(a))),
            Some((false, _)) => crate::i18n::t("Auto").to_string(),
            Some((true, a)) => crate::i18n::t(name(a)).to_string(),
        };
        let items = [("auto", "Auto"), ("supply", "Supply"), ("extract", "Extract"), ("fresh", "Fresh air"), ("exhaust", "Exhaust air")];
        if let Some(a) = choice_text(ui, shown, &items) {
            action = edit("air", json!(a));
        }
    });
    if let [p] = ds {
        prop_row(ui, "Length", |ui| read_only(ui, &len(p.length)));
        let flow = p.flow_m3h.map_or("—".to_string(), |f| format!("{f:.0} m³/h"));
        prop_row(ui, "Flow", |ui| read_only(ui, &flow));
        let velocity = p.velocity.map_or("—".to_string(), |v| format!("{v:.1} m/s"));
        prop_row(ui, "Velocity", |ui| read_only(ui, &velocity));
        if !p.gaps_mm.is_empty() {
            prop_row(ui, "Passes under", |ui| read_only(ui, &format!("{}", p.gaps_mm.len())));
        }
        // For a duct that ends in the open; empty = the balance with the opposite direction (DUCTEDIT openFlow null gives it back).
        let open = p.open_flow_m3h.map_or(String::new(), |f| format!("{f}"));
        prop_row(ui, "Open-end flow (m³/h)", |ui| {
            if let Some(f) = number(edit_field(ui, ui.scope_id().with(("open_flow", &ids)), open), bad) {
                action = edit("openFlow", json!(f));
            }
        });
    }
    action
}

// =====================================================================================
// Duct accessories
// =====================================================================================

fn accessory_section(ui: &mut egui::Ui, sel: &[Handle], accs: &[AccessoryParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Duct Accessory");
    let kind = shared(&accs.iter().map(|a| a.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let label = |k: AccessoryKind| match k {
            AccessoryKind::Damper => "Damper",
            AccessoryKind::FireDamper => "Fire damper",
            AccessoryKind::Silencer => "Silencer",
            AccessoryKind::FlexConnector => "Flexible connector",
        };
        let items = [("damper", "Damper"), ("fireDamper", "Fire damper"), ("silencer", "Silencer"), ("flexConnector", "Flexible connector")];
        if let Some(k) = choice(ui, kind.map_or(VARIES, label), &items) {
            action = Some(("ductaccessoryedit", json!({ "handles": ids, "type": k })));
        }
    });
    let length = shared(&accs.iter().map(|a| format!("{:.0}", a.length_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Length (mm)", |ui| {
        if let Some(l) = number(edit_field(ui, ui.scope_id().with(("acc_length", &ids)), length), bad) {
            action = Some(("ductaccessoryedit", json!({ "handles": ids, "length": l })));
        }
    });
    // The coefficient and the pressure drop: the one in use shows its value, the other is empty.
    let zeta =
        shared(&accs.iter().map(|a| if let cadcraft_doc::Loss::Zeta(z) = a.loss() { format!("{z}") } else { String::new() }).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Loss coefficient", |ui| {
        if let Some(z) = number(edit_field(ui, ui.scope_id().with(("acc_zeta", &ids)), zeta), bad) {
            action = Some(("ductaccessoryedit", json!({ "handles": ids, "zeta": z, "pressureDrop": Value::Null })));
        }
    });
    let pa =
        shared(&accs.iter().map(|a| if let cadcraft_doc::Loss::Pascals(p) = a.loss() { format!("{p}") } else { String::new() }).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Pressure drop (Pa)", |ui| {
        if let Some(p) = number(edit_field(ui, ui.scope_id().with(("acc_pa", &ids)), pa), bad) {
            action = Some(("ductaccessoryedit", json!({ "handles": ids, "pressureDrop": p })));
        }
    });
    if let [a] = accs {
        prop_row(ui, "Duct", |ui| read_only(ui, &format!("Ø {:.0} mm", a.diameter_mm)));
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

    #[test]
    fn a_wall_layer_typed_in_the_palette_changes_this_wall_and_the_button_gives_it_to_the_style() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let style = "Brick 200 + mineral wool 120 + plasterboard";
        let r = app.run("walladd", json!({ "points": [[0, 0], [4000, 0]], "style": style })).unwrap();
        let other = app.run("walladd", json!({ "points": [[0, 3000], [4000, 3000]], "style": style })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        let o = cadcraft_doc::Handle::parse_hex(other["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        let thick = |hn: &egui_kittest::Harness<'static, CadApp>, w| {
            cadcraft_doc::wall_of(hn.state().session.doc().unwrap().entity(w).unwrap()).unwrap().style.components[1].thickness
        };
        // The mineral wool, 120 mm.
        let field = hn.get_all_by_value("120").next().unwrap();
        field.focus();
        hn.run_steps(2);
        hn.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        hn.run_steps(2);
        hn.get_all_by_value("120").next().unwrap().type_text("80");
        hn.run_steps(2);
        hn.key_press(egui::Key::Enter);
        hn.run_steps(3);
        assert_eq!((thick(&hn, h), thick(&hn, o)), (80.0, 120.0), "this wall only");
        // The button: the style and the other wall take 80 too.
        hn.get_by_label("Apply to the whole style").click();
        hn.run_steps(3);
        assert_eq!(thick(&hn, o), 80.0);
        assert_eq!(hn.state().session.doc().unwrap().wall_style(style).unwrap().components[1].thickness, 80.0);
    }
}

#[cfg(test)]
mod hvac_tests {
    use cadcraft_engine::Session;
    use egui::vec2;
    use egui_kittest::kittest::Queryable;
    use serde_json::json;

    use crate::{CadApp, Services};

    fn palette(app: CadApp) -> egui_kittest::Harness<'static, CadApp> {
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(400.0, 1600.0))
            .build_ui_state(|ui, app: &mut CadApp| super::super::properties_section(app, ui), app);
        h.run_steps(3);
        h
    }

    fn type_into(h: &mut egui_kittest::Harness<'static, CadApp>, value: &str, text: &str) {
        h.get_all_by_value(value).next().unwrap().focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_all_by_value(value).next().unwrap().type_text(text);
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(3);
    }

    #[test]
    fn a_space_set_to_1200_m3h_in_the_palette_is_redrawn_with_its_terminals_counted() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("spaceadd", json!({ "points": [[0, 0], [10100, 0], [10100, 7000], [0, 7000]], "name": "Gym" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.run("airterminaladd", json!({ "space": r["handle"], "count": 4, "flow": 300 })).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.get_all_by_value("Gym").next().is_some());
        // The read-only rows are painted; what they show comes from here.
        assert_eq!(super::terminals_in(hn.state().session.doc().unwrap(), h), Some((4, 1200.0)));
        type_into(&mut hn, "25", "1200");
        let p = cadcraft_doc::space_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.flow_m3h, Some(1200.0));
        assert!(hn.get_all_by_value("1200").next().is_some(), "the Flow field shows the new flow");
    }

    #[test]
    fn a_space_takes_the_number_of_occupants_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("spaceadd", json!({ "points": [[0, 0], [6000, 0], [6000, 5000], [0, 5000]], "occupants": 6 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        type_into(&mut hn, "6", "20");
        let p = cadcraft_doc::space_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.occupants, 20);
    }

    #[test]
    fn selected_terminals_take_a_new_flow_together() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let a = app.run("airterminaladd", json!({ "at": [0, 0], "flow": 200 })).unwrap();
        let b = app.run("airterminaladd", json!({ "at": [2000, 0], "flow": 200 })).unwrap();
        let hs: Vec<cadcraft_doc::Handle> =
            [a, b].iter().map(|r| cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap()).collect();
        app.session.set_selection(hs.clone());
        let mut hn = palette(app);
        assert!(hn.get_all_by_label("SQ600-160").next().is_some());
        type_into(&mut hn, "200", "320");
        let d = hn.state().session.doc().unwrap();
        for h in hs {
            let t = cadcraft_doc::air_terminal_of(d.entity(h).unwrap()).unwrap();
            assert_eq!(t.flow_m3h, 320.0);
        }
    }

    #[test]
    fn a_selected_duct_shows_its_diameter_and_a_typed_one_is_imposed() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("ductadd", json!({ "points": [[0, 0], [3000, 0]] })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        type_into(&mut hn, "200", "315");
        let p = cadcraft_doc::duct_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.diameter_mm, 315.0);
        assert!(p.imposed, "a typed diameter is the user's: sizing leaves it");
        assert!(super::is_parametric(hn.state().session.doc().unwrap(), h), "a double-click opens Properties");
    }

    #[test]
    fn a_selected_accessory_takes_a_typed_pressure_drop_and_length() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        app.run("ductadd", json!({ "points": [[0, 0], [4000, 0]] })).unwrap();
        let r = app.run("ductaccessoryadd", json!({ "at": [2000, 0], "type": "silencer" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        // The silencer's length is 1000 mm and its pressure drop 30 Pa.
        type_into(&mut hn, "1000", "1200");
        type_into(&mut hn, "30", "45");
        let a = cadcraft_doc::accessory_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(a.length_mm, 1200.0);
        assert_eq!(a.loss(), cadcraft_doc::Loss::Pascals(45.0));
    }

    #[test]
    fn an_opening_selected_shows_its_distances_from_both_ends_and_a_typed_one_moves_it() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let w = app.run("walladd", json!({ "points": [[0, 0], [5000, 0]] })).unwrap();
        let w = cadcraft_doc::Handle::parse_hex(w["handle"].as_str().unwrap()).unwrap();
        let r = app.run("openingadd", json!({ "at": [1000, 0] })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        // 1000 centre, 900 wide: 550 from the start, 3550 from the end.
        assert!(hn.get_all_by_value("550").next().is_some() && hn.get_all_by_value("3550").next().is_some());
        type_into(&mut hn, "550", "300");
        let cut = |hn: &egui_kittest::Harness<'static, CadApp>| {
            cadcraft_doc::wall_of(hn.state().session.doc().unwrap().entity(w).unwrap()).unwrap().joints.openings[0]
        };
        assert_eq!((cut(&hn).from, cut(&hn).to), (300.0, 1200.0));
        // The width typed (900 → 1000) widens it about its centre.
        type_into(&mut hn, "900", "1000");
        let o = cadcraft_doc::opening_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(o.width_mm, 1000.0);
    }

    #[test]
    fn a_selected_zone_takes_the_height_and_altitude_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app
            .run("zoneadd", json!({ "points": [[0, 0], [3000, 0], [3000, 600], [0, 600]], "voidHeight": 250, "altitude": 2.45, "name": "Poutre" }))
            .unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.get_all_by_value("Poutre").next().is_some());
        type_into(&mut hn, "250", "300");
        let z = cadcraft_doc::zone_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((z.void_height_mm, z.bottom_mm), (300.0, Some(2450.0)));
        // The altitude is shown in metres.
        type_into(&mut hn, "2.45", "2.7");
        let z = cadcraft_doc::zone_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(z.bottom_mm, Some(2700.0));
    }

    #[test]
    fn a_selected_riser_takes_the_flow_and_the_shaft_typed_in_the_palette_and_opens_on_double_click() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("riseradd", json!({ "at": [0, 0], "width": 600, "depth": 400, "name": "VMC 3", "incomingFlow": 660 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        assert!(super::is_parametric(app.session.doc().unwrap(), h), "a double-click opens Properties");
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.get_all_by_value("VMC 3").next().is_some());
        type_into(&mut hn, "660", "900");
        let p = cadcraft_doc::riser_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.incoming_flow_m3h, Some(900.0));
        type_into(&mut hn, "600", "800");
        let p = cadcraft_doc::riser_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.shaft_mm(), (800.0, 400.0));
    }

    #[test]
    fn a_duct_on_auto_shows_the_type_and_the_air_direction_sizing_gave_it() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        app.run("ahuadd", json!({ "at": [-3000, 0], "type": "supplyUnit" })).unwrap();
        let r = app.run("ductadd", json!({ "points": [[-1700, 0], [-1700, 8000]] })).unwrap();
        app.run("airterminaladd", json!({ "at": [-1700, 8000], "flow": 350, "air": "supply" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let hn = palette(app);
        assert!(hn.query_all_by_label("Auto").count() >= 1 && hn.query_all_by_label_contains("Auto (").count() == 0, "not sized yet: Auto only");
        let mut app = hn.into_state();
        app.run("ductsize", json!({})).unwrap();
        let hn = palette(app);
        assert!(hn.query_by_label("Auto (Main)").is_some(), "the type sizing gave");
        assert!(hn.query_by_label("Auto (Supply)").is_some(), "the direction sizing found");
    }
}
