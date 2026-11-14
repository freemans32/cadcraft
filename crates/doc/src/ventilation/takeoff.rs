//! The quantities of one level: each drawing of a building is a floor, so the quantities of a drawing
//! are those of its level. Ducts by direction, section and altitude (lengths and surfaces), fittings
//! (elbows, tees, reducers), accessories, terminals, units, risers, equipment, and by space the
//! terminals it holds and the duct length inside it.
//!
//! Counted from what is drawn; the sizes are the ones sizing wrote, so size the ducts first.

use cadcraft_geom::Vec2;
use cadcraft_hvac::Fitting;
use serde::{Deserialize, Serialize};

use super::{AirDirection, DuctShape, accessory_of, ahu_of, air_terminal_of, equipment_of, riser_of};
use crate::{EntityKind, Result};

/// Most points sampled along one duct to find the length of it inside a space.
const SAMPLES: usize = 400;

/// The ducts of one direction, one section and one altitude.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuctQuantity {
    pub air: AirDirection,
    pub flexible: bool,
    /// "Ø200" or "400×250".
    pub section: String,
    /// The altitude of the underside, metres, to the centimetre, when known.
    pub altitude_m: Option<f64>,
    pub count: usize,
    pub length_m: f64,
    /// The outer surface, m²: the perimeter times the length.
    pub area_m2: f64,
}

/// Something counted: a fitting, an accessory, equipment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountQuantity {
    pub name: String,
    pub size: String,
    pub count: usize,
    /// The sum of the powers (kW, CV for a VRV unit) of equipment; none for the rest.
    pub power: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalQuantity {
    pub model: String,
    pub air: AirDirection,
    pub count: usize,
    pub flow_m3h: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitQuantity {
    pub name: String,
    pub kind: String,
    pub supply_m3h: Option<f64>,
    pub extract_m3h: Option<f64>,
    pub fresh_m3h: Option<f64>,
    pub exhaust_m3h: Option<f64>,
    pub width_mm: f64,
    pub depth_mm: f64,
    pub height_mm: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiserQuantity {
    pub name: String,
    pub air: AirDirection,
    pub up: bool,
    pub size: String,
    pub flow_m3h: Option<f64>,
    pub incoming_m3h: Option<f64>,
    /// The vertical run, m, when the riser has a height.
    pub length_m: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceQuantity {
    pub name: String,
    pub area_m2: f64,
    /// The fresh air the space asks, m³/h.
    pub needed_m3h: Option<f64>,
    pub supply_terminals: usize,
    pub supply_m3h: f64,
    pub extract_terminals: usize,
    pub extract_m3h: f64,
    /// The length of duct inside the space, metres, to a few centimetres.
    pub duct_m: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub duct_count: usize,
    pub duct_length_m: f64,
    pub duct_area_m2: f64,
    pub terminals: usize,
    pub equipment: usize,
    /// Vertical length of the risers, m.
    pub riser_length_m: f64,
}

/// The quantities of one level.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Takeoff {
    pub level: String,
    pub ducts: Vec<DuctQuantity>,
    pub fittings: Vec<CountQuantity>,
    pub accessories: Vec<CountQuantity>,
    pub terminals: Vec<TerminalQuantity>,
    pub units: Vec<UnitQuantity>,
    pub risers: Vec<RiserQuantity>,
    pub equipment: Vec<CountQuantity>,
    pub spaces: Vec<SpaceQuantity>,
    pub totals: Totals,
}

fn add_count(list: &mut Vec<CountQuantity>, name: &str, size: &str, power: Option<f64>) {
    match list.iter_mut().find(|c| c.name == name && c.size == size) {
        Some(c) => {
            c.count += 1;
            c.power = match (c.power, power) {
                (Some(a), Some(b)) => Some(a + b),
                (a, b) => a.or(b),
            };
        }
        None => list.push(CountQuantity { name: name.into(), size: size.into(), count: 1, power }),
    }
}

fn air_rank(a: AirDirection) -> usize {
    AirDirection::ALL.iter().position(|x| *x == a).unwrap_or(0)
}

impl crate::Drawing {
    /// The quantities of the HVAC objects in model space, for the level named `level`.
    pub fn takeoff(&self, level: &str) -> Result<Takeoff> {
        let mut t = Takeoff { level: level.to_string(), ..Takeoff::default() };
        // Ducts, by direction, material, section and altitude.
        let mut ducts = Vec::new();
        for e in self.model.iter() {
            let Some((p, ..)) = self.duct_ends(e.handle) else { continue };
            let length_m = p.length_m();
            let perimeter_m = match p.shape {
                DuctShape::Round => std::f64::consts::PI * p.diameter_mm / 1000.0,
                DuctShape::Rectangular => 2.0 * (p.width_mm + p.height_mm) / 1000.0,
            };
            ducts.push((p.air, p.flexible, p.size_label(), p.altitude_mm().map(|mm| (mm / 10.0).round() / 100.0), length_m, perimeter_m * length_m));
            if let Some(r) = &p.reducer {
                add_count(&mut t.fittings, "Reducer", &format!("{:.0} → {}", r.from_mm, p.size_label()), None);
            }
        }
        for (air, flexible, section, altitude_m, length_m, area_m2) in ducts {
            match t.ducts.iter_mut().find(|d| d.air == air && d.flexible == flexible && d.section == section && d.altitude_m == altitude_m) {
                Some(d) => {
                    d.count += 1;
                    d.length_m += length_m;
                    d.area_m2 += area_m2;
                }
                None => t.ducts.push(DuctQuantity { air, flexible, section, altitude_m, count: 1, length_m, area_m2 }),
            }
        }
        t.ducts.sort_by(|a, b| {
            air_rank(a.air)
                .cmp(&air_rank(b.air))
                .then(a.flexible.cmp(&b.flexible))
                .then(a.section.cmp(&b.section))
                .then(a.altitude_m.partial_cmp(&b.altitude_m).unwrap_or(std::cmp::Ordering::Equal))
        });
        // Elbows and tees: those of each network as it is read, with the section of the run that carries them.
        for net in self.duct_networks()? {
            for (i, seg) in net.network.segments.iter().enumerate() {
                let Some(h) = net.ducts.get(i) else { continue };
                let Some((p, ..)) = self.duct_ends(*h) else { continue };
                for f in &seg.fittings {
                    let name = match f {
                        Fitting::Elbow90 => "Elbow 90°",
                        Fitting::Elbow45 => "Elbow 45°",
                        Fitting::TeeStraight => "Tee, straight",
                        Fitting::TeeBranch => "Tee, branch",
                        _ => continue,
                    };
                    add_count(&mut t.fittings, name, &p.size_label(), None);
                }
            }
        }
        t.fittings.sort_by(|a, b| a.name.cmp(&b.name).then(a.size.cmp(&b.size)));
        // The rest, object by object.
        let mut terminals: Vec<(Vec2, AirDirection, f64)> = Vec::new();
        for e in self.model.iter() {
            let EntityKind::Insert(i) = &e.kind else { continue };
            if let Some(a) = accessory_of(e) {
                add_count(&mut t.accessories, a.kind.name(), &format!("Ø{:.0}", a.diameter_mm), None);
            } else if let Some(x) = air_terminal_of(e) {
                match t.terminals.iter_mut().find(|q| q.model == x.model.name && q.air == x.air) {
                    Some(q) => {
                        q.count += 1;
                        q.flow_m3h += x.flow_m3h;
                    }
                    None => t.terminals.push(TerminalQuantity { model: x.model.name.clone(), air: x.air, count: 1, flow_m3h: x.flow_m3h }),
                }
                terminals.push((Vec2::new(i.insert.x, i.insert.y), x.air, x.flow_m3h));
            } else if let Some(u) = ahu_of(e) {
                t.units.push(UnitQuantity {
                    name: u.name.clone(),
                    kind: u.kind.code().into(),
                    supply_m3h: u.flow_m3h,
                    extract_m3h: u.extract_flow_m3h,
                    fresh_m3h: u.fresh_flow_m3h,
                    exhaust_m3h: u.exhaust_flow_m3h,
                    width_mm: u.width_mm,
                    depth_mm: u.depth_mm,
                    height_mm: u.height_mm,
                });
            } else if let Some(r) = riser_of(e) {
                t.risers.push(RiserQuantity {
                    name: r.name.clone(),
                    air: r.air,
                    up: r.up,
                    size: r.size.map_or(String::new(), |s| s.label()),
                    flow_m3h: r.flow_m3h,
                    incoming_m3h: r.incoming_flow_m3h,
                    length_m: r.height_mm.map(|h| h / 1000.0),
                });
            } else if let Some(q) = equipment_of(e) {
                let name = if q.model.is_empty() { q.name.clone() } else { q.model.clone() };
                add_count(&mut t.equipment, &name, &format!("{:.0}x{:.0}x{:.0}", q.length_mm, q.width_mm, q.height_mm), q.capacity_kw);
            }
        }
        t.accessories.sort_by(|a, b| a.name.cmp(&b.name).then(a.size.cmp(&b.size)));
        t.equipment.sort_by(|a, b| a.name.cmp(&b.name));
        t.terminals.sort_by(|a, b| air_rank(a.air).cmp(&air_rank(b.air)).then(a.model.cmp(&b.model)));
        // Spaces: the terminals each holds, and the length of duct inside it (sampled along each duct).
        let axes: Vec<(Vec2, Vec2, f64)> =
            self.model.iter().filter_map(|e| self.duct_ends(e.handle)).map(|(p, a, b)| (a, b, p.units.mm_per_unit)).collect();
        for e in self.model.iter() {
            let Some((sp, outline)) = self.space_outline(e.handle) else { continue };
            let mut q = SpaceQuantity { name: sp.name.clone(), area_m2: sp.area_m2(), needed_m3h: sp.flow().ok(), ..SpaceQuantity::default() };
            for (at, air, flow) in &terminals {
                if cadcraft_geom::point_in_polygon(&outline, *at) {
                    match air {
                        AirDirection::Supply => (q.supply_terminals, q.supply_m3h) = (q.supply_terminals + 1, q.supply_m3h + flow),
                        AirDirection::Extract => (q.extract_terminals, q.extract_m3h) = (q.extract_terminals + 1, q.extract_m3h + flow),
                        _ => {}
                    }
                }
            }
            for (a, b, mm) in &axes {
                let length = a.dist(*b);
                let n = SAMPLES;
                let inside = (0..n).filter(|k| cadcraft_geom::point_in_polygon(&outline, *a + (*b - *a) * ((*k as f64 + 0.5) / n as f64))).count();
                q.duct_m += length * mm / 1000.0 * inside as f64 / n as f64;
            }
            q.duct_m = (q.duct_m * 100.0).round() / 100.0;
            t.spaces.push(q);
        }
        t.spaces.sort_by(|a, b| a.name.cmp(&b.name));
        t.units.sort_by(|a, b| a.name.cmp(&b.name));
        t.risers.sort_by(|a, b| a.name.cmp(&b.name));
        // Totals, and lengths to the centimetre.
        for d in &mut t.ducts {
            d.length_m = (d.length_m * 100.0).round() / 100.0;
            d.area_m2 = (d.area_m2 * 100.0).round() / 100.0;
        }
        t.totals = Totals {
            duct_count: t.ducts.iter().map(|d| d.count).sum(),
            duct_length_m: (t.ducts.iter().map(|d| d.length_m).sum::<f64>() * 100.0).round() / 100.0,
            duct_area_m2: (t.ducts.iter().map(|d| d.area_m2).sum::<f64>() * 100.0).round() / 100.0,
            terminals: t.terminals.iter().map(|q| q.count).sum(),
            equipment: t.equipment.iter().map(|c| c.count).sum(),
            riser_length_m: (t.risers.iter().filter_map(|r| r.length_m).sum::<f64>() * 100.0).round() / 100.0,
        };
        Ok(t)
    }
}

impl Takeoff {
    /// The quantities as a spreadsheet, `;` between cells (and a comma for the decimal point when
    /// `decimal_comma` is set, as a French spreadsheet reads it), one block per kind of object.
    pub fn to_csv(&self, decimal_comma: bool) -> String {
        let num = |v: f64| {
            let t = format!("{v}");
            if decimal_comma { t.replace('.', ",") } else { t }
        };
        let opt = |v: Option<f64>| v.map_or(String::new(), num);
        let air = |a: AirDirection| a.name().to_string();
        let cell = |t: &str| {
            if t.contains([';', '"', '\n']) { format!("\"{}\"", t.replace('"', "\"\"")) } else { t.to_string() }
        };
        let mut out = String::new();
        let mut row = |cols: &[String]| {
            out.push_str(&cols.iter().map(|c| cell(c)).collect::<Vec<_>>().join(";"));
            out.push('\n');
        };
        row(&["Level".into(), self.level.clone()]);
        row(&[]);
        row(&["Ducts".into()]);
        row(&["Air".into(), "Material".into(), "Section".into(), "Altitude (m)".into(), "Count".into(), "Length (m)".into(), "Area (m2)".into()]);
        for d in &self.ducts {
            row(&[
                air(d.air),
                (if d.flexible { "flexible" } else { "rigid" }).into(),
                d.section.clone(),
                opt(d.altitude_m),
                d.count.to_string(),
                num(d.length_m),
                num(d.area_m2),
            ]);
        }
        row(&[
            "Total".into(),
            String::new(),
            String::new(),
            String::new(),
            self.totals.duct_count.to_string(),
            num(self.totals.duct_length_m),
            num(self.totals.duct_area_m2),
        ]);
        for (title, list) in [("Fittings", &self.fittings), ("Accessories", &self.accessories), ("Equipment", &self.equipment)] {
            row(&[]);
            row(&[title.into()]);
            row(&["Name".into(), "Size".into(), "Count".into(), "Power".into()]);
            for c in list {
                row(&[c.name.clone(), c.size.clone(), c.count.to_string(), opt(c.power)]);
            }
        }
        row(&[]);
        row(&["Terminals".into()]);
        row(&["Model".into(), "Air".into(), "Count".into(), "Flow (m3/h)".into()]);
        for q in &self.terminals {
            row(&[q.model.clone(), air(q.air), q.count.to_string(), num(q.flow_m3h)]);
        }
        row(&[]);
        row(&["Units".into()]);
        row(&[
            "Name".into(),
            "Kind".into(),
            "Supply".into(),
            "Extract".into(),
            "Fresh air".into(),
            "Exhaust".into(),
            "Width (mm)".into(),
            "Depth (mm)".into(),
            "Height (mm)".into(),
        ]);
        for u in &self.units {
            row(&[
                u.name.clone(),
                u.kind.clone(),
                opt(u.supply_m3h),
                opt(u.extract_m3h),
                opt(u.fresh_m3h),
                opt(u.exhaust_m3h),
                num(u.width_mm),
                num(u.depth_mm),
                opt(u.height_mm),
            ]);
        }
        row(&[]);
        row(&["Risers".into()]);
        row(&[
            "Name".into(),
            "Air".into(),
            "Way".into(),
            "Size".into(),
            "Flow (m3/h)".into(),
            "From other floors (m3/h)".into(),
            "Height (m)".into(),
        ]);
        for r in &self.risers {
            row(&[
                r.name.clone(),
                air(r.air),
                (if r.up { "up" } else { "down" }).into(),
                r.size.clone(),
                opt(r.flow_m3h),
                opt(r.incoming_m3h),
                opt(r.length_m),
            ]);
        }
        if self.totals.riser_length_m > 0.0 {
            row(&["Total height".into(), String::new(), String::new(), String::new(), String::new(), String::new(), num(self.totals.riser_length_m)]);
        }
        row(&[]);
        row(&["Spaces".into()]);
        row(&[
            "Name".into(),
            "Area (m2)".into(),
            "Needed (m3/h)".into(),
            "Supply terminals".into(),
            "Supply (m3/h)".into(),
            "Extract terminals".into(),
            "Extract (m3/h)".into(),
            "Duct inside (m)".into(),
        ]);
        for s in &self.spaces {
            row(&[
                s.name.clone(),
                num(s.area_m2),
                opt(s.needed_m3h),
                s.supply_terminals.to_string(),
                num(s.supply_m3h),
                s.extract_terminals.to_string(),
                num(s.extract_m3h),
                num(s.duct_m),
            ]);
        }
        out
    }
}
