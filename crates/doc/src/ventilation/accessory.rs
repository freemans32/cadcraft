//! Duct accessories: dampers, fire dampers, silencers and flexible connectors, each a block
//! reference sitting on a duct, drawn to scale across it. Its loss comes from a coefficient ζ (the
//! default of its kind, or the user's) or, when set, a pressure drop in Pa from its data sheet. The
//! defaults are indicative values, not a manufacturer's.

use cadcraft_geom::{PolyVertex, Vec2};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, invalid, label, v3, xdata};
use crate::{EntityKind, Line, LwPolyline, RawTag, Result};

/// The registered application name of an accessory's extended data.
pub const ACCESSORY_APP: &str = "CADCRAFT_DUCTACC";
/// The layer of the accessories.
pub const ACCESSORY_LAYER: &str = "M-HVAC-ACC";
/// Longest accessory along its duct, mm.
pub const MAX_ACCESSORY_LENGTH_MM: f64 = 10_000.0;
/// Largest loss coefficient or pressure drop a user may give.
pub const MAX_ACCESSORY_ZETA: f64 = 1_000.0;
pub const MAX_ACCESSORY_PA: f64 = 10_000.0;
const XDATA_FORMAT: &str = "DUCTACC1";

fn default_label_mm() -> f64 {
    100.0
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessoryKind {
    /// A balancing damper (registre de réglage).
    #[default]
    Damper,
    /// A fire damper (clapet coupe-feu).
    FireDamper,
    Silencer,
    /// A flexible connector (manchette souple).
    FlexConnector,
}

impl AccessoryKind {
    pub const ALL: [AccessoryKind; 4] = [AccessoryKind::Damper, AccessoryKind::FireDamper, AccessoryKind::Silencer, AccessoryKind::FlexConnector];

    pub fn parse(text: &str) -> Option<AccessoryKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "damper" | "d" | "registre" => Some(AccessoryKind::Damper),
            "firedamper" | "fire" | "f" | "ccf" => Some(AccessoryKind::FireDamper),
            "silencer" | "s" | "silencieux" => Some(AccessoryKind::Silencer),
            "flexconnector" | "flex" | "manchette" => Some(AccessoryKind::FlexConnector),
            _ => None,
        }
    }

    /// The name a command takes.
    pub fn name(self) -> &'static str {
        match self {
            AccessoryKind::Damper => "damper",
            AccessoryKind::FireDamper => "fireDamper",
            AccessoryKind::Silencer => "silencer",
            AccessoryKind::FlexConnector => "flexConnector",
        }
    }

    /// The short code written on the plan.
    pub fn code(self) -> &'static str {
        match self {
            AccessoryKind::Damper => "REG",
            AccessoryKind::FireDamper => "CCF",
            AccessoryKind::Silencer => "SIL",
            AccessoryKind::FlexConnector => "MS",
        }
    }

    /// Length along the duct, mm.
    pub fn default_length_mm(self) -> f64 {
        match self {
            AccessoryKind::Damper => 150.0,
            AccessoryKind::FireDamper => 300.0,
            AccessoryKind::Silencer => 1000.0,
            AccessoryKind::FlexConnector => 200.0,
        }
    }

    /// The loss: a coefficient ζ, or a pressure drop in Pa for a silencer.
    pub fn default_loss(self) -> Loss {
        match self {
            AccessoryKind::Damper => Loss::Zeta(0.3),
            AccessoryKind::FireDamper => Loss::Zeta(0.4),
            AccessoryKind::Silencer => Loss::Pascals(30.0),
            AccessoryKind::FlexConnector => Loss::Zeta(0.05),
        }
    }
}

/// How an accessory's loss is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Loss {
    /// A coefficient: ζ times the dynamic pressure of the duct it sits on.
    Zeta(f64),
    /// A pressure drop in Pa, whatever the flow.
    Pascals(f64),
}

/// An accessory: what its block reference keeps. It is centred on the duct's axis at the block's
/// origin, its length along the block's x axis (the duct's direction).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryParams {
    pub kind: AccessoryKind,
    /// Length along the duct, mm.
    pub length_mm: f64,
    /// The diameter of the duct it sits on, mm (it is drawn across it).
    pub diameter_mm: f64,
    /// A loss coefficient set by the user; the default of the kind when neither this nor the
    /// pressure drop is set.
    #[serde(default)]
    pub zeta: Option<f64>,
    /// A pressure drop in Pa from a data sheet; it wins over the coefficient.
    #[serde(default)]
    pub pressure_drop_pa: Option<f64>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl AccessoryParams {
    pub fn validate(&self) -> Result<()> {
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if !(self.length_mm.is_finite() && self.length_mm > 0.0 && self.length_mm <= MAX_ACCESSORY_LENGTH_MM) {
            return Err(invalid(format!("An accessory is longer than 0 and at most {MAX_ACCESSORY_LENGTH_MM} mm.")));
        }
        if !(self.diameter_mm.is_finite() && (super::DUCT_DIAMETER_MM.0..=super::DUCT_DIAMETER_MM.1).contains(&self.diameter_mm)) {
            return Err(invalid("An accessory sits on a duct of 20 to 5000 mm."));
        }
        if self.zeta.is_some_and(|z| !(z.is_finite() && (0.0..=MAX_ACCESSORY_ZETA).contains(&z))) {
            return Err(invalid(format!("An accessory's loss coefficient is 0 to {MAX_ACCESSORY_ZETA}.")));
        }
        if self.pressure_drop_pa.is_some_and(|p| !(p.is_finite() && (0.0..=MAX_ACCESSORY_PA).contains(&p))) {
            return Err(invalid(format!("An accessory's pressure drop is 0 to {MAX_ACCESSORY_PA} Pa.")));
        }
        Ok(())
    }

    /// The loss to size with: the pressure drop if given, else the coefficient, else the kind's default.
    pub fn loss(&self) -> Loss {
        match (self.pressure_drop_pa, self.zeta) {
            (Some(pa), _) => Loss::Pascals(pa),
            (None, Some(z)) => Loss::Zeta(z),
            (None, None) => self.kind.default_loss(),
        }
    }

    /// Half the accessory's length and half the width drawn across the duct, drawing units.
    pub(super) fn half(&self) -> Vec2 {
        let r = self.units.to_units(self.diameter_mm) / 2.0;
        let overhang = (0.2 * r).max(self.units.to_units(20.0));
        // A silencer is wider than its duct.
        let width = if self.kind == AccessoryKind::Silencer { 1.3 * r } else { r } + overhang;
        Vec2::new(self.units.to_units(self.length_mm) / 2.0, width)
    }

    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let half = self.half();
        let r = self.units.to_units(self.diameter_mm) / 2.0;
        let line = |a: Vec2, b: Vec2| HvacPiece { layer: ACCESSORY_LAYER, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let poly = |pts: Vec<Vec2>, closed: bool| HvacPiece {
            layer: ACCESSORY_LAYER,
            kind: EntityKind::LwPolyline(LwPolyline {
                vertices: pts.into_iter().map(|p| PolyVertex { p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
                closed,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            }),
        };
        let corners = vec![Vec2::new(-half.x, -half.y), Vec2::new(half.x, -half.y), Vec2::new(half.x, half.y), Vec2::new(-half.x, half.y)];
        let mut v = Vec::new();
        match self.kind {
            AccessoryKind::Damper => {
                v.push(poly(corners, true));
                // The blade, half open.
                v.push(line(Vec2::new(-half.x * 0.6, -r), Vec2::new(half.x * 0.6, r)));
            }
            AccessoryKind::FireDamper => {
                v.push(poly(corners, true));
                v.push(line(Vec2::new(-half.x, -half.y), Vec2::new(half.x, half.y)));
                v.push(line(Vec2::new(-half.x, half.y), Vec2::new(half.x, -half.y)));
            }
            AccessoryKind::Silencer => {
                v.push(poly(corners, true));
                // The baffles, along the flow.
                for k in [-0.5, 0.0, 0.5] {
                    v.push(line(Vec2::new(-half.x * 0.85, r * k), Vec2::new(half.x * 0.85, r * k)));
                }
            }
            AccessoryKind::FlexConnector => {
                // The two flanges, and a zigzag along each side for the bellows.
                v.push(line(Vec2::new(-half.x, -half.y), Vec2::new(-half.x, half.y)));
                v.push(line(Vec2::new(half.x, -half.y), Vec2::new(half.x, half.y)));
                for side in [-1.0, 1.0] {
                    let teeth = 6;
                    let pts = (0..=teeth)
                        .map(|i| {
                            let x = -half.x + 2.0 * half.x * f64::from(i) / f64::from(teeth);
                            let bump = if i % 2 == 0 { 0.0 } else { 0.18 * r };
                            Vec2::new(x, side * (r + bump))
                        })
                        .collect();
                    v.push(poly(pts, false));
                }
            }
        }
        let h = self.units.to_units(self.label_height_mm);
        let text = match self.loss() {
            Loss::Pascals(p) if self.kind == AccessoryKind::Silencer => format!("{} {p:.0} Pa", self.kind.code()),
            _ => format!("{} Ø{:.0}", self.kind.code(), self.diameter_mm),
        };
        v.push(HvacPiece { layer: ACCESSORY_LAYER, kind: label(Vec2::new(0.0, -half.y - 1.2 * h), h, text) });
        Ok(v)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(ACCESSORY_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<AccessoryParams> {
        let p: AccessoryParams = xdata::from_xdata(ACCESSORY_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

impl AccessoryParams {
    /// The accessory as it will look at `at`, turned `rotation`; empty when it can't be drawn.
    pub fn preview(&self, at: Vec2, rotation: f64) -> Vec<crate::Entity> {
        self.generate().map(|p| super::preview_entities(p, at, rotation)).unwrap_or_default()
    }
}

/// The accessory an object is, when it is a block reference carrying usable accessory data.
pub fn accessory_of(e: &crate::Entity) -> Option<AccessoryParams> {
    match &e.kind {
        EntityKind::Insert(_) => AccessoryParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// The duct that `at` is on, with the foot of `at` on its axis and the duct's direction: the
    /// nearest duct whose axis passes within [`super::AXIS_REACH`] diameters of `at`, along its length.
    pub fn duct_under(&self, at: Vec2) -> Option<(crate::Handle, Vec2, f64)> {
        let units = HvacUnits::of(self);
        let mut best: Option<(f64, crate::Handle, Vec2, f64)> = None;
        for e in self.model.iter() {
            let Some((p, a, b)) = self.duct_ends(e.handle) else { continue };
            let along = (b - a).normalized();
            let t = (at - a).dot(along);
            let across = (at - a).cross(along).abs();
            let reach = units.to_units(10.0).max(super::AXIS_REACH * p.units.to_units(p.diameter_mm));
            if across <= reach && t >= 0.0 && t <= a.dist(b) && best.as_ref().is_none_or(|x| across < x.0) {
                best = Some((across, e.handle, a + along * t, (b - a).angle()));
            }
        }
        best.map(|(_, h, foot, angle)| (h, foot, angle))
    }

    /// Adds accessory `p` on the axis at `at`, turned `rotation` radians (the duct's direction).
    pub fn add_accessory(
        &mut self,
        space: &crate::Space,
        mut common: crate::Common,
        p: &AccessoryParams,
        at: Vec2,
        rotation: f64,
    ) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("An accessory needs a usable position and angle."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(ACCESSORY_APP, &common.xdata, p.to_xdata()?);
        common.layer = ACCESSORY_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(ACCESSORY_LAYER);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives accessory `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_accessory(&mut self, h: crate::Handle, p: &AccessoryParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(ACCESSORY_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }
}
