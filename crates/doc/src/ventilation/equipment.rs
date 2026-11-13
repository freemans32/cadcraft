//! Heating and cooling equipment seen from above: chillers, outdoor units (split, multi-split, Sky
//! Air, VRV) and indoor units (cassette, ducted, wall, ceiling, floor), each a block reference with
//! its generated outline, fans, service clearance and spec text, its parameters in extended data.
//!
//! Every value of an object is the user's to change (name, make and model, power, sizes, weight, number
//! of fans, clearance, connections); the library only fills them in from a model, by size or power.
//! The sizes of the standard models are plain numbers; the symbols are drawn here, in code.

use cadcraft_geom::{PolyVertex, Vec2};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, label_left, v3, xdata};
use crate::{Circle, EntityKind, Line, LwPolyline, RawTag, Result};

/// The registered application name of an equipment's extended data.
pub const EQUIPMENT_APP: &str = "CADCRAFT_EQUIP";
/// The layer of an equipment's outline and symbol; its service clearance is on its own layer.
pub const EQUIPMENT_LAYER: &str = "M-HVAC-EQUIP";
pub const EQUIPMENT_ZONE_LAYER: &str = "M-HVAC-EQUIP-ZONE";
const XDATA_FORMAT: &str = "EQUIP1";
/// Largest side of an equipment, mm.
pub const MAX_EQUIPMENT_MM: f64 = 100_000.0;
/// Most fans one equipment draws.
pub const MAX_EQUIPMENT_FANS: u32 = 32;
const MAX_TEXT_CHARS: usize = 64;

fn default_label_mm() -> f64 {
    100.0
}

/// What an equipment is: it sets how it is drawn from above.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EquipmentKind {
    /// An air-cooled chiller or heat pump: fans in rows over the coil.
    #[default]
    Chiller,
    /// A VRV or VRF outdoor unit.
    VrfOutdoor,
    /// A split, multi-split or Sky Air outdoor unit.
    SplitOutdoor,
    /// A ceiling cassette.
    Cassette,
    /// A ducted unit (gainable).
    Ducted,
    /// A wall-mounted unit.
    WallUnit,
    /// A ceiling-suspended unit (plafonnier).
    CeilingUnit,
    /// A floor or console unit.
    FloorUnit,
}

impl EquipmentKind {
    pub const ALL: [EquipmentKind; 8] = [
        EquipmentKind::Chiller,
        EquipmentKind::VrfOutdoor,
        EquipmentKind::SplitOutdoor,
        EquipmentKind::Cassette,
        EquipmentKind::Ducted,
        EquipmentKind::WallUnit,
        EquipmentKind::CeilingUnit,
        EquipmentKind::FloorUnit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            EquipmentKind::Chiller => "chiller",
            EquipmentKind::VrfOutdoor => "vrfOutdoor",
            EquipmentKind::SplitOutdoor => "splitOutdoor",
            EquipmentKind::Cassette => "cassette",
            EquipmentKind::Ducted => "ducted",
            EquipmentKind::WallUnit => "wallUnit",
            EquipmentKind::CeilingUnit => "ceilingUnit",
            EquipmentKind::FloorUnit => "floorUnit",
        }
    }

    pub fn parse(text: &str) -> Option<EquipmentKind> {
        let t: String = text.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        match t.as_str() {
            "chiller" | "chilleur" | "groupeeau" | "pac" | "pompeachaleur" => Some(EquipmentKind::Chiller),
            "vrfoutdoor" | "vrv" | "vrf" | "vrvoutdoor" | "groupevrv" => Some(EquipmentKind::VrfOutdoor),
            "splitoutdoor" | "split" | "multisplit" | "skyair" | "groupeexterieur" | "outdoor" => Some(EquipmentKind::SplitOutdoor),
            "cassette" => Some(EquipmentKind::Cassette),
            "ducted" | "gainable" => Some(EquipmentKind::Ducted),
            "wallunit" | "wall" | "mural" | "murale" => Some(EquipmentKind::WallUnit),
            "ceilingunit" | "ceiling" | "plafonnier" => Some(EquipmentKind::CeilingUnit),
            "floorunit" | "floor" | "console" | "allege" => Some(EquipmentKind::FloorUnit),
            _ => None,
        }
    }

    /// Whether the unit stands outdoors (fans over a coil, a service clearance all round).
    pub fn is_outdoor(self) -> bool {
        matches!(self, EquipmentKind::Chiller | EquipmentKind::VrfOutdoor | EquipmentKind::SplitOutdoor)
    }

    /// The service clearance a new equipment of the kind starts with, mm.
    pub fn default_clearance_mm(self) -> f64 {
        match self {
            EquipmentKind::Chiller => 1000.0,
            EquipmentKind::VrfOutdoor | EquipmentKind::SplitOutdoor => 500.0,
            _ => 0.0,
        }
    }
}

/// A model of the library: the values an equipment takes when it is chosen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentModel {
    pub name: String,
    pub kind: EquipmentKind,
    /// Free text; empty in the library.
    pub reference: String,
    /// Nominal cooling power, kW (CV for a VRV outdoor unit), and heating power, kW, for a heat pump.
    pub capacity_kw: Option<f64>,
    pub heating_kw: Option<f64>,
    /// Length (along the block's x axis), width (its y axis) and height, mm.
    pub length_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub weight_kg: Option<f64>,
    pub fans: u32,
    /// Connections, free text ("Victaulic 5\"", "1/4 - 3/8").
    pub connections: String,
}

/// An equipment: what its block reference keeps to regenerate it. It is centred on the block's origin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentParams {
    pub name: String,
    #[serde(default)]
    pub kind: EquipmentKind,
    /// The library model it was made from, or empty (an equipment is never tied to it).
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub capacity_kw: Option<f64>,
    #[serde(default)]
    pub heating_kw: Option<f64>,
    pub length_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    #[serde(default)]
    pub weight_kg: Option<f64>,
    #[serde(default)]
    pub fans: u32,
    /// The service zone drawn round the equipment, mm; 0 for none.
    #[serde(default)]
    pub clearance_mm: f64,
    #[serde(default)]
    pub connections: String,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

fn text_ok(t: &str) -> bool {
    t.chars().count() <= MAX_TEXT_CHARS && !t.chars().any(char::is_control)
}

impl EquipmentParams {
    /// A new equipment of `model`, its values the model's.
    pub fn from_model(m: &EquipmentModel, name: &str, units: HvacUnits) -> EquipmentParams {
        EquipmentParams {
            name: name.to_string(),
            kind: m.kind,
            model: m.name.clone(),
            reference: m.reference.clone(),
            capacity_kw: m.capacity_kw,
            heating_kw: m.heating_kw,
            length_mm: m.length_mm,
            width_mm: m.width_mm,
            height_mm: m.height_mm,
            weight_kg: m.weight_kg,
            fans: m.fans,
            clearance_mm: m.kind.default_clearance_mm(),
            connections: m.connections.clone(),
            label_height_mm: 100.0,
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_name("equipment", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        let side = |v: f64| v.is_finite() && (50.0..=MAX_EQUIPMENT_MM).contains(&v);
        if !(side(self.length_mm) && side(self.width_mm) && side(self.height_mm)) {
            return Err(invalid(format!("An equipment's sizes are 50 to {MAX_EQUIPMENT_MM} mm.")));
        }
        let power = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v > 0.0 && v <= 1.0e5);
        if !(power(self.capacity_kw) && power(self.heating_kw)) {
            return Err(invalid("An equipment's power is more than 0 and at most 100 000 kW."));
        }
        if self.weight_kg.is_some_and(|w| !(w.is_finite() && w > 0.0 && w <= 1.0e7)) {
            return Err(invalid("An equipment's weight is more than 0 and at most 10 000 000 kg."));
        }
        if self.fans > MAX_EQUIPMENT_FANS {
            return Err(invalid(format!("An equipment has at most {MAX_EQUIPMENT_FANS} fans.")));
        }
        if !(self.clearance_mm.is_finite() && (0.0..=10_000.0).contains(&self.clearance_mm)) {
            return Err(invalid("The service clearance is 0 to 10 000 mm."));
        }
        if !(text_ok(&self.model) && text_ok(&self.reference) && text_ok(&self.connections)) {
            return Err(invalid(format!(
                "The model, reference and connections have at most {MAX_TEXT_CHARS} characters, without control characters."
            )));
        }
        Ok(())
    }

    /// The spec lines written beside the equipment: the reference, "Dim. LxPxH" (length x width x
    /// height), the weight, the connections; only what is known.
    pub fn spec_lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.reference.trim().is_empty() {
            v.push(self.reference.trim().to_string());
        }
        v.push(format!("Dim. {:.0}x{:.0}x{:.0}H", self.length_mm, self.width_mm, self.height_mm));
        if let Some(w) = self.weight_kg {
            v.push(format!("Poids {w:.0}kg"));
        }
        if !self.connections.trim().is_empty() {
            v.push(self.connections.trim().to_string());
        }
        v
    }

    /// The text under the equipment: its name and its power.
    pub fn text(&self) -> String {
        let mut t = self.name.clone();
        // The power in kW, or in CV for a VRV outdoor unit, whose name already says it when it came from the library.
        let unit = if self.kind == EquipmentKind::VrfOutdoor { "CV" } else { "kW" };
        if !self.name.contains(unit) {
            match (self.capacity_kw, self.heating_kw) {
                (Some(c), Some(h)) => t.push_str(&format!(" {c} / {h} {unit}")),
                (Some(c), None) => t.push_str(&format!(" {c} {unit}")),
                (None, Some(h)) => t.push_str(&format!(" {h} {unit}")),
                (None, None) => {}
            }
        }
        t
    }

    /// The outline, the symbol of its kind, the service clearance and the text.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let u = |mm: f64| self.units.to_units(mm);
        let (hl, hw) = (u(self.length_mm) / 2.0, u(self.width_mm) / 2.0);
        let h = u(self.label_height_mm);
        let mut pieces = vec![polyline(EQUIPMENT_LAYER, rect_points(hl, hw), true)];
        let line = |a: Vec2, b: Vec2| HvacPiece { layer: EQUIPMENT_LAYER, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        match self.kind {
            EquipmentKind::Chiller | EquipmentKind::VrfOutdoor | EquipmentKind::SplitOutdoor => {
                // Fans in one row, or two over a wide unit: a circle crossed by two diameters.
                let n = self.fans as usize;
                let rows = if n >= 4 && self.width_mm >= 1600.0 { 2 } else { 1 };
                let cols = n.div_ceil(rows).max(1);
                let d = 0.8 * (2.0 * hl / cols as f64).min(2.0 * hw / rows as f64);
                for i in 0..n {
                    let (row, col) = (i / cols, i % cols);
                    let c = Vec2::new(-hl + (col as f64 + 0.5) * 2.0 * hl / cols as f64, hw - (row as f64 + 0.5) * 2.0 * hw / rows as f64);
                    pieces.push(HvacPiece { layer: EQUIPMENT_LAYER, kind: EntityKind::Circle(Circle { center: v3(c), radius: d / 2.0 }) });
                    let r = d / 2.0 * std::f64::consts::FRAC_1_SQRT_2;
                    pieces.push(line(c + Vec2::new(-r, -r), c + Vec2::new(r, r)));
                    pieces.push(line(c + Vec2::new(-r, r), c + Vec2::new(r, -r)));
                }
            }
            EquipmentKind::Cassette => {
                // The panel, an outlet slot along each side and the crossed diagonals of the grille.
                let (pl, pw) = (0.82 * hl, 0.82 * hw);
                pieces.push(polyline(EQUIPMENT_LAYER, rect_points(pl, pw), true));
                for (a, b) in [
                    (Vec2::new(-0.6 * pl, pw), Vec2::new(0.6 * pl, pw)),
                    (Vec2::new(-0.6 * pl, -pw), Vec2::new(0.6 * pl, -pw)),
                    (Vec2::new(pl, -0.6 * pw), Vec2::new(pl, 0.6 * pw)),
                    (Vec2::new(-pl, -0.6 * pw), Vec2::new(-pl, 0.6 * pw)),
                ] {
                    pieces.push(line(a, b));
                }
                pieces.push(line(Vec2::new(-0.5 * pl, -0.5 * pw), Vec2::new(0.5 * pl, 0.5 * pw)));
                pieces.push(line(Vec2::new(-0.5 * pl, 0.5 * pw), Vec2::new(0.5 * pl, -0.5 * pw)));
            }
            EquipmentKind::Ducted => {
                // A supply collar at one end, a wider return collar at the other, an arrow along the air.
                let depth = 0.12 * hl.min(hw) * 2.0;
                pieces.push(polyline(EQUIPMENT_LAYER, rect_points_at(hl + depth / 2.0, 0.0, depth / 2.0, 0.55 * hw), true));
                pieces.push(polyline(EQUIPMENT_LAYER, rect_points_at(-hl - depth / 2.0, 0.0, depth / 2.0, 0.7 * hw), true));
                let (from, to) = (Vec2::new(-0.5 * hl, 0.0), Vec2::new(0.5 * hl, 0.0));
                pieces.extend([line(from, to), line(to, to + Vec2::new(-0.12 * hl, 0.15 * hw)), line(to, to + Vec2::new(-0.12 * hl, -0.15 * hw))]);
            }
            EquipmentKind::WallUnit | EquipmentKind::CeilingUnit | EquipmentKind::FloorUnit => {
                // The air outlet: lines along the front, an arrow out of it; a ceiling unit also shows its fan.
                for k in [-0.55, 0.0, 0.55] {
                    pieces.push(line(Vec2::new(-0.8 * hl, hw * (0.55 + 0.3 * k)), Vec2::new(0.8 * hl, hw * (0.55 + 0.3 * k))));
                }
                let (from, to) = (Vec2::new(0.0, -hw), Vec2::new(0.0, -hw - 0.9 * hw.max(h)));
                pieces.extend([
                    line(from, to),
                    line(to, to + Vec2::new(-0.2 * hw.max(h), 0.3 * hw.max(h))),
                    line(to, to + Vec2::new(0.2 * hw.max(h), 0.3 * hw.max(h))),
                ]);
                if self.kind == EquipmentKind::CeilingUnit {
                    pieces.push(HvacPiece {
                        layer: EQUIPMENT_LAYER,
                        kind: EntityKind::Circle(Circle { center: v3(Vec2::new(-0.5 * hl, 0.0)), radius: 0.35 * hw }),
                    });
                }
            }
        }
        // The service clearance, dashed so it is not taken for the equipment.
        if self.clearance_mm > 0.0 {
            let c = u(self.clearance_mm);
            let (zl, zw) = (hl + c, hw + c);
            let (dash, gap) = (u(250.0), u(150.0));
            for (a, b) in [
                (Vec2::new(-zl, -zw), Vec2::new(zl, -zw)),
                (Vec2::new(zl, -zw), Vec2::new(zl, zw)),
                (Vec2::new(zl, zw), Vec2::new(-zl, zw)),
                (Vec2::new(-zl, zw), Vec2::new(-zl, -zw)),
            ] {
                let (len, dir) = (a.dist(b), (b - a).normalized());
                let step = (dash + gap).max(1e-9);
                let count = ((len / step).ceil() as usize).min(2_000);
                for k in 0..count {
                    let s = k as f64 * step;
                    pieces.push(HvacPiece {
                        layer: EQUIPMENT_ZONE_LAYER,
                        kind: EntityKind::Line(Line { a: v3(a + dir * s), b: v3(a + dir * (s + dash).min(len)) }),
                    });
                }
            }
        }
        // The name and power below; the spec lines to the right.
        let zone = u(self.clearance_mm);
        // A unit with an air outlet draws its arrow under it: the text goes below the arrow.
        let arrow =
            if matches!(self.kind, EquipmentKind::WallUnit | EquipmentKind::CeilingUnit | EquipmentKind::FloorUnit) { 1.1 * hw.max(h) } else { 0.0 };
        pieces.push(HvacPiece { layer: EQUIPMENT_LAYER, kind: label(Vec2::new(0.0, -hw - zone - arrow - 1.2 * h), h, self.text()) });
        let x = hl + zone + 2.0 * h;
        for (i, t) in self.spec_lines().into_iter().enumerate() {
            pieces.push(HvacPiece { layer: EQUIPMENT_LAYER, kind: label_left(Vec2::new(x, hw - (i as f64) * 1.6 * h - h), h, t) });
        }
        Ok(pieces)
    }

    /// The equipment as it will look at `at`, turned `rotation` radians; empty when it can't be drawn.
    pub fn preview(&self, at: Vec2, rotation: f64) -> Vec<crate::Entity> {
        self.generate().map(|p| super::preview_entities(p, at, rotation)).unwrap_or_default()
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(EQUIPMENT_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<EquipmentParams> {
        let p: EquipmentParams = xdata::from_xdata(EQUIPMENT_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

fn rect_points(hl: f64, hw: f64) -> Vec<Vec2> {
    rect_points_at(0.0, 0.0, hl, hw)
}

fn rect_points_at(cx: f64, cy: f64, hl: f64, hw: f64) -> Vec<Vec2> {
    vec![Vec2::new(cx - hl, cy - hw), Vec2::new(cx + hl, cy - hw), Vec2::new(cx + hl, cy + hw), Vec2::new(cx - hl, cy + hw)]
}

fn polyline(layer: &'static str, pts: Vec<Vec2>, closed: bool) -> HvacPiece {
    let vertices = pts.into_iter().map(|p| PolyVertex { p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect();
    HvacPiece { layer, kind: EntityKind::LwPolyline(LwPolyline { vertices, closed, const_width: 0.0, elevation: 0.0, plinegen: false }) }
}

/// The equipment an object is, when it is a block reference carrying usable equipment data.
pub fn equipment_of(e: &crate::Entity) -> Option<EquipmentParams> {
    match &e.kind {
        EntityKind::Insert(_) => EquipmentParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds equipment `p` centred at `at`, turned `rotation` radians.
    pub fn add_equipment(
        &mut self,
        space: &crate::Space,
        mut common: crate::Common,
        p: &EquipmentParams,
        at: Vec2,
        rotation: f64,
    ) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("An equipment needs a usable position and angle."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(EQUIPMENT_APP, &common.xdata, p.to_xdata()?);
        common.layer = EQUIPMENT_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(EQUIPMENT_LAYER);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives equipment `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_equipment(&mut self, h: crate::Handle, p: &EquipmentParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(EQUIPMENT_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }
}

// =====================================================================================
// The standard models
// =====================================================================================

#[allow(clippy::too_many_arguments)]
fn model(
    name: String,
    kind: EquipmentKind,
    capacity: f64,
    heating_kw: Option<f64>,
    (length_mm, width_mm, height_mm): (f64, f64, f64),
    fans: u32,
) -> EquipmentModel {
    EquipmentModel {
        name,
        kind,
        reference: String::new(),
        capacity_kw: Some(capacity),
        heating_kw,
        length_mm,
        width_mm,
        height_mm,
        weight_kg: None,
        fans,
        connections: String::new(),
    }
}

/// The models the library starts with: a name made of the kind and the power (kW; CV for a VRV unit),
/// sizes in millimetres as length x width x height, and the number of fans drawn. They only fill the
/// values in: nothing ties an equipment to them, and every value can be changed.
pub fn standard_equipment_models() -> Vec<EquipmentModel> {
    use EquipmentKind::{Cassette, CeilingUnit, Chiller, Ducted, FloorUnit, SplitOutdoor, VrfOutdoor, WallUnit};
    let mut v = Vec::new();
    // Chillers: cooling only, then heat pumps (cooling and heating powers), then the larger casings.
    let cooling = [
        (41.7, 2109.0, 1090.0, 1),
        (47.3, 2109.0, 1090.0, 1),
        (52.9, 2109.0, 1090.0, 1),
        (56.1, 2109.0, 1090.0, 1),
        (63.6, 2109.0, 1090.0, 1),
        (71.2, 2109.0, 1090.0, 1),
        (81.1, 2109.0, 1090.0, 1),
        (93.4, 2275.0, 2125.0, 2),
        (107.0, 2275.0, 2125.0, 2),
        (124.0, 2275.0, 2125.0, 2),
        (140.0, 2275.0, 2125.0, 2),
        (160.0, 2275.0, 2125.0, 2),
    ];
    for (kw, l, w, fans) in cooling {
        v.push(model(format!("Chiller {kw} kW"), Chiller, kw, None, (l, w, 1330.0), fans));
    }
    let heat_pumps = [
        (41.0, 42.7, 2109.0, 1090.0, 1),
        (43.1, 47.0, 2109.0, 1090.0, 1),
        (50.3, 53.5, 2109.0, 1090.0, 1),
        (60.2, 59.5, 2109.0, 1090.0, 1),
        (65.2, 67.2, 2109.0, 1090.0, 1),
        (74.3, 75.7, 2109.0, 1090.0, 1),
        (87.0, 91.7, 2275.0, 2125.0, 2),
        (99.9, 105.0, 2275.0, 2125.0, 2),
        (114.0, 118.0, 2275.0, 2125.0, 2),
        (132.0, 135.0, 2275.0, 2125.0, 2),
        (147.0, 150.0, 2275.0, 2125.0, 2),
    ];
    for (cool, heat, l, w, fans) in heat_pumps {
        v.push(model(format!("Heat-pump chiller {cool} kW"), Chiller, cool, Some(heat), (l, w, 1330.0), fans));
    }
    let large = [
        (168.0, 2410.0, 3),
        (181.0, 2410.0, 4),
        (198.0, 2410.0, 4),
        (216.0, 2410.0, 4),
        (261.0, 2410.0, 4),
        (300.0, 3604.0, 5),
        (331.0, 3604.0, 5),
        (365.0, 3604.0, 6),
        (397.0, 3604.0, 6),
        (430.0, 4797.0, 7),
        (464.0, 4797.0, 7),
        (523.0, 4797.0, 8),
    ];
    for (kw, l, fans) in large {
        v.push(model(format!("Chiller {kw} kW"), Chiller, kw, None, (l, 2253.0, 2297.0), fans));
    }
    v.push(model("Heat-pump chiller 570 kW".into(), Chiller, 570.0, None, (7185.0, 2259.0, 2324.0), 10));

    // VRV outdoor units, by power in CV (HP): the compact casing, then single modules.
    for hp in [4.0, 5.0, 6.0] {
        v.push(model(format!("VRV {hp} CV"), VrfOutdoor, hp, None, (940.0, 500.0, 823.0), 1));
    }
    for (hp, l) in [(8.0, 930.0), (10.0, 930.0), (12.0, 1240.0), (14.0, 1240.0), (16.0, 1240.0), (18.0, 1240.0), (20.0, 1240.0)] {
        v.push(model(format!("VRV {hp} CV"), VrfOutdoor, hp, None, (l, 765.0, 1685.0), if l > 1000.0 { 2 } else { 1 }));
    }
    // Split and multi-split outdoor units, by power in kW.
    for kw in [2.0, 2.5, 3.5] {
        v.push(model(format!("Split outdoor {kw} kW"), SplitOutdoor, kw, None, (840.0, 350.0, 552.0), 1));
    }
    for kw in [4.2, 5.0] {
        v.push(model(format!("Split outdoor {kw} kW"), SplitOutdoor, kw, None, (954.0, 401.0, 734.0), 1));
    }
    for kw in [7.1, 10.0, 12.5, 14.0] {
        v.push(model(format!("Split outdoor {kw} kW"), SplitOutdoor, kw, None, (1100.0, 490.0, 870.0), 1));
    }
    v.push(model("Split outdoor compact 7.1 kW".into(), SplitOutdoor, 7.1, None, (900.0, 350.0, 770.0), 1));
    for kw in [10.0, 12.5, 14.0] {
        v.push(model(format!("Split outdoor compact {kw} kW"), SplitOutdoor, kw, None, (940.0, 350.0, 990.0), 1));
    }
    for kw in [4.0, 5.0] {
        v.push(model(format!("Multi-split outdoor {kw} kW"), SplitOutdoor, kw, None, (852.0, 350.0, 552.0), 1));
    }
    for kw in [6.8, 8.0, 9.0] {
        v.push(model(format!("Multi-split outdoor {kw} kW"), SplitOutdoor, kw, None, (974.0, 408.0, 734.0), 1));
    }
    // Indoor units, by power in kW.
    for kw in [2.5, 3.5, 5.0, 6.0] {
        v.push(model(format!("Cassette 600 {kw} kW"), Cassette, kw, None, (620.0, 620.0, 260.0), 0));
    }
    for kw in [7.1, 10.0, 12.5, 14.0] {
        v.push(model(format!("Cassette 950 {kw} kW"), Cassette, kw, None, (950.0, 950.0, 288.0), 0));
    }
    for kw in [2.5, 3.5] {
        v.push(model(format!("Slim ducted {kw} kW"), Ducted, kw, None, (750.0, 620.0, 200.0), 0));
    }
    for kw in [5.0, 6.0] {
        v.push(model(format!("Slim ducted {kw} kW"), Ducted, kw, None, (1150.0, 620.0, 200.0), 0));
    }
    v.push(model("Ducted 7.1 kW".into(), Ducted, 7.1, None, (1000.0, 800.0, 245.0), 0));
    for kw in [10.0, 12.5, 14.0] {
        v.push(model(format!("Ducted {kw} kW"), Ducted, kw, None, (1400.0, 800.0, 245.0), 0));
    }
    v.push(model("High-pressure ducted 12.5 kW".into(), Ducted, 12.5, None, (1400.0, 700.0, 300.0), 0));
    for kw in [20.0, 25.0] {
        v.push(model(format!("High-pressure ducted {kw} kW"), Ducted, kw, None, (1490.0, 1100.0, 470.0), 0));
    }
    for kw in [2.0, 2.5, 3.5, 5.0] {
        v.push(model(format!("Wall unit {kw} kW"), WallUnit, kw, None, (804.0, 252.0, 298.0), 0));
    }
    for kw in [6.0, 7.1] {
        v.push(model(format!("Wall unit {kw} kW"), WallUnit, kw, None, (997.0, 292.0, 298.0), 0));
    }
    v.push(model("Wall unit 10 kW".into(), WallUnit, 10.0, None, (1200.0, 262.0, 340.0), 0));
    for kw in [3.5, 5.0] {
        v.push(model(format!("Ceiling unit {kw} kW"), CeilingUnit, kw, None, (960.0, 690.0, 235.0), 0));
    }
    v.push(model("Ceiling unit 7.1 kW".into(), CeilingUnit, 7.1, None, (1270.0, 690.0, 235.0), 0));
    for kw in [10.0, 12.5, 14.0] {
        v.push(model(format!("Ceiling unit {kw} kW"), CeilingUnit, kw, None, (1590.0, 690.0, 235.0), 0));
    }
    for kw in [2.5, 3.5, 5.0] {
        v.push(model(format!("Floor unit {kw} kW"), FloorUnit, kw, None, (750.0, 238.0, 600.0), 0));
    }
    v
}

/// The models of `kind`, smallest power first.
pub fn equipment_models_of(models: &[EquipmentModel], kind: EquipmentKind) -> Vec<EquipmentModel> {
    let mut v: Vec<EquipmentModel> = models.iter().filter(|m| m.kind == kind).cloned().collect();
    v.sort_by(|a, b| a.capacity_kw.unwrap_or(0.0).total_cmp(&b.capacity_kw.unwrap_or(0.0)).then(a.name.cmp(&b.name)));
    v
}

/// The smallest model of `kind` that gives `power` or more, else the largest one.
pub fn equipment_model_for(models: &[EquipmentModel], kind: EquipmentKind, power: f64) -> Option<EquipmentModel> {
    let of = equipment_models_of(models, kind);
    of.iter().find(|m| m.capacity_kw.is_some_and(|c| c >= power)).or(of.last()).cloned()
}

/// The model named `name` (case-insensitive).
pub fn equipment_model_named(models: &[EquipmentModel], name: &str) -> Option<EquipmentModel> {
    models.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim())).cloned()
}
