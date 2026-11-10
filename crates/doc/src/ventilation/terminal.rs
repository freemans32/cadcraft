//! Air terminals: diffusers and grilles drawn to scale, each made from a model of the drawing's
//! air terminal library (its shape, sizes, duct connection, largest flow and pressure drop).

use std::f64::consts::FRAC_1_SQRT_2;

use cadcraft_geom::{PolyVertex, Vec2};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, v3, xdata};
use crate::{Circle, EntityKind, Line, LwPolyline, RawTag, Result};

/// The registered application name of an air terminal's extended data.
pub const AIR_TERMINAL_APP: &str = "CADCRAFT_AIRTERMINAL";
/// The layer of an air terminal's block reference, outline and label.
pub const AIR_TERMINAL_LAYER: &str = "M-HVAC-DIFF";
/// Largest dimension of a terminal, millimetres (10 m).
pub const MAX_TERMINAL_SIZE_MM: f64 = 10_000.0;
/// Largest flow of one terminal model, m³/h.
pub const MAX_TERMINAL_FLOW_M3H: f64 = 100_000.0;
/// Largest pressure drop of one terminal model, Pa.
pub const MAX_TERMINAL_PRESSURE_PA: f64 = 2_000.0;
/// Most slots of a linear diffuser.
pub const MAX_SLOTS: u8 = 8;
const XDATA_FORMAT: &str = "AIRTERMINAL1";

/// The kinds of air terminal: the shape a model is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalKind {
    /// A square ceiling diffuser blowing four ways.
    SquareCeiling,
    Round,
    Rectangular,
    /// A slot diffuser.
    Linear,
    /// A round extract valve.
    Extract,
}

impl TerminalKind {
    pub const ALL: [TerminalKind; 5] =
        [TerminalKind::SquareCeiling, TerminalKind::Round, TerminalKind::Rectangular, TerminalKind::Linear, TerminalKind::Extract];

    /// The name commands take.
    pub fn name(self) -> &'static str {
        match self {
            TerminalKind::SquareCeiling => "squareCeiling",
            TerminalKind::Round => "round",
            TerminalKind::Rectangular => "rectangular",
            TerminalKind::Linear => "linear",
            TerminalKind::Extract => "extract",
        }
    }

    pub fn parse(text: &str) -> Option<TerminalKind> {
        let t = text.trim().to_ascii_lowercase();
        TerminalKind::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(&t) || k.keyword().eq_ignore_ascii_case(&t))
    }

    /// The command-line keyword.
    pub fn keyword(self) -> &'static str {
        match self {
            TerminalKind::SquareCeiling => "Square",
            TerminalKind::Round => "Round",
            TerminalKind::Rectangular => "Grille",
            TerminalKind::Linear => "Linear",
            TerminalKind::Extract => "Extract",
        }
    }
}

/// An air terminal model: one size of one kind.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalModel {
    pub name: String,
    pub kind: TerminalKind,
    /// Face width, mm (the diameter of a round face).
    pub width_mm: f64,
    /// Face depth, mm; equal to the width for square and round faces.
    pub depth_mm: f64,
    /// Slots of a linear diffuser.
    #[serde(default)]
    pub slots: u8,
    /// Diameter of the duct connection, mm.
    pub neck_mm: f64,
    pub max_flow_m3h: f64,
    #[serde(default)]
    pub pressure_drop_pa: f64,
}

impl TerminalModel {
    pub fn validate(&self) -> Result<()> {
        check_name("model", &self.name)?;
        let size = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_TERMINAL_SIZE_MM;
        if !size(self.width_mm) || !size(self.depth_mm) || !size(self.neck_mm) {
            return Err(invalid(format!("A terminal's sizes are more than 0 and at most {MAX_TERMINAL_SIZE_MM} mm.")));
        }
        if !(self.max_flow_m3h.is_finite() && self.max_flow_m3h > 0.0 && self.max_flow_m3h <= MAX_TERMINAL_FLOW_M3H) {
            return Err(invalid(format!("A terminal's largest flow is more than 0 and at most {MAX_TERMINAL_FLOW_M3H} m³/h.")));
        }
        if !(self.pressure_drop_pa.is_finite() && (0.0..=MAX_TERMINAL_PRESSURE_PA).contains(&self.pressure_drop_pa)) {
            return Err(invalid(format!("A terminal's pressure drop is 0 to {MAX_TERMINAL_PRESSURE_PA} Pa.")));
        }
        if self.kind == TerminalKind::Linear && !(1..=MAX_SLOTS).contains(&self.slots) {
            return Err(invalid(format!("A linear diffuser has 1 to {MAX_SLOTS} slots.")));
        }
        Ok(())
    }
}

/// Which way the air goes through a terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AirDirection {
    #[default]
    Supply,
    Extract,
}

impl AirDirection {
    pub fn parse(text: &str) -> Option<AirDirection> {
        match text.trim().to_ascii_lowercase().as_str() {
            "supply" | "s" => Some(AirDirection::Supply),
            "extract" | "return" | "e" | "r" => Some(AirDirection::Extract),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            AirDirection::Supply => "supply",
            AirDirection::Extract => "extract",
        }
    }
}

fn default_label_mm() -> f64 {
    100.0
}

/// An air terminal: what its block reference keeps to regenerate it. The terminal is drawn
/// centred on the block's origin, its width along the block's x axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalParams {
    /// A copy of the model, so the terminal stands on its own.
    pub model: TerminalModel,
    pub flow_m3h: f64,
    #[serde(default)]
    pub air: AirDirection,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

fn line(a: Vec2, b: Vec2) -> HvacPiece {
    HvacPiece { layer: AIR_TERMINAL_LAYER, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) }
}

fn rect(half: Vec2) -> HvacPiece {
    let corners = [Vec2::new(-half.x, -half.y), Vec2::new(half.x, -half.y), Vec2::new(half.x, half.y), Vec2::new(-half.x, half.y)];
    let poly = LwPolyline {
        vertices: corners.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    HvacPiece { layer: AIR_TERMINAL_LAYER, kind: EntityKind::LwPolyline(poly) }
}

fn circle(radius: f64) -> HvacPiece {
    HvacPiece { layer: AIR_TERMINAL_LAYER, kind: EntityKind::Circle(Circle { center: v3(Vec2::ZERO), radius }) }
}

/// An arrow of length `len` along +y from `from`, pointing away (supply) or back (extract).
fn arrow(from: Vec2, len: f64, air: AirDirection) -> Vec<HvacPiece> {
    let to = from + Vec2::new(0.0, len);
    let (tip, back) = match air {
        AirDirection::Supply => (to, -1.0),
        AirDirection::Extract => (from, 1.0),
    };
    let head = len * 0.35;
    vec![line(from, to), line(tip, tip + Vec2::new(-head * 0.6, back * head)), line(tip, tip + Vec2::new(head * 0.6, back * head))]
}

impl TerminalParams {
    pub fn validate(&self) -> Result<()> {
        self.model.validate()?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if !(self.flow_m3h.is_finite() && self.flow_m3h > 0.0 && self.flow_m3h <= MAX_TERMINAL_FLOW_M3H) {
            return Err(invalid(format!("A terminal's flow is more than 0 and at most {MAX_TERMINAL_FLOW_M3H} m³/h.")));
        }
        Ok(())
    }

    /// Half the face's width and depth, in drawing units.
    fn half(&self) -> Vec2 {
        let m = &self.model;
        let depth = match m.kind {
            TerminalKind::SquareCeiling | TerminalKind::Round | TerminalKind::Extract => m.width_mm,
            TerminalKind::Rectangular | TerminalKind::Linear => m.depth_mm,
        };
        Vec2::new(self.units.to_units(m.width_mm) / 2.0, self.units.to_units(depth) / 2.0)
    }

    fn face(&self) -> Vec<HvacPiece> {
        let half = self.half();
        let neck = self.units.to_units(self.model.neck_mm) / 2.0;
        match self.model.kind {
            TerminalKind::SquareCeiling => {
                // The face, the neck, and the four blowing directions from the neck to the corners.
                let mut v = vec![rect(half), circle(neck)];
                for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                    let corner = Vec2::new(sx * half.x, sy * half.y);
                    let from = Vec2::new(sx, sy) * (neck * FRAC_1_SQRT_2);
                    v.push(line(from, corner));
                }
                v
            }
            TerminalKind::Round => vec![circle(half.x), circle(neck)],
            TerminalKind::Extract => {
                let r = half.x * FRAC_1_SQRT_2;
                vec![circle(half.x), line(Vec2::new(-r, -r), Vec2::new(r, r)), line(Vec2::new(-r, r), Vec2::new(r, -r))]
            }
            TerminalKind::Rectangular => {
                let mut v = vec![rect(half)];
                for k in [-0.5, 0.0, 0.5] {
                    v.push(line(Vec2::new(-half.x * 0.9, half.y * k), Vec2::new(half.x * 0.9, half.y * k)));
                }
                v.extend(arrow(Vec2::new(0.0, half.y), half.y.max(half.x * 0.25), self.air));
                v
            }
            TerminalKind::Linear => {
                let mut v = vec![rect(half)];
                let n = f64::from(self.model.slots.max(1));
                for k in 1..self.model.slots {
                    let y = -half.y + 2.0 * half.y * f64::from(k) / n;
                    v.push(line(Vec2::new(-half.x, y), Vec2::new(half.x, y)));
                }
                v.extend(arrow(Vec2::new(0.0, half.y), (half.x * 0.15).max(half.y), self.air));
                v
            }
        }
    }

    /// The face drawn to scale with its symbol, and a label under it: model and flow.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let h = self.units.to_units(self.label_height_mm);
        let mut v = self.face();
        let at = Vec2::new(0.0, -self.half().y - 1.2 * h);
        v.push(HvacPiece { layer: AIR_TERMINAL_LAYER, kind: label(at, h, format!("{} {:.0} m³/h", self.model.name, self.flow_m3h)) });
        Ok(v)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(AIR_TERMINAL_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<TerminalParams> {
        let p: TerminalParams = xdata::from_xdata(AIR_TERMINAL_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The air terminal an object is, when it is a block reference carrying usable terminal data.
pub fn air_terminal_of(e: &crate::Entity) -> Option<TerminalParams> {
    match &e.kind {
        EntityKind::Insert(_) => TerminalParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// The smallest model of `kind` among `models` that carries `flow_m3h`; `None` when none does.
pub fn smallest_model_for(models: &[TerminalModel], kind: TerminalKind, flow_m3h: f64) -> Option<TerminalModel> {
    models
        .iter()
        .filter(|m| m.kind == kind && m.max_flow_m3h >= flow_m3h)
        .min_by(|a, b| a.max_flow_m3h.total_cmp(&b.max_flow_m3h).then(a.width_mm.total_cmp(&b.width_mm)))
        .cloned()
}

impl crate::Drawing {
    /// The drawing's air terminal model `name` (case-insensitive).
    pub fn air_terminal_model(&self, name: &str) -> Option<&TerminalModel> {
        self.air_terminal_models.iter().find(|m| m.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Every model a new terminal can use: the drawing's, then the standard ones it doesn't
    /// replace (a drawing model of the same name wins).
    pub fn available_air_terminal_models(&self) -> Vec<TerminalModel> {
        let mut v = self.air_terminal_models.clone();
        for m in super::standard_air_terminal_models() {
            if self.air_terminal_model(&m.name).is_none() {
                v.push(m);
            }
        }
        v
    }

    /// Adds terminal `p` centred at `at`, its width turned `rotation` radians from the x axis.
    pub fn add_air_terminal(
        &mut self,
        space: &crate::Space,
        mut common: crate::Common,
        p: &TerminalParams,
        at: Vec2,
        rotation: f64,
    ) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("A terminal needs a usable position and angle."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(AIR_TERMINAL_APP, &common.xdata, p.to_xdata()?);
        common.layer = AIR_TERMINAL_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(AIR_TERMINAL_LAYER);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }
}
