//! The quantities of one level: each drawing of a building is a floor, so the quantities of a drawing
//! are those of its level. Ducts by direction, section and altitude (lengths and surfaces), fittings
//! (elbows, tees, reducers), accessories, terminals, units, risers, equipment, and by space the
//! terminals it holds and the duct length inside it.
//!
//! Counted from what is drawn; the sizes are the ones sizing wrote, so size the ducts first.

use cadcraft_geom::Vec2;
use cadcraft_hvac::Fitting;
use serde::{Deserialize, Serialize};

use super::xlsx::{SheetCell, Workbook};
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

/// The pipes of one service and leg, one size and one insulation.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipeQuantity {
    pub service: super::PipeService,
    pub leg: super::PipeLeg,
    /// "DN50", or "Ø12.7" for a refrigerant tube.
    pub size: String,
    pub insulation_mm: f64,
    pub count: usize,
    pub length_m: f64,
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
    /// The terminals the space holds, by model.
    #[serde(default)]
    pub terminal_models: Vec<TerminalQuantity>,
    /// The duct inside the space, by direction and section.
    #[serde(default)]
    pub ducts: Vec<SpaceDuct>,
}

/// The length of duct of one direction and one section inside a space.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceDuct {
    pub air: AirDirection,
    pub flexible: bool,
    pub section: String,
    pub length_m: f64,
}

/// The calculation of one network, as sizing computes it now: each run's section, flow, velocity
/// and losses, and the pressure the unit must supply.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteNetwork {
    /// The unit or riser the network starts from.
    pub origin: String,
    pub air: AirDirection,
    pub total_flow_m3h: f64,
    pub required_pa: f64,
    pub segments: Vec<NoteSegment>,
    pub warnings: Vec<String>,
}

/// One run of a [`NoteNetwork`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteSegment {
    /// The duct's handle, as it is shown in the drawing's properties.
    pub duct: String,
    pub section: String,
    pub length_m: f64,
    pub flow_m3h: f64,
    pub velocity_ms: f64,
    pub gradient_pa_m: f64,
    pub friction_pa: f64,
    pub fittings_pa: f64,
    pub terminal_pa: f64,
    pub total_pa: f64,
    /// What set the section: velocity, friction, imposed or largest.
    pub governing: String,
    /// Whether the run is on the critical path (the one that sets the unit's pressure).
    pub critical: bool,
}

/// The calculation note: the networks of the drawing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcNote {
    pub networks: Vec<NoteNetwork>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub pipe_count: usize,
    pub pipe_length_m: f64,
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
    pub pipes: Vec<PipeQuantity>,
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
        let mut terminals: Vec<(Vec2, AirDirection, f64, String)> = Vec::new();
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
                terminals.push((Vec2::new(i.insert.x, i.insert.y), x.air, x.flow_m3h, x.model.name.clone()));
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
        let axes: Vec<(Vec2, Vec2, f64, AirDirection, bool, String)> = self
            .model
            .iter()
            .filter_map(|e| self.duct_ends(e.handle))
            .map(|(p, a, b)| (a, b, p.units.mm_per_unit, p.air, p.flexible, p.size_label()))
            .collect();
        for e in self.model.iter() {
            let Some((sp, outline)) = self.space_outline(e.handle) else { continue };
            let mut q = SpaceQuantity { name: sp.name.clone(), area_m2: sp.area_m2(), needed_m3h: sp.flow().ok(), ..SpaceQuantity::default() };
            for (at, air, flow, model) in &terminals {
                if cadcraft_geom::point_in_polygon(&outline, *at) {
                    match q.terminal_models.iter_mut().find(|m| m.model == *model && m.air == *air) {
                        Some(m) => (m.count, m.flow_m3h) = (m.count + 1, m.flow_m3h + flow),
                        None => q.terminal_models.push(TerminalQuantity { model: model.clone(), air: *air, count: 1, flow_m3h: *flow }),
                    }
                    match air {
                        AirDirection::Supply => (q.supply_terminals, q.supply_m3h) = (q.supply_terminals + 1, q.supply_m3h + flow),
                        AirDirection::Extract => (q.extract_terminals, q.extract_m3h) = (q.extract_terminals + 1, q.extract_m3h + flow),
                        _ => {}
                    }
                }
            }
            for (a, b, mm, air, flexible, section) in &axes {
                let length = a.dist(*b);
                let n = SAMPLES;
                let inside = (0..n).filter(|k| cadcraft_geom::point_in_polygon(&outline, *a + (*b - *a) * ((*k as f64 + 0.5) / n as f64))).count();
                let here = length * mm / 1000.0 * inside as f64 / n as f64;
                q.duct_m += here;
                if inside > 0 {
                    match q.ducts.iter_mut().find(|d| d.air == *air && d.flexible == *flexible && d.section == *section) {
                        Some(d) => d.length_m += here,
                        None => q.ducts.push(SpaceDuct { air: *air, flexible: *flexible, section: section.clone(), length_m: here }),
                    }
                }
            }
            q.duct_m = (q.duct_m * 100.0).round() / 100.0;
            for d in &mut q.ducts {
                d.length_m = (d.length_m * 100.0).round() / 100.0;
            }
            q.ducts.sort_by(|a, b| air_rank(a.air).cmp(&air_rank(b.air)).then(a.section.cmp(&b.section)));
            q.terminal_models.sort_by(|a, b| air_rank(a.air).cmp(&air_rank(b.air)).then(a.model.cmp(&b.model)));
            t.spaces.push(q);
        }
        t.spaces.sort_by(|a, b| a.name.cmp(&b.name));
        t.units.sort_by(|a, b| a.name.cmp(&b.name));
        t.risers.sort_by(|a, b| a.name.cmp(&b.name));
        t.pipes = self.pipe_quantities();
        // Totals, and lengths to the centimetre.
        for d in &mut t.ducts {
            d.length_m = (d.length_m * 100.0).round() / 100.0;
            d.area_m2 = (d.area_m2 * 100.0).round() / 100.0;
        }
        t.totals = Totals {
            pipe_count: t.pipes.iter().map(|q| q.count).sum(),
            pipe_length_m: (t.pipes.iter().map(|q| q.length_m).sum::<f64>() * 100.0).round() / 100.0,
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

/// A label in `lang` ("fr" for French, any other language is English).
fn tr(lang: &str, key: &str) -> String {
    if !lang.to_ascii_lowercase().starts_with("fr") {
        return key.to_string();
    }
    let fr = match key {
        "Summary" => "Récapitulatif",
        "Ducts" => "Gaines",
        "Pipes" => "Tuyauteries",
        "Pipe count" => "Nombre de tuyaux",
        "Pipe length (m)" => "Longueur de tuyauterie (m)",
        "Service" => "Service",
        "Leg" => "Branche",
        "Insulation (mm)" => "Isolant (mm)",
        "chilledWater" => "Eau glacée",
        "hotWater" => "Eau chaude",
        "changeOver" => "Change-over",
        "condensate" => "Condensats",
        "refrigerantLiquid" => "Frigorifique liquide",
        "refrigerantGas" => "Frigorifique gaz",
        "pipe-supply" => "Départ",
        "pipe-return" => "Retour",
        "Fittings and accessories" => "Raccords et accessoires",
        "Terminals" => "Bouches",
        "Units and equipment" => "Appareils",
        "Spaces" => "Locaux",
        "Space detail" => "Détail des locaux",
        "Calculation note" => "Note de calcul",
        "Level" => "Niveau",
        "Air" => "Air",
        "Material" => "Matière",
        "rigid" => "rigide",
        "flexible" => "souple",
        "Section" => "Section",
        "Altitude (m)" => "Altitude (m)",
        "Count" => "Nombre",
        "Length (m)" => "Longueur (m)",
        "Area (m²)" => "Surface (m²)",
        "Total" => "Total",
        "Fittings" => "Raccords",
        "Accessories" => "Accessoires",
        "Name" => "Désignation",
        "Size" => "Dimensions",
        "Power" => "Puissance",
        "Model" => "Modèle",
        "Flow (m³/h)" => "Débit (m³/h)",
        "Units" => "Centrales",
        "Type" => "Type",
        "Supply (m³/h)" => "Soufflage (m³/h)",
        "Extract (m³/h)" => "Reprise (m³/h)",
        "Fresh air (m³/h)" => "Air neuf (m³/h)",
        "Exhaust (m³/h)" => "Rejet (m³/h)",
        "Width (mm)" => "Largeur (mm)",
        "Depth (mm)" => "Profondeur (mm)",
        "Height (mm)" => "Hauteur (mm)",
        "Risers" => "Colonnes",
        "Way" => "Sens",
        "up" => "monte",
        "down" => "descend",
        "From other floors (m³/h)" => "Venant des autres étages (m³/h)",
        "Height (m)" => "Hauteur (m)",
        "Equipment" => "Équipements",
        "Terminals in the space" => "Bouches du local",
        "Space" => "Local",
        "Area" => "Surface",
        "Needed (m³/h)" => "Besoin (m³/h)",
        "Supply terminals" => "Bouches de soufflage",
        "Extract terminals" => "Bouches de reprise",
        "Duct inside (m)" => "Gaine dans le local (m)",
        "Kind" => "Nature",
        "Terminal" => "Bouche",
        "Duct" => "Gaine",
        "Duct count" => "Nombre de gaines",
        "Duct length (m)" => "Longueur de gaine (m)",
        "Duct area (m²)" => "Surface de gaine (m²)",
        "Riser height (m)" => "Hauteur de colonnes (m)",
        "Origin" => "Origine",
        "Total flow (m³/h)" => "Débit total (m³/h)",
        "Pressure to supply (Pa)" => "Pression à fournir (Pa)",
        "Velocity (m/s)" => "Vitesse (m/s)",
        "Gradient (Pa/m)" => "Perte linéique (Pa/m)",
        "Friction (Pa)" => "Frottement (Pa)",
        "Fittings (Pa)" => "Raccords (Pa)",
        "Terminal (Pa)" => "Bouche (Pa)",
        "Total (Pa)" => "Total (Pa)",
        "Set by" => "Imposé par",
        "Critical path" => "Chemin critique",
        "yes" => "oui",
        "velocity" => "vitesse",
        "friction" => "perte de charge",
        "imposed" => "imposé",
        "largest" => "plus grande taille",
        "Warnings" => "Avertissements",
        "Elbow 90°" => "Coude 90°",
        "Elbow 45°" => "Coude 45°",
        "Tee, straight" => "Té, passage droit",
        "Tee, branch" => "Té, piquage",
        "Reducer" => "Réducteur",
        "damper" => "Registre",
        "fireDamper" => "Clapet coupe-feu",
        "silencer" => "Silencieux",
        "flexConnector" => "Manchette souple",
        "supply" => "Soufflage",
        "extract" => "Reprise",
        "fresh" => "Air neuf",
        "exhaust" => "Rejet",
        "Run the sizing first: the sections are the ones it wrote." => "Dimensionner les gaines d'abord : les sections sont celles qu'il a écrites.",
        other => other,
    };
    fr.to_string()
}

impl Takeoff {
    /// The quantities as an Excel workbook (.xlsx): a summary, one sheet each for the ducts, the
    /// fittings and accessories, the terminals, the units and equipment, the spaces (summary and
    /// detail) and, when `note` is given, the calculation note. Labels in `lang` ("fr", else English).
    pub fn to_xlsx(&self, note: Option<&CalcNote>, lang: &str) -> Result<Vec<u8>> {
        use SheetCell as C;
        let t = |k: &str| tr(lang, k);
        let air = |a: AirDirection| t(a.name());
        let head = |cols: &[&str]| cols.iter().map(|c| C::head(t(c))).collect::<Vec<_>>();
        let mut wb = Workbook::new();
        // Summary.
        let num = |v: f64| C::Num((v * 100.0).round() / 100.0);
        let rows = vec![
            vec![C::head(t("Level")), C::text(self.level.clone())],
            vec![],
            vec![C::head(t("Duct count")), C::Num(self.totals.duct_count as f64)],
            vec![C::head(t("Duct length (m)")), num(self.totals.duct_length_m)],
            vec![C::head(t("Duct area (m²)")), num(self.totals.duct_area_m2)],
            vec![C::head(t("Terminals")), C::Num(self.totals.terminals as f64)],
            vec![C::head(t("Equipment")), C::Num(self.totals.equipment as f64)],
            vec![C::head(t("Riser height (m)")), num(self.totals.riser_length_m)],
            vec![C::head(t("Pipe count")), C::Num(self.totals.pipe_count as f64)],
            vec![C::head(t("Pipe length (m)")), num(self.totals.pipe_length_m)],
            vec![],
            vec![C::text(t("Run the sizing first: the sections are the ones it wrote."))],
        ];
        wb.sheet(&t("Summary"), rows)?;
        // Ducts.
        let mut rows = vec![head(&["Air", "Material", "Section", "Altitude (m)", "Count", "Length (m)", "Area (m²)"])];
        for d in &self.ducts {
            rows.push(vec![
                C::text(air(d.air)),
                C::text(t(if d.flexible { "flexible" } else { "rigid" })),
                C::text(d.section.clone()),
                C::opt(d.altitude_m),
                C::Num(d.count as f64),
                num(d.length_m),
                num(d.area_m2),
            ]);
        }
        rows.push(vec![
            C::head(t("Total")),
            C::Empty,
            C::Empty,
            C::Empty,
            C::Num(self.totals.duct_count as f64),
            num(self.totals.duct_length_m),
            num(self.totals.duct_area_m2),
        ]);
        wb.sheet(&t("Ducts"), rows)?;
        // Pipes.
        let mut rows = vec![head(&["Service", "Leg", "Size", "Insulation (mm)", "Count", "Length (m)"])];
        for q in &self.pipes {
            rows.push(vec![
                C::text(t(q.service.name())),
                C::text(if q.service.has_legs() { t(&format!("pipe-{}", q.leg.name())) } else { String::new() }),
                C::text(q.size.clone()),
                num(q.insulation_mm),
                C::Num(q.count as f64),
                num(q.length_m),
            ]);
        }
        rows.push(vec![C::head(t("Total")), C::Empty, C::Empty, C::Empty, C::Num(self.totals.pipe_count as f64), num(self.totals.pipe_length_m)]);
        wb.sheet(&t("Pipes"), rows)?;
        // Fittings and accessories.
        let mut rows = Vec::new();
        for (title, list) in [("Fittings", &self.fittings), ("Accessories", &self.accessories)] {
            if !rows.is_empty() {
                rows.push(vec![]);
            }
            rows.push(vec![C::head(t(title))]);
            rows.push(head(&["Name", "Size", "Count"]));
            for c in list {
                rows.push(vec![C::text(t(&c.name)), C::text(c.size.clone()), C::Num(c.count as f64)]);
            }
        }
        wb.sheet(&t("Fittings and accessories"), rows)?;
        // Terminals.
        let mut rows = vec![head(&["Model", "Air", "Count", "Flow (m³/h)"])];
        for q in &self.terminals {
            rows.push(vec![C::text(q.model.clone()), C::text(air(q.air)), C::Num(q.count as f64), num(q.flow_m3h)]);
        }
        wb.sheet(&t("Terminals"), rows)?;
        // Units, risers, equipment.
        let mut rows = vec![vec![C::head(t("Units"))]];
        rows.push(head(&[
            "Name",
            "Type",
            "Supply (m³/h)",
            "Extract (m³/h)",
            "Fresh air (m³/h)",
            "Exhaust (m³/h)",
            "Width (mm)",
            "Depth (mm)",
            "Height (mm)",
        ]));
        for u in &self.units {
            rows.push(vec![
                C::text(u.name.clone()),
                C::text(u.kind.clone()),
                C::opt(u.supply_m3h),
                C::opt(u.extract_m3h),
                C::opt(u.fresh_m3h),
                C::opt(u.exhaust_m3h),
                num(u.width_mm),
                num(u.depth_mm),
                C::opt(u.height_mm),
            ]);
        }
        rows.push(vec![]);
        rows.push(vec![C::head(t("Risers"))]);
        rows.push(head(&["Name", "Air", "Way", "Section", "Flow (m³/h)", "From other floors (m³/h)", "Height (m)"]));
        for r in &self.risers {
            rows.push(vec![
                C::text(r.name.clone()),
                C::text(air(r.air)),
                C::text(t(if r.up { "up" } else { "down" })),
                C::text(r.size.clone()),
                C::opt(r.flow_m3h),
                C::opt(r.incoming_m3h),
                C::opt(r.length_m),
            ]);
        }
        rows.push(vec![]);
        rows.push(vec![C::head(t("Equipment"))]);
        rows.push(head(&["Name", "Size", "Count", "Power"]));
        for c in &self.equipment {
            rows.push(vec![C::text(c.name.clone()), C::text(c.size.clone()), C::Num(c.count as f64), C::opt(c.power)]);
        }
        wb.sheet(&t("Units and equipment"), rows)?;
        // Spaces: the summary, then the detail of what each holds.
        let mut rows = vec![head(&[
            "Space",
            "Area",
            "Needed (m³/h)",
            "Supply terminals",
            "Supply (m³/h)",
            "Extract terminals",
            "Extract (m³/h)",
            "Duct inside (m)",
        ])];
        for q in &self.spaces {
            rows.push(vec![
                C::text(q.name.clone()),
                num(q.area_m2),
                C::opt(q.needed_m3h),
                C::Num(q.supply_terminals as f64),
                num(q.supply_m3h),
                C::Num(q.extract_terminals as f64),
                num(q.extract_m3h),
                num(q.duct_m),
            ]);
        }
        wb.sheet(&t("Spaces"), rows)?;
        let mut rows = vec![head(&["Space", "Kind", "Air", "Name", "Count", "Length (m)", "Flow (m³/h)"])];
        for q in &self.spaces {
            for m in &q.terminal_models {
                rows.push(vec![
                    C::text(q.name.clone()),
                    C::text(t("Terminal")),
                    C::text(air(m.air)),
                    C::text(m.model.clone()),
                    C::Num(m.count as f64),
                    C::Empty,
                    num(m.flow_m3h),
                ]);
            }
            for d in &q.ducts {
                let name = if d.flexible { format!("{} ({})", d.section, t("flexible")) } else { d.section.clone() };
                rows.push(vec![C::text(q.name.clone()), C::text(t("Duct")), C::text(air(d.air)), C::text(name), C::Empty, num(d.length_m), C::Empty]);
            }
        }
        wb.sheet(&t("Space detail"), rows)?;
        // The calculation note.
        if let Some(note) = note {
            let mut rows = Vec::new();
            for n in &note.networks {
                if !rows.is_empty() {
                    rows.push(vec![]);
                }
                rows.push(vec![
                    C::head(t("Origin")),
                    C::text(n.origin.clone()),
                    C::head(t("Air")),
                    C::text(air(n.air)),
                    C::head(t("Total flow (m³/h)")),
                    num(n.total_flow_m3h),
                    C::head(t("Pressure to supply (Pa)")),
                    num(n.required_pa),
                ]);
                rows.push(head(&[
                    "Duct",
                    "Section",
                    "Length (m)",
                    "Flow (m³/h)",
                    "Velocity (m/s)",
                    "Gradient (Pa/m)",
                    "Friction (Pa)",
                    "Fittings (Pa)",
                    "Terminal (Pa)",
                    "Total (Pa)",
                    "Set by",
                    "Critical path",
                ]));
                for g in &n.segments {
                    rows.push(vec![
                        C::text(g.duct.clone()),
                        C::text(g.section.clone()),
                        num(g.length_m),
                        num(g.flow_m3h),
                        num(g.velocity_ms),
                        num(g.gradient_pa_m),
                        num(g.friction_pa),
                        num(g.fittings_pa),
                        num(g.terminal_pa),
                        num(g.total_pa),
                        C::text(t(&g.governing)),
                        if g.critical { C::text(t("yes")) } else { C::Empty },
                    ]);
                }
                for w in &n.warnings {
                    rows.push(vec![C::head(t("Warnings")), C::text(w.clone())]);
                }
            }
            wb.sheet(&t("Calculation note"), rows)?;
        }
        wb.to_bytes()
    }

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
        row(&[]);
        row(&["Pipes".into()]);
        row(&["Service".into(), "Leg".into(), "Size".into(), "Insulation (mm)".into(), "Count".into(), "Length (m)".into()]);
        for q in &self.pipes {
            row(&[
                q.service.name().into(),
                (if q.service.has_legs() { q.leg.name() } else { "" }).into(),
                q.size.clone(),
                num(q.insulation_mm),
                q.count.to_string(),
                num(q.length_m),
            ]);
        }
        row(&["Total".into(), String::new(), String::new(), String::new(), self.totals.pipe_count.to_string(), num(self.totals.pipe_length_m)]);
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

impl crate::Drawing {
    /// The pipes of model space, by service and leg, size and insulation, lengths to the centimetre.
    fn pipe_quantities(&self) -> Vec<PipeQuantity> {
        let mut pipes: Vec<PipeQuantity> = Vec::new();
        for e in self.model.iter() {
            let Some((p, ..)) = self.pipe_ends(e.handle) else { continue };
            let leg = if p.service.has_legs() { p.leg } else { super::PipeLeg::Supply };
            let size = p.size_text();
            match pipes.iter_mut().find(|q| q.service == p.service && q.leg == leg && q.size == size && q.insulation_mm == p.insulation_mm) {
                Some(q) => {
                    q.count += 1;
                    q.length_m += p.length_m();
                }
                None => pipes.push(PipeQuantity { service: p.service, leg, size, insulation_mm: p.insulation_mm, count: 1, length_m: p.length_m() }),
            }
        }
        let rank = |s: super::PipeService| super::PipeService::ALL.iter().position(|x| *x == s).unwrap_or(0);
        pipes.sort_by(|a, b| rank(a.service).cmp(&rank(b.service)).then((a.leg as u8).cmp(&(b.leg as u8))).then(a.size.cmp(&b.size)));
        for q in &mut pipes {
            q.length_m = (q.length_m * 100.0).round() / 100.0;
        }
        pipes
    }
}
