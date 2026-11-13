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
pub const DUCT_LAYER: &str = "M-HVAC-DUCT-SUP";
pub const DUCT_EXTRACT_LAYER: &str = "M-HVAC-DUCT-EXTR";
/// The layers of the fresh air (air neuf) and the exhaust air (rejet) ducts.
pub const DUCT_FRESH_LAYER: &str = "M-HVAC-DUCT-FRESH";
pub const DUCT_EXHAUST_LAYER: &str = "M-HVAC-DUCT-EXH";
/// Largest side of a unit, mm.
pub const MAX_AHU_SIZE_MM: f64 = 50_000.0;
/// Smallest and largest duct diameter, mm.
pub const DUCT_DIAMETER_MM: (f64, f64) = (20.0, 5_000.0);
/// Largest height available above a false ceiling, mm.
pub const MAX_VOID_MM: f64 = 5_000.0;
/// Most stretches one duct is drawn broken over, where it passes under others.
pub const MAX_GAPS: usize = 32;
/// Most jogs (a rise or a dip round another duct, four 45° elbows each) one duct makes.
pub const MAX_JOGS: usize = 8;

/// A duct that goes over or under another by four 45° elbows: it leaves its line, runs level past
/// the other duct, then comes back. The plan shows the four elbows across it; the flat part lies
/// between the second and the third.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuctJog {
    /// The middle of the flat part, mm along the duct from its start.
    pub centre_mm: f64,
    /// Half the length of the flat part, mm.
    pub half_mm: f64,
    /// How far the duct rises or dips, mm: at 45° the rise takes as much length, on each side.
    pub shift_mm: f64,
    /// Whether it dips (goes under) rather than rises.
    #[serde(default)]
    pub down: bool,
}

impl DuctJog {
    /// The positions of the four elbows, mm along the duct.
    pub fn elbows_mm(&self) -> [f64; 4] {
        [
            self.centre_mm - self.half_mm - self.shift_mm,
            self.centre_mm - self.half_mm,
            self.centre_mm + self.half_mm,
            self.centre_mm + self.half_mm + self.shift_mm,
        ]
    }
}
/// Longest duct run, metres.
pub const MAX_DUCT_LENGTH_M: f64 = 10_000.0;
const AHU_FORMAT: &str = "AHU1";
const DUCT_FORMAT: &str = "DUCT1";

fn default_label_mm() -> f64 {
    100.0
}

/// What kind of unit an air handling unit is: it sets how it is drawn and how big it starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AhuKind {
    /// A double-flux unit (CTA double flux): supply and extract in one box.
    #[default]
    DoubleFlow,
    /// A single-flux supply unit (CTA simple flux soufflage).
    SupplyUnit,
    /// A single-flux extract unit (CTA simple flux extraction).
    ExtractUnit,
    /// A ventilation box (caisson VMC).
    VmcBox,
    /// A compact double-flux unit with its collars stacked at each end (CTA double flux compacte,
    /// "verticale"): at each end the upper collar is over the lower one, which a plan shows side
    /// by side and close together. Extract over supply on one end, exhaust over fresh air on the other.
    StackedDoubleFlow,
}

impl AhuKind {
    pub const ALL: [AhuKind; 5] = [AhuKind::DoubleFlow, AhuKind::SupplyUnit, AhuKind::ExtractUnit, AhuKind::VmcBox, AhuKind::StackedDoubleFlow];

    pub fn parse(text: &str) -> Option<AhuKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "doubleflow" | "doubleflux" | "double" | "df" | "dual" => Some(AhuKind::DoubleFlow),
            "supplyunit" | "supply" | "soufflage" | "s" => Some(AhuKind::SupplyUnit),
            "extractunit" | "extract" | "extraction" | "e" => Some(AhuKind::ExtractUnit),
            "vmcbox" | "vmc" | "box" | "caisson" => Some(AhuKind::VmcBox),
            "stackeddoubleflow" | "stacked" | "compact" | "compacte" | "vertical" | "verticale" => Some(AhuKind::StackedDoubleFlow),
            _ => None,
        }
    }

    /// The name a command takes.
    pub fn name(self) -> &'static str {
        match self {
            AhuKind::DoubleFlow => "doubleFlow",
            AhuKind::SupplyUnit => "supplyUnit",
            AhuKind::ExtractUnit => "extractUnit",
            AhuKind::VmcBox => "vmcBox",
            AhuKind::StackedDoubleFlow => "stackedDoubleFlow",
        }
    }

    /// The short code written on the unit.
    pub fn code(self) -> &'static str {
        match self {
            AhuKind::DoubleFlow => "CTA DF",
            AhuKind::SupplyUnit => "CTA S",
            AhuKind::ExtractUnit => "CTA E",
            AhuKind::VmcBox => "VMC",
            AhuKind::StackedDoubleFlow => "CTA DF",
        }
    }

    /// The names of its connections, in the order [`AhuParams::ports`] lists them.
    pub fn port_names(self) -> &'static [&'static str] {
        match self {
            AhuKind::DoubleFlow | AhuKind::StackedDoubleFlow => &["Left top", "Left bottom", "Right top", "Right bottom"],
            AhuKind::SupplyUnit | AhuKind::ExtractUnit => &["Left", "Right"],
            AhuKind::VmcBox => &["Left", "Right", "Top left", "Top right"],
        }
    }

    /// The direction of the air at each connection by default: a double-flux unit takes fresh air in
    /// and rejects exhaust on its left and supplies and extracts on its right.
    pub fn default_ports(self) -> Vec<AirDirection> {
        use AirDirection::{Exhaust, Extract, Fresh, Supply};
        match self {
            AhuKind::DoubleFlow => vec![Fresh, Exhaust, Supply, Extract],
            AhuKind::SupplyUnit => vec![Fresh, Supply],
            AhuKind::ExtractUnit => vec![Extract, Exhaust],
            // A VMC box only extracts and exhausts: air comes in by an extract collar and goes out by an exhaust one.
            AhuKind::VmcBox => vec![Extract, Exhaust, Extract, Exhaust],
            AhuKind::StackedDoubleFlow => vec![Exhaust, Fresh, Extract, Supply],
        }
    }

    /// Width and depth a new unit of the kind starts with, mm.
    pub fn default_size_mm(self) -> (f64, f64) {
        match self {
            AhuKind::DoubleFlow => (2000.0, 1000.0),
            AhuKind::SupplyUnit | AhuKind::ExtractUnit => (2500.0, 900.0),
            AhuKind::VmcBox => (700.0, 550.0),
            AhuKind::StackedDoubleFlow => (1600.0, 1510.0),
        }
    }
}

/// An air handling unit: what its block reference keeps. The unit is centred on the block's
/// origin, its width along the block's x axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AhuParams {
    pub name: String,
    #[serde(default)]
    pub kind: AhuKind,
    /// The direction of the air at each connection, in the order of [`AhuKind::port_names`]; empty:
    /// the kind's own ([`AhuKind::default_ports`]). A duct that starts at a connection takes its
    /// direction, so the four ducts of a double-flux unit need no telling.
    #[serde(default)]
    pub ports: Vec<AirDirection>,
    pub width_mm: f64,
    pub depth_mm: f64,
    /// The height of the casing, mm, when it is known: written on the unit's spec block (the plan shows no height).
    #[serde(default)]
    pub height_mm: Option<f64>,
    /// The make and model ("CIAT FLOWAY RHE 5000"), written on the spec block.
    #[serde(default)]
    pub model: String,
    /// The weight, kg, written on the spec block.
    #[serde(default)]
    pub weight_kg: Option<f64>,
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
    /// And for the fresh air (air neuf) coming in and the exhaust air (rejet) going out.
    #[serde(default)]
    pub fresh_flow_m3h: Option<f64>,
    #[serde(default)]
    pub fresh_pressure_pa: Option<f64>,
    #[serde(default)]
    pub exhaust_flow_m3h: Option<f64>,
    #[serde(default)]
    pub exhaust_pressure_pa: Option<f64>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl AhuParams {
    /// The direction of the air at each connection: the ones set, else the kind's.
    pub fn roles(&self) -> Vec<AirDirection> {
        if self.ports.len() == self.kind.default_ports().len() { self.ports.clone() } else { self.kind.default_ports() }
    }

    /// Where each connection ends, in the block's coordinates, and the direction it points out of
    /// the unit, in the order of [`AhuKind::port_names`].
    pub fn port_tips(&self) -> Vec<(Vec2, Vec2)> {
        let half = self.half();
        let (hx, hy) = (half.x, half.y);
        let stick = 0.13 * hy.min(hx);
        let (l, r, t) = (Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0));
        match self.kind {
            AhuKind::DoubleFlow => vec![
                (Vec2::new(-hx - stick, 0.5 * hy), l),
                (Vec2::new(-hx - stick, -0.5 * hy), l),
                (Vec2::new(hx + stick, 0.5 * hy), r),
                (Vec2::new(hx + stick, -0.5 * hy), r),
            ],
            AhuKind::SupplyUnit | AhuKind::ExtractUnit => vec![(Vec2::new(-hx - stick, 0.0), l), (Vec2::new(hx + stick, 0.0), r)],
            // The two collars of an end are one over the other: a plan sees them at the same point.
            AhuKind::StackedDoubleFlow => {
                vec![
                    (Vec2::new(-hx - stick, 0.0), l),
                    (Vec2::new(-hx - stick, 0.0), l),
                    (Vec2::new(hx + stick, 0.0), r),
                    (Vec2::new(hx + stick, 0.0), r),
                ]
            }
            AhuKind::VmcBox => {
                vec![
                    (Vec2::new(-hx - stick, 0.0), l),
                    (Vec2::new(hx + stick, 0.0), r),
                    (Vec2::new(-0.5 * hx, hy + stick), t),
                    (Vec2::new(0.5 * hx, hy + stick), t),
                ]
            }
        }
    }

    /// An arrow and a tag at each connection: air coming in (fresh air, extract) points at the unit,
    /// air going out (supply, exhaust) away from it, each on the layer and in the colour of its direction.
    fn port_marks(&self, label_height: f64) -> Vec<HvacPiece> {
        let half = self.half();
        let len = 0.45 * half.y.min(half.x);
        let gap = 0.1 * half.y.min(half.x);
        let mut v = Vec::new();
        for (idx, ((mut tip, normal), role)) in self.port_tips().into_iter().zip(self.roles()).enumerate() {
            // Stacked collars share a point, so their marks are drawn apart: the upper one above, the lower one below.
            if self.kind == AhuKind::StackedDoubleFlow {
                tip.y += if idx % 2 == 0 { 0.22 * half.y } else { -0.22 * half.y };
            }
            let layer = match role {
                AirDirection::Supply => super::AIR_SUPPLY_LAYER,
                AirDirection::Extract => super::AIR_EXTRACT_LAYER,
                AirDirection::Fresh => super::AIR_FRESH_LAYER,
                AirDirection::Exhaust => super::AIR_EXHAUST_LAYER,
            };
            let from = tip + normal * gap;
            let to = from + normal * len;
            let line = |a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
            // The head at the far end for air going out, at the unit's end for air coming in.
            let (head_at, back) = if role.leaves_the_unit() { (to, -normal) } else { (from, normal) };
            let head = len * 0.3;
            let side = normal.perp() * (head * 0.6);
            v.extend([line(from, to), line(head_at, head_at + back * head + side), line(head_at, head_at + back * head - side)]);
            v.push(HvacPiece { layer, kind: label(to + normal * (1.2 * label_height), label_height * 0.8, role.tag().into()) });
        }
        v
    }

    /// The flow and the pressure sized for a direction, if it has been.
    pub fn figures(&self, air: AirDirection) -> (Option<f64>, Option<f64>) {
        match air {
            AirDirection::Supply => (self.flow_m3h, self.pressure_pa),
            AirDirection::Extract => (self.extract_flow_m3h, self.extract_pressure_pa),
            AirDirection::Fresh => (self.fresh_flow_m3h, self.fresh_pressure_pa),
            AirDirection::Exhaust => (self.exhaust_flow_m3h, self.exhaust_pressure_pa),
        }
    }

    /// Keeps the flow and the pressure sized for a direction.
    pub fn set_figures(&mut self, air: AirDirection, flow: f64, pressure: f64) {
        let (f, p) = (Some(flow), Some(pressure));
        match air {
            AirDirection::Supply => (self.flow_m3h, self.pressure_pa) = (f, p),
            AirDirection::Extract => (self.extract_flow_m3h, self.extract_pressure_pa) = (f, p),
            AirDirection::Fresh => (self.fresh_flow_m3h, self.fresh_pressure_pa) = (f, p),
            AirDirection::Exhaust => (self.exhaust_flow_m3h, self.exhaust_pressure_pa) = (f, p),
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_name("unit", &self.name)?;
        if !(self.ports.is_empty() || self.ports.len() == self.kind.default_ports().len()) {
            return Err(invalid(format!("A {} has {} connections.", self.kind.code(), self.kind.default_ports().len())));
        }
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        let size = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_AHU_SIZE_MM;
        if !size(self.width_mm) || !size(self.depth_mm) {
            return Err(invalid(format!("A unit's sizes are more than 0 and at most {MAX_AHU_SIZE_MM} mm.")));
        }
        if self.height_mm.is_some_and(|v| !size(v)) || self.weight_kg.is_some_and(|v| !(v.is_finite() && v > 0.0 && v <= 1e7)) {
            return Err(invalid("A unit's height and weight are more than 0 (the height at most 100 000 mm)."));
        }
        if self.model.chars().count() > super::MAX_HVAC_NAME_CHARS || self.model.chars().any(char::is_control) {
            return Err(invalid(format!("A unit's model has at most {} characters, without control characters.", super::MAX_HVAC_NAME_CHARS)));
        }
        for v in AirDirection::ALL
            .into_iter()
            .flat_map(|a| {
                let (f, p) = self.figures(a);
                [f, p]
            })
            .flatten()
        {
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
        let h = self.units.to_units(self.label_height_mm);
        // The name and, for each direction sized, its flow and pressure: "S 1200 / E 1100 m³/h ...".
        // One prefix per direction tells them apart when there are several.
        let mut text = self.name.clone();
        let known: Vec<(AirDirection, f64, Option<f64>)> =
            AirDirection::ALL.into_iter().filter_map(|a| self.figures(a).0.map(|f| (a, f, self.figures(a).1))).collect();
        let tagged = |a: AirDirection, v: f64| if known.len() > 1 { format!("{} {v:.0}", a.tag()) } else { format!("{v:.0}") };
        if !known.is_empty() {
            let flows: Vec<String> = known.iter().map(|(a, f, _)| tagged(*a, *f)).collect();
            text.push_str(&format!(" {} m³/h", flows.join(" / ")));
            let pressures: Vec<String> = known.iter().filter_map(|(a, _, p)| p.map(|p| tagged(*a, p))).collect();
            if !pressures.is_empty() {
                text.push_str(&format!(" {} Pa", pressures.join(" / ")));
            }
        }
        let mut pieces = self.body(half);
        // The kind's code inside the first compartment, at the top (above a box, which has no room);
        // the name and the figures below the unit.
        let code_at = match self.kind {
            AhuKind::DoubleFlow | AhuKind::StackedDoubleFlow => Vec2::new(-half.x / 2.0, half.y * 0.8),
            AhuKind::VmcBox => Vec2::new(0.0, half.y + 1.6 * h),
            _ => Vec2::new(0.0, half.y * 0.8),
        };
        pieces.push(HvacPiece { layer: AHU_LAYER, kind: label(code_at, h, self.kind.code().into()) });
        pieces.push(HvacPiece { layer: AHU_LAYER, kind: label(Vec2::new(0.0, -half.y - 1.2 * h), h, text) });
        pieces.extend(self.port_marks(h));
        // The spec block to the right, beyond the collars' arrows, its lines under one another.
        let x = half.x + 0.9 * half.x.min(half.y) + 3.0 * h;
        for (i, line) in self.spec_lines().into_iter().enumerate() {
            pieces.push(HvacPiece { layer: AHU_LAYER, kind: super::label_left(Vec2::new(x, half.y - (i as f64) * 1.6 * h - h), h, line) });
        }
        Ok(pieces)
    }

    /// The spec block written beside the unit, one line each: the make and model, "Dim. LxPxH" (width
    /// x depth x height) and the weight; only what is known.
    pub fn spec_lines(&self) -> Vec<String> {
        let mut v = Vec::new();
        if !self.model.trim().is_empty() {
            v.push(self.model.trim().to_string());
        }
        if let Some(h) = self.height_mm {
            v.push(format!("Dim. {:.0}x{:.0}x{:.0}H", self.width_mm, self.depth_mm, h));
        }
        if let Some(w) = self.weight_kg {
            v.push(format!("Poids {w:.0}kg"));
        }
        v
    }

    /// The casing, what is inside it and the collars its ducts connect to, as the kind has them.
    fn body(&self, half: Vec2) -> Vec<HvacPiece> {
        let (hx, hy) = (half.x, half.y);
        let rect = |cx: f64, cy: f64, w: f64, d: f64| {
            let corners = [(cx - w, cy - d), (cx + w, cy - d), (cx + w, cy + d), (cx - w, cy + d)];
            HvacPiece {
                layer: AHU_LAYER,
                kind: EntityKind::LwPolyline(LwPolyline {
                    vertices: corners.iter().map(|&(x, y)| PolyVertex { p: Vec2::new(x, y), bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
                    closed: true,
                    const_width: 0.0,
                    elevation: 0.0,
                    plinegen: false,
                }),
            }
        };
        let line = |a: Vec2, b: Vec2| HvacPiece { layer: AHU_LAYER, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        // A fan: its housing and three blades.
        let fan = |c: Vec2, r: f64| {
            let mut v = vec![HvacPiece { layer: AHU_LAYER, kind: EntityKind::Circle(crate::Circle { center: v3(c), radius: r }) }];
            for k in 0..3 {
                let a = f64::from(k) * std::f64::consts::TAU / 3.0;
                v.push(line(c, c + Vec2::from_angle(a) * (0.9 * r)));
                v.push(line(c + Vec2::from_angle(a) * (0.9 * r), c + Vec2::from_angle(a + 0.7) * (0.55 * r)));
            }
            v
        };
        // A collar on a side: a short rectangle sticking out of the casing.
        let stick = 0.13 * hy.min(hx);
        let side_collar = |right: bool, y: f64, half_size: f64| {
            let x = if right { hx + stick / 2.0 } else { -hx - stick / 2.0 };
            rect(x, y, stick / 2.0, half_size)
        };
        let inset = 0.04 * hy.min(hx);
        let mut v = vec![rect(0.0, 0.0, hx, hy), rect(0.0, 0.0, hx - inset, hy - inset)];
        match self.kind {
            AhuKind::DoubleFlow | AhuKind::StackedDoubleFlow => {
                // Two compartments, a fan in each, and two collars on each short side.
                v.push(line(Vec2::new(0.0, -(hy - inset)), Vec2::new(0.0, hy - inset)));
                let r = 0.3 * hy.min(hx / 2.0);
                v.extend(fan(Vec2::new(-hx / 2.0, 0.0), r));
                v.extend(fan(Vec2::new(hx / 2.0, 0.0), r));
                for right in [false, true] {
                    for y in [-0.5 * hy, 0.5 * hy] {
                        v.push(side_collar(right, y, 0.22 * hy));
                    }
                }
            }
            AhuKind::SupplyUnit | AhuKind::ExtractUnit => {
                // Three sections along the air: filter, fan, coil (its fins as slanted lines); a collar at each end.
                let (x1, x2) = (-hx / 3.0, hx / 3.0);
                for x in [x1, x2] {
                    v.push(line(Vec2::new(x, -(hy - inset)), Vec2::new(x, hy - inset)));
                }
                let section = (x2 - x1) / 2.0;
                v.extend(fan(Vec2::new(0.0, 0.0), 0.32 * hy.min(section * 2.0)));
                for k in 1..4 {
                    let x = x2 + (hx - inset - x2) * f64::from(k) / 4.0;
                    v.push(line(Vec2::new(x - 0.04 * hx, -(hy - inset)), Vec2::new(x + 0.04 * hx, hy - inset)));
                }
                for k in 1..4 {
                    let x = -hx + inset + (x1 + hx - inset) * f64::from(k) / 4.0;
                    v.push(line(Vec2::new(x, -(hy - inset)), Vec2::new(x, hy - inset)));
                }
                for right in [false, true] {
                    v.push(side_collar(right, 0.0, 0.3 * hy));
                }
            }
            AhuKind::VmcBox => {
                v.extend(fan(Vec2::ZERO, 0.38 * hy.min(hx)));
                for right in [false, true] {
                    v.push(side_collar(right, 0.0, 0.28 * hy));
                }
                // Two spigots on top, as such a box has: the air out and the air in.
                for x in [-0.5 * hx, 0.5 * hx] {
                    v.push(rect(x, hy + stick / 2.0, 0.2 * hy.min(hx), stick / 2.0));
                }
            }
        }
        v
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
    /// The type sizing gave the run when its own is Auto (main, branch or connection), set at the
    /// last sizing: what Properties shows beside "Auto".
    #[serde(default)]
    pub kind_found: Option<DuctKind>,
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
    /// Which way the air runs in the run, set by sizing: along the block's x axis (`true`) or against
    /// it. Drawn as an arrow on the run; `None` until the network is sized.
    #[serde(default)]
    pub flow_forward: Option<bool>,
    /// The altitude of the run's underside, mm, when a height zone it runs in gives one: written on its label.
    #[serde(default)]
    pub bottom_mm: Option<f64>,
    /// The flow, m³/h, leaving by this duct's free end, when it starts on a unit's collar and leads
    /// to no terminal (an open end, as when air is blown or taken freely). None: the unit's balance.
    #[serde(default)]
    pub open_flow_m3h: Option<f64>,
    /// Where the run passes under another duct, mm along it from its start: its sides are drawn
    /// broken there (a plan's way of showing which duct goes over).
    #[serde(default)]
    pub gaps_mm: Vec<[f64; 2]>,
    /// The rises or dips the run makes round other ducts, four 45° elbows each.
    #[serde(default)]
    pub jogs: Vec<DuctJog>,
    /// A flexible duct (gaine souple): drawn as bellows, with the roughness of one.
    #[serde(default)]
    pub flexible: bool,
    /// A reducer where the run joins the one before it, drawn by sizing when the diameters differ.
    #[serde(default)]
    pub reducer: Option<DuctReducer>,
    /// The elbow where the run joins the one before it, drawn by sizing: curved arcs across the run's
    /// width, the two runs being drawn shorter by the length it takes.
    #[serde(default)]
    pub elbow: Option<DuctElbow>,
    /// Length, mm, not drawn at each end because an elbow takes it (this run's own or its neighbour's).
    #[serde(default)]
    pub trim_start_mm: f64,
    #[serde(default)]
    pub trim_end_mm: f64,
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

/// The elbow where a run turns away from the one it continues.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuctElbow {
    /// How far the run turns from the one before it, degrees: positive to the left of the air's way.
    pub angle_deg: f64,
    /// The radius of the duct's axis through the bend, mm.
    pub radius_mm: f64,
    /// Whether the run is entered by its far end (the bend is then at its end, not its origin).
    pub at_end: bool,
}

impl DuctElbow {
    /// How far from the corner the bend starts along each run, mm.
    pub fn tangent_mm(&self) -> f64 {
        self.radius_mm * (self.angle_deg.abs().to_radians() / 2.0).tan()
    }
}

/// Half the angle of a reducer's cone.
const REDUCER_HALF_ANGLE_DEG: f64 = 15.0;
/// Most vertices of a flexible duct's side.
const MAX_BELLOWS_VERTICES: usize = 2_000;

impl DuctParams {
    pub fn validate(&self) -> Result<()> {
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if let Some(e) = self.elbow
            && !(e.angle_deg.is_finite() && e.angle_deg.abs() > 0.0 && e.angle_deg.abs() < 180.0 && e.radius_mm.is_finite() && e.radius_mm > 0.0)
        {
            return Err(invalid("An elbow turns by 0 to 180 degrees, with a radius above 0."));
        }
        if ![self.trim_start_mm, self.trim_end_mm].iter().all(|t| t.is_finite() && (0.0..=MAX_DUCT_LENGTH_M * 1000.0).contains(t)) {
            return Err(invalid("A trimmed length is 0 or more."));
        }
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
        if self.gaps_mm.len() > MAX_GAPS
            || self.gaps_mm.iter().any(|g| !(g[0].is_finite() && g[1].is_finite() && 0.0 <= g[0] && g[0] < g[1] && g[1] <= 1e9))
        {
            return Err(invalid(format!("A duct passes under at most {MAX_GAPS} others, each over a stretch of its length.")));
        }
        let length_mm = self.length * self.units.mm_per_unit;
        let fits = |j: &DuctJog| {
            [j.centre_mm, j.half_mm, j.shift_mm].iter().all(|v| v.is_finite())
                && j.half_mm >= 0.0
                && (20.0..=5_000.0).contains(&j.shift_mm)
                && j.elbows_mm()[0] >= -1e-6
                && j.elbows_mm()[3] <= length_mm + 1e-6
        };
        if self.jogs.len() > MAX_JOGS || !self.jogs.iter().all(fits) {
            return Err(invalid(format!("A duct makes at most {MAX_JOGS} jogs, each along its length (a rise of 20 to 5000 mm).")));
        }
        for v in [self.flow_m3h, self.velocity, self.open_flow_m3h].into_iter().flatten() {
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
            AirDirection::Fresh => DUCT_FRESH_LAYER,
            AirDirection::Exhaust => DUCT_EXHAUST_LAYER,
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
        // What an elbow takes at each end; a cone is drawn only where no elbow is.
        let (trim0, trim1) = (self.units.to_units(self.trim_start_mm), self.units.to_units(self.trim_end_mm));
        let cone = self.reducer.filter(|red| if red.at_end { trim1 <= 0.0 } else { trim0 <= 0.0 }).map(|red| {
            let r0 = self.units.to_units(red.from_mm) / 2.0;
            let len = ((r0 - r).abs() / REDUCER_HALF_ANGLE_DEG.to_radians().tan()).min(0.4 * self.length);
            (r0, len, red.at_end)
        });
        let (x0, x1) = match cone {
            Some((_, len, false)) => (len, self.length - trim1),
            Some((_, len, true)) => (trim0, self.length - len),
            None => (trim0, self.length - trim1),
        };
        let (x0, x1) = (x0.min(self.length), x1.max(x0).min(self.length));
        // The stretches drawn: the run less where it passes under another duct.
        let solid = self.solid_stretches(x0, x1);
        for sign in [1.0, -1.0] {
            for (k, (s, e)) in solid.iter().copied().enumerate() {
                let mut pts = Vec::new();
                if let (Some((r0, _, false)), 0) = (cone, k) {
                    pts.push(Vec2::new(0.0, sign * r0));
                }
                pts.extend(self.side_points(s, e, sign * r));
                if let (Some((r0, _, true)), true) = (cone, k + 1 == solid.len()) {
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
        }
        // Each jog: four elbows across the run, the outer two bulging toward the flat part, the inner two away.
        for j in &self.jogs {
            for (k, x) in j.elbows_mm().into_iter().enumerate() {
                let bulge = if k == 0 || k == 2 { 1.0 } else { -1.0 };
                let x = self.units.to_units(x);
                pieces.push(HvacPiece {
                    layer,
                    kind: EntityKind::LwPolyline(LwPolyline {
                        vertices: vec![
                            PolyVertex { p: Vec2::new(x, -r), bulge, start_width: 0.0, end_width: 0.0 },
                            PolyVertex { p: Vec2::new(x, r), bulge: 0.0, start_width: 0.0, end_width: 0.0 },
                        ],
                        closed: false,
                        const_width: 0.0,
                        elevation: 0.0,
                        plinegen: false,
                    }),
                });
            }
        }
        // The arrow and the label sit on the longest stretch that is drawn.
        let (sx, ex) = solid.iter().copied().max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0))).unwrap_or((x0, x1));
        if let Some(e) = self.elbow {
            pieces.extend(self.elbow_pieces(e, r, layer));
        }
        // An arrow a quarter of the way along, the way the air runs.
        if let Some(forward) = self.flow_forward {
            let len = (1.6 * r).min(0.15 * (ex - sx));
            if len > 0.0 {
                let at = sx + 0.25 * (ex - sx);
                let dir = Vec2::new(if forward { 1.0 } else { -1.0 }, 0.0);
                let (tail, tip) = (Vec2::new(at, 0.0) - dir * (len / 2.0), Vec2::new(at, 0.0) + dir * (len / 2.0));
                let head = len * 0.35;
                let side = dir.perp() * (head * 0.6);
                let l = |a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
                pieces.extend([l(tail, tip), l(tip, tip - dir * head + side), l(tip, tip - dir * head - side)]);
            }
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
        if let Some(b) = self.bottom_mm {
            text.push_str(&format!(" Ai:{:.2}", b / 1000.0));
        }
        let h = self.units.to_units(self.label_height_mm);
        // A flexible duct is narrow and its bellows busy: its label sits above it.
        let (y, up) = if self.flexible { (r * 1.5 + 1.2 * h, if flip_label { -1.0 } else { 1.0 }) } else { (0.0, 1.0) };
        for j in &self.jogs {
            let at = Vec2::new(self.units.to_units(j.centre_mm), -(r + 0.9 * h));
            pieces.push(HvacPiece { layer, kind: label(at, 0.8 * h, format!("{}{:.0}", if j.down { "-" } else { "+" }, j.shift_mm)) });
        }
        let mid = if self.gaps_mm.is_empty() { self.length / 2.0 } else { (sx + ex) / 2.0 };
        let mut t = label(Vec2::new(mid, y * up), h, text);
        if let EntityKind::Text(x) = &mut t {
            x.rotation = if flip_label { std::f64::consts::PI } else { 0.0 };
        }
        pieces.push(HvacPiece { layer, kind: t });
        Ok(pieces)
    }

    /// The two arcs of an elbow, in the run's block coordinates: the entry end is the origin (or, if
    /// the run is entered by its far end, the end turned half a turn about the middle).
    fn elbow_pieces(&self, e: DuctElbow, half_width: f64, layer: &'static str) -> Vec<HvacPiece> {
        let phi = e.angle_deg.to_radians();
        let radius = self.units.to_units(e.radius_mm);
        let tangent = radius * (phi.abs() / 2.0).tan();
        let side = phi.signum();
        // The centre is on the inside of the turn; the arc runs from the parent's tangent point to the run's.
        let (centre, start, end) = if side > 0.0 {
            (Vec2::new(tangent, radius), -std::f64::consts::FRAC_PI_2 - phi, -std::f64::consts::FRAC_PI_2)
        } else {
            (Vec2::new(tangent, -radius), std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2 - phi)
        };
        let (centre, start, end) = if e.at_end {
            (Vec2::new(self.length - centre.x, -centre.y), start + std::f64::consts::PI, end + std::f64::consts::PI)
        } else {
            (centre, start, end)
        };
        [radius + half_width, (radius - half_width).max(0.0)]
            .into_iter()
            .map(|r| HvacPiece { layer, kind: EntityKind::Arc(crate::Arc { center: v3(centre), radius: r, start, end }) })
            .collect()
    }

    /// The vertices of one side of the straight part, from `x0` to `x1` at height `y`: two points
    /// for a rigid duct, a zigzag for a flexible one.
    /// `x0`–`x1` less the stretches the run passes under other ducts, in drawing units along the run.
    fn solid_stretches(&self, x0: f64, x1: f64) -> Vec<(f64, f64)> {
        let mut gaps: Vec<(f64, f64)> = self.gaps_mm.iter().map(|g| (self.units.to_units(g[0]), self.units.to_units(g[1]))).collect();
        gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut out, mut from) = (Vec::new(), x0);
        for (g0, g1) in gaps {
            if g0 > from && g0 < x1 {
                out.push((from, g0));
            }
            from = from.max(g1);
        }
        if from < x1 {
            out.push((from, x1));
        }
        out
    }

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
/// `tol`) of the axis and farther than that from either end. A branch that starts nearer an end is
/// at a corner, not a tee: ends that close already meet when the network is read, and a cut there
/// would leave a short piece that leads nowhere. `None` otherwise.
pub(super) fn interior_cut(a: Vec2, b: Vec2, diameter: f64, at: Vec2, tol: f64) -> Option<Vec2> {
    let length = a.dist(b);
    let along = (b - a).normalized();
    let t = (at - a).dot(along);
    let across = (at - a).cross(along).abs();
    let reach = tol.max(AXIS_REACH * diameter);
    (across <= reach && t > reach && t < length - reach).then(|| a + along * t)
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
        // The stretches it passes under stay where they were, each on the part it lies on.
        let cut_mm = a.dist(cut) * p.units.mm_per_unit;
        let rest_mm = cut.dist(b) * p.units.mm_per_unit;
        let clip = |g: &[f64; 2], lo: f64, hi: f64| {
            let (f, t) = (g[0].max(lo), g[1].min(hi));
            (f < t).then_some([f - lo, t - lo])
        };
        // A jog lies on one part, or is dropped when the cut falls inside it.
        let jogs_in = |lo: f64, hi: f64| -> Vec<DuctJog> {
            p.jogs
                .iter()
                .filter(|j| j.elbows_mm()[0] >= lo - 1e-6 && j.elbows_mm()[3] <= hi + 1e-6)
                .map(|j| DuctJog { centre_mm: j.centre_mm - lo, ..*j })
                .collect()
        };
        let first = DuctParams {
            length: a.dist(cut),
            gaps_mm: p.gaps_mm.iter().filter_map(|g| clip(g, 0.0, cut_mm)).collect(),
            jogs: jogs_in(0.0, cut_mm),
            ..p.clone()
        };
        let second_params = DuctParams {
            gaps_mm: p.gaps_mm.iter().filter_map(|g| clip(g, cut_mm, cut_mm + rest_mm)).collect(),
            jogs: jogs_in(cut_mm, cut_mm + rest_mm),
            ..p
        };
        self.regenerate_duct(h, &first)?;
        let second = self.add_duct(space, common, second_params, cut, b)?;
        Ok(Some(second))
    }
}
