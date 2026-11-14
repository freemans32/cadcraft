//! Pipework drawn in plan: the water networks of an air-conditioning plant (chilled water, hot
//! water, and the change-over network that carries one or the other by the season), condensate
//! drains and refrigerant lines. A pipe is one straight run drawn as two lines to scale, with its
//! size and service written on it, as a block reference with its parameters in extended data like
//! the ducts; the network the runs make is read back from where their ends meet.
//!
//! Colours. Condensate drains are blue with a yellow band and chilled water blue with a white one
//! (ISO 14726, the identification colours of piping: BU–YE–BU and BU–WH–BU); a plan shows the band
//! as a line along the axis between the two blue sides. The other services have no colour taken
//! from a standard here: hot water is red, the change-over network violet, the refrigerant liquid
//! line orange and the gas line grey, the supply of a water network in the strong tone and its
//! return in the light one.

use cadcraft_geom::Vec2;
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, invalid, label, v3, xdata};
use crate::{EntityKind, Line, RawTag, Result};

/// The registered application name of a pipe's extended data.
pub const PIPE_APP: &str = "CADCRAFT_PIPE";
const PIPE_FORMAT: &str = "PIPE1";

/// The layers of the pipes: chilled water, hot water and change-over, supply and return; the
/// condensate drains; the refrigerant liquid and gas lines.
pub const PIPE_CHW_SUPPLY_LAYER: &str = "M-PIPE-CHWS";
pub const PIPE_CHW_RETURN_LAYER: &str = "M-PIPE-CHWR";
pub const PIPE_HW_SUPPLY_LAYER: &str = "M-PIPE-HWS";
pub const PIPE_HW_RETURN_LAYER: &str = "M-PIPE-HWR";
pub const PIPE_CO_SUPPLY_LAYER: &str = "M-PIPE-COS";
pub const PIPE_CO_RETURN_LAYER: &str = "M-PIPE-COR";
pub const PIPE_CONDENSATE_LAYER: &str = "M-PIPE-COND";
pub const PIPE_REFRIGERANT_LIQUID_LAYER: &str = "M-PIPE-REFL";
pub const PIPE_REFRIGERANT_GAS_LAYER: &str = "M-PIPE-REFG";
/// A refrigerant link drawn as one run for the liquid and the gas lines together.
pub const PIPE_REFRIGERANT_PAIR_LAYER: &str = "M-PIPE-REFP";

/// Copper refrigeration tube sizes, which are given in inches: the label and the outer diameter, mm.
pub const REFRIGERANT_TUBES: [(&str, f64); 12] = [
    ("1/4", 6.35),
    ("3/8", 9.52),
    ("1/2", 12.7),
    ("5/8", 15.88),
    ("3/4", 19.05),
    ("7/8", 22.2),
    ("1 1/8", 28.58),
    ("1 3/8", 34.9),
    ("1 5/8", 41.28),
    ("2 1/8", 53.98),
    ("2 5/8", 66.68),
    ("3 1/8", 79.38),
];

/// The size of a refrigerant tube of outer diameter `mm`, as it is spoken: in inches ("3/8\"") when it is a standard
/// size, else the diameter in millimetres.
pub fn refrigerant_size_label(mm: f64) -> String {
    match REFRIGERANT_TUBES.iter().find(|(_, d)| (d - mm).abs() < 0.15) {
        Some((l, _)) => format!("{l}\""),
        None => format!("Ø{mm:.1}"),
    }
}

/// A refrigerant tube size typed as inches ("3/8", "3/8\"", "1 1/8", "1-1/8", "5/8in", "1.125\"") or as millimetres
/// ("9.52", "12 mm"): the outer diameter in millimetres. A bare number is millimetres; a mark (`"`, `in`) or a fraction
/// makes it inches.
pub fn parse_refrigerant_size(text: &str) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase().replace(['”', '″'], "\"");
    let (body, inches_mark) = if let Some(b) = t.strip_suffix('"').or_else(|| t.strip_suffix("in")).or_else(|| t.strip_suffix("pouces")) {
        (b.trim().to_string(), true)
    } else if let Some(b) = t.strip_suffix("mm") {
        (b.trim().to_string(), false)
    } else {
        (t.clone(), false)
    };
    if body.trim_start().starts_with('-') {
        return None;
    }
    let body = body.replace('-', " ");
    let mm = if body.contains('/') {
        let mut whole = 0.0;
        let mut frac = None;
        for part in body.split_whitespace() {
            if let Some((n, d)) = part.split_once('/') {
                let (n, d) = (n.parse::<f64>().ok()?, d.parse::<f64>().ok()?);
                (d != 0.0).then_some(())?;
                frac = Some(n / d);
            } else {
                whole = part.parse::<f64>().ok()?;
            }
        }
        (whole + frac?) * 25.4
    } else {
        let v = body.trim().parse::<f64>().ok()?;
        if inches_mark { v * 25.4 } else { v }
    };
    // A standard size is the tube's real outer diameter (3/8" is 9.52 mm, not 9.525).
    let mm = REFRIGERANT_TUBES.iter().find(|(_, d)| (d - mm).abs() < 0.1).map_or(mm, |(_, d)| *d);
    (mm.is_finite() && mm > 0.0).then_some(mm)
}

/// The gap between the two tubes of a refrigerant pair, mm.
const PAIR_GAP_MM: f64 = 20.0;

/// Smallest and largest nominal diameter (outer diameter for a refrigerant line), mm.
pub const PIPE_DIAMETER_MM: (f64, f64) = (4.0, 600.0);
/// Largest insulation thickness, mm.
pub const MAX_PIPE_INSULATION_MM: f64 = 200.0;
/// Largest load typed on a run, kW.
pub const MAX_PIPE_LOAD_KW: f64 = 1_000_000.0;

/// Steel pipe sizes (ISO 6708 nominal diameter, ISO 4200 outer diameter), mm.
pub const STEEL_PIPE_SIZES: [(f64, f64); 15] = [
    (10.0, 17.2),
    (15.0, 21.3),
    (20.0, 26.9),
    (25.0, 33.7),
    (32.0, 42.4),
    (40.0, 48.3),
    (50.0, 60.3),
    (65.0, 76.1),
    (80.0, 88.9),
    (100.0, 114.3),
    (125.0, 139.7),
    (150.0, 168.3),
    (200.0, 219.1),
    (250.0, 273.0),
    (300.0, 323.9),
];

/// What a pipe carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PipeService {
    /// Chilled water (eau glacée).
    #[default]
    ChilledWater,
    /// Hot water (eau chaude).
    HotWater,
    /// The change-over network (change over): one pair of pipes that carries chilled water in summer and hot water in winter.
    ChangeOver,
    /// A condensate drain (évacuation de condensats), by gravity.
    Condensate,
    /// A refrigerant liquid line.
    RefrigerantLiquid,
    /// A refrigerant gas (suction) line.
    RefrigerantGas,
    /// The liquid and the gas lines of a refrigerant link, drawn together as one run.
    RefrigerantPair,
}

impl PipeService {
    pub const ALL: [PipeService; 7] = [
        PipeService::ChilledWater,
        PipeService::HotWater,
        PipeService::ChangeOver,
        PipeService::Condensate,
        PipeService::RefrigerantLiquid,
        PipeService::RefrigerantGas,
        PipeService::RefrigerantPair,
    ];

    /// The name a command takes.
    pub fn name(self) -> &'static str {
        match self {
            PipeService::ChilledWater => "chilledWater",
            PipeService::HotWater => "hotWater",
            PipeService::ChangeOver => "changeOver",
            PipeService::Condensate => "condensate",
            PipeService::RefrigerantLiquid => "refrigerantLiquid",
            PipeService::RefrigerantGas => "refrigerantGas",
            PipeService::RefrigerantPair => "refrigerantPair",
        }
    }

    pub fn parse(text: &str) -> Option<PipeService> {
        let t: String = text.trim().to_ascii_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
        PipeService::ALL.into_iter().find(|s| s.name().to_ascii_lowercase() == t).or(match t.as_str() {
            "chw" | "eauglacee" | "eauglacée" | "eauglace" | "eauglacé" => Some(PipeService::ChilledWater),
            "hw" | "eauchaude" => Some(PipeService::HotWater),
            "co" | "changeover" | "reversible" => Some(PipeService::ChangeOver),
            "cond" | "condensats" | "condensat" | "evacuation" | "drain" => Some(PipeService::Condensate),
            "refl" | "liquide" | "liquid" | "frigoliquide" => Some(PipeService::RefrigerantLiquid),
            "refg" | "gaz" | "gas" | "frigogaz" => Some(PipeService::RefrigerantGas),
            "ref" | "refp" | "pair" | "paire" | "liaison" | "liaisonfrigo" | "frigo" | "liquidegaz" => Some(PipeService::RefrigerantPair),
            _ => None,
        })
    }

    /// Whether the service has a supply and a return pipe (the water networks).
    pub fn has_legs(self) -> bool {
        matches!(self, PipeService::ChilledWater | PipeService::HotWater | PipeService::ChangeOver)
    }

    /// Whether the diameter of a pipe of the service is the outer diameter of a copper tube (refrigerant),
    /// not the nominal diameter of a steel pipe.
    pub fn is_refrigerant(self) -> bool {
        matches!(self, PipeService::RefrigerantLiquid | PipeService::RefrigerantGas | PipeService::RefrigerantPair)
    }
}

/// Which pipe of a water network: the supply (départ) or the return (retour).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PipeLeg {
    #[default]
    Supply,
    Return,
}

impl PipeLeg {
    pub fn parse(text: &str) -> Option<PipeLeg> {
        match text.trim().to_ascii_lowercase().as_str() {
            "supply" | "s" | "depart" | "départ" | "aller" => Some(PipeLeg::Supply),
            "return" | "r" | "retour" => Some(PipeLeg::Return),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PipeLeg::Supply => "supply",
            PipeLeg::Return => "return",
        }
    }
}

fn default_label_mm() -> f64 {
    100.0
}

/// A pipe run: what its block reference keeps. The run starts at the block's origin and goes along
/// the block's x axis for `length`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipeParams {
    /// Length, in drawing units.
    pub length: f64,
    pub service: PipeService,
    /// The supply or the return, for a water network; the supply for the other services.
    #[serde(default)]
    pub leg: PipeLeg,
    /// The nominal diameter of a steel pipe, mm; the outer diameter of a refrigerant line's copper tube.
    pub dn_mm: f64,
    /// The outer diameter of the gas tube of a refrigerant pair, mm (`dn_mm` is then the liquid tube); 0 for the others.
    #[serde(default)]
    pub gas_dn_mm: f64,
    /// Whether the user chose the size: sizing then leaves it.
    #[serde(default)]
    pub imposed: bool,
    /// The insulation round the pipe, mm: it widens what a plan shows.
    #[serde(default)]
    pub insulation_mm: f64,
    /// The load served at this run's free end, kW (a unit's coil, say), or, for a condensate drain, the flow it
    /// takes in there, l/h. Sizing adds these up the network.
    #[serde(default)]
    pub load_kw: Option<f64>,
    /// Whether this run starts at the source of its network (a chiller, a boiler): sizing reads the network from it.
    #[serde(default)]
    pub source: bool,
    /// The flow the run carries once sized, m³/h (litres per hour for a condensate drain: the value is l/h / 1000).
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The velocity in the run, m/s, once sized.
    #[serde(default)]
    pub velocity: Option<f64>,
    /// The pressure lost per metre, Pa/m, once sized.
    #[serde(default)]
    pub loss_pa_m: Option<f64>,
    /// Which way the fluid runs, along the block's x axis (`true`) or against it, once sized.
    #[serde(default)]
    pub flow_forward: Option<bool>,
    /// The altitude of the run's underside, mm above the drawing's zero, when the user gave it.
    #[serde(default)]
    pub level_mm: Option<f64>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl PipeParams {
    /// A run of `service` (the supply), `dn_mm` across, `length` drawing units long.
    pub fn new(service: PipeService, dn_mm: f64, units: HvacUnits) -> PipeParams {
        PipeParams {
            length: 1.0,
            service,
            leg: PipeLeg::Supply,
            dn_mm,
            gas_dn_mm: if service == PipeService::RefrigerantPair { 15.88 } else { 0.0 },
            imposed: false,
            insulation_mm: 0.0,
            load_kw: None,
            source: false,
            flow_m3h: None,
            velocity: None,
            loss_pa_m: None,
            flow_forward: None,
            level_mm: None,
            label_height_mm: default_label_mm(),
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if !(self.length.is_finite() && self.length > 0.0 && self.length <= 1e9) {
            return Err(invalid("A pipe has a usable length."));
        }
        if !(self.dn_mm.is_finite() && (PIPE_DIAMETER_MM.0..=PIPE_DIAMETER_MM.1).contains(&self.dn_mm)) {
            return Err(invalid(format!("A pipe is {} to {} mm across.", PIPE_DIAMETER_MM.0, PIPE_DIAMETER_MM.1)));
        }
        if self.service == PipeService::RefrigerantPair
            && !(self.gas_dn_mm.is_finite() && (PIPE_DIAMETER_MM.0..=PIPE_DIAMETER_MM.1).contains(&self.gas_dn_mm))
        {
            return Err(invalid(format!("The gas tube is {} to {} mm across.", PIPE_DIAMETER_MM.0, PIPE_DIAMETER_MM.1)));
        }
        if !(self.insulation_mm.is_finite() && (0.0..=MAX_PIPE_INSULATION_MM).contains(&self.insulation_mm)) {
            return Err(invalid(format!("The insulation is 0 to {MAX_PIPE_INSULATION_MM} mm.")));
        }
        if self.load_kw.is_some_and(|l| !(l.is_finite() && (0.0..=MAX_PIPE_LOAD_KW).contains(&l))) {
            return Err(invalid("The load of a pipe is 0 or more."));
        }
        if self.level_mm.is_some_and(|l| !(l.is_finite() && l.abs() <= 1e6)) {
            return Err(invalid("The altitude of a pipe is not usable."));
        }
        self.units.validate()?;
        check_label_height(self.label_height_mm)
    }

    /// The outer diameter of the pipe, mm: the steel pipe of the nominal diameter, or the copper tube.
    pub fn outer_diameter_mm(&self) -> f64 {
        if self.service.is_refrigerant() {
            return self.dn_mm;
        }
        STEEL_PIPE_SIZES.iter().min_by(|a, b| (a.0 - self.dn_mm).abs().total_cmp(&(b.0 - self.dn_mm).abs())).map_or(self.dn_mm, |(_, od)| *od)
    }

    /// The width a plan shows: the outer diameter and the insulation either side; for a refrigerant pair, the two
    /// tubes side by side with a small gap.
    pub fn plan_width_mm(&self) -> f64 {
        if self.service == PipeService::RefrigerantPair {
            let (l, g) = self.pair_widths_mm();
            return l + g + PAIR_GAP_MM;
        }
        self.outer_diameter_mm() + 2.0 * self.insulation_mm
    }

    /// The widths of the liquid tube and of the gas tube of a refrigerant pair, with their insulation, mm.
    fn pair_widths_mm(&self) -> (f64, f64) {
        (self.dn_mm + 2.0 * self.insulation_mm, self.gas_dn_mm + 2.0 * self.insulation_mm)
    }

    /// The size as written: "DN50", the inches of a refrigerant tube ("3/8\""), or both tubes of a pair ("3/8\" - 5/8\"").
    pub fn size_text(&self) -> String {
        match self.service {
            PipeService::RefrigerantPair => format!("{} - {}", refrigerant_size_label(self.dn_mm), refrigerant_size_label(self.gas_dn_mm)),
            PipeService::RefrigerantLiquid | PipeService::RefrigerantGas => refrigerant_size_label(self.dn_mm),
            _ => format!("DN{:.0}", self.dn_mm),
        }
    }

    /// The layer of the run.
    pub fn layer(&self) -> &'static str {
        layer_of(self.service, self.leg)
    }

    /// Length in metres.
    pub fn length_m(&self) -> f64 {
        self.length * self.units.mm_per_unit / 1000.0
    }

    /// The short code of the service written on the run.
    pub fn tag(&self) -> &'static str {
        match (self.service, self.leg) {
            (PipeService::ChilledWater, PipeLeg::Supply) => "CHW-S",
            (PipeService::ChilledWater, PipeLeg::Return) => "CHW-R",
            (PipeService::HotWater, PipeLeg::Supply) => "HW-S",
            (PipeService::HotWater, PipeLeg::Return) => "HW-R",
            (PipeService::ChangeOver, PipeLeg::Supply) => "CO-S",
            (PipeService::ChangeOver, PipeLeg::Return) => "CO-R",
            (PipeService::Condensate, _) => "COND",
            (PipeService::RefrigerantLiquid, _) => "REF-L",
            (PipeService::RefrigerantGas, _) => "REF-G",
            (PipeService::RefrigerantPair, _) => "REF",
        }
    }

    /// The text written on the run: its size, its service and what sizing found.
    pub fn label_text(&self) -> String {
        let mut text = format!("{} {}", self.size_text(), self.tag());
        if let Some(f) = self.flow_m3h {
            if self.service == PipeService::Condensate {
                text.push_str(&format!(" {:.1} l/h", f * 1000.0));
            } else {
                text.push_str(&format!(" {f:.2} m³/h"));
            }
        }
        if let Some(l) = self.level_mm {
            text.push_str(&format!(" Ai:{:.2}", l / 1000.0));
        }
        text
    }

    /// The run drawn to scale: its two sides, the band along its axis for the services that have one, a
    /// label and, once sized, an arrow. `flip_label` turns the label half a turn, so it reads upright on a
    /// run drawn leftwards.
    pub fn generate(&self, flip_label: bool) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let r = self.units.to_units(self.plan_width_mm()) / 2.0;
        let layer = self.layer();
        let line = |layer: &'static str, a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let mut pieces = if self.service == PipeService::RefrigerantPair {
            // The liquid tube on one side, the gas tube on the other, each as two lines on its own layer.
            let (wl, wg) = self.pair_widths_mm();
            let u = |mm: f64| self.units.to_units(mm);
            let (liquid, gas) = (PIPE_REFRIGERANT_LIQUID_LAYER, PIPE_REFRIGERANT_GAS_LAYER);
            let l_lo = -r;
            let l_hi = -r + u(wl);
            let g_hi = r;
            let g_lo = r - u(wg);
            let run = |layer: &'static str, y: f64| line(layer, Vec2::new(0.0, y), Vec2::new(self.length, y));
            vec![run(liquid, l_lo), run(liquid, l_hi), run(gas, g_lo), run(gas, g_hi)]
        } else {
            vec![line(layer, Vec2::new(0.0, -r), Vec2::new(self.length, -r)), line(layer, Vec2::new(0.0, r), Vec2::new(self.length, r))]
        };
        if let Some(band) = band_layer(self.service, self.leg) {
            pieces.push(line(band, Vec2::ZERO, Vec2::new(self.length, 0.0)));
        }
        if let Some(forward) = self.flow_forward {
            let len = (2.4 * r).min(0.15 * self.length);
            let at = 0.25 * self.length;
            let dir = Vec2::new(if forward { 1.0 } else { -1.0 }, 0.0);
            let (tail, tip) = (Vec2::new(at, 0.0) - dir * (len / 2.0), Vec2::new(at, 0.0) + dir * (len / 2.0));
            let head = len * 0.4;
            let side = dir.perp() * (head * 0.6);
            pieces.extend([line(layer, tail, tip), line(layer, tip, tip - dir * head + side), line(layer, tip, tip - dir * head - side)]);
        }
        let h = self.units.to_units(self.label_height_mm);
        let up = if flip_label { -1.0 } else { 1.0 };
        let mut t = label(Vec2::new(self.length / 2.0, (r + 1.1 * h) * up), h, self.label_text());
        if let EntityKind::Text(x) = &mut t {
            x.rotation = if flip_label { std::f64::consts::PI } else { 0.0 };
        }
        pieces.push(HvacPiece { layer, kind: t });
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(PIPE_APP, PIPE_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<PipeParams> {
        let p: PipeParams = xdata::from_xdata(PIPE_APP, PIPE_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The layer of a pipe of `service` and `leg`.
pub fn pipe_layer_of(service: PipeService, leg: PipeLeg) -> &'static str {
    layer_of(service, leg)
}

fn layer_of(service: PipeService, leg: PipeLeg) -> &'static str {
    match (service, leg) {
        (PipeService::ChilledWater, PipeLeg::Supply) => PIPE_CHW_SUPPLY_LAYER,
        (PipeService::ChilledWater, PipeLeg::Return) => PIPE_CHW_RETURN_LAYER,
        (PipeService::HotWater, PipeLeg::Supply) => PIPE_HW_SUPPLY_LAYER,
        (PipeService::HotWater, PipeLeg::Return) => PIPE_HW_RETURN_LAYER,
        (PipeService::ChangeOver, PipeLeg::Supply) => PIPE_CO_SUPPLY_LAYER,
        (PipeService::ChangeOver, PipeLeg::Return) => PIPE_CO_RETURN_LAYER,
        (PipeService::Condensate, _) => PIPE_CONDENSATE_LAYER,
        (PipeService::RefrigerantLiquid, _) => PIPE_REFRIGERANT_LIQUID_LAYER,
        (PipeService::RefrigerantGas, _) => PIPE_REFRIGERANT_GAS_LAYER,
        (PipeService::RefrigerantPair, _) => PIPE_REFRIGERANT_PAIR_LAYER,
    }
}

/// The layer string of the line along the axis of a pipe that has an identification band, coded
/// `layer|linetype|colour layer`: the pipe's layer, drawn in the colour of the band.
fn band_layer(service: PipeService, leg: PipeLeg) -> Option<&'static str> {
    match (service, leg) {
        (PipeService::ChilledWater, PipeLeg::Supply) => Some("M-PIPE-CHWS|ByLayer|M-PIPE-BAND-WHITE"),
        (PipeService::ChilledWater, PipeLeg::Return) => Some("M-PIPE-CHWR|ByLayer|M-PIPE-BAND-WHITE"),
        (PipeService::Condensate, _) => Some("M-PIPE-COND|ByLayer|M-PIPE-BAND-YELLOW"),
        _ => None,
    }
}

/// The colour a pipe layer, or a band, is created with.
pub fn pipe_layer_color(name: &str) -> Option<cadcraft_color::Color> {
    use cadcraft_color::{Color, Rgb};
    match name {
        // ISO 14726: blue, with a white band for chilled water and a yellow one for condensate.
        PIPE_CHW_SUPPLY_LAYER | PIPE_CONDENSATE_LAYER => Some(Color::True(Rgb(0x1e, 0x6f, 0xd9))),
        PIPE_CHW_RETURN_LAYER => Some(Color::True(Rgb(0x66, 0xb3, 0xff))),
        "M-PIPE-BAND-WHITE" => Some(Color::Index(7)),
        "M-PIPE-BAND-YELLOW" => Some(Color::True(Rgb(0xf5, 0xc4, 0x00))),
        // Not from a standard: hot water red, change-over violet, refrigerant orange (liquid) and grey (gas).
        PIPE_HW_SUPPLY_LAYER => Some(Color::True(Rgb(0xe0, 0x24, 0x24))),
        PIPE_HW_RETURN_LAYER => Some(Color::True(Rgb(0xff, 0x8a, 0x80))),
        PIPE_CO_SUPPLY_LAYER => Some(Color::True(Rgb(0x8e, 0x44, 0xad))),
        PIPE_CO_RETURN_LAYER => Some(Color::True(Rgb(0xc3, 0x9b, 0xd3))),
        PIPE_REFRIGERANT_LIQUID_LAYER => Some(Color::True(Rgb(0xe6, 0x7e, 0x22))),
        PIPE_REFRIGERANT_GAS_LAYER => Some(Color::True(Rgb(0x8d, 0x99, 0xa3))),
        PIPE_REFRIGERANT_PAIR_LAYER => Some(Color::True(Rgb(0xc8, 0x9a, 0x6a))),
        _ => None,
    }
}

/// The pipe an object is, when it is a block reference carrying usable pipe data.
pub fn pipe_of(e: &crate::Entity) -> Option<PipeParams> {
    match &e.kind {
        EntityKind::Insert(_) => PipeParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds a pipe from `a` to `b`; its `length` is set from the two points.
    pub fn add_pipe(&mut self, space: &crate::Space, mut common: crate::Common, mut p: PipeParams, a: Vec2, b: Vec2) -> Result<crate::Handle> {
        if !a.is_finite() || !b.is_finite() {
            return Err(invalid("A pipe needs usable end points."));
        }
        p.length = a.dist(b);
        let rotation = (b - a).angle();
        let pieces = p.generate(super::duct::reads_upside_down(rotation))?;
        common.xdata = xdata::replace(PIPE_APP, &common.xdata, p.to_xdata()?);
        common.layer = p.layer().into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(p.layer());
        let insert = crate::Insert { block, insert: v3(a), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives pipe `h` the parameters `p` (its length as it is now), redrawing it in place.
    pub fn regenerate_pipe(&mut self, h: crate::Handle, p: &PipeParams) -> Result<()> {
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let EntityKind::Insert(i) = &e.kind else { return Err(invalid("The object is not a block reference.")) };
        let pieces = p.generate(super::duct::reads_upside_down(i.rotation))?;
        let tags = xdata::replace(PIPE_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = p.layer();
        self.ensure_layer(layer);
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
    }

    /// Pipe `h` with its data and its two ends in drawing coordinates.
    pub fn pipe_ends(&self, h: crate::Handle) -> Option<(PipeParams, Vec2, Vec2)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = pipe_of(e)?;
        let a = Vec2::new(i.insert.x, i.insert.y);
        let b = a + Vec2::from_angle(i.rotation) * (p.length * i.scale.x);
        Some((p, a, b))
    }
}
