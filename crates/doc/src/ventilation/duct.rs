//! Air handling units and round duct runs drawn in plan. A unit is a rectangle with its name and
//! flow; a duct is one straight run, drawn as two lines with its diameter and flow. Each is a
//! block reference with its parameters as extended data, like the other ventilation objects; the
//! network they form is read back from where their ends meet ([`super::DuctNetwork`]), never stored.

use cadcraft_geom::{PolyVertex, Vec2};
use serde::{Deserialize, Serialize};

use super::{AirDirection, HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, v3, xdata};
use crate::{EntityKind, Line, LwPolyline, RawTag, Result};

/// The registered application name of an air handling unit's extended data.
pub const AHU_APP: &str = "CADCRAFT_AHU";
/// The layer of an air handling unit.
pub const AHU_LAYER: &str = "M-HVAC-AHU";
/// The registered application name of a duct's extended data.
pub const DUCT_APP: &str = "CADCRAFT_DUCT";
/// The layer of the ducts.
pub const DUCT_LAYER: &str = "M-HVAC-DUCT";
/// Largest side of a unit, mm.
pub const MAX_AHU_SIZE_MM: f64 = 50_000.0;
/// Smallest and largest duct diameter, mm.
pub const DUCT_DIAMETER_MM: (f64, f64) = (20.0, 5_000.0);
/// Longest duct run, metres.
pub const MAX_DUCT_LENGTH_M: f64 = 10_000.0;
const AHU_FORMAT: &str = "AHU1";
const DUCT_FORMAT: &str = "DUCT1";

fn default_label_mm() -> f64 {
    100.0
}

/// An air handling unit: what its block reference keeps. The unit is centred on the block's
/// origin, its width along the block's x axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AhuParams {
    pub name: String,
    pub width_mm: f64,
    pub depth_mm: f64,
    #[serde(default)]
    pub air: AirDirection,
    /// The flow the network asks of the unit, m³/h, once the network is sized.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The pressure the unit must supply, Pa, once the network is sized.
    #[serde(default)]
    pub pressure_pa: Option<f64>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl AhuParams {
    pub fn validate(&self) -> Result<()> {
        check_name("unit", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        let size = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_AHU_SIZE_MM;
        if !size(self.width_mm) || !size(self.depth_mm) {
            return Err(invalid(format!("A unit's sizes are more than 0 and at most {MAX_AHU_SIZE_MM} mm.")));
        }
        for v in [self.flow_m3h, self.pressure_pa].into_iter().flatten() {
            if !(v.is_finite() && (0.0..=1e9).contains(&v)) {
                return Err(invalid("A unit's flow and pressure are 0 or more."));
            }
        }
        Ok(())
    }

    /// Half the unit's width and depth, in drawing units.
    pub fn half(&self) -> Vec2 {
        Vec2::new(self.units.to_units(self.width_mm) / 2.0, self.units.to_units(self.depth_mm) / 2.0)
    }

    /// The unit drawn to scale: its outline, the crossed diagonals of a fan, and its label.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let half = self.half();
        let corners = [Vec2::new(-half.x, -half.y), Vec2::new(half.x, -half.y), Vec2::new(half.x, half.y), Vec2::new(-half.x, half.y)];
        let outline = LwPolyline {
            vertices: corners.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
            closed: true,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        };
        let line = |a: Vec2, b: Vec2| HvacPiece { layer: AHU_LAYER, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let h = self.units.to_units(self.label_height_mm);
        let mut text = self.name.clone();
        if let Some(f) = self.flow_m3h {
            text.push_str(&format!(" {f:.0} m³/h"));
        }
        if let Some(p) = self.pressure_pa {
            text.push_str(&format!(" {p:.0} Pa"));
        }
        Ok(vec![
            HvacPiece { layer: AHU_LAYER, kind: EntityKind::LwPolyline(outline) },
            line(corners[0], corners[2]),
            line(corners[1], corners[3]),
            HvacPiece { layer: AHU_LAYER, kind: label(Vec2::new(0.0, -half.y - 1.2 * h), h, text) },
        ])
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(AHU_APP, AHU_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<AhuParams> {
        let p: AhuParams = xdata::from_xdata(AHU_APP, AHU_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The air handling unit an object is, when it is a block reference carrying usable unit data.
pub fn ahu_of(e: &crate::Entity) -> Option<AhuParams> {
    match &e.kind {
        EntityKind::Insert(_) => AhuParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// Where a duct sits in the network: it sets its velocity limit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DuctKind {
    /// Decided from the network: a connection to one terminal, a main along the trunk, else a branch.
    #[default]
    Auto,
    Main,
    Branch,
    Connection,
}

impl DuctKind {
    pub fn parse(text: &str) -> Option<DuctKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "auto" | "a" => Some(DuctKind::Auto),
            "main" | "m" | "trunk" => Some(DuctKind::Main),
            "branch" | "b" => Some(DuctKind::Branch),
            "connection" | "c" => Some(DuctKind::Connection),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            DuctKind::Auto => "auto",
            DuctKind::Main => "main",
            DuctKind::Branch => "branch",
            DuctKind::Connection => "connection",
        }
    }
}

/// A duct run: what its block reference keeps. The run starts at the block's origin and goes
/// along the block's x axis for `length`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuctParams {
    /// Length, in drawing units.
    pub length: f64,
    pub diameter_mm: f64,
    /// Whether the user chose the diameter: sizing then leaves it.
    #[serde(default)]
    pub imposed: bool,
    #[serde(default)]
    pub kind: DuctKind,
    #[serde(default)]
    pub air: AirDirection,
    /// The flow the run carries, m³/h, once the network is sized.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The air velocity in the run, m/s, once the network is sized.
    #[serde(default)]
    pub velocity: Option<f64>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl DuctParams {
    pub fn validate(&self) -> Result<()> {
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        let metres = self.length * self.units.mm_per_unit / 1000.0;
        if !(self.length.is_finite() && self.length > 0.0 && metres <= MAX_DUCT_LENGTH_M) {
            return Err(invalid(format!("A duct is longer than 0 and at most {MAX_DUCT_LENGTH_M} m.")));
        }
        if !(self.diameter_mm.is_finite() && (DUCT_DIAMETER_MM.0..=DUCT_DIAMETER_MM.1).contains(&self.diameter_mm)) {
            return Err(invalid(format!("A duct's diameter is {} to {} mm.", DUCT_DIAMETER_MM.0, DUCT_DIAMETER_MM.1)));
        }
        for v in [self.flow_m3h, self.velocity].into_iter().flatten() {
            if !(v.is_finite() && (0.0..=1e9).contains(&v)) {
                return Err(invalid("A duct's flow and velocity are 0 or more."));
            }
        }
        Ok(())
    }

    /// Length in metres.
    pub fn length_m(&self) -> f64 {
        self.length * self.units.mm_per_unit / 1000.0
    }

    /// The run drawn to scale: its two sides and a label (diameter, flow) on it. `flip_label`
    /// turns the label half a turn, so it reads upright on a run drawn leftwards.
    pub fn generate(&self, flip_label: bool) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let r = self.units.to_units(self.diameter_mm) / 2.0;
        let side =
            |y: f64| HvacPiece { layer: DUCT_LAYER, kind: EntityKind::Line(Line { a: v3(Vec2::new(0.0, y)), b: v3(Vec2::new(self.length, y)) }) };
        let mut text = format!("Ø{:.0}", self.diameter_mm);
        if let Some(f) = self.flow_m3h {
            text.push_str(&format!(" {f:.0} m³/h"));
        }
        let mut t = label(Vec2::new(self.length / 2.0, 0.0), self.units.to_units(self.label_height_mm), text);
        if let EntityKind::Text(x) = &mut t {
            x.rotation = if flip_label { std::f64::consts::PI } else { 0.0 };
        }
        Ok(vec![side(r), side(-r), HvacPiece { layer: DUCT_LAYER, kind: t }])
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(DUCT_APP, DUCT_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<DuctParams> {
        let p: DuctParams = xdata::from_xdata(DUCT_APP, DUCT_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The duct an object is, when it is a block reference carrying usable duct data.
pub fn duct_of(e: &crate::Entity) -> Option<DuctParams> {
    match &e.kind {
        EntityKind::Insert(_) => DuctParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// Whether a run turned `rotation` radians reads upside down without a flipped label.
fn reads_upside_down(rotation: f64) -> bool {
    let a = rotation.rem_euclid(std::f64::consts::TAU);
    a > std::f64::consts::FRAC_PI_2 + 1e-9 && a <= 3.0 * std::f64::consts::FRAC_PI_2 + 1e-9
}

impl crate::Drawing {
    /// Adds unit `p` centred at `at`, its width turned `rotation` radians from the x axis.
    pub fn add_ahu(&mut self, space: &crate::Space, mut common: crate::Common, p: &AhuParams, at: Vec2, rotation: f64) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("A unit needs a usable position and angle."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(AHU_APP, &common.xdata, p.to_xdata()?);
        common.layer = AHU_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(AHU_LAYER);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives unit `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_ahu(&mut self, h: crate::Handle, p: &AhuParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(AHU_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }

    /// Adds a duct from `a` to `b`; its `length` is set from the two points.
    pub fn add_duct(&mut self, space: &crate::Space, mut common: crate::Common, mut p: DuctParams, a: Vec2, b: Vec2) -> Result<crate::Handle> {
        if !a.is_finite() || !b.is_finite() {
            return Err(invalid("A duct needs usable end points."));
        }
        p.length = a.dist(b);
        let rotation = (b - a).angle();
        let pieces = p.generate(reads_upside_down(rotation))?;
        common.xdata = xdata::replace(DUCT_APP, &common.xdata, p.to_xdata()?);
        common.layer = DUCT_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(DUCT_LAYER);
        let insert = crate::Insert { block, insert: v3(a), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives duct `h` the parameters `p` (its length as it is now), redrawing it in place.
    pub fn regenerate_duct(&mut self, h: crate::Handle, p: &DuctParams) -> Result<()> {
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let EntityKind::Insert(i) = &e.kind else { return Err(invalid("The object is not a block reference.")) };
        let pieces = p.generate(reads_upside_down(i.rotation))?;
        let tags = xdata::replace(DUCT_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }

    /// Duct `h` with its data and its two ends in drawing coordinates.
    pub fn duct_ends(&self, h: crate::Handle) -> Option<(DuctParams, Vec2, Vec2)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = duct_of(e)?;
        let a = Vec2::new(i.insert.x, i.insert.y);
        let b = a + Vec2::from_angle(i.rotation) * (p.length * i.scale.x);
        Some((p, a, b))
    }

    /// Cuts the duct under `at` in two, when `at` is inside it and not at an end, so a branch can
    /// start there: the first part keeps `h`, the second is new. `None` when `at` is not on the
    /// duct's interior (within `tol` drawing units).
    pub fn split_duct_at(&mut self, space: &crate::Space, h: crate::Handle, at: Vec2, tol: f64) -> Result<Option<crate::Handle>> {
        let Some((p, a, b)) = self.duct_ends(h) else { return Ok(None) };
        let along = (b - a).normalized();
        let t = (at - a).dot(along);
        let across = (at - a).cross(along).abs();
        if !(across <= tol && t > tol && t < a.dist(b) - tol) {
            return Ok(None);
        }
        let cut = a + along * t;
        let common = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.clone();
        let first = DuctParams { length: t, ..p.clone() };
        self.regenerate_duct(h, &first)?;
        let second = self.add_duct(space, common, p, cut, b)?;
        Ok(Some(second))
    }
}
