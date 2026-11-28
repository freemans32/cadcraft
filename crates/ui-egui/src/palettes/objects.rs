//! Properties of the parametric objects drawn as block references (walls, spaces, air
//! terminals): when the selection is only such objects of one kind, a group of their own parameters, each change running the object's edit
//! command so the object is regenerated, as the command line would.

use cadcraft_doc::{
    AccessoryKind, AccessoryParams, AhuKind, AhuParams, AirDirection, CableUsage, Drawing, DuctKind, DuctParams, DuctShape, Handle, OpeningKind,
    OpeningParams, PartParams, PipeParams, PipeRiserParams, PipeService, RiserParams, SlabParams, SpaceParams, TerminalKind, TerminalParams,
    WallJustification, ZoneParams, accessory_of, ahu_of, air_terminal_of, ceiling_of, duct_of, opening_of, part_of, pipe_of, pipe_riser_of, rail_of,
    slab_of, space_of, stair_of, struct_of, wall_of, zone_of,
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
    let equipment: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::equipment_of(e))).collect();
    if !equipment.is_empty() && equipment.len() == sel.len() {
        return equipment_section(ui, d, sel, &equipment, bad);
    }
    let risers: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::any_riser_of(e))).collect();
    if !risers.is_empty() && risers.len() == sel.len() {
        if risers.iter().all(RiserParams::is_cable) {
            return cable_riser_section(ui, d, sel, &risers, bad);
        }
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
    let pipes: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| pipe_of(e))).collect();
    if !pipes.is_empty() && pipes.len() == sel.len() {
        return pipe_section(ui, sel, &pipes, &d.network_styles, len, bad);
    }
    let pipe_risers: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| pipe_riser_of(e))).collect();
    if !pipe_risers.is_empty() && pipe_risers.len() == sel.len() {
        return pipe_riser_section(ui, sel, &pipe_risers, bad);
    }
    let parts: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| part_of(e))).collect();
    if !parts.is_empty() && parts.len() == sel.len() {
        return part_section(ui, sel, &parts, &d.part_library(), bad);
    }
    let controls: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::control_of(e))).collect();
    if !controls.is_empty() && controls.len() == sel.len() {
        return control_section(ui, sel, &controls, bad);
    }
    let furniture: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::furniture_of(e))).collect();
    if !furniture.is_empty() && furniture.len() == sel.len() {
        return furniture_section(ui, sel, &furniture, bad);
    }
    let fixtures: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::sanitary_of(e))).collect();
    if !fixtures.is_empty() && fixtures.len() == sel.len() {
        return sanitary_section(ui, sel, &fixtures, bad);
    }
    let devices: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::device_of(e))).collect();
    if !devices.is_empty() && devices.len() == sel.len() {
        return device_section(ui, sel, &devices, bad);
    }
    let cables: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| cadcraft_doc::cable_of(e))).collect();
    if !cables.is_empty() && cables.len() == sel.len() {
        return cable_section(ui, d, sel, &cables, bad);
    }
    let slabs: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| slab_of(e))).collect();
    if !slabs.is_empty() && slabs.len() == sel.len() {
        return slab_section(ui, sel, &slabs, bad);
    }
    if let [h] = sel
        && let Some(e) = d.entity(*h)
    {
        if let Some(c) = ceiling_of(e) {
            return ceiling_section(ui, sel, &c, bad);
        }
        if let Some(st) = stair_of(e) {
            return stair_section(ui, sel, &st, bad);
        }
        if let Some(r) = rail_of(e) {
            return rail_section(ui, sel, &r, bad);
        }
        if let Some(c) = cadcraft_doc::curtain_of(e) {
            return curtain_section(ui, sel, &c, bad);
        }
        if let Some(r) = cadcraft_doc::roof_of(e) {
            return roof_section(ui, sel, &r, bad);
        }
    }
    let structs: Vec<_> = sel.iter().filter_map(|h| d.entity(*h).and_then(|e| struct_of(e))).collect();
    if !structs.is_empty() && structs.len() == sel.len() {
        return struct_section(ui, sel, &structs, bad);
    }
    None
}

/// The line tags a pipe riser can be given.
const RISER_TAGS: [(&str, &str); 11] = [
    ("CHW-S", "Chilled water supply"),
    ("CHW-R", "Chilled water return"),
    ("HW-S", "Hot water supply"),
    ("HW-R", "Hot water return"),
    ("CO-S", "Change-over supply"),
    ("CO-R", "Change-over return"),
    ("COND", "Condensate drain"),
    ("REF-L", "Refrigerant liquid line"),
    ("REF-G", "Refrigerant gas line"),
    ("REF", "Refrigerant link (liquid + gas)"),
    ("REF-HR", "Refrigerant link, heat recovery (3 tubes)"),
];

/// A line tag with what it stands for, as the menus of the riser show it ("CHW-S  Chilled water supply").
fn line_label(tag: &str) -> String {
    match RISER_TAGS.iter().find(|(t, _)| t.eq_ignore_ascii_case(tag)) {
        Some((t, name)) => format!("{t}  {}", crate::i18n::t(name)),
        None => tag.to_string(),
    }
}

/// The rows of the network a pipe is on: its name (typed, it is the name of the whole network), and the colour and the line type of the
/// network's style. The colour is chosen from the palette, and shown as a swatch.
fn network_rows(ui: &mut egui::Ui, ids: &[String], networks: Vec<String>, styles: &[cadcraft_doc::NetworkStyle], action: &mut Action) {
    let name = shared(&networks).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Network", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("network_name", ids)), name.clone()).filter(|n| n != VARIES && n.trim() != name.trim()) {
            *action = Some(("pipeedit", json!({ "handles": ids, "network": n })));
        }
    });
    let Some(style) = styles.iter().find(|st| cadcraft_doc::same_network(&st.name, &name)) else { return };
    let style_edit = |key: &str, value: Value| Some(("pipenetwork", json!({ "style": { "name": style.name, key: value } })));
    prop_row(ui, "Network colour", |ui| {
        // A swatch of the colour, then the menu of the palette.
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 14.0), Sense::hover());
        ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(style.color[0], style.color[1], style.color[2]));
        let shown = cadcraft_doc::NETWORK_PALETTE.iter().find(|(_, c)| *c == style.color).map_or("Custom", |(n, _)| n);
        if let Some(chosen) = pick_menu(ui, crate::i18n::t(shown), cadcraft_doc::NETWORK_PALETTE.iter().map(|(n, _)| (*n).to_string())) {
            *action = style_edit("color", json!(chosen));
        }
    });
    prop_row(ui, "Network line type", |ui| {
        let shown = cadcraft_doc::NETWORK_LINETYPES.iter().find(|(n, _)| *n == style.linetype).map_or(style.linetype.as_str(), |(_, w)| w);
        let items: Vec<(&'static str, &'static str)> = cadcraft_doc::NETWORK_LINETYPES.iter().map(|(n, w)| (*n, *w)).collect();
        if let Some(lt) = choice(ui, shown, &items) {
            *action = style_edit("linetype", json!(lt));
        }
    });
}

/// The Properties group of pipe risers: their name, direction and height, and the lines each holds (its size, and what it carries to
/// the other floors), with what sizing found; lines are added and removed.
fn pipe_riser_section(ui: &mut egui::Ui, sel: &[Handle], rs: &[PipeRiserParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("piperiseredit", json!({ "handles": ids, key: value })));
    group(ui, "Pipe Riser");
    let name = shared(&rs.iter().map(|r| r.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("riser_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let up = shared(&rs.iter().map(|r| r.up).collect::<Vec<_>>());
    prop_row(ui, "Direction", |ui| {
        let shown = up.map_or(VARIES, |u| if u { "Up" } else { "Down" });
        if let Some(k) = choice(ui, shown, &[("up", "Up"), ("down", "Down")]) {
            action = edit("up", json!(k == "up"));
        }
    });
    let height =
        shared(&rs.iter().map(|r| r.height_mm.map_or(String::new(), |h| format!("{h}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height (mm)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_height", &ids)), height).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("height", Value::Null);
            } else if let Some(n) = number(Some(t), bad) {
                action = edit("height", json!(n));
            }
        }
    });
    // The lines of one riser: their size, and what they carry to the other floors.
    let [r] = rs else { return action };
    for (i, line) in r.lines.iter().enumerate() {
        let n = i + 1;
        let pipe = line.pipe(r.units);
        let tag = pipe.tag();
        let line_edit = |key: &str, value: Value| Some(("piperiseredit", json!({ "handles": ids, "line": { "index": n, key: value } })));
        // Sizes: a steel pipe in millimetres, a refrigerant tube in inches (3/8, 1 1/8) or millimetres.
        let tubes: Vec<(&str, &str, f64)> = if line.service.is_refrigerant() {
            let mut v = vec![("dn", "liquid", line.dn_mm)];
            if line.service.has_gas_tube() {
                v.push(("gas", if line.service == cadcraft_doc::PipeService::RefrigerantTriple { "suction gas" } else { "gas" }, line.gas_dn_mm));
            }
            if line.service == cadcraft_doc::PipeService::RefrigerantTriple {
                v.push(("hpGas", "HP/LP gas", line.gas2_dn_mm));
            }
            v
        } else {
            vec![("dn", "size", line.dn_mm)]
        };
        for (key, what, value) in tubes {
            let label = crate::tf!("Line {n} {tag}: {what}", n = n, tag = tag, what = crate::i18n::t(what));
            let shown = if line.service.is_refrigerant() { cadcraft_doc::refrigerant_size_label(value).replace('"', "") } else { format!("{value}") };
            prop_row(ui, &label, |ui| {
                if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_size", n, key, &ids)), shown).filter(|t| !t.trim().is_empty()) {
                    if line.service.is_refrigerant() {
                        if cadcraft_doc::parse_refrigerant_size(&t).is_some() {
                            action = line_edit(key, json!(t));
                        } else {
                            *bad = Some(t);
                        }
                    } else if let Some(mm) = number(Some(t), bad) {
                        action = line_edit(key, json!(mm));
                    }
                }
            });
        }
        // The way the line goes through this level: chosen for each line (a drain always goes down).
        let direction_label = crate::tf!("Line {n} {tag}: direction", n = n, tag = tag);
        prop_row(ui, &direction_label, |ui| {
            if line.service == cadcraft_doc::PipeService::Condensate {
                read_only(ui, crate::i18n::t("Goes down (a drain)"));
                return;
            }
            let shown = match line.goes_up {
                Some(true) => "Goes up",
                Some(false) => "Goes down",
                None if line.flows_up(r.up) => "Auto (goes up)",
                None => "Auto (goes down)",
            };
            let items = [("up", "Goes up"), ("down", "Goes down"), ("auto", "Auto")];
            if let Some(d) = choice(ui, shown, &items) {
                action = line_edit("direction", json!(d));
            }
        });
        let unit = match line.service {
            cadcraft_doc::PipeService::Condensate => "l/h",
            s if s.is_refrigerant() => "index",
            _ => "kW",
        };
        let label = crate::tf!("Line {n} {tag}: carried to the other floors ({unit})", n = n, tag = tag, unit = unit);
        let incoming = line.incoming.map_or(String::new(), |v| format!("{v}"));
        prop_row(ui, &label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_incoming", n, &ids)), incoming) {
                if t.trim().is_empty() {
                    action = line_edit("incoming", Value::Null);
                } else if let Some(v) = number(Some(t), bad) {
                    action = line_edit("incoming", json!(v));
                }
            }
        });
        if let (Some(load), Some(flow)) = (line.load, line.flow_m3h) {
            let label = crate::tf!("Line {n} {tag}: carried here", n = n, tag = tag);
            let text = if line.service == cadcraft_doc::PipeService::Condensate {
                format!("{load:.0} l/h")
            } else {
                format!("{load:.0} kW, {flow:.2} m³/h")
            };
            prop_row(ui, &label, |ui| read_only(ui, &text));
        }
    }
    prop_row(ui, "Lines", |ui| {
        if let Some(tag) = pick_menu(ui, crate::i18n::t("Add a line"), RISER_TAGS.iter().map(|(t, _)| line_label(t))) {
            action = edit("addLines", json!([tag.split_whitespace().next().unwrap_or_default()]));
        }
        let names: Vec<String> = r.lines.iter().enumerate().map(|(i, l)| format!("{} {}", i + 1, line_label(l.pipe(r.units).tag()))).collect();
        let removed =
            pick_menu(ui, crate::i18n::t("Remove a line"), names).and_then(|c| c.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()));
        if let Some(n) = removed {
            action = edit("removeLine", json!(n));
        }
    });
    action
}

/// The "Altitude of the underside (m)" row of an element that has one (stored in mm): empty clears it.
fn altitude_row(ui: &mut egui::Ui, cmd: &'static str, ids: &[String], values: Vec<Option<f64>>, bad: &mut Option<String>, action: &mut Action) {
    let texts: Vec<String> = values.iter().map(|v| v.map_or(String::new(), |b| format!("{}", b / 1000.0))).collect();
    let v = shared(&texts).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Altitude of the underside (m)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("altitude_row", cmd, ids)), v).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                *action = Some((cmd, json!({ "handles": ids, "altitude": Value::Null })));
            } else if let Some(m) = number(Some(t), bad) {
                *action = Some((cmd, json!({ "handles": ids, "altitude": m })));
            }
        }
    });
}

/// A text field of the electrical groups: what the objects share (or varies), and the command a change asks for.
fn elec_text(ui: &mut egui::Ui, cmd: &'static str, label: &str, key: &str, ids: &[String], values: Vec<String>, action: &mut Action) {
    let v = shared(&values).unwrap_or_else(|| VARIES.into());
    prop_row(ui, label, |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("elec_text", key, ids)), v).filter(|t| t != VARIES) {
            *action = Some((cmd, json!({ "handles": ids, key: t.trim() })));
        }
    });
}

/// A number field of the electrical groups; empty clears it when `clearable`.
fn elec_number(
    ui: &mut egui::Ui,
    (cmd, label, key, clearable): (&'static str, &str, &str, bool),
    ids: &[String],
    values: Vec<Option<f64>>,
    bad: &mut Option<String>,
    action: &mut Action,
) {
    let texts: Vec<String> = values.iter().map(|v| v.map_or(String::new(), |f| format!("{f}"))).collect();
    let v = shared(&texts).unwrap_or_else(|| VARIES.into());
    prop_row(ui, label, |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("elec_number", key, ids)), v).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                if clearable {
                    *action = Some((cmd, json!({ "handles": ids, key: Value::Null })));
                }
            } else if let Some(n) = number(Some(t), bad) {
                *action = Some((cmd, json!({ "handles": ids, key: n })));
            }
        }
    });
}

/// The Properties group of furniture and appliances: their model, name, size, altitude and power.
fn furniture_section(ui: &mut egui::Ui, sel: &[Handle], fs: &[cadcraft_doc::FurnitureParams], bad: &mut Option<String>) -> Action {
    const CMD: &str = "furnitureedit";
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Furniture");
    let model = shared(&fs.iter().map(|f| f.model.clone()).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let shown = model.as_deref().and_then(cadcraft_doc::furniture_model).map_or(VARIES, |m| m.label);
        // The models by group, a submenu each.
        ui.menu_button(crate::i18n::t(shown), |ui| {
            for g in cadcraft_doc::furniture_groups() {
                ui.menu_button(crate::i18n::t(g), |ui| {
                    for m in cadcraft_doc::furniture_models().iter().filter(|m| m.group == *g) {
                        if ui.button(crate::i18n::t(m.label)).clicked() {
                            action = Some((CMD, json!({ "handles": ids, "model": m.id })));
                            ui.close();
                        }
                    }
                });
            }
        });
    });
    elec_text(ui, CMD, "Name", "name", &ids, fs.iter().map(|f| f.name.clone()).collect(), &mut action);
    elec_number(ui, (CMD, "Width (mm)", "width", false), &ids, fs.iter().map(|f| Some(f.width_mm)).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Depth (mm)", "depth", false), &ids, fs.iter().map(|f| Some(f.depth_mm)).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Height (mm)", "height", false), &ids, fs.iter().map(|f| Some(f.height_mm)).collect(), bad, &mut action);
    altitude_row(ui, CMD, &ids, fs.iter().map(|f| f.bottom_mm).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Power (W)", "power", true), &ids, fs.iter().map(|f| f.power_w).collect(), bad, &mut action);
    action
}

/// The Properties group of sanitary fixtures: their type, name, size, drain, flows and altitude.
fn sanitary_section(ui: &mut egui::Ui, sel: &[Handle], fs: &[cadcraft_doc::SanitaryParams], bad: &mut Option<String>) -> Action {
    use cadcraft_doc::{PipeService, SanitaryKind};
    const CMD: &str = "fixtureedit";
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Sanitary Fixture");
    let kind = shared(&fs.iter().map(|f| f.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let all: Vec<(&'static str, &'static str)> = SanitaryKind::ALL.iter().map(|k| (k.name(), super::plumbing::sanitary_kind_label(*k))).collect();
        if let Some(k) = choice(ui, kind.map_or(VARIES, super::plumbing::sanitary_kind_label), &all) {
            action = Some((CMD, json!({ "handles": ids, "kind": k })));
        }
    });
    elec_text(ui, CMD, "Name", "name", &ids, fs.iter().map(|f| f.name.clone()).collect(), &mut action);
    elec_number(ui, (CMD, "Width (mm)", "width", false), &ids, fs.iter().map(|f| Some(f.width_mm)).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Depth (mm)", "depth", false), &ids, fs.iter().map(|f| Some(f.depth_mm)).collect(), bad, &mut action);
    let drain = shared(&fs.iter().map(|f| f.drain).collect::<Vec<_>>());
    let drain_label = |d: Option<PipeService>| d.map_or("None", super::pipe_service_label);
    prop_row(ui, "Drain", |ui| {
        let items = [("wasteWater", "Waste water (EU)"), ("soilWater", "Soil water (EV)"), ("rainWater", "Rain water (EP)"), ("none", "None")];
        if let Some(v) = choice(ui, drain.map_or(VARIES, drain_label), &items) {
            action = Some((CMD, json!({ "handles": ids, "drain": if v == "none" { Value::Null } else { json!(v) } })));
        }
    });
    elec_number(ui, (CMD, "Drain diameter (mm)", "drainDn", false), &ids, fs.iter().map(|f| Some(f.drain_dn_mm)).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Cold water (l/s)", "cold", true), &ids, fs.iter().map(|f| f.cold_ls).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Hot water (l/s)", "hot", true), &ids, fs.iter().map(|f| f.hot_ls).collect(), bad, &mut action);
    altitude_row(ui, CMD, &ids, fs.iter().map(|f| f.bottom_mm).collect(), bad, &mut action);
    action
}

/// The Properties group of control devices: tag, the controller it is wired to, what it serves, its circuit, and what a controller offers.
fn control_section(ui: &mut egui::Ui, sel: &[Handle], cs: &[cadcraft_doc::ControlParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Control Device");
    if let Some(k) = shared(&cs.iter().map(|c| c.kind).collect::<Vec<_>>()) {
        prop_row(ui, "Type", |ui| read_only(ui, k.name()));
    }
    const CMD: &str = "controledit";
    elec_text(ui, CMD, "Tag", "name", &ids, cs.iter().map(|c| c.name.clone()).collect(), &mut action);
    elec_text(ui, CMD, "Wired to controller", "controller", &ids, cs.iter().map(|c| c.controller.clone()).collect(), &mut action);
    elec_text(ui, CMD, "Serves", "served", &ids, cs.iter().map(|c| c.served.clone()).collect(), &mut action);
    elec_text(ui, CMD, "Circuit", "circuit", &ids, cs.iter().map(|c| c.circuit.clone()).collect(), &mut action);
    elec_text(ui, CMD, "Address", "address", &ids, cs.iter().map(|c| c.address.clone()).collect(), &mut action);
    if cs.iter().all(|c| c.kind.is_automation()) {
        elec_text(ui, CMD, "Bus", "bus", &ids, cs.iter().map(|c| c.bus.clone()).collect(), &mut action);
        elec_text(ui, CMD, "Bus protocol", "protocol", &ids, cs.iter().map(|c| c.protocol.clone()).collect(), &mut action);
    }
    elec_number(ui, (CMD, "Setpoint", "setpoint", true), &ids, cs.iter().map(|c| c.setpoint).collect(), bad, &mut action);
    elec_number(ui, (CMD, "Height (mm)", "height", false), &ids, cs.iter().map(|c| Some(c.height_mm)).collect(), bad, &mut action);
    if cs.iter().all(|c| matches!(c.kind, cadcraft_doc::ControlKind::Controller | cadcraft_doc::ControlKind::IoModule)) {
        for (label, key, get) in [
            ("Analogue inputs offered", "ai", (|c: &cadcraft_doc::ControlParams| c.capacity.ai) as fn(&cadcraft_doc::ControlParams) -> u32),
            ("Analogue outputs offered", "ao", |c| c.capacity.ao),
            ("Digital inputs offered", "di", |c| c.capacity.di),
            ("Digital outputs offered", "do", |c| c.capacity.dout),
        ] {
            elec_number(ui, (CMD, label, key, false), &ids, cs.iter().map(|c| Some(f64::from(get(c)))).collect(), bad, &mut action);
        }
    }
    action
}

/// The Properties group of electrical devices: tag, circuit, power, flux, height and, for a board, its underside, its size and its main breaker.
fn device_section(ui: &mut egui::Ui, sel: &[Handle], ds: &[cadcraft_doc::DeviceParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Electrical Device");
    if let Some(k) = shared(&ds.iter().map(|d| d.kind).collect::<Vec<_>>()) {
        prop_row(ui, "Type", |ui| read_only(ui, k.name()));
    }
    elec_text(ui, "elecedit", "Tag", "name", &ids, ds.iter().map(|d| d.name.clone()).collect(), &mut action);
    elec_text(ui, "elecedit", "Circuit", "circuit", &ids, ds.iter().map(|d| d.circuit.clone()).collect(), &mut action);
    elec_text(ui, "elecedit", "Address", "address", &ids, ds.iter().map(|d| d.address.clone()).collect(), &mut action);
    elec_number(ui, ("elecedit", "Power (W)", "power", false), &ids, ds.iter().map(|d| Some(d.power_w)).collect(), bad, &mut action);
    if ds.iter().all(|d| d.kind.is_light()) {
        elec_number(ui, ("elecedit", "Flux (lm)", "flux", true), &ids, ds.iter().map(|d| d.flux_lm).collect(), bad, &mut action);
    }
    let board = ds.iter().all(|d| d.kind == cadcraft_doc::DeviceKind::Panel);
    let height = if board { "Underside height (mm)" } else { "Height (mm)" };
    elec_number(ui, ("elecedit", height, "height", false), &ids, ds.iter().map(|d| Some(d.height_mm)).collect(), bad, &mut action);
    if board {
        for (label, key, get) in [
            ("Board height (mm)", "boardHeight", (|d: &cadcraft_doc::DeviceParams| d.board_mm().0) as fn(&cadcraft_doc::DeviceParams) -> f64),
            ("Board width (mm)", "boardWidth", |d| d.board_mm().1),
            ("Board depth (mm)", "boardDepth", |d| d.board_mm().2),
        ] {
            elec_number(ui, ("elecedit", label, key, false), &ids, ds.iter().map(|d| Some(get(d))).collect(), bad, &mut action);
        }
        elec_number(ui, ("elecedit", "Main breaker (A)", "supply", true), &ids, ds.iter().map(|d| d.supply_a).collect(), bad, &mut action);
    }
    action
}

/// The label of a use of cables, in the cable type menu.
fn usage_label(u: cadcraft_doc::CableUsage) -> &'static str {
    use cadcraft_doc::CableUsage::{Bus, Data, Lighting, Power, Remote, Safety};
    match u {
        Power => "Power",
        Lighting => "Lighting",
        Data => "Data",
        Safety => "Safety",
        Bus => "Control bus",
        Remote => "Remote control",
    }
}

/// The Properties group of cables: circuit, use, section, protection and the conditions of the laying.
fn cable_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], cs: &[cadcraft_doc::CableParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Cable");
    elec_text(ui, "elecedit", "Circuit", "circuit", &ids, cs.iter().map(|c| c.circuit.clone()).collect(), &mut action);
    // Unticked: the cables are drawn without their label (ticked only when all of them show it).
    let shown = cs.iter().all(|c| !c.hide_label);
    prop_row(ui, "Show the label", |ui| {
        let mut on = shown;
        if ui.checkbox(&mut on, "").changed() {
            action = Some(("elecedit", json!({ "handles": ids, "label": on })));
        }
    });
    let below = shared(&cs.iter().map(|c| c.label_below).collect::<Vec<_>>());
    prop_row(ui, "Label position", |ui| {
        let shown = below.map_or(VARIES, |b| if b { "Below the cable" } else { "Above the cable" });
        if let Some(k) = choice(ui, shown, &[("above", "Above the cable"), ("below", "Below the cable")]) {
            action = Some(("elecedit", json!({ "handles": ids, "labelSide": k })));
        }
    });
    let usage = shared(&cs.iter().map(|c| c.usage.name().to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Use", |ui| {
        if let Some(u) = pick_menu(ui, crate::i18n::t(&usage), cadcraft_doc::CableUsage::ALL.iter().map(|u| u.name().to_string())) {
            action = Some(("elecedit", json!({ "handles": ids, "usage": u })));
        }
    });
    let phases = shared(&cs.iter().map(|c| c.phases.name().to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Phases", |ui| {
        if let Some(u) = pick_menu(ui, crate::i18n::t(&phases), cadcraft_doc::ElecPhases::ALL.iter().map(|u| u.name().to_string())) {
            action = Some(("elecedit", json!({ "handles": ids, "phases": u })));
        }
    });
    let laying = shared(&cs.iter().map(|c| c.laying.name().to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Laying (B, C, E, F)", |ui| {
        if let Some(u) = pick_menu(ui, &laying, cadcraft_doc::ElecLaying::ALL.iter().map(|u| u.name().to_string())) {
            action = Some(("elecedit", json!({ "handles": ids, "laying": u })));
        }
    });
    // The section is the cable as it is written: "3G1.5", "5G2.5", "2x0.75 blindé", or a section alone; empty is to be sized.
    let written = shared(&cs.iter().map(|c| c.spec_text()).collect::<Vec<_>>());
    prop_row(ui, "Section or cable (3G1.5, 2x0.75…)", |ui| {
        let shown = written.clone().unwrap_or_else(|| VARIES.into());
        if let Some(t) = edit_field(ui, ui.scope_id().with(("cable_spec", &ids)), shown).filter(|t| t != VARIES) {
            action = Some(("elecedit", json!({ "handles": ids, "section": if t.trim().is_empty() { Value::Null } else { json!(t.trim()) } })));
        }
    });
    // The library: a cable type gives its cable (and its use); the cable of this one can be kept under a name for other cables.
    // Typed in: the name of a cable type of the library gives that type, anything else is the cable as written (as in the section's box).
    prop_row(ui, "Cable type", |ui| {
        let shown = written.clone().unwrap_or_else(|| VARIES.into());
        let field = ui.scope_id().with(("cable_type_typed", &ids));
        ui.scope(|ui| {
            ui.set_max_width((ui.available_width() - 30.0).max(60.0));
            if let Some(t) = edit_field(ui, field, shown).map(|t| t.trim().to_string()).filter(|t| t != VARIES) {
                let known = d.available_cable_types().into_iter().find(|c| c.name.eq_ignore_ascii_case(&t)).map(|c| c.name);
                action = Some(match known {
                    Some(name) => ("elecedit", json!({ "handles": ids, "cableType": name })),
                    None => ("elecedit", json!({ "handles": ids, "section": if t.is_empty() { Value::Null } else { json!(t) } })),
                });
            }
        });
        ui.menu_button("▾", |ui| {
            let types = d.available_cable_types();
            for usage in cadcraft_doc::CableUsage::ALL {
                let of_use: Vec<_> = types.iter().filter(|t| t.usage == Some(usage)).collect();
                if of_use.is_empty() {
                    continue;
                }
                ui.menu_button(crate::i18n::t(usage_label(usage)), |ui| {
                    for t in of_use {
                        if ui.button(&t.name).on_hover_text(t.spec.text()).clicked() {
                            action = Some(("elecedit", json!({ "handles": ids, "cableType": t.name })));
                            ui.close();
                        }
                    }
                });
            }
        });
    });
    if let (Some(spec), Some(first)) = (written.as_deref().and_then(cadcraft_doc::parse_cable_spec), cs.first()) {
        prop_row(ui, "Save as cable type", |ui| {
            if let Some(name) = edit_field(ui, ui.scope_id().with(("cable_type_save", &ids)), String::new()).filter(|n| !n.trim().is_empty()) {
                action = Some(("cabletypeadd", json!({ "name": name.trim(), "spec": spec.text(), "usage": first.usage.name() })));
            }
        });
    }
    elec_number(ui, ("elecedit", "Breaker (A)", "protection", true), &ids, cs.iter().map(|c| c.protection_a).collect(), bad, &mut action);
    elec_number(ui, ("elecedit", "Power factor", "cosPhi", false), &ids, cs.iter().map(|c| Some(c.cos_phi)).collect(), bad, &mut action);
    elec_number(ui, ("elecedit", "Ambient (°C)", "ambient", false), &ids, cs.iter().map(|c| Some(c.ambient_c)).collect(), bad, &mut action);
    elec_number(
        ui,
        ("elecedit", "Circuits laid together", "grouped", false),
        &ids,
        cs.iter().map(|c| Some(f64::from(c.grouped))).collect(),
        bad,
        &mut action,
    );
    elec_number(ui, ("elecedit", "Extra length (m)", "extra", false), &ids, cs.iter().map(|c| Some(c.extra_m)).collect(), bad, &mut action);
    let len: f64 = cs.iter().map(|c| c.length_m()).sum();
    prop_row(ui, "Length (m)", |ui| read_only(ui, &format!("{len:.2}")));
    action
}

/// The Properties group of pipework parts: their tag, size, volume, power, flow and head, their outlets and their connections.
fn part_section(ui: &mut egui::Ui, sel: &[Handle], ps: &[PartParams], library: &[cadcraft_doc::PartEntry], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("pipepartedit", json!({ "handles": ids, key: value })));
    group(ui, "Pipework Part");
    let name = shared(&ps.iter().map(|p| p.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Tag", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("part_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    // The model: the others of the same kind are one click away, as the type of an air handling unit is.
    let kind = shared(&ps.iter().map(|p| p.kind).collect::<Vec<_>>());
    let model = shared(&ps.iter().map(|p| p.model.to_string()).collect::<Vec<String>>());
    let on = shared(&ps.iter().map(|p| p.network.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Network", |ui| read_only(ui, if on.is_empty() { "—" } else { &on }));
    prop_row(ui, "Model", |ui| {
        let fr = crate::i18n::current_language().starts_with("fr");
        let name_of = |m: &cadcraft_doc::PartEntry| if fr { m.name_fr.clone() } else { m.name.clone() };
        let same: Vec<_> = kind.map(|k| library.iter().filter(|m| m.kind() == k).cloned().collect()).unwrap_or_else(Vec::new);
        let shown = match &model {
            Some(m) if !m.is_empty() => same.iter().find(|x| x.name == *m).map_or_else(|| m.clone(), name_of),
            Some(_) => "—".to_string(),
            None => VARIES.to_string(),
        };
        // The model is told by its name: either language is understood.
        if let Some(chosen) = pick_menu(ui, &shown, same.iter().map(name_of)) {
            action = edit("model", json!(chosen));
        }
    });
    // Numbers that may be left empty (not known): a number sets, empty clears.
    for (label, key, get) in [
        ("Nominal diameter (mm)", "dn", (|p: &PartParams| p.dn_mm) as fn(&PartParams) -> Option<f64>),
        ("Volume (l)", "volume", |p| p.volume_l),
        ("Power (kW)", "power", |p| p.power_kw),
        ("Flow (m³/h)", "flow", |p| p.flow_m3h),
        ("Head (kPa)", "head", |p| p.head_kpa),
        ("Weight (kg)", "weight", |p| p.weight_kg),
        ("Pressure loss (kPa)", "loss", |p| p.loss_kpa),
        ("Kvs (m³/h)", "kvs", |p| p.kvs),
        ("Equivalent length (m)", "equivalentLength", |p| p.equivalent_length_m),
        ("Loss coefficient ζ", "zeta", |p| p.zeta),
    ] {
        let v = shared(&ps.iter().map(|p| get(p).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("part_value", key, &ids)), v).filter(|t| t != VARIES) {
                if t.trim().is_empty() {
                    action = edit(key, Value::Null);
                } else if let Some(n) = number(Some(t), bad) {
                    action = edit(key, json!(n));
                }
            }
        });
    }
    // The dimensions, in the order the data sheets give them: height, width (along the pipe), depth.
    group(ui, "Dimensions (H × W × D)");
    for (label, key, get) in [
        ("Height H (mm)", "height", (|p: &PartParams| p.height_mm) as fn(&PartParams) -> Option<f64>),
        ("Width W (mm)", "length", |p| Some(p.length_mm)),
        ("Depth D (mm)", "width", |p| Some(p.width_mm)),
    ] {
        let v = shared(&ps.iter().map(|p| get(p).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("part_dim", key, &ids)), v).filter(|t| t != VARIES) {
                if t.trim().is_empty() && key == "height" {
                    action = edit(key, Value::Null);
                } else if let Some(n) = number(Some(t), bad) {
                    action = edit(key, json!(n));
                }
            }
        });
    }
    if ps.iter().any(|p| p.outlets > 0) {
        let outlets = shared(&ps.iter().map(|p| p.outlets.to_string()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Outlets", |ui| {
            if let Some(n) = number(edit_field(ui, ui.scope_id().with(("part_outlets", &ids)), outlets).filter(|t| t != VARIES), bad) {
                action = edit("outlets", json!(n.round()));
            }
        });
    }
    let connections = shared(&ps.iter().map(|p| p.connections.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Connections", |ui| {
        if let Some(c) = edit_field(ui, ui.scope_id().with(("part_connections", &ids)), connections).filter(|c| c != VARIES) {
            action = edit("connections", json!(c));
        }
    });
    altitude_row(ui, "pipepartedit", &ids, ps.iter().map(|p| p.bottom_mm).collect(), bad, &mut action);
    action
}

/// The Properties group of a ceiling grid: name, tile, turn, height of the underside, and what it counts.
fn ceiling_section(ui: &mut egui::Ui, sel: &[Handle], c: &cadcraft_doc::CeilingParams, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Ceiling Grid");
    elec_text(ui, "designedit", "Tag", "name", &ids, vec![c.name.clone()], &mut action);
    elec_text(ui, "designedit", "Tile (mm)", "tile", &ids, vec![format!("{:.0}x{:.0}", c.tile_x_mm, c.tile_y_mm)], &mut action);
    elec_number(ui, ("designedit", "Grid angle (degrees)", "angle", false), &ids, vec![Some(c.angle_deg)], bad, &mut action);
    elec_number(
        ui,
        ("designedit", "Elevation of the underside (m)", "elevation", false),
        &ids,
        vec![Some(c.elevation_mm / 1000.0)],
        bad,
        &mut action,
    );
    let t = c.tiles();
    prop_row(ui, "Tiles", |ui| read_only(ui, &format!("{} + {}", t.whole, t.cut)));
    prop_row(ui, "Area", |ui| read_only(ui, &format!("{:.2} m²", t.area_m2)));
    action
}

/// The Properties group of a stair: name, rise, riser, tread and width, with the risers it needs and whether it is comfortable.
fn stair_section(ui: &mut egui::Ui, sel: &[Handle], st: &cadcraft_doc::StairParams, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Stair");
    elec_text(ui, "designedit", "Tag", "name", &ids, vec![st.name.clone()], &mut action);
    for (label, key, v) in [
        ("Rise (mm)", "rise", st.rise_mm),
        ("Highest riser (mm)", "riserMax", st.riser_max_mm),
        ("Tread (mm)", "going", st.going_mm),
        ("Width (mm)", "width", st.width_mm),
    ] {
        elec_number(ui, ("designedit", label, key, false), &ids, vec![Some(v)], bad, &mut action);
    }
    prop_row(ui, "Risers", |ui| read_only(ui, &format!("{} x {:.0} mm", st.risers(), st.riser_mm())));
    prop_row(ui, "Run (mm)", |ui| read_only(ui, &format!("{:.0}", st.run_mm())));
    prop_row(ui, "Comfort (2 riser + tread)", |ui| {
        read_only(ui, &format!("{:.0} mm {}", 2.0 * st.riser_mm() + st.going_mm, if st.comfortable() { "OK" } else { "!" }))
    });
    action
}

/// The Properties group of a railing: name, height, spacing of the posts, with its length and posts.
fn rail_section(ui: &mut egui::Ui, sel: &[Handle], r: &cadcraft_doc::RailParams, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Railing");
    elec_text(ui, "designedit", "Tag", "name", &ids, vec![r.name.clone()], &mut action);
    elec_number(ui, ("designedit", "Height (mm)", "height", false), &ids, vec![Some(r.height_mm)], bad, &mut action);
    elec_number(ui, ("designedit", "Post spacing (mm)", "postSpacing", false), &ids, vec![Some(r.post_spacing_mm)], bad, &mut action);
    prop_row(ui, "Length (m)", |ui| read_only(ui, &format!("{:.3}", r.length_mm() / 1000.0)));
    prop_row(ui, "Posts", |ui| read_only(ui, &r.posts().len().to_string()));
    action
}

/// The Properties group of a roof: kind, pitch, direction of the ridge, eaves height, with its areas, rise and ridge.
fn roof_section(ui: &mut egui::Ui, sel: &[Handle], r: &cadcraft_doc::RoofParams, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Roof");
    const CMD: &str = "roofedit";
    elec_text(ui, CMD, "Tag", "name", &ids, vec![r.name.clone()], &mut action);
    prop_row(ui, "Kind", |ui| {
        let items = [("shed", "Shed"), ("gable", "Gable"), ("hip", "Hip")];
        if let Some(k) = choice(ui, r.kind.name(), &items) {
            action = Some((CMD, json!({ "handles": ids, "kind": k })));
        }
    });
    elec_number(ui, (CMD, "Pitch (°)", "pitch", false), &ids, vec![Some(r.pitch_deg)], bad, &mut action);
    elec_number(ui, (CMD, "Ridge direction (°)", "angle", true), &ids, vec![Some(r.angle_deg)], bad, &mut action);
    elec_number(ui, (CMD, "Eaves height (m)", "elevation", true), &ids, vec![Some(r.elevation_mm / 1000.0)], bad, &mut action);
    elec_number(ui, (CMD, "Overhang (mm)", "overhang", false), &ids, vec![Some(r.overhang_mm)], bad, &mut action);
    let n = r.count();
    prop_row(ui, "Plan area (m²)", |ui| read_only(ui, &format!("{:.2}", n.plan_m2)));
    prop_row(ui, "Slope area (m²)", |ui| read_only(ui, &format!("{:.2}", n.area_m2)));
    prop_row(ui, "Rise (m)", |ui| read_only(ui, &format!("{:.2}", n.rise_m)));
    prop_row(ui, "Ridge length (m)", |ui| read_only(ui, &format!("{:.2}", n.ridge_m)));
    action
}

/// The Properties group of a curtain wall: name, height, spacing of mullions and transoms, depth, foot, with its length, panels and area.
fn curtain_section(ui: &mut egui::Ui, sel: &[Handle], c: &cadcraft_doc::CurtainParams, bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    group(ui, "Curtain Wall");
    const CMD: &str = "curtainedit";
    elec_text(ui, CMD, "Tag", "name", &ids, vec![c.name.clone()], &mut action);
    elec_number(ui, (CMD, "Height (mm)", "height", false), &ids, vec![Some(c.height_mm)], bad, &mut action);
    elec_number(ui, (CMD, "Mullion spacing (mm)", "panel", false), &ids, vec![Some(c.panel_mm)], bad, &mut action);
    elec_number(ui, (CMD, "Transom spacing (mm)", "transom", false), &ids, vec![Some(c.transom_mm)], bad, &mut action);
    elec_number(ui, (CMD, "Frame depth (mm)", "depth", false), &ids, vec![Some(c.depth_mm)], bad, &mut action);
    elec_number(ui, (CMD, "Elevation (m)", "elevation", true), &ids, vec![Some(c.elevation_mm / 1000.0)], bad, &mut action);
    let n = c.count();
    prop_row(ui, "Length (m)", |ui| read_only(ui, &format!("{:.3}", c.length_mm() / 1000.0)));
    prop_row(ui, "Panels", |ui| read_only(ui, &n.panels.to_string()));
    prop_row(ui, "Area (m²)", |ui| read_only(ui, &format!("{:.2}", n.area_m2)));
    action
}

/// The Properties group of columns and beams: tag, section, height (a column), elevation, with the mass and the top.
fn struct_section(ui: &mut egui::Ui, sel: &[Handle], ss: &[cadcraft_doc::StructParams], bad: &mut Option<String>) -> Action {
    use cadcraft_doc::StructKind;
    let ids = handles(sel);
    let mut action: Action = None;
    let kind = shared(&ss.iter().map(|p| p.kind).collect::<Vec<_>>());
    group(
        ui,
        match kind {
            Some(StructKind::Column) => "Column",
            Some(StructKind::Beam) => "Beam",
            None => "Structure",
        },
    );
    elec_text(ui, "structedit", "Tag", "name", &ids, ss.iter().map(|p| p.name.clone()).collect(), &mut action);
    elec_text(ui, "structedit", "Section", "section", &ids, ss.iter().map(|p| p.section.clone()).collect(), &mut action);
    if kind == Some(StructKind::Column) {
        elec_number(ui, ("structedit", "Height (mm)", "height", false), &ids, ss.iter().map(|p| Some(p.height_mm)).collect(), bad, &mut action);
    }
    let (label, key) = match kind {
        Some(StructKind::Column) => ("Elevation of the foot (m)", "elevation"),
        Some(StructKind::Beam) => ("Elevation of the underside (m)", "elevation"),
        None => ("Elevation (m)", "elevation"),
    };
    elec_number(ui, ("structedit", label, key, false), &ids, ss.iter().map(|p| Some(p.elevation_mm / 1000.0)).collect(), bad, &mut action);
    if let [p] = ss {
        prop_row(ui, "Top (m)", |ui| read_only(ui, &format!("{:+.2}", p.top_mm() / 1000.0)));
        if p.kind == StructKind::Beam {
            prop_row(ui, "Length (m)", |ui| read_only(ui, &format!("{:.3}", p.length_mm() / 1000.0)));
        }
        prop_row(ui, "Mass (kg)", |ui| read_only(ui, &format!("{:.1}", p.mass_kg())));
    } else {
        let total: f64 = ss.iter().map(cadcraft_doc::StructParams::mass_kg).sum();
        prop_row(ui, "Mass (kg)", |ui| read_only(ui, &format!("{total:.1}")));
    }
    action
}

/// The Properties group of slabs: their kind, name, thickness and the elevation of their top, with the area and volume.
fn slab_section(ui: &mut egui::Ui, sel: &[Handle], ss: &[SlabParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("slabedit", json!({ "handles": ids, key: value })));
    group(ui, "Slab");
    let kind = shared(&ss.iter().map(|p| p.kind).collect::<Vec<_>>());
    prop_row(ui, "Kind", |ui| {
        let label = |k: cadcraft_doc::SlabKind| match k {
            cadcraft_doc::SlabKind::Floor => "Floor slab",
            cadcraft_doc::SlabKind::Roof => "Roof slab",
            cadcraft_doc::SlabKind::Foundation => "Foundation slab",
        };
        let items = [("floor", "Floor slab"), ("roof", "Roof slab"), ("foundation", "Foundation slab")];
        if let Some(k) = choice(ui, kind.map_or(VARIES, label), &items) {
            action = edit("kind", json!(k));
        }
    });
    let thickness = shared(&ss.iter().map(|p| format!("{}", p.thickness_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Thickness (mm)", |ui| {
        if let Some(mm) = number(edit_field(ui, ui.scope_id().with(("slab_thickness", &ids)), thickness).filter(|t| t != VARIES), bad) {
            action = edit("thickness", json!(mm));
        }
    });
    let elevation = shared(&ss.iter().map(|p| format!("{}", p.elevation_mm / 1000.0)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Elevation of the top (m)", |ui| {
        if let Some(m) = number(edit_field(ui, ui.scope_id().with(("slab_elevation", &ids)), elevation).filter(|t| t != VARIES), bad) {
            action = edit("elevation", json!(m));
        }
    });
    let slope = shared(&ss.iter().map(|p| format!("{}", p.slope_pct)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Slope (%)", |ui| {
        if let Some(v) = number(edit_field(ui, ui.scope_id().with(("slab_slope", &ids)), slope).filter(|t| t != VARIES), bad) {
            action = edit("slope", json!(v));
        }
    });
    if ss.iter().any(|p| p.slope_pct > 0.0) {
        let dir = shared(&ss.iter().map(|p| format!("{}", p.slope_direction_deg)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Slope direction (degrees)", |ui| {
            if let Some(v) = number(edit_field(ui, ui.scope_id().with(("slab_slope_dir", &ids)), dir).filter(|t| t != VARIES), bad) {
                action = edit("slopeDirection", json!(v));
            }
        });
    }
    if let [p] = ss {
        prop_row(ui, "Area", |ui| read_only(ui, &format!("{:.2} m²", p.area_m2())));
        prop_row(ui, "Volume", |ui| read_only(ui, &format!("{:.2} m³", p.volume_m3())));
        if !p.holes.is_empty() {
            prop_row(ui, "Openings", |ui| read_only(ui, &format!("{} ({:.2} m²)", p.holes.len(), p.holes_area_m2())));
        }
    }
    action
}

/// Whether object `h` is a parametric object with a Properties group of its own.
pub(crate) fn is_parametric(d: &Drawing, h: Handle) -> bool {
    d.entity(h).is_some_and(|e| {
        wall_of(e).is_some()
            || space_of(e).is_some()
            || air_terminal_of(e).is_some()
            || duct_of(e).is_some()
            || pipe_of(e).is_some()
            || slab_of(e).is_some()
            || struct_of(e).is_some()
            || ceiling_of(e).is_some()
            || stair_of(e).is_some()
            || rail_of(e).is_some()
            || cadcraft_doc::curtain_of(e).is_some()
            || cadcraft_doc::roof_of(e).is_some()
            || part_of(e).is_some()
            || pipe_riser_of(e).is_some()
            || accessory_of(e).is_some()
            || cadcraft_doc::any_riser_of(e).is_some()
            || cadcraft_doc::equipment_of(e).is_some()
    })
}

/// The label of a pipe service in Properties.
pub(crate) fn pipe_service_label(s: PipeService) -> &'static str {
    match s {
        PipeService::ChilledWater => "Chilled water",
        PipeService::HotWater => "Hot water",
        PipeService::ChangeOver => "Change-over",
        PipeService::Condensate => "Condensate drain",
        PipeService::RefrigerantLiquid => "Refrigerant liquid line",
        PipeService::RefrigerantGas => "Refrigerant gas line",
        PipeService::RefrigerantPair => "Refrigerant link (liquid + gas)",
        PipeService::RefrigerantTriple => "Refrigerant link, heat recovery (3 tubes)",
        PipeService::WasteWater => "Waste water (EU)",
        PipeService::SoilWater => "Soil water (EV)",
        PipeService::RainWater => "Rain water (EP)",
        PipeService::ColdWater => "Cold water (EF)",
        PipeService::DomesticHotWater => "Domestic hot water (ECS)",
        PipeService::HotWaterLoop => "Hot water return loop",
    }
}

/// The Properties group of pipes: their service, size, insulation, altitude and load, and what sizing found.
fn pipe_section(
    ui: &mut egui::Ui,
    sel: &[Handle],
    ps: &[PipeParams],
    styles: &[cadcraft_doc::NetworkStyle],
    len: &dyn Fn(f64) -> String,
    bad: &mut Option<String>,
) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("pipeedit", json!({ "handles": ids, key: value })));
    group(ui, "Pipe");
    // Unticked: the runs are drawn without their label (ticked only when all of them show it).
    let shown = ps.iter().all(|p| !p.hide_label);
    prop_row(ui, "Show the label", |ui| {
        let mut on = shown;
        if ui.checkbox(&mut on, "").changed() {
            action = edit("label", json!(on));
        }
    });
    let service = shared(&ps.iter().map(|p| p.service).collect::<Vec<_>>());
    prop_row(ui, "Service", |ui| {
        let items = [
            ("chilledWater", "Chilled water"),
            ("hotWater", "Hot water"),
            ("changeOver", "Change-over"),
            ("condensate", "Condensate drain"),
            ("refrigerantLiquid", "Refrigerant liquid line"),
            ("refrigerantGas", "Refrigerant gas line"),
            ("refrigerantPair", "Refrigerant link (liquid + gas)"),
            ("refrigerantTriple", "Refrigerant link, heat recovery (3 tubes)"),
        ];
        if let Some(k) = choice(ui, service.map_or(VARIES, pipe_service_label), &items) {
            action = edit("service", json!(k));
        }
    });
    if ps.iter().all(|p| p.service.has_legs()) {
        let leg = shared(&ps.iter().map(|p| p.leg).collect::<Vec<_>>());
        prop_row(ui, "Leg", |ui| {
            let shown = leg.map_or(VARIES, |l| if l == cadcraft_doc::PipeLeg::Supply { "Supply" } else { "Return" });
            if let Some(k) = choice(ui, shown, &[("supply", "Supply"), ("return", "Return")]) {
                action = edit("leg", json!(k));
            }
        });
    }
    let imposed = shared(&ps.iter().map(|p| p.imposed).collect::<Vec<_>>());
    prop_row(ui, "Size set by", |ui| {
        let shown = imposed.map_or(VARIES, |i| if i { "User" } else { "Sizing" });
        if let Some(c) = choice(ui, shown, &[("auto", "Sizing"), ("keep", "User")]) {
            if c == "auto" {
                action = edit("dn", json!("auto"));
            } else if let [p] = ps {
                action = edit("dn", json!(p.dn_mm));
            }
        }
    });
    let refrigerant = ps.iter().all(|p| p.service.is_refrigerant());
    if refrigerant {
        // A refrigerant tube is sized in inches ("3/8"); a size typed in mm is understood too.
        let pair = ps.iter().all(|p| p.service.has_gas_tube());
        let triple = ps.iter().all(|p| p.service == PipeService::RefrigerantTriple);
        let mut tube = |ui: &mut egui::Ui, label: &str, key: &'static str, get: fn(&PipeParams) -> f64, action: &mut Action| {
            let shown = shared(&ps.iter().map(|p| cadcraft_doc::refrigerant_size_label(get(p)).replace('"', "")).collect::<Vec<_>>())
                .unwrap_or_else(|| VARIES.into());
            prop_row(ui, label, |ui| {
                if let Some(t) = edit_field(ui, ui.scope_id().with(("pipe_tube", key, &ids)), shown).filter(|t| t != VARIES) {
                    if cadcraft_doc::parse_refrigerant_size(&t).is_some() {
                        *action = edit(key, json!(t));
                    } else {
                        *bad = Some(crate::i18n::t("Requires a size in inches (3/8, 1 1/8) or millimetres.").to_string());
                    }
                }
            });
        };
        tube(ui, if pair { "Liquid tube (inches)" } else { "Tube diameter (inches)" }, "dn", |p| p.dn_mm, &mut action);
        if pair {
            tube(ui, if triple { "Suction gas tube (inches)" } else { "Gas tube (inches)" }, "gas", |p| p.gas_dn_mm, &mut action);
        }
        if triple {
            tube(ui, "High/low-pressure gas tube (inches)", "hpGas", |p| p.gas2_dn_mm, &mut action);
        }
    } else {
        let dn = shared(&ps.iter().map(|p| format!("{}", p.dn_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Nominal diameter (mm)", |ui| {
            if let Some(mm) = number(edit_field(ui, ui.scope_id().with(("pipe_dn", &ids)), dn).filter(|t| t != VARIES), bad) {
                action = edit("dn", json!(mm));
            }
        });
    }
    let insulation = shared(&ps.iter().map(|p| format!("{}", p.insulation_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Insulation (mm)", |ui| {
        if let Some(mm) = number(edit_field(ui, ui.scope_id().with(("pipe_insulation", &ids)), insulation).filter(|t| t != VARIES), bad) {
            action = edit("insulation", json!(mm));
        }
    });
    let altitude = shared(&ps.iter().map(|p| p.level_mm.map(|b| format!("{}", b / 1000.0)).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Altitude of the underside (m)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("pipe_altitude", &ids)), altitude).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("altitude", Value::Null);
            } else if let Some(m) = number(Some(t), bad) {
                action = edit("altitude", json!(m));
            }
        }
    });
    let drain = ps.iter().all(|p| p.service == PipeService::Condensate);
    let load = shared(&ps.iter().map(|p| p.load_kw.map(|l| format!("{l}")).unwrap_or_default()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, if drain { "Load at the free end (l/h)" } else { "Load at the free end (kW)" }, |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("pipe_load", &ids)), load).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("load", Value::Null);
            } else if let Some(v) = number(Some(t), bad) {
                action = edit("load", json!(v));
            }
        }
    });
    network_rows(ui, &ids, ps.iter().map(|p| p.network.clone()).collect(), styles, &mut action);
    let source = shared(&ps.iter().map(|p| p.source).collect::<Vec<_>>());
    prop_row(ui, "Starts at the source", |ui| {
        let mut on = source == Some(true);
        if ui.checkbox(&mut on, "").changed() {
            action = edit("source", json!(on));
        }
    });
    if let [p] = ps {
        prop_row(ui, "Length", |ui| read_only(ui, &len(p.length)));
        if let Some(f) = p.flow_m3h {
            let text = if p.service == PipeService::Condensate { format!("{:.1} l/h", f * 1000.0) } else { format!("{f:.2} m³/h") };
            prop_row(ui, "Flow", |ui| read_only(ui, &text));
        }
        if let Some(v) = p.velocity {
            prop_row(ui, "Velocity", |ui| read_only(ui, &format!("{v:.2} m/s")));
        }
        if let Some(l) = p.loss_pa_m {
            prop_row(ui, "Pressure loss", |ui| read_only(ui, &format!("{l:.0} Pa/m")));
        }
    }
    action
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
    // Walls edited by hand, scaled or drawn by an older version are left alone by the edits below: they can be redrawn from their data.
    let by_hand = sel.iter().filter(|h| d.entity(**h).is_some() && !d.wall_is_intact(**h)).count();
    if by_hand > 0 {
        prop_row(ui, "Edited by hand", |ui| {
            if ui
                .button(crate::i18n::t("Regenerate"))
                .on_hover_text(crate::i18n::t("Redraws these walls from their data: what was done to them by hand is lost."))
                .clicked()
            {
                action = Some(("walledit", json!({ "handles": ids, "force": true })));
            }
        });
    }
    // Turns the wall round: the same footprint, its layers in the opposite order (the plasterboard on the other face).
    prop_row(ui, "Layers order", |ui| {
        if ui
            .button(crate::i18n::t("Flip wall"))
            .on_hover_text(crate::i18n::t("Puts the layers of the wall in the opposite order, on the same footprint."))
            .clicked()
        {
            action = Some(("walledit", json!({ "handles": ids, "flip": true })));
        }
    });
    let height = shared(&walls.iter().map(|w| format!("{:.0}", w.height)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height (mm)", |ui| {
        if let Some(h) = number(edit_field(ui, ui.scope_id().with(("wall_height", &ids)), height), bad) {
            action = Some(("walledit", json!({ "handles": ids, "height": h })));
        }
    });
    // Whether each end of an open wall is joined to the walls it meets, or left as drawn.
    if walls.iter().all(|w| !w.closed) {
        for (label, key, joined) in
            [("Join at start", "freeStart", walls.iter().all(|w| !w.start_free)), ("Join at end", "freeEnd", walls.iter().all(|w| !w.end_free))]
        {
            prop_row(ui, label, |ui| {
                let mut on = joined;
                if ui.checkbox(&mut on, "").changed() {
                    action = Some(("walledit", json!({ "handles": ids, key: !on })));
                }
            });
        }
    }
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
        let path = w.centreline();
        let n = path.len();
        for i in 0..n {
            let next = if i + 1 < n {
                i + 1
            } else if w.closed {
                0
            } else {
                break;
            };
            if let (Some(a), Some(b)) = (path.get(i), path.get(next)) {
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
    altitude_row(ui, "airterminaledit", &ids, ts.iter().map(|t| t.bottom_mm).collect(), bad, &mut action);
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
    // Distances to the edge of the opening, from the face at each end of the wall segment it is in (the
    // face of the wall it meets, or its end): typing one moves it.
    if let ([o], [h]) = (openings, sel)
        && let Some((_, hit)) = d.opening_host(*h)
    {
        let mm = o.units.mm_per_unit;
        let (from_start, from_end) = ((hit.along - o.width() / 2.0 - hit.start_face) * mm, (hit.end_face - hit.along - o.width() / 2.0) * mm);
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
    // A free line written under the height: "Under the beam", for instance.
    let note = shared(&zones.iter().map(|z| z.note.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Note", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("zone_note", &ids)), note).filter(|n| n != VARIES) {
            action = edit("note", json!(n));
        }
    });
    action
}

// =====================================================================================
// Equipment (chillers, outdoor and indoor units)
// =====================================================================================

fn equipment_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], items: &[cadcraft_doc::EquipmentParams], bad: &mut Option<String>) -> Action {
    use cadcraft_doc::EquipmentKind;
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("equipmentedit", json!({ "handles": ids, key: value })));
    group(ui, "Equipment");
    let name = shared(&items.iter().map(|e| e.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("equip_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let kind = shared(&items.iter().map(|e| e.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let all: Vec<(&'static str, &'static str)> =
            EquipmentKind::ALL.iter().map(|k| (k.name(), super::building::equipment_kind_label(*k))).collect();
        if let Some(k) = choice(ui, kind.map_or(VARIES, super::building::equipment_kind_label), &all) {
            action = edit("kind", json!(k));
        }
    });
    // The model: choosing one gives its values, which stay the user's to change afterwards.
    let model = shared(&items.iter().map(|e| e.model.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Model", |ui| {
        let names: Vec<String> = kind
            .map(|k| cadcraft_doc::equipment_models_of(&d.available_equipment_models(), k).into_iter().map(|m| m.name).collect())
            .unwrap_or_default();
        let shown = if model.is_empty() { "—".to_string() } else { model.clone() };
        if let Some(n) = pick_menu(ui, &shown, names) {
            action = edit("model", json!(n));
        }
    });
    // The values of this equipment kept as a model of the drawing, to give them to other equipment by name.
    if let [only] = sel {
        prop_row(ui, "Save as model of the drawing", |ui| {
            if let Some(name) = edit_field(ui, ui.scope_id().with(("equip_model_save", only)), String::new()).filter(|n| !n.trim().is_empty()) {
                action = Some(("equipmentmodelsave", json!({ "handle": only.hex(), "name": name.trim() })));
            }
        });
    }
    prop_row(ui, "Reference", |ui| {
        let reference = shared(&items.iter().map(|e| e.reference.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        if let Some(r) = edit_field(ui, ui.scope_id().with(("equip_ref", &ids)), reference).filter(|r| r != VARIES) {
            action = edit("reference", json!(r));
        }
    });
    // The electrical power it draws and the circuit that feeds it.
    let electric =
        shared(&items.iter().map(|e| e.electric_kw.map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Electrical power (kW)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("equip_elec", &ids)), electric).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("electricKw", Value::Null);
            } else if let Some(n) = number(Some(t), bad) {
                action = edit("electricKw", json!(n));
            }
        }
    });
    let on = shared(&items.iter().map(|e| e.circuit.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Circuit", |ui| {
        if let Some(c) = edit_field(ui, ui.scope_id().with(("equip_circuit", &ids)), on).filter(|c| c != VARIES) {
            action = edit("circuit", json!(c.trim()));
        }
    });
    let addr = shared(&items.iter().map(|e| e.address.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Address", |ui| {
        if let Some(c) = edit_field(ui, ui.scope_id().with(("equip_address", &ids)), addr).filter(|c| c != VARIES) {
            action = edit("address", json!(c.trim()));
        }
    });
    // Numbers: empty clears a power or the weight. A VRV outdoor unit's power is in CV.
    let all_vrv = items.iter().all(|e| e.kind == EquipmentKind::VrfOutdoor);
    type Get = fn(&cadcraft_doc::EquipmentParams) -> Option<f64>;
    let numbers: [(&str, &str, Get, bool); 9] = [
        (if all_vrv { "Power (CV)" } else { "Power (kW)" }, "capacityKw", |e| e.capacity_kw, true),
        ("Heating power (kW)", "heatingKw", |e| e.heating_kw, true),
        ("Length (mm)", "length", |e| Some(e.length_mm), false),
        ("Width (mm)", "width", |e| Some(e.width_mm), false),
        ("Height (mm)", "height", |e| Some(e.height_mm), false),
        ("Weight (kg)", "weight", |e| e.weight_kg, true),
        ("Fans", "fans", |e| Some(f64::from(e.fans)), false),
        ("Service clearance (mm)", "clearance", |e| Some(e.clearance_mm), false),
        ("Refrigerant charge (kg)", "charge", |e| e.charge_kg, true),
    ];
    // The refrigerant and its charge are those of the system an outdoor unit of a refrigerant link is the head of.
    let refrigerant_system = items.iter().all(|e| matches!(e.kind, EquipmentKind::VrfOutdoor | EquipmentKind::SplitOutdoor));
    // The fans are drawn on the units that stand outdoors only: no row for the others.
    let any_outdoor = items.iter().any(|e| e.kind.is_outdoor());
    for (label, key, get, clearable) in numbers.into_iter().filter(|n| (n.1 != "fans" || any_outdoor) && (n.1 != "charge" || refrigerant_system)) {
        let v = shared(&items.iter().map(|e| get(e).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("equip_num", key, &ids)), v).filter(|t| t != VARIES) {
                if t.trim().is_empty() && clearable {
                    action = edit(key, Value::Null);
                } else if let Some(n) = number(Some(t), bad) {
                    action = edit(key, json!(n));
                }
            }
        });
    }
    if refrigerant_system {
        prop_row(ui, "Refrigerant", |ui| {
            let r = shared(&items.iter().map(|e| e.refrigerant.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
            if let Some(t) = edit_field(ui, ui.scope_id().with(("equip_refrigerant", &ids)), r).filter(|t| t != VARIES) {
                action = edit("refrigerant", json!(t));
            }
        });
    }
    // A ducted unit: the duct connections, each with its size, on the side the air comes in by (left) or goes out by (right). Typed as a list of sizes: one
    // for each connection ("315, 400x250" is two). A duct laid on a side gives the unit its connection by itself.
    if items.iter().all(|e| e.kind.takes_ducts()) {
        let size_text = |sizes: Vec<cadcraft_doc::RiserSize>| -> String {
            sizes.iter().map(|z| z.label().replace('Ø', "").replace('×', "x")).collect::<Vec<_>>().join(", ")
        };
        for (count_label, label, key, inlet) in [
            ("Number of air inlets", "Air inlets (sizes, mm)", "inlets", true),
            ("Number of air outlets", "Air outlets (sizes, mm)", "outlets", false),
        ] {
            // How many: a unit that was given none has one of each, as it is drawn.
            let n = shared(&items.iter().map(|e| if e.air_ports.is_empty() { 1 } else { e.air_sizes(inlet).len() }).collect::<Vec<_>>());
            let shown = n.map_or_else(|| VARIES.to_string(), |n| n.to_string());
            prop_row(ui, count_label, |ui| {
                if let Some(c) = edit_field(ui, ui.scope_id().with(("equip_air_count", key, &ids)), shown)
                    .filter(|t| t != VARIES)
                    .and_then(|t| number(Some(t), bad))
                {
                    // A whole number: the engine reads it as one (a float such as 3.0 is accepted too).
                    action = edit(if inlet { "inletCount" } else { "outletCount" }, json!(c.round().max(0.0) as u64));
                }
            });
            let v = shared(&items.iter().map(|e| size_text(e.air_sizes(inlet))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
            prop_row(ui, label, |ui| {
                if let Some(t) = edit_field(ui, ui.scope_id().with(("equip_air", key, &ids)), v).filter(|t| t != VARIES) {
                    action = edit(key, json!(t.trim()));
                }
            });
        }
    }
    // The connections of the other equipment: a text written beside it on the plan (a pipe size, say).
    if items.iter().all(|e| !e.kind.takes_ducts()) {
        prop_row(ui, "Connections (text on the plan)", |ui| {
            let c = shared(&items.iter().map(|e| e.connections.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
            if let Some(t) = edit_field(ui, ui.scope_id().with(("equip_conn", &ids)), c).filter(|t| t != VARIES) {
                action = edit("connections", json!(t));
            }
        });
    }
    // Whether the size is written beside the equipment.
    let size_shown = shared(&items.iter().map(|e| e.show_size).collect::<Vec<_>>());
    prop_row(ui, "Show size", |ui| {
        let mut on = size_shown == Some(true);
        if ui.checkbox(&mut on, "").changed() {
            action = edit("showSize", json!(on));
        }
    });
    altitude_row(ui, "equipmentedit", &ids, items.iter().map(|e| e.bottom_mm).collect(), bad, &mut action);
    action
}

// =====================================================================================
// Risers
// =====================================================================================

/// The Properties group of risers of cables or of a bus: name, direction, shaft and height, and the lines each holds (its use, its circuit or bus, its
/// section and its loads); lines are added and removed.
fn cable_riser_section(ui: &mut egui::Ui, d: &Drawing, sel: &[Handle], risers: &[RiserParams], bad: &mut Option<String>) -> Action {
    let ids = handles(sel);
    let mut action: Action = None;
    let edit = |key: &str, value: Value| Some(("riseredit", json!({ "handles": ids, key: value })));
    group(ui, "Cable riser");
    let name = shared(&risers.iter().map(|r| r.name.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Name", |ui| {
        if let Some(n) = edit_field(ui, ui.scope_id().with(("criser_name", &ids)), name).filter(|n| !n.trim().is_empty() && n != VARIES) {
            action = edit("name", json!(n));
        }
    });
    let up = shared(&risers.iter().map(|r| r.up).collect::<Vec<_>>());
    prop_row(ui, "Direction", |ui| {
        let shown = up.map_or(VARIES, |u| if u { "Goes up" } else { "Goes down" });
        if let Some(d) = choice(ui, shown, &[("up", "Goes up"), ("down", "Goes down")]) {
            action = edit("direction", json!(d));
        }
    });
    for (label, key, pick) in [("Shaft width (mm)", "width", 0), ("Shaft depth (mm)", "depth", 1)] {
        let v = shared(&risers.iter().map(|r| format!("{:.0}", if pick == 0 { r.shaft_mm().0 } else { r.shaft_mm().1 })).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(n) = number(edit_field(ui, ui.scope_id().with(("criser_shaft", key, &ids)), v), bad) {
                action = edit(key, json!(n));
            }
        });
    }
    let height =
        shared(&risers.iter().map(|r| r.height_mm.map_or(String::new(), |h| format!("{h}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Height, floor to floor (mm)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("criser_height", &ids)), height).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("height", Value::Null);
            } else if let Some(n) = number(Some(t), bad) {
                action = edit("height", json!(n));
            }
        }
    });
    // The lines of one riser: each is a cable (or a bus) that goes through the shaft.
    let [r] = risers else { return action };
    for (i, line) in r.cable.lines.iter().enumerate() {
        let n = i + 1;
        let line_edit = |key: &str, value: Value| Some(("riseredit", json!({ "handles": ids, "line": { "index": n, key: value } })));
        let use_label = crate::tf!("Line {n}: use", n = n);
        prop_row(ui, &use_label, |ui| {
            let shown = match line.usage {
                CableUsage::Power => "Power",
                CableUsage::Lighting => "Lighting",
                CableUsage::Data => "Data",
                CableUsage::Safety => "Safety",
                CableUsage::Bus => "Control bus",
                CableUsage::Remote => "Remote control",
            };
            let items = [
                ("power", "Power"),
                ("lighting", "Lighting"),
                ("data", "Data"),
                ("safety", "Safety"),
                ("bus", "Control bus"),
                ("remote", "Remote control"),
            ];
            if let Some(u) = choice(ui, shown, &items) {
                action = line_edit("usage", json!(u));
            }
        });
        let circuit_label = crate::tf!("Line {n}: circuit or bus", n = n);
        prop_row(ui, &circuit_label, |ui| {
            if let Some(c) = edit_field(ui, ui.scope_id().with(("criser_circuit", n, &ids)), line.circuit.clone()) {
                action = line_edit("circuit", json!(c.trim()));
            }
        });
        // The cable as it is written: "3G1.5", "2x0.75 blindé", or a section alone.
        let cable_label = crate::tf!("Line {n}: section or cable (3G1.5, 2x0.75…)", n = n);
        prop_row(ui, &cable_label, |ui| {
            let shown = line.cable_text().map(|t| t.replace(" mm²", "")).unwrap_or_default();
            if let Some(t) = edit_field(ui, ui.scope_id().with(("criser_cable", n, &ids)), shown) {
                action = line_edit("section", if t.trim().is_empty() { Value::Null } else { json!(t.trim()) });
            }
        });
        // The way of the line at this level: the riser's, or its own (up, down, or passing through going up or down).
        let way_label = crate::tf!("Line {n}: direction", n = n);
        prop_row(ui, &way_label, |ui| {
            let items = [
                ("auto", "As the riser"),
                ("up", "Goes up"),
                ("down", "Goes down"),
                ("throughUp", "Passes through, going up"),
                ("throughDown", "Passes through, going down"),
            ];
            let key = line.way.map_or("auto", cadcraft_doc::LineWay::name);
            let shown = items.iter().find(|(k, _)| *k == key).map_or("As the riser", |(_, w)| w);
            if let Some(k) = choice(ui, shown, &items) {
                action = line_edit("way", json!(k));
            }
        });
        // A colour of its own for the line, chosen as a layer's is (kept as "#rrggbb"); none: the colour of its circuit's wire.
        let colour_label = crate::tf!("Line {n}: colour", n = n);
        prop_row(ui, &colour_label, |ui| {
            let own = line.own_colour().and_then(cadcraft_doc::parse_hex_colour).map(|(r, g, b)| cadcraft_color::Rgb(r, g, b));
            let id = ui.scope_id().with(("criser_colour", n, &ids));
            let width = (ui.available_width() - 30.0).max(60.0);
            let picked = match own {
                Some(rgb) => crate::layers::color_button(ui, id, cadcraft_color::Color::True(rgb), false, width),
                None => crate::layers::color_button_resolved(
                    ui,
                    id,
                    cadcraft_color::Color::ByLayer,
                    // The colour of the layer of its circuit's cables, as the line is drawn.
                    d.layers
                        .iter()
                        .find(|l| l.name.eq_ignore_ascii_case(line.piece_layer()))
                        .map_or(cadcraft_color::Rgb(255, 255, 255), |l| l.color.resolve(cadcraft_color::Color::WHITE, cadcraft_color::Color::WHITE)),
                    "Circuit colour",
                    false,
                    width,
                ),
            };
            if let Some(c) = picked {
                let cadcraft_color::Rgb(r, g, b) = c.resolve(cadcraft_color::Color::WHITE, cadcraft_color::Color::WHITE);
                action = line_edit("colour", json!(format!("#{r:02x}{g:02x}{b:02x}")));
            }
            if own.is_some() && ui.small_button("×").on_hover_text(crate::i18n::t("Back to the circuit colour")).clicked() {
                action = line_edit("colour", Value::Null);
            }
        });
        let mut numbers: Vec<(String, &str, Option<f64>)> = Vec::new();
        if !line.usage.is_signal() {
            numbers.push((crate::tf!("Line {n}: load (kW)", n = n), "load", line.load_kw));
            numbers.push((crate::tf!("Line {n}: load from other floors (kW)", n = n), "incomingLoad", line.incoming_load_kw));
        }
        for (label, key, value) in numbers {
            let shown = value.map_or(String::new(), |f| format!("{f}"));
            prop_row(ui, &label, |ui| {
                if let Some(t) = edit_field(ui, ui.scope_id().with(("criser_num", n, key, &ids)), shown) {
                    if t.trim().is_empty() {
                        action = line_edit(key, Value::Null);
                    } else if let Some(f) = number(Some(t), bad) {
                        action = line_edit(key, json!(f));
                    }
                }
            });
        }
    }
    prop_row(ui, "Lines", |ui| {
        if let Some(usage) = pick_menu(ui, crate::i18n::t("Add a line"), CableUsage::ALL.iter().map(|u| u.name().to_string())) {
            action = edit("addLines", json!([{ "usage": usage }]));
        }
        let names: Vec<String> = r.cable.lines.iter().enumerate().map(|(i, l)| format!("{} {}", i + 1, l.text())).collect();
        let removed =
            pick_menu(ui, crate::i18n::t("Remove a line"), names).and_then(|c| c.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()));
        if let Some(n) = removed {
            action = edit("removeLine", json!(n));
        }
    });
    action
}

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
    // The section of the part that goes to or comes from the other floors (the lower end of the leader): empty, it is sized for that flow.
    let lower =
        shared(&risers.iter().map(|r| r.incoming_size.map_or(String::new(), |s| s.label().replace('Ø', "").replace('×', "x"))).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Size to other floor (mm)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("riser_incoming_size", &ids)), lower).filter(|t| t != VARIES) {
            action = edit("incomingSize", json!(if t.trim().is_empty() { Value::Null } else { json!(t.trim()) }));
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
    // The altitude of each connection, in metres, in the order of the collars, separated by commas ("2.7, 2.7, 3, "): an empty one has none.
    prop_row(ui, "Connection altitudes (m, one per collar)", |ui| {
        let text = shared(
            &units
                .iter()
                .map(|p| p.port_levels_mm.iter().map(|l| l.map_or(String::new(), |m| format!("{}", m / 1000.0))).collect::<Vec<_>>().join(", "))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|| VARIES.into());
        if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_levels", &ids)), text).filter(|t| t != VARIES) {
            let parsed: Option<Vec<Value>> = t
                .split(',')
                .map(|x| match x.trim() {
                    "" => Some(Value::Null),
                    v => v.replace(',', ".").parse::<f64>().ok().filter(|m| m.is_finite()).map(|m| json!(m)),
                })
                .collect();
            match parsed {
                Some(list) if !t.trim().is_empty() => action = edit("portLevels", Value::Array(list)),
                Some(_) => action = edit("portLevels", Value::Null),
                None => *bad = Some(crate::i18n::t("Requires altitudes in metres, separated by commas.").to_string()),
            }
        }
    });
    let kind = shared(&units.iter().map(|p| p.kind).collect::<Vec<_>>());
    prop_row(ui, "Type", |ui| {
        let label = |k: AhuKind| match k {
            AhuKind::DoubleFlow => "Double-flow unit, horizontal",
            AhuKind::SupplyUnit => "Supply unit",
            AhuKind::ExtractUnit => "Extract unit",
            AhuKind::VmcBox => "Ventilation box",
            AhuKind::StackedDoubleFlow => "Double-flow unit, vertical",
        };
        let items = [
            ("doubleFlow", "Double-flow unit, horizontal"),
            ("stackedDoubleFlow", "Double-flow unit, vertical"),
            ("supplyUnit", "Supply unit"),
            ("extractUnit", "Extract unit"),
            ("vmcBox", "Ventilation box"),
        ];
        if let Some(k) = choice(ui, kind.map_or(VARIES, label), &items) {
            action = edit("type", json!(k));
        }
    });
    // A double-flux unit lies flat (its collars side by side at each end) or stands (the two collars of an end one over the other).
    if units.iter().all(|p| matches!(p.kind, AhuKind::DoubleFlow | AhuKind::StackedDoubleFlow)) {
        let vertical = shared(&units.iter().map(|p| p.kind == AhuKind::StackedDoubleFlow).collect::<Vec<_>>());
        prop_row(ui, "Orientation", |ui| {
            let shown = vertical.map_or(VARIES, |v| if v { "Vertical" } else { "Horizontal" });
            if let Some(o) = choice(ui, shown, &[("doubleFlow", "Horizontal"), ("stackedDoubleFlow", "Vertical")]) {
                action = edit("type", json!(o));
            }
        });
    }
    let electric =
        shared(&units.iter().map(|e| e.electric_kw.map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Electrical power (kW)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_elec", &ids)), electric).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("electricKw", Value::Null);
            } else if let Some(n) = number(Some(t), bad) {
                action = edit("electricKw", json!(n));
            }
        }
    });
    let on = shared(&units.iter().map(|e| e.circuit.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Circuit", |ui| {
        if let Some(c) = edit_field(ui, ui.scope_id().with(("ahu_circuit", &ids)), on).filter(|c| c != VARIES) {
            action = edit("circuit", json!(c.trim()));
        }
    });
    let addr = shared(&units.iter().map(|e| e.address.clone()).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Address", |ui| {
        if let Some(c) = edit_field(ui, ui.scope_id().with(("ahu_address", &ids)), addr).filter(|c| c != VARIES) {
            action = edit("address", json!(c.trim()));
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
    // The coils are optional: folded away unless the unit has one. Each power (kW) is the load of the water pipes that end at the unit (an electric coil:
    // a load of its circuit); empty: no such coil.
    let has_a_coil = units.iter().any(|u| {
        let c = &u.coils;
        c.chilled_kw.is_some() || c.hot_kw.is_some() || c.change_over_kw.is_some() || c.direct_expansion_kw.is_some() || c.electric_kw.is_some()
    });
    egui::CollapsingHeader::new(crate::i18n::t("Coils (optional)")).id_salt(ui.scope_id().with(("ahu_coils", &ids))).default_open(has_a_coil).show(
        ui,
        |ui| {
            for (label, key, get) in [
                ("Chilled water coil (kW)", "chilledWater", (|u: &AhuParams| u.coils.chilled_kw) as fn(&AhuParams) -> Option<f64>),
                ("Hot water coil (kW)", "hotWater", |u| u.coils.hot_kw),
                ("Change-over coil (kW)", "changeOver", |u| u.coils.change_over_kw),
                ("Direct expansion coil (kW)", "directExpansion", |u| u.coils.direct_expansion_kw),
                ("Electric coil (kW)", "electric", |u| u.coils.electric_kw),
            ] {
                let v = shared(&units.iter().map(|u| get(u).map_or(String::new(), |f| format!("{f}"))).collect::<Vec<_>>())
                    .unwrap_or_else(|| VARIES.into());
                prop_row(ui, label, |ui| {
                    if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_coil", key, &ids)), v).filter(|t| t != VARIES) {
                        let value = if t.trim().is_empty() { Some(Value::Null) } else { number(Some(t), bad).map(|n| json!(n)) };
                        if let Some(value) = value {
                            action = Some(("ahuedit", json!({ "handles": ids, "coils": { key: value } })));
                        }
                    }
                });
            }
            // An electric coil is single-phase or three-phase: the circuit that feeds it is checked against it.
            let phases = shared(&units.iter().map(|u| u.coils.electric_single_phase).collect::<Vec<_>>());
            prop_row(ui, "Electric coil phases", |ui| {
                let shown = phases.map_or(VARIES, |single| if single { "Single phase" } else { "Three phase" });
                if let Some(p) = choice(ui, shown, &[("single", "Single phase"), ("three", "Three phase")]) {
                    action = Some(("ahuedit", json!({ "handles": ids, "coils": { "electricPhases": p } })));
                }
            });
        },
    );
    if let [u] = units {
        // The flow of each direction the unit has: typed here it is kept by sizing (empty gives it back); the pressure is read-only.
        let roles = u.roles();
        let ids_u = ids.clone();
        for (label, key, air, f, pa) in [
            ("Supply flow (m³/h)", "flow", AirDirection::Supply, u.flow_m3h, u.pressure_pa),
            ("Extract flow (m³/h)", "extractFlow", AirDirection::Extract, u.extract_flow_m3h, u.extract_pressure_pa),
            ("Fresh air flow (m³/h)", "freshFlow", AirDirection::Fresh, u.fresh_flow_m3h, u.fresh_pressure_pa),
            ("Exhaust air flow (m³/h)", "exhaustFlow", AirDirection::Exhaust, u.exhaust_flow_m3h, u.exhaust_pressure_pa),
        ] {
            if !roles.contains(&air) && f.is_none() {
                continue;
            }
            let typed = u.typed_flows.contains(&air);
            let shown = f.map_or(String::new(), |f| format!("{f:.0}"));
            prop_row(ui, label, |ui| {
                if let Some(t) = edit_field(ui, ui.scope_id().with(("ahu_flow", key, &ids_u)), shown).filter(|t| t != VARIES) {
                    if t.trim().is_empty() {
                        action = Some(("ahuedit", json!({ "handles": ids_u, key: "auto" })));
                    } else if let Some(v) = number(Some(t), bad) {
                        action = Some(("ahuedit", json!({ "handles": ids_u, key: v })));
                    }
                }
            });
            if typed {
                prop_row(ui, "Flow set by", |ui| read_only(ui, crate::i18n::t("User")));
            }
            if let Some(p) = pa {
                prop_row(ui, "Pressure (Pa)", |ui| read_only(ui, &format!("{p:.0} Pa")));
            }
        }
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
                prop_row(ui, &format!("{} ({})", crate::i18n::t(name), crate::i18n::t("size W×H")), |ui| {
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
    altitude_row(ui, "ahuedit", &ids, units.iter().map(|u| u.bottom_mm).collect(), bad, &mut action);
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
    // Unticked: the runs are drawn without their label (ticked only when all of them show it).
    let shown = ds.iter().all(|p| !p.hide_label);
    prop_row(ui, "Show the label", |ui| {
        let mut on = shown;
        if ui.checkbox(&mut on, "").changed() {
            action = edit("label", json!(on));
        }
    });
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
    // The section is chosen here for every duct selected: round, or rectangular (its sides start from the diameter), as sizing would size it.
    let shape = shared(&ds.iter().map(|p| p.shape).collect::<Vec<_>>());
    prop_row(ui, "Section", |ui| {
        let items = [("round", "Round"), ("rectangular", "Rectangular")];
        let shown = match (shape, ds) {
            (Some(s), [p]) => format!("{} ({})", crate::i18n::t(if s == DuctShape::Round { "Round" } else { "Rectangular" }), p.size_label()),
            (Some(s), _) => crate::i18n::t(if s == DuctShape::Round { "Round" } else { "Rectangular" }).to_string(),
            (None, _) => crate::i18n::t(VARIES).to_string(),
        };
        if let Some(c) = choice_text(ui, shown, &items) {
            action = edit("shape", json!(c));
        }
    });
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
    // A side typed changes that side of every duct selected (the other is the duct's own, or its diameter when it is round); an empty field left as
    // it was changes nothing.
    prop_row(ui, "Width (mm)", |ui| {
        if let Some(w) = number(edit_field(ui, ui.scope_id().with(("duct_width", &ids)), width).filter(|t| !t.trim().is_empty() && t != VARIES), bad)
        {
            action = edit("width", json!(w));
        }
    });
    prop_row(ui, "Height (mm)", |ui| {
        if let Some(h) =
            number(edit_field(ui, ui.scope_id().with(("duct_height", &ids)), height).filter(|t| !t.trim().is_empty() && t != VARIES), bad)
        {
            action = edit("height", json!(h));
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
    // The insulation: none, round the duct (outside) or lining it (inside), and how thick.
    let insulation = shared(&ds.iter().map(|p| p.insulation.map(|i| i.side)).collect::<Vec<_>>());
    prop_row(ui, "Insulation", |ui| {
        let shown = match insulation {
            None => VARIES,
            Some(None) => "None",
            Some(Some(cadcraft_doc::InsulationSide::Outside)) => "Outside",
            Some(Some(cadcraft_doc::InsulationSide::Inside)) => "Inside",
        };
        if let Some(c) = choice(ui, shown, &[("none", "None"), ("outside", "Outside"), ("inside", "Inside")]) {
            action = edit("insulation", json!(c));
        }
    });
    if ds.iter().all(|p| p.insulation.is_some()) {
        let thickness = shared(&ds.iter().map(|p| p.insulation.map(|i| format!("{}", i.thickness_mm)).unwrap_or_default()).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Insulation thickness (mm)", |ui| {
            if let Some(mm) = number(edit_field(ui, ui.scope_id().with(("duct_insulation", &ids)), thickness), bad) {
                action = edit("insulationThickness", json!(mm));
            }
        });
    }
    // The ends cut on the slant (whistles), degrees: 0 or empty is square.
    for (label, key, id, at_end) in
        [("Start cut (degrees)", "cutStart", "duct_cut_start", false), ("End cut (degrees)", "cutEnd", "duct_cut_end", true)]
    {
        let cut = shared(&ds.iter().map(|p| format!("{}", p.cut_deg[usize::from(at_end)])).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, label, |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with((id, &ids)), cut).filter(|t| t != VARIES) {
                if t.trim().is_empty() {
                    action = edit(key, Value::Null);
                } else if let Some(deg) = number(Some(t), bad) {
                    action = edit(key, json!(deg));
                }
            }
        });
    }
    // A reducer whose cone was shaped by its grips: this gives back the symmetric 15 degree one.
    if ds.iter().any(|p| p.reducer.is_some() && (p.reducer_shape.is_some() || p.reducer_len_mm.is_some())) {
        prop_row(ui, "Reducer cone", |ui| {
            if ui.button(crate::i18n::t("Back to automatic")).clicked() {
                action = edit("coneAuto", json!(true));
            }
        });
    }
    // The altitude of the underside, metres: empty gives it back to the height zone. Where two joined runs
    // differ, sizing makes the step between them and, where two cross, the lower passes under.
    let altitude = shared(&ds.iter().map(|p| p.level_mm.map(|b| format!("{}", b / 1000.0)).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Altitude of the underside (m)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("duct_altitude", &ids)), altitude).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("altitude", Value::Null);
            } else if let Some(m) = number(Some(t), bad) {
                action = edit("altitude", json!(m));
            }
        }
    });
    // A sloping run: the altitude of its far end, metres (empty: level). Its true length, which loses the pressure, is longer than its length in plan.
    let end = shared(&ds.iter().map(|p| p.end_level_mm.map(|b| format!("{}", b / 1000.0)).unwrap_or_default()).collect::<Vec<_>>())
        .unwrap_or_else(|| VARIES.into());
    prop_row(ui, "Altitude of the far end (m)", |ui| {
        if let Some(t) = edit_field(ui, ui.scope_id().with(("duct_end_altitude", &ids)), end).filter(|t| t != VARIES) {
            if t.trim().is_empty() {
                action = edit("endAltitude", Value::Null);
            } else if let Some(m) = number(Some(t), bad) {
                action = edit("endAltitude", json!(m));
            }
        }
    });
    if let [p] = ds {
        prop_row(ui, "Length", |ui| read_only(ui, &len(p.length)));
        if p.rise_mm().abs() > 0.5 {
            prop_row(ui, "True length (m)", |ui| read_only(ui, &format!("{:.2}", p.length_m())));
        }
        // The flow the run carries: typed here, sizing keeps it and sizes the run for it; empty gives it back to the terminals.
        let flow = p.flow_m3h.map_or(String::new(), |f| format!("{f:.0}"));
        prop_row(ui, "Flow (m³/h)", |ui| {
            if let Some(t) = edit_field(ui, ui.scope_id().with(("duct_flow", &ids)), flow).filter(|t| t != VARIES) {
                if t.trim().is_empty() {
                    action = edit("flow", json!("auto"));
                } else if let Some(v) = number(Some(t), bad) {
                    action = edit("flow", json!(v));
                }
            }
        });
        if p.flow_set {
            prop_row(ui, "Flow set by", |ui| read_only(ui, crate::i18n::t("User")));
        }
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
            AccessoryKind::Hood => "Covered outlet",
        };
        let items = [
            ("damper", "Damper"),
            ("fireDamper", "Fire damper"),
            ("silencer", "Silencer"),
            ("flexConnector", "Flexible connector"),
            ("hood", "Covered outlet"),
        ];
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
    if accs.iter().all(|a| a.kind == AccessoryKind::Hood) {
        // A hood's opening and its air (which sets its colour).
        let width = shared(&accs.iter().map(|a| format!("{:.0}", a.diameter_mm)).collect::<Vec<_>>()).unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Width (mm)", |ui| {
            if let Some(w) = number(edit_field(ui, ui.scope_id().with(("acc_width", &ids)), width), bad) {
                action = Some(("ductaccessoryedit", json!({ "handles": ids, "width": w })));
            }
        });
        let height = shared(&accs.iter().map(|a| a.height_mm.map_or_else(String::new, |h| format!("{h:.0}"))).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Height (mm), empty if round", |ui| {
            if let Some(h) = number(edit_field(ui, ui.scope_id().with(("acc_height", &ids)), height), bad) {
                action = Some(("ductaccessoryedit", json!({ "handles": ids, "height": h })));
            }
        });
        let over = shared(&accs.iter().map(|a| a.overhang_mm.map_or_else(|| "0".to_string(), |o| format!("{o:.0}"))).collect::<Vec<_>>())
            .unwrap_or_else(|| VARIES.into());
        prop_row(ui, "Hood overhang (mm)", |ui| {
            if let Some(o) = number(edit_field(ui, ui.scope_id().with(("acc_overhang", &ids)), over), bad) {
                action = Some(("ductaccessoryedit", json!({ "handles": ids, "overhang": o })));
            }
        });
        let follows = shared(&accs.iter().map(|a| !a.fixed).collect::<Vec<_>>());
        prop_row(ui, "Follows its opening", |ui| {
            let shown = follows.map_or(VARIES, |f| if f { "Yes" } else { "No" });
            if let Some(k) = choice(ui, shown, &[("yes", "Yes"), ("no", "No")]) {
                action = Some(("ductaccessoryedit", json!({ "handles": ids, "follow": k == "yes" })));
            }
        });
        let air = shared(&accs.iter().map(|a| a.air).collect::<Vec<_>>());
        prop_row(ui, "Air", |ui| {
            let shown = air.map_or(VARIES, |a| match a {
                None => "Not set",
                Some(AirDirection::Supply) => "Supply",
                Some(AirDirection::Extract) => "Extract",
                Some(AirDirection::Fresh) => "Fresh air",
                Some(AirDirection::Exhaust) => "Exhaust air",
            });
            let items = [("none", "Not set"), ("supply", "Supply"), ("extract", "Extract"), ("fresh", "Fresh air"), ("exhaust", "Exhaust air")];
            if let Some(k) = choice(ui, shown, &items) {
                let v = if k == "none" { Value::Null } else { json!(k) };
                action = Some(("ductaccessoryedit", json!({ "handles": ids, "air": v })));
            }
        });
    } else if let [a] = accs {
        prop_row(ui, "Duct", |ui| read_only(ui, &format!("Ø {:.0} mm", a.diameter_mm)));
    }
    altitude_row(ui, "ductaccessoryedit", &ids, accs.iter().map(|a| a.bottom_mm).collect(), bad, &mut action);
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
    fn a_selected_pipe_shows_its_size_and_a_typed_one_is_imposed() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("pipeadd", json!({ "points": [[0, 0], [3000, 0]], "service": "hotWater" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Hot water").is_some(), "the service is shown");
        type_into(&mut hn, "40", "65");
        let p = cadcraft_doc::pipe_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.dn_mm, 65.0);
        assert!(p.imposed, "a typed size is the user's: sizing leaves it");
        assert!(super::is_parametric(hn.state().session.doc().unwrap(), h), "a double-click opens Properties");
    }

    #[test]
    fn a_selected_refrigerant_link_shows_its_tubes_in_inches_and_takes_inches_typed() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("pipeadd", json!({ "points": [[0, 0], [3000, 0]], "service": "refrigerantPair" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Refrigerant link (liquid + gas)").is_some(), "the service is shown");
        type_into(&mut hn, "5/8", "3/4");
        let p = cadcraft_doc::pipe_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((p.dn_mm, p.gas_dn_mm), (9.52, 19.05));
    }

    #[test]
    fn a_selected_pipework_part_takes_its_values_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("pipepartadd", json!({ "at": [0, 0], "model": "Expansion vessel 24 l" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        assert!(super::is_parametric(app.session.doc().unwrap(), h), "a double-click opens Properties");
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        type_into(&mut hn, "24", "35");
        let p = cadcraft_doc::part_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.volume_l, Some(35.0));
    }

    #[test]
    fn a_selected_box_takes_its_outlets_and_dimensions_and_another_model_from_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("pipepartadd", json!({ "at": [0, 0], "model": "Branch box, 4 outlets (R32 heat recovery)" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let before = cadcraft_doc::part_of(app.session.doc().unwrap().entity(h).unwrap()).unwrap();
        let mut hn = palette(app);
        // The outlets typed: the box is redrawn with them, and as long as they need.
        type_into(&mut hn, "4", "6");
        let p = cadcraft_doc::part_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.outlets, 6);
        assert!(p.length_mm > before.length_mm);
        // The height, the width and the depth are fields to type in, in that order.
        assert!(
            hn.query_by_label("Height H (mm)").is_some()
                && hn.query_by_label("Width W (mm)").is_some()
                && hn.query_by_label("Depth D (mm)").is_some()
        );
        let height = before.height_mm.map(|v| format!("{v}")).unwrap_or_default();
        type_into(&mut hn, &height, "420");
        let p = cadcraft_doc::part_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.height_mm, Some(420.0));
    }

    #[test]
    fn a_selected_pipe_riser_shows_its_lines_and_takes_their_sizes_and_loads_typed() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("piperiseradd", json!({ "points": [[0, 0], [800, 0], [800, 500], [0, 500]], "lines": ["CHW-S", "REF"] })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        assert!(super::is_parametric(app.session.doc().unwrap(), h), "a double-click opens Properties");
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        // The size of the chilled-water line is typed (the user's: sizing leaves it), and what it carries.
        type_into(&mut hn, "50", "80");
        let p = cadcraft_doc::pipe_riser_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((p.lines[0].dn_mm, p.lines[0].imposed), (80.0, true));
        // The liquid tube of the link is typed in inches.
        type_into(&mut hn, "3/8", "1/2");
        let p = cadcraft_doc::pipe_riser_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.lines[1].dn_mm, 12.7);
    }

    #[test]
    fn a_name_typed_on_a_pipe_in_the_palette_is_the_name_of_its_whole_network_and_its_colour_is_chosen_there() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        app.run("equipmentadd", json!({ "at": [0, 0], "kind": "chiller", "capacityKw": 50 })).unwrap();
        app.run("equipmentadd", json!({ "at": [6000, 0], "kind": "cassette", "capacityKw": 5, "length": 800, "width": 600, "height": 300 })).unwrap();
        let a = app.run("pipeadd", json!({ "points": [[0, 0], [5800, 0]], "service": "chilledWater" })).unwrap();
        let b = app.run("pipeadd", json!({ "points": [[5800, 200], [0, 200]], "service": "chilledWater", "leg": "return" })).unwrap();
        let (ha, hb) = (a["handles"][0].as_str().unwrap().to_string(), b["handles"][0].as_str().unwrap().to_string());
        let handle = |t: &str| cadcraft_doc::Handle::parse_hex(t).unwrap();
        app.run("pipenetwork", json!({})).unwrap();
        app.session.set_selection(vec![handle(&ha)]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Network colour").is_some() && hn.query_by_label("Network line type").is_some(), "a named network shows its style");
        type_into(&mut hn, "CHW1", "Toiture");
        let d = hn.state().session.doc().unwrap();
        let net = |h: &str| cadcraft_doc::pipe_of(d.entity(handle(h)).unwrap()).unwrap().network;
        assert_eq!((net(&ha), net(&hb)), ("Toiture".to_string(), "Toiture".to_string()), "the return is on the network too");
        assert!(d.network_style("toiture").is_some());
    }

    #[test]
    fn a_selected_column_shows_its_section_and_takes_a_typed_height_and_section() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("columnadd", json!({ "at": [0, 0], "section": "HEA 200", "height": 3000, "elevation": 0.5 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        for label in ["Section", "Height (mm)", "Elevation of the foot (m)", "Top (m)", "Mass (kg)"] {
            assert!(hn.query_by_label(label).is_some(), "{label} is shown");
        }
        type_into(&mut hn, "3000", "3600");
        type_into(&mut hn, "HEA 200", "IPN 300");
        let p = cadcraft_doc::struct_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((p.height_mm, p.section.as_str(), p.elevation_mm), (3600.0, "IPN 300", 500.0));
        assert!(super::is_parametric(hn.state().session.doc().unwrap(), h));
        // A section that is not in the table is refused and leaves the column as it is.
        type_into(&mut hn, "IPN 300", "XYZ 1");
        let p = cadcraft_doc::struct_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.section, "IPN 300");
    }

    #[test]
    fn a_selected_stair_and_ceiling_show_what_they_count_and_take_a_typed_value() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("stairadd", json!({ "at": [0, 0], "rise": 3000 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        for label in ["Rise (mm)", "Tread (mm)", "Risers", "Comfort (2 riser + tread)"] {
            assert!(hn.query_by_label(label).is_some(), "{label} is shown");
        }
        type_into(&mut hn, "3000", "3400");
        let st = cadcraft_doc::stair_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((st.rise_mm, st.risers()), (3400.0, 19));
        // A ceiling grid: its tile typed, and what it counts.
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let c = app.run("ceilingadd", json!({ "points": [[0, 0], [3000, 0], [3000, 2400], [0, 2400]] })).unwrap();
        let hc = cadcraft_doc::Handle::parse_hex(c["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![hc]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Tiles").is_some() && hn.query_by_label("Tile (mm)").is_some());
        type_into(&mut hn, "600x600", "600x1200");
        let cp = cadcraft_doc::ceiling_of(hn.state().session.doc().unwrap().entity(hc).unwrap()).unwrap();
        assert_eq!((cp.tile_x_mm, cp.tile_y_mm), (600.0, 1200.0));
    }

    #[test]
    fn a_selected_slab_takes_a_typed_thickness() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("slabadd", json!({ "points": [[0, 0], [4000, 0], [4000, 3000], [0, 3000]] })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Floor slab").is_some(), "the kind is shown");
        type_into(&mut hn, "200", "300");
        let p = cadcraft_doc::slab_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.thickness_mm, 300.0);
        assert!(super::is_parametric(hn.state().session.doc().unwrap(), h));
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
    fn a_selected_riser_of_cables_shows_its_circuit_and_takes_the_load_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app
            .run("elecriser", json!({ "at": [0, 0], "width": 400, "depth": 400, "name": "CP 7", "circuit": "TD1-C4", "incomingLoad": 4 }))
            .unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        assert!(super::is_parametric(app.session.doc().unwrap(), h), "a double-click opens Properties");
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.get_all_by_value("CP 7").next().is_some() && hn.get_all_by_value("TD1-C4").next().is_some());
        assert!(hn.query_all_by_label_contains("section or cable").count() >= 1, "the rows of a cable");
        assert_eq!(hn.query_all_by_label_contains("Air").count(), 0, "not the rows of air");
        type_into(&mut hn, "4", "9");
        let p = cadcraft_doc::any_riser_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.cable.lines.first().and_then(|l| l.incoming_load_kw), Some(9.0));
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

    #[test]
    fn a_selected_air_handling_unit_takes_the_power_of_its_coils_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("ahuadd", json!({ "at": [0, 0], "coils": { "chilledWater": 45 } })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        type_into(&mut hn, "45", "60");
        let u = cadcraft_doc::ahu_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((u.coils.chilled_kw, u.coils.hot_kw), (Some(60.0), None));
    }

    #[test]
    fn a_selected_equipment_shows_its_model_and_takes_the_sizes_and_power_typed_in_the_palette() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("equipmentadd", json!({ "at": [0, 0], "model": "Chiller 93.4 kW", "name": "GE1" })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handle"].as_str().unwrap()).unwrap();
        assert!(super::is_parametric(app.session.doc().unwrap(), h), "a double-click opens Properties");
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        assert!(hn.query_by_label("Chiller 93.4 kW").is_some(), "the model is shown");
        type_into(&mut hn, "2275", "3000");
        type_into(&mut hn, "93.4", "100");
        let e = cadcraft_doc::equipment_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!((e.length_mm, e.capacity_kw, e.width_mm), (3000.0, Some(100.0), 2125.0));
    }

    #[test]
    fn a_duct_takes_the_altitude_of_its_underside_typed_in_the_palette_and_gives_it_back_to_its_zone() {
        let mut app = CadApp::new(Session::new(), Services::default());
        *app.session.doc_mut().unwrap() = cadcraft_doc::Drawing::new_metric();
        let r = app.run("ductadd", json!({ "points": [[0, 0], [3000, 0]], "diameter": 200, "altitude": 2.5 })).unwrap();
        let h = cadcraft_doc::Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
        app.session.set_selection(vec![h]);
        let mut hn = palette(app);
        type_into(&mut hn, "2.5", "2.8");
        let p = cadcraft_doc::duct_of(hn.state().session.doc().unwrap().entity(h).unwrap()).unwrap();
        assert_eq!(p.level_mm, Some(2800.0));
    }
}
