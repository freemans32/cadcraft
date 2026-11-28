//! Duct accessories: dampers, fire dampers, silencers and flexible connectors, each a block
//! reference sitting on a duct, drawn to scale across it. Its loss comes from a coefficient ζ (the
//! default of its kind, or the user's) or, when set, a pressure drop in Pa from its data sheet. The
//! defaults are indicative values, not a manufacturer's.

use cadcraft_geom::{PolyVertex, Vec2};
use serde::{Deserialize, Serialize};

use super::{AirDirection, HvacPiece, HvacUnits, check_label_height, invalid, label, v3, xdata};
use crate::{EntityKind, Line, LwPolyline, RawTag, Result};

/// The registered application name of an accessory's extended data.
pub const ACCESSORY_APP: &str = "CADCRAFT_DUCTACC";
/// The layer of the accessories.
pub const ACCESSORY_LAYER: &str = "M-HVAC-ACC";
/// The layer of the fire dampers (clapets coupe-feu): a safety device, drawn in red.
pub const ACCESSORY_FIRE_LAYER: &str = "M-HVAC-FIRE";
/// Longest accessory along its duct, mm.
pub const MAX_ACCESSORY_LENGTH_MM: f64 = 10_000.0;
/// Largest loss coefficient or pressure drop a user may give.
pub const MAX_ACCESSORY_ZETA: f64 = 1_000.0;
pub const MAX_ACCESSORY_PA: f64 = 10_000.0;
const XDATA_FORMAT: &str = "DUCTACC1";

fn default_label_mm() -> f64 {
    100.0
}

/// The model of a hood, as a plan shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HoodModel {
    /// A casing with its roof and a mesh on its face (capot pare-pluie).
    #[default]
    Standard,
    /// The same with a fine mesh against birds (grillage anti-volatiles).
    BirdMesh,
    /// A gooseneck: the duct turned back down (col de cygne).
    Gooseneck,
    /// A cap on a duct going up through a roof (chapeau de toiture).
    RoofCap,
}

impl HoodModel {
    pub const ALL: [HoodModel; 4] = [HoodModel::Standard, HoodModel::BirdMesh, HoodModel::Gooseneck, HoodModel::RoofCap];

    pub fn name(self) -> &'static str {
        match self {
            HoodModel::Standard => "standard",
            HoodModel::BirdMesh => "birdMesh",
            HoodModel::Gooseneck => "gooseneck",
            HoodModel::RoofCap => "roofCap",
        }
    }

    /// What the take-off and the Properties call it.
    pub fn label(self) -> &'static str {
        match self {
            HoodModel::Standard => "Hood",
            HoodModel::BirdMesh => "Hood with bird mesh",
            HoodModel::Gooseneck => "Gooseneck",
            HoodModel::RoofCap => "Roof cap",
        }
    }

    /// The code written on the plan.
    pub fn code(self) -> &'static str {
        match self {
            HoodModel::Standard => "CAP",
            HoodModel::BirdMesh => "CAP-GAV",
            HoodModel::Gooseneck => "CDC",
            HoodModel::RoofCap => "CHT",
        }
    }

    pub fn parse(text: &str) -> Option<HoodModel> {
        let t = text.trim().to_ascii_lowercase().replace([' ', '-', '_'], "");
        HoodModel::ALL.into_iter().find(|m| m.name().to_ascii_lowercase() == t || m.code().to_ascii_lowercase().replace('-', "") == t)
    }

    fn is_standard(&self) -> bool {
        *self == HoodModel::Standard
    }
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
    /// A covered outlet or inlet (sortie couverte, capot pare-pluie): a hood with a mesh on its face, at the end of a duct, on a unit's
    /// collar or free against a wall. Drawn from its origin outwards, the mesh at the far end.
    Hood,
}

impl AccessoryKind {
    pub const ALL: [AccessoryKind; 5] =
        [AccessoryKind::Damper, AccessoryKind::FireDamper, AccessoryKind::Silencer, AccessoryKind::FlexConnector, AccessoryKind::Hood];

    pub fn parse(text: &str) -> Option<AccessoryKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "damper" | "d" | "registre" => Some(AccessoryKind::Damper),
            "firedamper" | "fire" | "f" | "ccf" => Some(AccessoryKind::FireDamper),
            "silencer" | "s" | "silencieux" => Some(AccessoryKind::Silencer),
            "flexconnector" | "flex" | "manchette" => Some(AccessoryKind::FlexConnector),
            "hood" | "h" | "capot" | "sortie couverte" | "sortiecouverte" => Some(AccessoryKind::Hood),
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
            AccessoryKind::Hood => "hood",
        }
    }

    /// The short code written on the plan.
    pub fn code(self) -> &'static str {
        match self {
            AccessoryKind::Damper => "REG",
            AccessoryKind::FireDamper => "CCF",
            AccessoryKind::Silencer => "SIL",
            AccessoryKind::FlexConnector => "MS",
            AccessoryKind::Hood => "CAP",
        }
    }

    /// Length along the duct, mm.
    pub fn default_length_mm(self) -> f64 {
        match self {
            AccessoryKind::Damper => 150.0,
            AccessoryKind::FireDamper => 300.0,
            AccessoryKind::Silencer => 1000.0,
            AccessoryKind::FlexConnector => 200.0,
            AccessoryKind::Hood => 600.0,
        }
    }

    /// The loss: a coefficient ζ, or a pressure drop in Pa for a silencer.
    pub fn default_loss(self) -> Loss {
        match self {
            AccessoryKind::Damper => Loss::Zeta(0.3),
            AccessoryKind::FireDamper => Loss::Zeta(0.4),
            AccessoryKind::Silencer => Loss::Pascals(30.0),
            AccessoryKind::FlexConnector => Loss::Zeta(0.05),
            AccessoryKind::Hood => Loss::Zeta(2.5),
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
    /// The altitude of the underside of the element, mm above the drawing's zero, when it is given: written beside it ("Ai:2.50") and kept for the takeoff.
    #[serde(default)]
    pub bottom_mm: Option<f64>,
    #[serde(default)]
    pub units: HvacUnits,
    /// A hood's air (air neuf, rejet…): it is then drawn on the layer, and so in the colour, of the ducts of that air.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub air: Option<AirDirection>,
    /// A hood's height, mm, when its opening is a rectangle (`diameter_mm` is then its width); none for a round one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_mm: Option<f64>,
    /// A hood's size and air were set by hand: it stops following the duct or collar it covers (see [`crate::Drawing::refresh_hoods`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
    /// How far a hood's casing sticks out on each side of its opening, mm; none (0) draws it as wide as the opening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overhang_mm: Option<f64>,
    /// A hood's model.
    #[serde(default, skip_serializing_if = "HoodModel::is_standard")]
    pub hood_model: HoodModel,
}

impl AccessoryParams {
    /// The size as written: "Ø250" or "400×250".
    pub fn size_text(&self) -> String {
        match self.height_mm {
            Some(h) => format!("{:.0}×{h:.0}", self.diameter_mm),
            None => format!("Ø{:.0}", self.diameter_mm),
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_label_height(self.label_height_mm)?;
        super::check_altitude(self.bottom_mm)?;
        self.units.validate()?;
        if !(self.length_mm.is_finite() && self.length_mm > 0.0 && self.length_mm <= MAX_ACCESSORY_LENGTH_MM) {
            return Err(invalid(format!("An accessory is longer than 0 and at most {MAX_ACCESSORY_LENGTH_MM} mm.")));
        }
        if !(self.diameter_mm.is_finite() && (super::DUCT_DIAMETER_MM.0..=super::DUCT_DIAMETER_MM.1).contains(&self.diameter_mm)) {
            return Err(invalid("An accessory sits on a duct of 20 to 5000 mm."));
        }
        if self.height_mm.is_some_and(|h| !(h.is_finite() && (super::DUCT_DIAMETER_MM.0..=super::DUCT_DIAMETER_MM.1).contains(&h))) {
            return Err(invalid("An accessory's height is 20 to 5000 mm."));
        }
        if self.overhang_mm.is_some_and(|o| !(o.is_finite() && (0.0..=2000.0).contains(&o))) {
            return Err(invalid("A hood's overhang is 0 to 2000 mm."));
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
        if self.kind == AccessoryKind::Hood {
            // As wide as the opening it covers, plus the overhang asked for.
            let out = self.units.to_units(self.overhang_mm.unwrap_or(0.0));
            return Vec2::new(self.units.to_units(self.length_mm) / 2.0, r + out);
        }
        let overhang = (0.2 * r).max(self.units.to_units(20.0));
        // A silencer is wider than its duct.
        let width = if self.kind == AccessoryKind::Silencer { 1.3 * r } else { r } + overhang;
        Vec2::new(self.units.to_units(self.length_mm) / 2.0, width)
    }

    /// The layer of the accessory: its own for a fire damper, the accessories' for the others.
    pub fn layer(&self) -> &'static str {
        match (self.kind, self.air) {
            (AccessoryKind::FireDamper, _) => ACCESSORY_FIRE_LAYER,
            // A hood with an air is drawn like the ducts of that air.
            (AccessoryKind::Hood, Some(AirDirection::Supply)) => super::DUCT_LAYER,
            (AccessoryKind::Hood, Some(AirDirection::Extract)) => super::DUCT_EXTRACT_LAYER,
            (AccessoryKind::Hood, Some(AirDirection::Fresh)) => super::DUCT_FRESH_LAYER,
            (AccessoryKind::Hood, Some(AirDirection::Exhaust)) => super::DUCT_EXHAUST_LAYER,
            _ => ACCESSORY_LAYER,
        }
    }

    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let half = self.half();
        let r = self.units.to_units(self.diameter_mm) / 2.0;
        let layer = super::by_block(self.layer());
        let line = |a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let poly = |pts: Vec<Vec2>, closed: bool| HvacPiece {
            layer,
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
            AccessoryKind::Hood if self.hood_model != HoodModel::Standard && self.hood_model != HoodModel::BirdMesh => {
                v.extend(self.hood_shape(half).into_iter().map(|(pts, closed)| poly(pts, closed)));
            }
            AccessoryKind::Hood => {
                // From the origin (the end of the duct) outwards: the casing, then the mesh band at the far end as crossed diagonals (finer against birds).
                let len = 2.0 * half.x;
                let casing = vec![Vec2::new(0.0, -half.y), Vec2::new(len, -half.y), Vec2::new(len, half.y), Vec2::new(0.0, half.y)];
                v.push(poly(casing, true));
                let edge = 0.78 * len;
                v.push(line(Vec2::new(edge, -half.y), Vec2::new(edge, half.y)));
                // The roof's slope, seen from above: a line from each rear corner to the mesh.
                v.push(line(Vec2::new(0.0, -half.y), Vec2::new(edge, -0.6 * half.y)));
                v.push(line(Vec2::new(0.0, half.y), Vec2::new(edge, 0.6 * half.y)));
                let cells = if self.hood_model == HoodModel::BirdMesh { 8 } else { 4 };
                for k in 0..cells {
                    let y0 = -half.y + 2.0 * half.y * f64::from(k) / f64::from(cells);
                    let y1 = y0 + 2.0 * half.y / f64::from(cells);
                    v.push(line(Vec2::new(edge, y0), Vec2::new(len, y1)));
                    v.push(line(Vec2::new(edge, y1), Vec2::new(len, y0)));
                }
            }
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
            // A hood's size and length, then its air: "CAP 400×250 L600 R".
            _ if self.kind == AccessoryKind::Hood => {
                let air = self.air.map_or(String::new(), |a| format!(" {}", a.tag()));
                format!("{} {} L{:.0}{air}", self.hood_model.code(), self.size_text(), self.length_mm)
            }
            _ => format!("{} Ø{:.0}", self.kind.code(), self.diameter_mm),
        };
        let text = match super::altitude_tag(self.bottom_mm) {
            Some(t) => format!("{text} {t}"),
            None => text,
        };
        // Under the accessory; a roof cap is centred on its duct and wider than it.
        let roof_cap = self.kind == AccessoryKind::Hood && self.hood_model == HoodModel::RoofCap;
        let label_x = if self.kind == AccessoryKind::Hood && !roof_cap { half.x } else { 0.0 };
        let below = if roof_cap { 1.6 * half.y } else { half.y };
        v.push(HvacPiece { layer, kind: label(Vec2::new(label_x, -below - 1.2 * h), h, text) });
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
    /// The outline of a gooseneck or a roof cap, from its origin (the end of the duct) outwards, as polylines (points, closed).
    fn hood_shape(&self, half: Vec2) -> Vec<(Vec<Vec2>, bool)> {
        let (len, w) = (2.0 * half.x, half.y);
        let arc = |cx: f64, r: f64, from: f64, to: f64| -> Vec<Vec2> {
            (0..=16).map(|k| from + (to - from) * f64::from(k) / 16.0).map(|a| Vec2::new(cx + r * a.cos(), r * a.sin())).collect()
        };
        match self.hood_model {
            // The duct goes on, then turns back on itself: its two sides and the half circle of the bend, seen from above.
            HoodModel::Gooseneck => {
                let straight = (len - w).max(0.0);
                let mut outer = vec![Vec2::new(0.0, -w), Vec2::new(straight, -w)];
                outer.extend(arc(straight, w, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2));
                outer.push(Vec2::new(0.0, w));
                let inner = vec![Vec2::new(straight, -w), Vec2::new(straight, w)];
                vec![(outer, false), (inner, false)]
            }
            // The cap, wider than the duct, seen from above: its rim and the duct under it.
            HoodModel::RoofCap => {
                let rim = arc(0.0, 1.6 * w, 0.0, std::f64::consts::TAU);
                let duct = arc(0.0, w, 0.0, std::f64::consts::TAU);
                vec![(rim, true), (duct, true)]
            }
            _ => Vec::new(),
        }
    }

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

/// Where a hood goes: the point it starts from (the end of a duct, or of a unit's collar), the direction it points
/// out (radians) and the width of the opening it covers, mm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HoodAnchor {
    pub origin: Vec2,
    pub angle: f64,
    pub width_mm: f64,
    /// The duct it covers the end of, when it is on one: its loss is then in that duct's network.
    pub duct: Option<crate::Handle>,
    /// The height of a rectangular opening, mm.
    pub height_mm: Option<f64>,
    /// The air of the duct or collar, when it is known.
    pub air: Option<AirDirection>,
}

/// The width of a collar whose size is not given, mm.
const DEFAULT_COLLAR_MM: f64 = 400.0;
/// How far inside the end of a duct a hood starts, mm: so that the hood is read as sitting on that duct.
const HOOD_NUDGE_MM: f64 = 1.0;
/// How near a unit's collar `at` has to be to pick it, mm.
const COLLAR_REACH_MM: f64 = 300.0;

impl crate::Drawing {
    /// The opening a hood put at `at` covers: the nearer end of the duct under `at`, else the collar of a unit within reach.
    /// `None` when there is neither (the hood then goes free, where the user put it). A duct wins: one that starts at a collar
    /// is covered at its own end, not at the collar it leaves.
    pub fn hood_anchor(&self, at: Vec2) -> Option<HoodAnchor> {
        let units = HvacUnits::of(self);
        if let Some((h, ..)) = self.duct_under(at)
            && let Some((p, a, b)) = self.duct_ends(h)
        {
            let towards_b = at.dist(b) <= at.dist(a);
            let (end, outward) = if towards_b { (b, (b - a).normalized()) } else { (a, (a - b).normalized()) };
            let height_mm = (p.shape == super::DuctShape::Rectangular).then_some(p.height_mm);
            return Some(HoodAnchor {
                origin: end - outward * units.to_units(HOOD_NUDGE_MM),
                angle: outward.angle(),
                width_mm: p.plan_width_mm(),
                duct: Some(h),
                height_mm,
                air: Some(p.air),
            });
        }
        let reach = units.to_units(COLLAR_REACH_MM);
        let mut best: Option<(f64, HoodAnchor)> = None;
        for e in self.model.iter() {
            let (EntityKind::Insert(i), Some(p)) = (&e.kind, super::ahu_of(e)) else { continue };
            let (s, c) = i.rotation.sin_cos();
            let turn = |v: Vec2| Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c);
            let base = Vec2::new(i.insert.x, i.insert.y);
            for (idx, (tip, normal)) in p.port_tips().into_iter().enumerate() {
                let world = base + turn(tip);
                let d = world.dist(at);
                if d <= reach && best.as_ref().is_none_or(|b| d < b.0) {
                    let size = p.port_sizes.get(idx).copied().flatten();
                    let width = size.map_or(DEFAULT_COLLAR_MM, |z| z.width_mm());
                    let height_mm = match size {
                        Some(super::RiserSize::Rectangular { height_mm, .. }) => Some(height_mm),
                        _ => None,
                    };
                    let air = p.roles().get(idx).copied();
                    best = Some((d, HoodAnchor { origin: world, angle: turn(normal).angle(), width_mm: width, duct: None, height_mm, air }));
                }
            }
        }
        best.map(|(_, a)| a)
    }

    /// Gives each hood that follows its opening (not `fixed`) the width, height and air of the duct or collar it covers, when
    /// they changed. A hood is told from other objects by the opening being where it starts. Returns how many changed.
    pub fn refresh_hoods(&mut self) -> Result<usize> {
        let hoods: Vec<(crate::Handle, AccessoryParams, Vec2)> = self
            .model
            .iter()
            .filter_map(|e| {
                let a = accessory_of(e).filter(|a| a.kind == AccessoryKind::Hood && !a.fixed)?;
                let EntityKind::Insert(i) = &e.kind else { return None };
                Some((e.handle, a, Vec2::new(i.insert.x, i.insert.y)))
            })
            .collect();
        let mut changed = 0;
        for (h, mut a, at) in hoods {
            let Some(found) = self.hood_anchor(at) else { continue };
            // The opening it covers has to be where the hood starts, not another one near it.
            if found.origin.dist(at) > a.units.to_units(5.0) {
                continue;
            }
            if (a.diameter_mm, a.height_mm) == (found.width_mm, found.height_mm) && a.air == found.air {
                continue;
            }
            (a.diameter_mm, a.height_mm, a.air) = (found.width_mm, found.height_mm, found.air);
            if a.validate().is_ok() {
                self.regenerate_accessory(h, &a)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

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
            let reach = units.to_units(10.0).max(super::AXIS_REACH * p.units.to_units(p.plan_width_mm()));
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
        common.layer = p.layer().into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(p.layer());
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives accessory `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_accessory(&mut self, h: crate::Handle, p: &AccessoryParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(ACCESSORY_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = p.layer();
        self.ensure_layer(layer);
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
    }
}
