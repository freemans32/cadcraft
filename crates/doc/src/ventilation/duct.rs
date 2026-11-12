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
/// The layer of the supply ducts, and of the extract ducts.
pub const DUCT_LAYER: &str = "M-HVAC-DUCT";
pub const DUCT_EXTRACT_LAYER: &str = "M-HVAC-DUCT-EXTR";
/// Largest side of a unit, mm.
pub const MAX_AHU_SIZE_MM: f64 = 50_000.0;
/// Smallest and largest duct diameter, mm.
pub const DUCT_DIAMETER_MM: (f64, f64) = (20.0, 5_000.0);
/// Largest height available above a false ceiling, mm.
pub const MAX_VOID_MM: f64 = 5_000.0;
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
    /// The supply flow the network asks of the unit, m³/h, once the network is sized. A unit is
    /// double-flux: the supply and the extract networks start from it.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The pressure the unit must supply to the supply network, Pa, once sized.
    #[serde(default)]
    pub pressure_pa: Option<f64>,
    /// The same for the extract network.
    #[serde(default)]
    pub extract_flow_m3h: Option<f64>,
    #[serde(default)]
    pub extract_pressure_pa: Option<f64>,
    /// The height available above the false ceiling where its ducts run, mm: a duct too big for it
    /// when round is made rectangular, as tall as it leaves room for. A space may say its own.
    #[serde(default)]
    pub void_height_mm: Option<f64>,
    /// What is kept back from that height for hangers, insulation and tolerances, mm.
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f64,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

fn default_margin_mm() -> f64 {
    100.0
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
        if self.void_height_mm.is_some_and(|h| !(h.is_finite() && (DUCT_DIAMETER_MM.0..=MAX_VOID_MM).contains(&h))) {
            return Err(invalid(format!("The height available is {} to {MAX_VOID_MM} mm.", DUCT_DIAMETER_MM.0)));
        }
        if !(self.margin_mm.is_finite() && (0.0..=MAX_VOID_MM).contains(&self.margin_mm)) {
            return Err(invalid(format!("The margin is 0 to {MAX_VOID_MM} mm.")));
        }
        for v in [self.flow_m3h, self.pressure_pa, self.extract_flow_m3h, self.extract_pressure_pa].into_iter().flatten() {
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
        // Supply first, extract after; one prefix tells them apart when both are known.
        let flows = [("S", self.flow_m3h), ("E", self.extract_flow_m3h)];
        let both = flows.iter().all(|(_, f)| f.is_some());
        let part = |tag: &str, v: Option<f64>, unit: &str| v.map(|v| if both { format!("{tag} {v:.0}{unit}") } else { format!("{v:.0}{unit}") });
        let flow: Vec<String> = [part("S", self.flow_m3h, ""), part("E", self.extract_flow_m3h, "")].into_iter().flatten().collect();
        if !flow.is_empty() {
            text.push_str(&format!(" {} m³/h", flow.join(" / ")));
        }
        let pa: Vec<String> = [part("S", self.pressure_pa, ""), part("E", self.extract_pressure_pa, "")].into_iter().flatten().collect();
        if !pa.is_empty() {
            text.push_str(&format!(" {} Pa", pa.join(" / ")));
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

impl AhuParams {
    /// The unit as it will look at `at`, turned `rotation`; empty when it can't be drawn.
    pub fn preview(&self, at: Vec2, rotation: f64) -> Vec<crate::Entity> {
        self.generate().map(|p| super::preview_entities(p, at, rotation)).unwrap_or_default()
    }
}

impl DuctParams {
    /// The run from `a` to `b` as it will look; empty when it can't be drawn.
    pub fn preview(&self, a: Vec2, b: Vec2) -> Vec<crate::Entity> {
        let rotation = (b - a).angle();
        let p = DuctParams { length: a.dist(b), ..self.clone() };
        p.generate(reads_upside_down(rotation)).map(|pieces| super::preview_entities(pieces, a, rotation)).unwrap_or_default()
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
    /// The diameter of a round duct; for a rectangular one, the equivalent diameter that loses the
    /// same pressure per metre (set by sizing, or from the sides).
    pub diameter_mm: f64,
    #[serde(default)]
    pub shape: DuctShape,
    /// The sides of a rectangular duct, mm: the width is what a plan shows. Zero for a round duct.
    #[serde(default)]
    pub width_mm: f64,
    #[serde(default)]
    pub height_mm: f64,
    /// Whether the user chose the size: sizing then leaves it.
    #[serde(default)]
    pub imposed: bool,
    #[serde(default)]
    pub kind: DuctKind,
    #[serde(default)]
    pub air: AirDirection,
    /// Whether the user fixed the air direction. When not, sizing sets it from the terminals the
    /// run serves, so a duct to extract terminals is an extract duct without being told.
    #[serde(default)]
    pub air_set: bool,
    /// The flow the run carries, m³/h, once the network is sized.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The air velocity in the run, m/s, once the network is sized.
    #[serde(default)]
    pub velocity: Option<f64>,
    /// A flexible duct (gaine souple): drawn as bellows, with the roughness of one.
    #[serde(default)]
    pub flexible: bool,
    /// A reducer where the run joins the one before it, drawn by sizing when the diameters differ.
    #[serde(default)]
    pub reducer: Option<DuctReducer>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

/// The shape of a duct's section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DuctShape {
    #[default]
    Round,
    Rectangular,
}

impl DuctShape {
    pub fn parse(text: &str) -> Option<DuctShape> {
        match text.trim().to_ascii_lowercase().as_str() {
            "round" | "circular" | "r" | "ronde" => Some(DuctShape::Round),
            "rectangular" | "rect" | "rectangle" | "rectangulaire" => Some(DuctShape::Rectangular),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            DuctShape::Round => "round",
            DuctShape::Rectangular => "rectangular",
        }
    }
}

/// The cone where a duct run joins a larger or smaller one.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuctReducer {
    /// The diameter of the run it joins, mm.
    pub from_mm: f64,
    /// Whether the junction is at the run's far end (`false`: at its start, the block's origin).
    pub at_end: bool,
}

/// Half the angle of a reducer's cone.
const REDUCER_HALF_ANGLE_DEG: f64 = 15.0;
/// Most vertices of a flexible duct's side.
const MAX_BELLOWS_VERTICES: usize = 2_000;

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
        if self.shape == DuctShape::Rectangular {
            let side = |v: f64| v.is_finite() && (DUCT_DIAMETER_MM.0..=DUCT_DIAMETER_MM.1).contains(&v);
            if !(side(self.width_mm) && side(self.height_mm)) {
                return Err(invalid(format!("A rectangular duct's sides are {} to {} mm.", DUCT_DIAMETER_MM.0, DUCT_DIAMETER_MM.1)));
            }
        }
        for v in [self.flow_m3h, self.velocity].into_iter().flatten() {
            if !(v.is_finite() && (0.0..=1e9).contains(&v)) {
                return Err(invalid("A duct's flow and velocity are 0 or more."));
            }
        }
        Ok(())
    }

    /// The width a plan shows, mm: the diameter, or the width of a rectangular duct.
    pub fn plan_width_mm(&self) -> f64 {
        match self.shape {
            DuctShape::Round => self.diameter_mm,
            DuctShape::Rectangular => self.width_mm,
        }
    }

    /// The size as written on the plan: "Ø200" or "400×250".
    pub fn size_label(&self) -> String {
        match self.shape {
            DuctShape::Round => format!("Ø{:.0}", self.diameter_mm),
            DuctShape::Rectangular => format!("{:.0}×{:.0}", self.width_mm, self.height_mm),
        }
    }

    /// The layer of the run: supply and extract ducts have their own, in their own colours.
    pub fn layer(&self) -> &'static str {
        match self.air {
            AirDirection::Supply => DUCT_LAYER,
            AirDirection::Extract => DUCT_EXTRACT_LAYER,
        }
    }

    /// Length in metres.
    pub fn length_m(&self) -> f64 {
        self.length * self.units.mm_per_unit / 1000.0
    }

    /// The run drawn to scale: its two sides and a label (diameter, flow) on it. `flip_label`
    /// turns the label half a turn, so it reads upright on a run drawn leftwards.
    pub fn generate(&self, flip_label: bool) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let r = self.units.to_units(self.plan_width_mm()) / 2.0;
        let layer = self.layer();
        let mut pieces = Vec::new();
        // The cone at the junction, if there is one; the straight part runs between x0 and x1.
        let cone = self.reducer.map(|red| {
            let r0 = self.units.to_units(red.from_mm) / 2.0;
            let len = ((r0 - r).abs() / REDUCER_HALF_ANGLE_DEG.to_radians().tan()).min(0.4 * self.length);
            (r0, len, red.at_end)
        });
        let (x0, x1) = match cone {
            Some((_, len, false)) => (len, self.length),
            Some((_, len, true)) => (0.0, self.length - len),
            None => (0.0, self.length),
        };
        for sign in [1.0, -1.0] {
            let mut pts = Vec::new();
            if let Some((r0, _, false)) = cone {
                pts.push(Vec2::new(0.0, sign * r0));
            }
            pts.extend(self.side_points(x0, x1, sign * r));
            if let Some((r0, _, true)) = cone {
                pts.push(Vec2::new(self.length, sign * r0));
            }
            pieces.push(HvacPiece {
                layer,
                kind: EntityKind::LwPolyline(LwPolyline {
                    vertices: pts.into_iter().map(|p| PolyVertex { p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
                    closed: false,
                    const_width: 0.0,
                    elevation: 0.0,
                    plinegen: false,
                }),
            });
        }
        // A line across where the cone meets the straight run.
        if let Some((_, len, at_end)) = cone {
            let x = if at_end { self.length - len } else { len };
            pieces.push(HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(Vec2::new(x, -r)), b: v3(Vec2::new(x, r)) }) });
        }
        let mut text = self.size_label();
        if let Some(f) = self.flow_m3h {
            text.push_str(&format!(" {f:.0} m³/h"));
        }
        let h = self.units.to_units(self.label_height_mm);
        // A flexible duct is narrow and its bellows busy: its label sits above it.
        let (y, up) = if self.flexible { (r * 1.5 + 1.2 * h, if flip_label { -1.0 } else { 1.0 }) } else { (0.0, 1.0) };
        let mut t = label(Vec2::new(self.length / 2.0, y * up), h, text);
        if let EntityKind::Text(x) = &mut t {
            x.rotation = if flip_label { std::f64::consts::PI } else { 0.0 };
        }
        pieces.push(HvacPiece { layer, kind: t });
        Ok(pieces)
    }

    /// The vertices of one side of the straight part, from `x0` to `x1` at height `y`: two points
    /// for a rigid duct, a zigzag for a flexible one.
    fn side_points(&self, x0: f64, x1: f64, y: f64) -> Vec<Vec2> {
        if !self.flexible || x1 <= x0 {
            return vec![Vec2::new(x0, y), Vec2::new(x1, y)];
        }
        let d = self.units.to_units(self.diameter_mm);
        // Folds a third of a diameter apart, each bulging 30 % of the radius outwards.
        let teeth = (((x1 - x0) / (0.35 * d)).ceil() as usize).clamp(2, MAX_BELLOWS_VERTICES / 2) * 2;
        let amp = 0.3 * (d / 2.0) * y.signum();
        (0..=teeth)
            .map(|i| {
                let x = x0 + (x1 - x0) * i as f64 / teeth as f64;
                Vec2::new(x, if i % 2 == 1 { y + amp } else { y })
            })
            .collect()
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

/// How far from its axis, in diameters, a point may be and still be on a duct: snapping to the
/// lines drawn for a duct lands a radius off its axis.
pub(crate) const AXIS_REACH: f64 = 0.75;

/// Where the duct from `a` to `b` of `diameter` (drawing units) is cut by a branch ending or
/// starting at `at`: the foot of `at` on the axis, when `at` is within [`AXIS_REACH`] diameters (or
/// `tol`) of the axis and more than `tol` from either end. `None` otherwise.
pub(super) fn interior_cut(a: Vec2, b: Vec2, diameter: f64, at: Vec2, tol: f64) -> Option<Vec2> {
    let length = a.dist(b);
    let along = (b - a).normalized();
    let t = (at - a).dot(along);
    let across = (at - a).cross(along).abs();
    (across <= tol.max(AXIS_REACH * diameter) && t > tol && t < length - tol).then(|| a + along * t)
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
        common.layer = p.layer().into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(p.layer());
        let insert = crate::Insert { block, insert: v3(a), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives duct `h` the parameters `p` (its length as it is now), redrawing it in place.
    pub fn regenerate_duct(&mut self, h: crate::Handle, p: &DuctParams) -> Result<()> {
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let EntityKind::Insert(i) = &e.kind else { return Err(invalid("The object is not a block reference.")) };
        let pieces = p.generate(reads_upside_down(i.rotation))?;
        let tags = xdata::replace(DUCT_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        // The run's layer follows its air direction.
        let layer = p.layer();
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
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
    /// duct's interior (see [`interior_cut`]).
    pub fn split_duct_at(&mut self, space: &crate::Space, h: crate::Handle, at: Vec2, tol: f64) -> Result<Option<crate::Handle>> {
        let Some((p, a, b)) = self.duct_ends(h) else { return Ok(None) };
        let Some(cut) = interior_cut(a, b, p.units.to_units(p.plan_width_mm()), at, tol) else { return Ok(None) };
        let common = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.clone();
        let first = DuctParams { length: a.dist(cut), ..p.clone() };
        self.regenerate_duct(h, &first)?;
        let second = self.add_duct(space, common, p, cut, b)?;
        Ok(Some(second))
    }
}
