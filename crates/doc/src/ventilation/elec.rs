//! Electrical objects of a building plan: the devices (sockets, switches, luminaires, detectors, emergency lighting, distribution boards) and
//! the cables that join them in circuits. Like the pipework, each is a block reference to an anonymous block holding a symbol drawn to
//! scale (every symbol is drawn here, in code), its data travelling as extended data of its own registered application. A circuit is a name:
//! the cables and the devices that carry the same name are one circuit, and the cables of a circuit have a layer of their own, in the colour
//! and the line type of the style the circuit has (the style of the pipe networks, by name).
//!
//! The calculations (current, cable section, voltage drop) are in [`cadcraft_hvac`]; this module draws, stores and reads.

use cadcraft_geom::Vec2;
use serde::{Deserialize, Serialize};

use super::part::{circle, half_circle, line, poly, rect, text_in};
use super::pipe_networks::network_layer;
use super::{CableSpec, HvacPiece, HvacUnits, check_label_height, invalid, label, v3, xdata};
use crate::{EntityKind, RawTag, Result};

pub const CABLE_APP: &str = "CADCRAFT_CABLE";
pub const DEVICE_APP: &str = "CADCRAFT_ELECDEVICE";
const CABLE_FORMAT: &str = "CABLE1";
const DEVICE_FORMAT: &str = "ELECDEVICE1";
/// The sizes a distribution board is given, mm (its height, width and depth).
const BOARD_MM: (f64, f64) = (50.0, 5000.0);

/// The layers of the cables by use, of the devices and of the luminaires, of the distribution boards and of the safety devices.
pub const CABLE_POWER_LAYER: &str = "E-CABLE-POWER";
pub const CABLE_LIGHT_LAYER: &str = "E-CABLE-LIGHT";
pub const CABLE_DATA_LAYER: &str = "E-CABLE-DATA";
pub const CABLE_SAFETY_LAYER: &str = "E-CABLE-SAFETY";
pub const CABLE_BUS_LAYER: &str = "E-CABLE-BUS";
/// The remote control cables: from a remote control or a thermostat to its unit.
pub const CABLE_REMOTE_LAYER: &str = "E-CABLE-RMT";
pub const DEVICE_LAYER: &str = "E-DEVICE";
pub const LIGHT_LAYER: &str = "E-LIGHT";
pub const PANEL_LAYER: &str = "E-PANEL";
pub const SAFETY_LAYER: &str = "E-SAFETY";

/// The longest name of a circuit or of a device tag.
pub const MAX_CIRCUIT_NAME_CHARS: usize = 32;
fn default_label_mm() -> f64 {
    80.0
}

pub(super) fn yes() -> bool {
    true
}

/// What an address adds to the text beside an object: " @12" when the object has an address and it is shown, else nothing.
pub fn address_suffix(address: &str, show: bool) -> String {
    let a = address.trim();
    if show && !a.is_empty() { format!(" @{a}") } else { String::new() }
}

fn default_cos_phi() -> f64 {
    0.9
}

fn default_ambient() -> f64 {
    30.0
}

fn default_grouped() -> u32 {
    1
}

/// What a cable is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CableUsage {
    #[default]
    Power,
    Lighting,
    Data,
    Safety,
    /// A communication bus of the building controls (BACnet, Modbus, KNX).
    Bus,
    /// A remote control cable (télécommande): from a wired remote control or a thermostat to its unit. Not a bus, not a power cable.
    Remote,
}

impl CableUsage {
    pub const ALL: [CableUsage; 6] =
        [CableUsage::Power, CableUsage::Lighting, CableUsage::Data, CableUsage::Safety, CableUsage::Bus, CableUsage::Remote];

    pub fn name(self) -> &'static str {
        match self {
            CableUsage::Power => "power",
            CableUsage::Lighting => "lighting",
            CableUsage::Data => "data",
            CableUsage::Safety => "safety",
            CableUsage::Bus => "bus",
            CableUsage::Remote => "remote",
        }
    }

    /// Whether the cables of this use carry no power worth a calculation: data, bus and remote controls.
    pub fn is_signal(self) -> bool {
        matches!(self, CableUsage::Data | CableUsage::Bus | CableUsage::Remote)
    }

    pub fn parse(text: &str) -> Option<CableUsage> {
        let t = text.trim().to_ascii_lowercase();
        CableUsage::ALL.into_iter().find(|u| u.name() == t).or(match t.as_str() {
            "force" | "prises" | "puissance" => Some(CableUsage::Power),
            "eclairage" | "éclairage" | "lumiere" | "lumière" => Some(CableUsage::Lighting),
            "vdi" | "courants faibles" | "faibles" => Some(CableUsage::Data),
            "securite" | "sécurité" | "ssi" | "baes" => Some(CableUsage::Safety),
            "gtb" | "bacnet" | "modbus" | "knx" => Some(CableUsage::Bus),
            "telecommande" | "télécommande" | "commande" | "remotecontrol" | "remote control" | "thermostat" => Some(CableUsage::Remote),
            _ => None,
        })
    }

    pub fn layer(self) -> &'static str {
        match self {
            CableUsage::Power => CABLE_POWER_LAYER,
            CableUsage::Lighting => CABLE_LIGHT_LAYER,
            CableUsage::Data => CABLE_DATA_LAYER,
            CableUsage::Safety => CABLE_SAFETY_LAYER,
            CableUsage::Bus => CABLE_BUS_LAYER,
            CableUsage::Remote => CABLE_REMOTE_LAYER,
        }
    }

    /// The voltage drop a circuit of this use may have: lighting has the stricter limit.
    pub fn drop_usage(self) -> cadcraft_hvac::Usage {
        if self == CableUsage::Lighting { cadcraft_hvac::Usage::Lighting } else { cadcraft_hvac::Usage::Other }
    }
}

macro_rules! plain_enum {
    ($(#[$m:meta])* $name:ident => $calc:path { $($var:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub enum $name { #[default] $($var),+ }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var),+];
            pub fn name(self) -> &'static str { match self { $($name::$var => $text),+ } }
            pub fn parse(text: &str) -> Option<$name> {
                let t = text.trim().to_ascii_lowercase();
                $name::ALL.iter().copied().find(|v| v.name().to_ascii_lowercase() == t)
            }
            /// The same in the calculation crate.
            pub fn calc(self) -> $calc { match self { $($name::$var => <$calc>::$var),+ } }
        }
    };
}

plain_enum!(
    /// The metal of the conductors.
    ElecConductor => cadcraft_hvac::Conductor { Copper => "copper", Aluminium => "aluminium" }
);
plain_enum!(
    /// The insulation of the cable.
    ElecInsulation => cadcraft_hvac::Insulation { Pvc => "pvc", Pr => "pr" }
);
plain_enum!(
    /// The selection letter of the laying.
    ElecLaying => cadcraft_hvac::Laying { B => "B", C => "C", E => "E", F => "F" }
);
plain_enum!(
    /// Single-phase 230 V or three-phase 400 V.
    ElecPhases => cadcraft_hvac::Phases { Single => "single", Three => "three" }
);

/// A cable run: what its block reference keeps. The run starts at the block's origin and goes along its x axis for `length`; a cable that turns is
/// several of them, the circuit name joining them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CableParams {
    /// Length, in drawing units.
    pub length: f64,
    /// The circuit the cable is part of ("TD1-C3": the distribution board, then the circuit), or empty.
    #[serde(default)]
    pub circuit: String,
    #[serde(default)]
    pub usage: CableUsage,
    #[serde(default)]
    pub phases: ElecPhases,
    #[serde(default)]
    pub conductor: ElecConductor,
    #[serde(default)]
    pub insulation: ElecInsulation,
    #[serde(default)]
    pub laying: ElecLaying,
    /// The section, mm², or none while it is to be sized.
    #[serde(default)]
    pub section_mm2: Option<f64>,
    /// The number of conductors as the user wrote them (3G1.5, 2x0.75): none for the usual ones of the circuit's phases, 3G or 5G.
    #[serde(default)]
    pub cores: Option<u32>,
    /// Whether one of the conductors is an earth (the G of 3G1.5), when `cores` is given.
    #[serde(default)]
    pub earth: bool,
    /// Whether the cable is shielded.
    #[serde(default)]
    pub shielded: bool,
    /// The rating of the circuit breaker that protects the circuit, A, or none while it is to be chosen.
    #[serde(default)]
    pub protection_a: Option<f64>,
    #[serde(default = "default_cos_phi")]
    pub cos_phi: f64,
    /// The ambient temperature, °C, and the number of circuits laid touching this one (itself included).
    #[serde(default = "default_ambient")]
    pub ambient_c: f64,
    #[serde(default = "default_grouped")]
    pub grouped: u32,
    /// The cable's own length is longer than its drawing by this much (rises, drops, slack), metres.
    #[serde(default)]
    pub extra_m: f64,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    /// The label is not drawn (the cable keeps its circuit, section and sums).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_label: bool,
    /// The label is under the cable instead of above it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub label_below: bool,
    /// The cable as the user wrote it when it does not read as a section ("U/UTP cat 6 LSOH"): written in the label as it is; the cable has no section and
    /// is not sized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<String>,
    /// The part of the network the run belongs to, named by the user: listed with its length in the take-off.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subnetwork: String,
    /// The colour of the sub-network ("#rrggbb"), when it has its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnetwork_colour: Option<String>,
    #[serde(default)]
    pub units: HvacUnits,
}

impl CableParams {
    pub fn new(circuit: &str, usage: CableUsage, units: HvacUnits) -> CableParams {
        CableParams {
            length: 1.0,
            circuit: circuit.trim().to_string(),
            usage,
            phases: ElecPhases::Single,
            conductor: ElecConductor::Copper,
            insulation: ElecInsulation::Pvc,
            laying: ElecLaying::B,
            section_mm2: None,
            cores: None,
            earth: false,
            shielded: false,
            protection_a: None,
            cos_phi: default_cos_phi(),
            ambient_c: default_ambient(),
            grouped: 1,
            extra_m: 0.0,
            label_height_mm: default_label_mm(),
            hide_label: false,
            label_below: false,
            written: None,
            subnetwork: String::new(),
            subnetwork_colour: None,
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        super::check_subnetwork(&self.subnetwork, self.subnetwork_colour.as_deref())?;
        if !(self.length.is_finite() && self.length > 0.0 && self.length <= 1e9) {
            return Err(invalid("A cable has a usable length."));
        }
        check_circuit_name(&self.circuit, true)?;
        // As small as the conductors of a data or bus cable (0.22 mm², a cat. 6 pair 0.25 mm²); the circuit check warns of a power conductor below 1.5 mm².
        if self.section_mm2.is_some_and(|s| !(s.is_finite() && (0.05..=630.0).contains(&s))) {
            return Err(invalid("The section of a cable is 0.05 to 630 mm²."));
        }
        if self.written.as_deref().is_some_and(|w| w.trim().is_empty() || w.chars().count() > 80 || w.chars().any(char::is_control)) {
            return Err(invalid("The cable as it is written is 1 to 80 characters."));
        }
        if self.protection_a.is_some_and(|a| !(a.is_finite() && (1.0..=2500.0).contains(&a))) {
            return Err(invalid("The rating of a protection is 1 to 2500 A."));
        }
        if !(self.cos_phi.is_finite() && (0.1..=1.0).contains(&self.cos_phi)) {
            return Err(invalid("The power factor is 0.1 to 1."));
        }
        if !(self.ambient_c.is_finite() && (-20.0..=80.0).contains(&self.ambient_c)) {
            return Err(invalid("The ambient temperature is -20 to 80 °C."));
        }
        if !(1..=100).contains(&self.grouped) {
            return Err(invalid("The number of circuits laid together is 1 to 100."));
        }
        if !(self.extra_m.is_finite() && (0.0..=1000.0).contains(&self.extra_m)) {
            return Err(invalid("The extra length of a cable is 0 to 1000 m."));
        }
        self.units.validate()?;
        check_label_height(self.label_height_mm)
    }

    /// The length of the cable, metres: its drawing and the extra it has.
    pub fn length_m(&self) -> f64 {
        self.length * self.units.mm_per_unit / 1000.0 + self.extra_m
    }

    /// The conductors of the cable as it is written: the ones given, else 3G for a single-phase circuit (phase, neutral and earth), 5G for a three-phase one.
    pub fn cores(&self) -> u32 {
        self.cores.unwrap_or(if self.phases == ElecPhases::Three { 5 } else { 3 })
    }

    /// The cable as it is written ("3G2.5", "2x0.75 blindé"), or empty while the section is not chosen.
    pub fn size_text(&self) -> String {
        self.section_mm2.map_or(String::new(), |s| {
            let (cores, earth) = if self.cores.is_some() { (self.cores(), self.earth) } else { (self.cores(), true) };
            CableSpec { cores: Some(cores), earth, shielded: self.shielded, section_mm2: s }.text()
        })
    }

    /// What the section field of the cable says: the cable as it was written ("3G2.5", "2x0.75 blindé", or the section alone), empty while it is to be sized.
    pub fn spec_text(&self) -> String {
        if let Some(w) = &self.written {
            return w.clone();
        }
        self.section_mm2.map_or(String::new(), |s| CableSpec { cores: self.cores, earth: self.earth, shielded: self.shielded, section_mm2: s }.text())
    }

    /// Gives the cable the conductors, the earth, the shield and the section of `spec`.
    pub fn set_spec(&mut self, spec: &CableSpec) {
        (self.cores, self.earth, self.shielded, self.section_mm2) = (spec.cores, spec.earth, spec.shielded, Some(spec.section_mm2));
        self.written = None;
    }

    /// The layer of the cable: its use's, or the circuit's copy of it.
    pub fn layer(&self) -> &'static str {
        network_layer(self.usage.layer(), &self.circuit)
    }

    pub fn label_text(&self) -> String {
        let size = self.written.clone().unwrap_or_else(|| self.size_text());
        [self.circuit.trim().to_string(), size].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(" ")
    }

    /// The run drawn to scale: its centre line and a label. They are on layer 0, so they take the layer of the cable (its circuit's, or the one it was
    /// put on) and its colour.
    pub fn generate(&self, flip_label: bool) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let layer = super::subnetwork_layer("0", self.subnetwork_colour.as_deref());
        let mut pieces = vec![HvacPiece { layer, kind: line((0.0, 0.0), (self.length, 0.0)) }];
        let text = self.label_text();
        if !text.is_empty() && !self.hide_label {
            let h = self.units.to_units(self.label_height_mm);
            // Above the cable as it reads, or under it.
            let up = if flip_label { -1.0 } else { 1.0 } * if self.label_below { -1.0 } else { 1.0 };
            let mut t = label(Vec2::new(self.length / 2.0, 1.2 * h * up), h, text);
            if let EntityKind::Text(x) = &mut t {
                x.rotation = if flip_label { std::f64::consts::PI } else { 0.0 };
            }
            pieces.push(HvacPiece { layer, kind: t });
        }
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(CABLE_APP, CABLE_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<CableParams> {
        let p: CableParams = xdata::from_xdata(CABLE_APP, CABLE_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// A circuit name is letters, digits, spaces, hyphens, underscores and dots; empty is allowed where `empty_ok`.
pub fn check_circuit_name(name: &str, empty_ok: bool) -> Result<()> {
    if name.trim().is_empty() {
        return if empty_ok { Ok(()) } else { Err(invalid("A circuit needs a name.")) };
    }
    if name.chars().count() > MAX_CIRCUIT_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(invalid(format!("A circuit's name has at most {MAX_CIRCUIT_NAME_CHARS} characters, without control characters.")));
    }
    Ok(())
}

/// The distribution board a circuit name starts from: what comes before the first hyphen ("TD1" for "TD1-C3").
pub fn circuit_panel(circuit: &str) -> &str {
    circuit.split('-').next().unwrap_or("").trim()
}

/// The cable an object is, when it is a block reference carrying usable cable data.
pub fn cable_of(e: &crate::Entity) -> Option<CableParams> {
    match &e.kind {
        EntityKind::Insert(_) => CableParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// What a device is: the symbol it is drawn with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    #[default]
    Socket,
    SocketSpecial,
    Switch,
    SwitchDouble,
    SwitchTwoWay,
    PushButton,
    Dimmer,
    Presence,
    DataOutlet,
    Light,
    Recessed,
    DownLight,
    WallLight,
    StripLight,
    Emergency,
    ExitSign,
    Panel,
}

impl DeviceKind {
    pub const ALL: [DeviceKind; 17] = [
        DeviceKind::Socket,
        DeviceKind::SocketSpecial,
        DeviceKind::Switch,
        DeviceKind::SwitchDouble,
        DeviceKind::SwitchTwoWay,
        DeviceKind::PushButton,
        DeviceKind::Dimmer,
        DeviceKind::Presence,
        DeviceKind::DataOutlet,
        DeviceKind::Light,
        DeviceKind::Recessed,
        DeviceKind::DownLight,
        DeviceKind::WallLight,
        DeviceKind::StripLight,
        DeviceKind::Emergency,
        DeviceKind::ExitSign,
        DeviceKind::Panel,
    ];

    pub fn name(self) -> &'static str {
        match self {
            DeviceKind::Socket => "Socket",
            DeviceKind::SocketSpecial => "Dedicated socket",
            DeviceKind::Switch => "Switch",
            DeviceKind::SwitchDouble => "Double switch",
            DeviceKind::SwitchTwoWay => "Two-way switch",
            DeviceKind::PushButton => "Push button",
            DeviceKind::Dimmer => "Dimmer",
            DeviceKind::Presence => "Presence detector",
            DeviceKind::DataOutlet => "Data outlet",
            DeviceKind::Light => "Ceiling light",
            DeviceKind::Recessed => "Recessed light 600x600",
            DeviceKind::DownLight => "Downlight",
            DeviceKind::WallLight => "Wall light",
            DeviceKind::StripLight => "Strip light",
            DeviceKind::Emergency => "Emergency light",
            DeviceKind::ExitSign => "Exit sign",
            DeviceKind::Panel => "Distribution board",
        }
    }

    pub fn parse(text: &str) -> Option<DeviceKind> {
        let key = |t: &str| t.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
        let want = key(text);
        DeviceKind::ALL.into_iter().find(|k| key(k.name()) == want || key(&format!("{k:?}")) == want)
    }

    /// The prefix of the tag a new device gets ("P1", "I2", "L3").
    pub fn tag_prefix(self) -> &'static str {
        match self {
            DeviceKind::Socket | DeviceKind::SocketSpecial => "P",
            DeviceKind::Switch | DeviceKind::SwitchDouble | DeviceKind::SwitchTwoWay | DeviceKind::PushButton | DeviceKind::Dimmer => "I",
            DeviceKind::Presence => "D",
            DeviceKind::DataOutlet => "RJ",
            DeviceKind::Light | DeviceKind::Recessed | DeviceKind::DownLight | DeviceKind::WallLight | DeviceKind::StripLight => "L",
            DeviceKind::Emergency | DeviceKind::ExitSign => "S",
            DeviceKind::Panel => "TD",
        }
    }

    pub fn is_light(self) -> bool {
        matches!(self, DeviceKind::Light | DeviceKind::Recessed | DeviceKind::DownLight | DeviceKind::WallLight | DeviceKind::StripLight)
    }

    pub fn is_emergency(self) -> bool {
        matches!(self, DeviceKind::Emergency | DeviceKind::ExitSign)
    }

    pub fn layer(self) -> &'static str {
        match self {
            DeviceKind::Panel => PANEL_LAYER,
            DeviceKind::Emergency | DeviceKind::ExitSign => SAFETY_LAYER,
            k if k.is_light() => LIGHT_LAYER,
            _ => DEVICE_LAYER,
        }
    }

    /// The height a new device is put at, mm above the floor: sockets low, controls at a height within the 0.90 m to 1.30 m of the accessibility rules,
    /// luminaires at the ceiling; a distribution board, the height of its underside.
    pub fn default_height_mm(self) -> f64 {
        match self {
            DeviceKind::Socket | DeviceKind::SocketSpecial | DeviceKind::DataOutlet => 300.0,
            DeviceKind::Switch | DeviceKind::SwitchDouble | DeviceKind::SwitchTwoWay | DeviceKind::PushButton | DeviceKind::Dimmer => 1100.0,
            DeviceKind::Presence => 2500.0,
            DeviceKind::Light | DeviceKind::Recessed | DeviceKind::DownLight | DeviceKind::StripLight => 2600.0,
            DeviceKind::WallLight => 2000.0,
            DeviceKind::Emergency => 2200.0,
            DeviceKind::ExitSign => 2200.0,
            DeviceKind::Panel => 1000.0,
        }
    }

    /// The power a new device is given, W: a figure to type over (a design allowance for a socket, a typical figure for a luminaire).
    pub fn default_power_w(self) -> f64 {
        match self {
            DeviceKind::Socket => 500.0,
            DeviceKind::SocketSpecial => 3000.0,
            DeviceKind::Light | DeviceKind::Recessed => 40.0,
            DeviceKind::DownLight => 20.0,
            DeviceKind::WallLight => 20.0,
            DeviceKind::StripLight => 36.0,
            DeviceKind::Emergency | DeviceKind::ExitSign => 5.0,
            DeviceKind::Presence => 2.0,
            _ => 0.0,
        }
    }

    /// The luminous flux a new luminaire is given, lumen.
    pub fn default_flux_lm(self) -> Option<f64> {
        match self {
            DeviceKind::Light | DeviceKind::Recessed => Some(4000.0),
            DeviceKind::DownLight => Some(2000.0),
            DeviceKind::WallLight => Some(1500.0),
            DeviceKind::StripLight => Some(4000.0),
            _ => None,
        }
    }
}

/// A device: what its block reference keeps. Its place is the block's origin, turned by the reference's rotation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceParams {
    pub kind: DeviceKind,
    /// The tag written beside the symbol ("P12"); for a distribution board, its name ("TD1").
    #[serde(default)]
    pub name: String,
    /// The circuit the device is on; empty for none (a distribution board has none: it is where circuits start).
    #[serde(default)]
    pub circuit: String,
    /// The power it draws, W.
    #[serde(default)]
    pub power_w: f64,
    /// The luminous flux of a luminaire, lumen.
    #[serde(default)]
    pub flux_lm: Option<f64>,
    /// The height it is fitted at, mm above the floor; for a distribution board, the height of its underside.
    pub height_mm: f64,
    /// A distribution board's own size, mm: its height, its width and its depth (the plan shows the width by the depth). `None` for the usual 1000 x 600 x 400.
    #[serde(default)]
    pub board_height_mm: Option<f64>,
    #[serde(default)]
    pub board_width_mm: Option<f64>,
    #[serde(default)]
    pub board_depth_mm: Option<f64>,
    /// The rating of the main breaker of a distribution board, A.
    #[serde(default)]
    pub supply_a: Option<f64>,
    /// The address of the device on its bus (KNX, DALI…), written beside it when `show_address`.
    #[serde(default)]
    pub address: String,
    #[serde(default = "yes")]
    pub show_address: bool,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl DeviceParams {
    pub fn new(kind: DeviceKind, name: &str, units: HvacUnits) -> DeviceParams {
        DeviceParams {
            kind,
            name: name.trim().to_string(),
            circuit: String::new(),
            power_w: kind.default_power_w(),
            flux_lm: kind.default_flux_lm(),
            height_mm: kind.default_height_mm(),
            board_height_mm: None,
            board_width_mm: None,
            board_depth_mm: None,
            supply_a: (kind == DeviceKind::Panel).then_some(63.0),
            address: String::new(),
            show_address: true,
            label_height_mm: default_label_mm(),
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_circuit_name(&self.name, true)?;
        check_circuit_name(&self.circuit, true)?;
        check_circuit_name(&self.address, true)?;
        if !(self.power_w.is_finite() && (0.0..=1.0e7).contains(&self.power_w)) {
            return Err(invalid("The power of a device is 0 or more (W)."));
        }
        if self.flux_lm.is_some_and(|f| !(f.is_finite() && (0.0..=1.0e6).contains(&f))) {
            return Err(invalid("The flux of a luminaire is 0 or more (lm)."));
        }
        if !(self.height_mm.is_finite() && (0.0..=50_000.0).contains(&self.height_mm)) {
            return Err(invalid("The height of a device is 0 to 50000 mm."));
        }
        for (v, what) in [(self.board_height_mm, "height"), (self.board_width_mm, "width"), (self.board_depth_mm, "depth")] {
            if v.is_some_and(|v| !(v.is_finite() && (BOARD_MM.0..=BOARD_MM.1).contains(&v))) {
                return Err(invalid(format!("The {what} of a distribution board is {} to {} mm.", BOARD_MM.0, BOARD_MM.1)));
            }
        }
        if self.supply_a.is_some_and(|a| !(a.is_finite() && (1.0..=10_000.0).contains(&a))) {
            return Err(invalid("The rating of a board's supply is 1 to 10000 A."));
        }
        self.units.validate()?;
        check_label_height(self.label_height_mm)
    }

    pub fn layer(&self) -> &'static str {
        self.kind.layer()
    }

    /// A distribution board's height, width and depth, mm.
    pub fn board_mm(&self) -> (f64, f64, f64) {
        (self.board_height_mm.unwrap_or(1000.0), self.board_width_mm.unwrap_or(600.0), self.board_depth_mm.unwrap_or(400.0))
    }

    /// The text beside the symbol: its tag, and its circuit when it has one.
    pub fn label_text(&self) -> String {
        let t = [self.name.trim(), self.circuit.trim()].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(" ");
        format!("{t}{}", address_suffix(&self.address, self.show_address))
    }

    /// The symbol drawn to scale, centred on the origin, and its label under it.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let u = |mm: f64| self.units.to_units(mm);
        let layer = super::by_block(self.layer());
        let mut shapes: Vec<EntityKind> = Vec::new();
        let mut half_height = 150.0;
        match self.kind {
            DeviceKind::Socket | DeviceKind::SocketSpecial => {
                shapes.push(circle((0.0, 0.0), 110.0));
                shapes.push(line((0.0, 110.0), (0.0, 210.0)));
                shapes.push(line((-110.0, 0.0), (110.0, 0.0)));
                if self.kind == DeviceKind::SocketSpecial {
                    shapes.push(circle((0.0, 0.0), 150.0));
                    half_height = 210.0;
                }
            }
            DeviceKind::Switch | DeviceKind::SwitchDouble | DeviceKind::SwitchTwoWay | DeviceKind::PushButton | DeviceKind::Dimmer => {
                shapes.push(circle((0.0, 0.0), 80.0));
                shapes.push(line((57.0, 57.0), (230.0, 230.0)));
                match self.kind {
                    DeviceKind::SwitchDouble => shapes.push(line((57.0, -57.0), (230.0, -230.0))),
                    DeviceKind::SwitchTwoWay => shapes.push(line((200.0, 260.0), (290.0, 170.0))),
                    DeviceKind::PushButton => shapes.push(circle((0.0, 0.0), 40.0)),
                    DeviceKind::Dimmer => shapes.push(poly(&[(-60.0, -50.0), (60.0, -50.0), (0.0, 50.0)], true)),
                    _ => {}
                }
                half_height = 230.0;
            }
            DeviceKind::Presence => {
                shapes.push(rect(0.0, 0.0, 130.0, 130.0));
                shapes.push(text_in((0.0, 0.0), 90.0, "D"));
            }
            DeviceKind::DataOutlet => {
                shapes.push(rect(0.0, 0.0, 150.0, 110.0));
                shapes.push(text_in((0.0, 0.0), 90.0, "RJ"));
            }
            DeviceKind::Light => {
                shapes.push(circle((0.0, 0.0), 170.0));
                shapes.push(line((-120.0, -120.0), (120.0, 120.0)));
                shapes.push(line((-120.0, 120.0), (120.0, -120.0)));
                half_height = 170.0;
            }
            // A 600 x 600 luminaire for a false ceiling: its square, crossed.
            DeviceKind::Recessed => {
                shapes.push(rect(0.0, 0.0, 300.0, 300.0));
                shapes.push(line((-300.0, -300.0), (300.0, 300.0)));
                shapes.push(line((-300.0, 300.0), (300.0, -300.0)));
                half_height = 300.0;
            }
            DeviceKind::DownLight => {
                shapes.push(circle((0.0, 0.0), 120.0));
                shapes.push(circle((0.0, 0.0), 45.0));
                half_height = 120.0;
            }
            DeviceKind::WallLight => {
                shapes.push(half_circle((0.0, 0.0), 150.0));
                shapes.push(line((-150.0, 0.0), (150.0, 0.0)));
            }
            DeviceKind::StripLight => {
                shapes.push(rect(0.0, 0.0, 600.0, 75.0));
                shapes.push(line((-600.0, 0.0), (600.0, 0.0)));
                half_height = 75.0;
            }
            DeviceKind::Emergency => {
                shapes.push(rect(0.0, 0.0, 170.0, 100.0));
                shapes.push(text_in((0.0, 0.0), 80.0, "BAES"));
                half_height = 100.0;
            }
            DeviceKind::ExitSign => {
                shapes.push(rect(0.0, 0.0, 200.0, 100.0));
                shapes.push(line((-140.0, 0.0), (140.0, 0.0)));
                shapes.push(line((140.0, 0.0), (80.0, 40.0)));
                shapes.push(line((140.0, 0.0), (80.0, -40.0)));
                half_height = 100.0;
            }
            DeviceKind::Panel => {
                // Its width by its depth in plan.
                let (_, w, d) = self.board_mm();
                shapes.push(rect(0.0, 0.0, w / 2.0, d / 2.0));
                shapes.push(line((-w / 2.0, d / 2.0), (w / 2.0, -d / 2.0)));
                half_height = d / 2.0;
            }
        }
        let mut pieces: Vec<HvacPiece> = shapes
            .into_iter()
            .map(|mut kind| {
                kind.transform(&cadcraft_geom::Mat3::scale_about(Vec2::ZERO, 1.0 / self.units.mm_per_unit));
                HvacPiece { layer, kind }
            })
            .collect();
        let text = self.label_text();
        if !text.is_empty() {
            let h = u(self.label_height_mm);
            pieces.push(HvacPiece { layer, kind: label(Vec2::new(0.0, -(u(half_height) + 1.1 * h)), h, text) });
        }
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(DEVICE_APP, DEVICE_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<DeviceParams> {
        let p: DeviceParams = xdata::from_xdata(DEVICE_APP, DEVICE_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The device an object is, when it is a block reference carrying usable device data.
pub fn device_of(e: &crate::Entity) -> Option<DeviceParams> {
    match &e.kind {
        EntityKind::Insert(_) => DeviceParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// The colour an electrical layer is created with.
pub fn elec_layer_color(name: &str) -> Option<cadcraft_color::Color> {
    use cadcraft_color::{Color, Rgb};
    match name {
        CABLE_POWER_LAYER => Some(Color::True(Rgb(0xd0, 0x30, 0x30))),
        CABLE_LIGHT_LAYER => Some(Color::True(Rgb(0xe0, 0xb0, 0x20))),
        CABLE_DATA_LAYER => Some(Color::True(Rgb(0x30, 0x90, 0xd0))),
        CABLE_SAFETY_LAYER | SAFETY_LAYER => Some(Color::True(Rgb(0x20, 0xb0, 0x50))),
        CABLE_BUS_LAYER => Some(Color::True(Rgb(0x90, 0x60, 0xd0))),
        // Pink, to be told from the bus (violet) and from the power, lighting, data and safety cables.
        CABLE_REMOTE_LAYER => Some(Color::True(Rgb(0xff, 0x6f, 0xb5))),
        DEVICE_LAYER => Some(Color::True(Rgb(0xc0, 0x60, 0x60))),
        LIGHT_LAYER => Some(Color::True(Rgb(0xf0, 0xd0, 0x40))),
        PANEL_LAYER => Some(Color::True(Rgb(0xe0, 0x80, 0x20))),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds a cable run from `a` to `b`; its `length` is set from the two points. The circuit gets a style (a colour of the palette) when it has none.
    pub fn add_cable(&mut self, space: &crate::Space, mut common: crate::Common, mut p: CableParams, a: Vec2, b: Vec2) -> Result<crate::Handle> {
        if !a.is_finite() || !b.is_finite() {
            return Err(invalid("A cable needs usable end points."));
        }
        p.length = a.dist(b);
        let rotation = (b - a).angle();
        if !p.circuit.trim().is_empty() && self.network_style(&p.circuit).is_none() {
            self.set_network_style(&p.circuit, None, None)?;
        }
        let pieces = p.generate(super::duct::reads_upside_down(rotation))?;
        common.xdata = xdata::replace(CABLE_APP, &common.xdata, p.to_xdata()?);
        common.layer = p.layer().into();
        let block = self.hvac_block(pieces);
        // Its pieces are on layer 0: the cable's layer is made here, with its colour.
        self.ensure_hvac_layer(p.layer());
        let insert = crate::Insert { block, insert: v3(a), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives cable `h` the parameters `p` (its length as it is now), redrawing it in place.
    pub fn regenerate_cable(&mut self, h: crate::Handle, p: &CableParams) -> Result<()> {
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let EntityKind::Insert(i) = &e.kind else { return Err(invalid("The object is not a cable.")) };
        let flip = super::duct::reads_upside_down(i.rotation);
        // A cable put on a layer of the user's keeps it; one on its circuit's layer follows its circuit.
        let on_its_own_layer = cable_of(e).is_none_or(|old| e.common.layer == old.layer());
        if !p.circuit.trim().is_empty() && self.network_style(&p.circuit).is_none() {
            self.set_network_style(&p.circuit, None, None)?;
        }
        let pieces = p.generate(flip)?;
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let tags = xdata::replace(CABLE_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = p.layer();
        self.ensure_hvac_layer(layer);
        if on_its_own_layer {
            self.modify_entity(h, |e| e.common.layer = layer.into())?;
        }
        Ok(())
    }

    /// Cable `h` with its data and its two ends in drawing coordinates.
    pub fn cable_ends(&self, h: crate::Handle) -> Option<(CableParams, Vec2, Vec2)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = cable_of(e)?;
        let a = Vec2::new(i.insert.x, i.insert.y);
        let b = a + Vec2::from_angle(i.rotation) * (p.length * i.scale.x);
        Some((p, a, b))
    }

    /// Drags grip `index` of cable `h` (0 and 2 its ends, 1 its middle: the run moves) to `to`, redrawing it.
    pub fn stretch_cable_grip(&mut self, h: crate::Handle, index: usize, to: Vec2) -> Result<bool> {
        let Some((mut p, a, b)) = self.cable_ends(h) else { return Ok(false) };
        if !to.is_finite() || index > 2 {
            return Ok(false);
        }
        let Some(EntityKind::Insert(i)) = self.entity(h).map(|e| e.kind.clone()) else { return Ok(false) };
        let d = to - a.mid(b);
        let (na, nb) = match index {
            0 => (to, b),
            1 => (a + d, b + d),
            _ => (a, to),
        };
        let len = na.dist(nb);
        if i.scale.x <= 0.0 || len * i.scale.x * p.units.mm_per_unit < 1.0 {
            return Ok(false);
        }
        p.length = len;
        let rotation = (nb - na).angle();
        self.modify_entity(h, |e| {
            if let EntityKind::Insert(ins) = &mut e.kind {
                ins.insert = v3(na);
                ins.rotation = rotation;
            }
        })?;
        self.regenerate_cable(h, &p)?;
        Ok(true)
    }

    /// Adds a device of `p` at `at`, turned `rotation` radians.
    pub fn add_device(
        &mut self,
        space: &crate::Space,
        mut common: crate::Common,
        p: &DeviceParams,
        at: Vec2,
        rotation: f64,
    ) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("A device needs a usable position."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(DEVICE_APP, &common.xdata, p.to_xdata()?);
        let layer = p.layer();
        common.layer = layer.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(layer);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives device `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_device(&mut self, h: crate::Handle, p: &DeviceParams) -> Result<()> {
        let pieces = p.generate()?;
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let tags = xdata::replace(DEVICE_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = p.layer();
        self.ensure_layer(layer);
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
    }
}

impl crate::Drawing {
    /// Gives back their colour to the layers of cables left white (a few versions made them white), as a new cable's layer gets it: the colour of its
    /// use, or of its circuit's network. Returns the layers given their colour.
    pub fn restore_cable_layer_colours(&mut self) -> Vec<String> {
        use cadcraft_color::Color;
        let fixes: Vec<(String, Color)> = self
            .layers
            .iter()
            .filter(|l| l.name.to_ascii_uppercase().starts_with("E-CABLE") && l.color == Color::WHITE)
            .filter_map(|l| {
                let c = self.network_look(&l.name).map(|(c, _)| c).or_else(|| self.hvac_colour(&l.name))?;
                (c != Color::WHITE).then(|| (l.name.clone(), c))
            })
            .collect();
        for (name, c) in &fixes {
            if let Some(l) = self.layers.iter_mut().find(|l| l.name == *name) {
                l.color = *c;
            }
        }
        fixes.into_iter().map(|(n, _)| n).collect()
    }
}

/// The layer of a cable of `usage` on `circuit`, for callers that have no cable yet.
pub fn cable_layer_of(usage: CableUsage, circuit: &str) -> &'static str {
    network_layer(usage.layer(), circuit)
}
