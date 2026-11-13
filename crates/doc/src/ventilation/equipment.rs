//! Heating and cooling equipment seen from above: chillers, outdoor units (split, multi-split, Sky
//! Air, VRV) and indoor units (cassette, ducted, wall, ceiling, floor), each a block reference with
//! its generated outline, fans, service clearance and spec text, its parameters in extended data.
//!
//! Every value of an object is the user's to change (name, make and model, power, sizes, weight, number
//! of fans, clearance, connections); the library only fills them in from a model, by size or power.
//! The sizes of the standard models come from the public catalogues of the manufacturers, as numbers
//! only: the symbols are drawn here, in code.

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
    /// The make and series the sizes come from, free text, empty for none.
    pub reference: String,
    /// Nominal cooling power, kW, and heating power for a heat pump.
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
        match (self.capacity_kw, self.heating_kw) {
            (Some(c), Some(h)) => t.push_str(&format!(" {c} / {h} kW")),
            (Some(c), None) => t.push_str(&format!(" {c} kW")),
            (None, Some(h)) => t.push_str(&format!(" {h} kW")),
            (None, None) => {}
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
    reference: &str,
    capacity_kw: f64,
    heating_kw: Option<f64>,
    (length_mm, width_mm, height_mm): (f64, f64, f64),
    weight_kg: Option<f64>,
    fans: u32,
    connections: &str,
) -> EquipmentModel {
    EquipmentModel {
        name,
        kind,
        reference: reference.to_string(),
        capacity_kw: Some(capacity_kw),
        heating_kw,
        length_mm,
        width_mm,
        height_mm,
        weight_kg,
        fans,
        connections: connections.to_string(),
    }
}

/// The models the library starts with, taken from the public catalogues (Carrier air-cooled chillers
/// 30RB, 30RQ, 30RBM, 30RQP; Daikin split, multi-split, Sky Air, VRV and indoor units): sizes in
/// millimetres, as length x width x height, weights and powers as published. They are a starting
/// point, never a constraint: every value of a placed equipment can be changed.
pub fn standard_equipment_models() -> Vec<EquipmentModel> {
    use EquipmentKind::{Cassette, CeilingUnit, Chiller, Ducted, FloorUnit, SplitOutdoor, VrfOutdoor, WallUnit};
    let mut v = Vec::new();
    // ---- Chillers (Carrier). 30RB R-32 cooling only, sizes 040R-160R.
    let rb = [
        ("040R", 41.7, 2109.0, 1090.0, 408.0, 0.0, 1),
        ("045R", 47.3, 2109.0, 1090.0, 409.0, 0.0, 1),
        ("050R", 52.9, 2109.0, 1090.0, 428.0, 0.0, 1),
        ("055R", 56.1, 2109.0, 1090.0, 428.0, 0.0, 1),
        ("060R", 63.6, 2109.0, 1090.0, 435.0, 0.0, 1),
        ("070R", 71.2, 2109.0, 1090.0, 446.0, 0.0, 1),
        ("080R", 81.1, 2109.0, 1090.0, 454.0, 0.0, 1),
        ("090R", 93.4, 2275.0, 2125.0, 672.0, 0.0, 2),
        ("100R", 107.0, 2275.0, 2125.0, 734.0, 0.0, 2),
        ("120R", 124.0, 2275.0, 2125.0, 743.0, 0.0, 2),
        ("140R", 140.0, 2275.0, 2125.0, 861.0, 0.0, 2),
        ("160R", 160.0, 2275.0, 2125.0, 877.0, 0.0, 2),
    ];
    for (size, kw, l, w, kg, _, fans) in rb {
        v.push(model(format!("Chiller {kw} kW"), Chiller, &format!("30RB {size}"), kw, None, (l, w, 1330.0), Some(kg), fans, ""));
    }
    // 30RQ heat pumps, sizes 040R-160R: cooling and heating (40/45 °C water) powers.
    let rq = [
        ("040R", 41.0, 42.7, 2109.0, 1090.0, 444.0, 1),
        ("045R", 43.1, 47.0, 2109.0, 1090.0, 446.0, 1),
        ("050R", 50.3, 53.5, 2109.0, 1090.0, 469.0, 1),
        ("060R", 60.2, 59.5, 2109.0, 1090.0, 496.0, 1),
        ("070R", 65.2, 67.2, 2109.0, 1090.0, 506.0, 1),
        ("080R", 74.3, 75.7, 2109.0, 1090.0, 515.0, 1),
        ("090R", 87.0, 91.7, 2275.0, 2125.0, 759.0, 2),
        ("100R", 99.9, 105.0, 2275.0, 2125.0, 818.0, 2),
        ("120R", 114.0, 118.0, 2275.0, 2125.0, 866.0, 2),
        ("140R", 132.0, 135.0, 2275.0, 2125.0, 996.0, 2),
        ("160R", 147.0, 150.0, 2275.0, 2125.0, 1000.0, 2),
    ];
    for (size, cool, heat, l, w, kg, fans) in rq {
        v.push(model(format!("Heat-pump chiller {cool} kW"), Chiller, &format!("30RQ {size}"), cool, Some(heat), (l, w, 1330.0), Some(kg), fans, ""));
    }
    // 30RBM sizes 160-520 (R-410A): three casings.
    let rbm = [
        ("160", 168.0, 2410.0, 1216.0, 3),
        ("180", 181.0, 2410.0, 1257.0, 4),
        ("200", 198.0, 2410.0, 1257.0, 4),
        ("220", 216.0, 2410.0, 1387.0, 4),
        ("260", 261.0, 2410.0, 1408.0, 4),
        ("300", 300.0, 3604.0, 1865.0, 5),
        ("330", 331.0, 3604.0, 1901.0, 5),
        ("360", 365.0, 3604.0, 2069.0, 6),
        ("400", 397.0, 3604.0, 2125.0, 6),
        ("430", 430.0, 4797.0, 2545.0, 7),
        ("470", 464.0, 4797.0, 2563.0, 7),
        ("520", 523.0, 4797.0, 2761.0, 8),
    ];
    for (size, kw, l, kg, fans) in rbm {
        v.push(model(format!("Chiller {kw} kW"), Chiller, &format!("30RBM {size}"), kw, None, (l, 2253.0, 2297.0), Some(kg), fans, ""));
    }
    // 30RQP 570R, the heat pump of the larger casing (sizes from its dimension drawing).
    v.push(model("Heat-pump chiller 570 kW".into(), Chiller, "30RQP 570R", 570.0, None, (7185.0, 2259.0, 2324.0), None, 10, "Victaulic 5\""));

    // ---- VRV outdoor units (Daikin): the compact ones, then the single modules of 8 to 20 HP.
    v.push(model("VRV outdoor 4-6 HP".into(), VrfOutdoor, "VRV IV-S 4-6", 15.5, None, (940.0, 500.0, 823.0), Some(89.0), 1, ""));
    let vrv = [
        (8, 22.4, 930.0, 194.0),
        (10, 28.0, 930.0, 194.0),
        (12, 33.5, 1240.0, 194.0),
        (14, 40.0, 1240.0, 275.0),
        (16, 45.0, 1240.0, 275.0),
        (18, 50.4, 1240.0, 309.0),
        (20, 56.0, 1240.0, 309.0),
    ];
    for (hp, kw, l, kg) in vrv {
        let fans = if l > 1000.0 { 2 } else { 1 };
        v.push(model(format!("VRV outdoor {hp} HP"), VrfOutdoor, &format!("VRV {hp} HP"), kw, None, (l, 765.0, 1685.0), Some(kg), fans, ""));
    }
    // ---- Split, multi-split and Sky Air outdoor units.
    let split = [
        ("Split outdoor 2-3.5 kW", "RXM 20-35", 3.5, (840.0, 350.0, 552.0), 33.0, 1, "1/4 - 3/8"),
        ("Split outdoor 4.2-5 kW", "RXM 42-50", 5.0, (954.0, 408.0, 734.0), 49.0, 1, "1/4 - 1/2"),
        ("Multi-split outdoor 2 ports 4-5 kW", "2MXM 40-50", 5.0, (852.0, 350.0, 552.0), 41.0, 1, ""),
        ("Multi-split outdoor 2-5 ports 6.8-9 kW", "2-5MXM 68-90", 9.0, (974.0, 408.0, 734.0), 63.0, 1, ""),
        ("Sky Air outdoor 7.1 kW", "RZAG 71", 7.1, (1100.0, 490.0, 870.0), 81.0, 1, ""),
        ("Sky Air outdoor 10-14 kW", "RZAG 100-140", 14.0, (1100.0, 490.0, 870.0), 85.0, 1, ""),
        ("Sky Air outdoor compact 7.1 kW", "RZASG 71", 7.1, (900.0, 350.0, 770.0), 60.0, 1, ""),
        ("Sky Air outdoor compact 10-14 kW", "RZASG 100-140", 14.0, (940.0, 350.0, 990.0), 70.0, 1, ""),
        ("Sky Air outdoor compact 10-14 kW (AZAS)", "AZAS 100-140", 14.0, (940.0, 350.0, 990.0), 70.0, 1, ""),
        ("VRV-S outdoor 4-6 HP", "RXYSA 4-6", 15.5, (1100.0, 500.0, 869.0), 0.0, 1, ""),
    ];
    for (name, reference, kw, dims, kg, fans, conn) in split {
        let kind = if name.starts_with("VRV") { VrfOutdoor } else { SplitOutdoor };
        v.push(model(name.into(), kind, reference, kw, None, dims, (kg > 0.0).then_some(kg), fans, conn));
    }
    // ---- Indoor units.
    let indoor = [
        ("Cassette 600 2.5-6 kW", Cassette, "FFA 25-60", 6.0, (620.0, 620.0, 260.0), 0.0),
        ("Cassette 840 7.1 kW", Cassette, "FCAG 71", 7.1, (950.0, 950.0, 288.0), 21.0),
        ("Cassette 840 10-14 kW", Cassette, "FCAG 100-140", 14.0, (950.0, 950.0, 288.0), 24.0),
        ("Cassette 950 7.1-12.5 kW", Cassette, "FUA 71-125", 12.5, (950.0, 950.0, 198.0), 25.0),
        ("Slim ducted 2.5-3.5 kW", Ducted, "FDXM 25-35", 3.5, (750.0, 620.0, 200.0), 21.0),
        ("Slim ducted 5-6 kW", Ducted, "FDXM 50-60", 6.0, (1150.0, 620.0, 200.0), 30.0),
        ("Ducted 7.1 kW", Ducted, "FBA 71", 7.1, (1000.0, 800.0, 245.0), 35.0),
        ("Ducted 10-14 kW", Ducted, "FBA 100-140", 14.0, (1400.0, 800.0, 245.0), 46.0),
        ("High-pressure ducted 12.5 kW", Ducted, "FDA 125", 12.5, (1400.0, 700.0, 300.0), 45.0),
        ("High-pressure ducted 20 kW", Ducted, "FDA 200", 20.0, (1490.0, 1100.0, 470.0), 104.0),
        ("High-pressure ducted 25 kW", Ducted, "FDA 250", 25.0, (1490.0, 1100.0, 470.0), 115.0),
        ("Wall unit 2-5 kW", WallUnit, "FTXM 20-50", 5.0, (804.0, 252.0, 298.0), 11.5),
        ("Wall unit 6-7.1 kW", WallUnit, "FTXM 60-71", 7.1, (997.0, 292.0, 298.0), 14.5),
        ("Slim wall unit 2-5 kW", WallUnit, "FTXJ 20-50", 5.0, (900.0, 212.0, 305.0), 12.0),
        ("Sky Air wall unit 7.1 kW", WallUnit, "FAA 71", 7.1, (1050.0, 269.0, 290.0), 14.0),
        ("Sky Air wall unit 10 kW", WallUnit, "FAA 100", 10.0, (1200.0, 262.0, 340.0), 18.0),
        ("Ceiling unit 3.5-5 kW", CeilingUnit, "FHA 35-50", 5.0, (960.0, 690.0, 235.0), 0.0),
        ("Ceiling unit 7.1 kW", CeilingUnit, "FHA 71", 7.1, (1270.0, 690.0, 235.0), 34.0),
        ("Ceiling unit 10-14 kW", CeilingUnit, "FHA 100-140", 14.0, (1590.0, 690.0, 235.0), 0.0),
        ("Console 2.5-3.5 kW", FloorUnit, "FVXM 25-35", 3.5, (750.0, 238.0, 600.0), 0.0),
        ("Console 5 kW", FloorUnit, "FVXM 50", 5.0, (750.0, 238.0, 600.0), 0.0),
        ("Ducted 2.5 kW", Ducted, "FDXM", 2.5, (750.0, 620.0, 200.0), 21.0),
        ("Wall unit 2 kW", WallUnit, "FTXM 20", 2.0, (804.0, 252.0, 298.0), 11.5),
        ("Wall unit 2.5 kW", WallUnit, "FTXM 25", 2.5, (804.0, 252.0, 298.0), 11.5),
        ("Wall unit 3.5 kW", WallUnit, "FTXM 35", 3.5, (804.0, 252.0, 298.0), 11.5),
        ("Wall unit 5 kW", WallUnit, "FTXM 50", 5.0, (804.0, 252.0, 298.0), 11.5),
    ];
    for (name, kind, reference, kw, dims, kg) in indoor {
        v.push(model(name.into(), kind, reference, kw, None, dims, (kg > 0.0).then_some(kg), 0, ""));
    }
    v
}

/// The models of `kind`, smallest power first.
pub fn equipment_models_of(models: &[EquipmentModel], kind: EquipmentKind) -> Vec<EquipmentModel> {
    let mut v: Vec<EquipmentModel> = models.iter().filter(|m| m.kind == kind).cloned().collect();
    v.sort_by(|a, b| a.capacity_kw.unwrap_or(0.0).total_cmp(&b.capacity_kw.unwrap_or(0.0)).then(a.name.cmp(&b.name)));
    v
}

/// The smallest model of `kind` that gives `kw` or more, else the largest one.
pub fn equipment_model_for(models: &[EquipmentModel], kind: EquipmentKind, kw: f64) -> Option<EquipmentModel> {
    let of = equipment_models_of(models, kind);
    of.iter().find(|m| m.capacity_kw.is_some_and(|c| c >= kw)).or(of.last()).cloned()
}

/// The model named `name` (case-insensitive).
pub fn equipment_model_named(models: &[EquipmentModel], name: &str) -> Option<EquipmentModel> {
    models.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim())).cloned()
}
