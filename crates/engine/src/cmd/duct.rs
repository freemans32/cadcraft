//! Ventilation ducts in plan: AHUADD places an air handling unit, DUCTADD draws round duct runs
//! one segment at a time (a run started on another one cuts it, so branches join), DUCTEDIT
//! changes runs already drawn and DUCTSIZE sizes every network from its terminals back to its unit.
//! The network is read from the drawing each time (`Drawing::duct_networks`), never stored.

use cadcraft_doc::{
    AhuKind, AhuParams, AirDirection, Drawing, DuctInsulation, DuctKind, DuctParams, DuctShape, EntityKind, Handle, HvacUnits, InsulationSide,
    MAX_DUCTS,
};
use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Governing, size_network};
use serde_json::{Value, json};

use super::{CommandSpec, bad, f64_or, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ahuadd", "Air Handling Unit", run_ahuadd)
            .menu(&["Draw", "HVAC", "Air Handling Unit"])
            .params(
                "{at, name?, type?: doubleFlow|supplyUnit|extractUnit|vmcBox|stackedDoubleFlow (default doubleFlow), width? (mm), depth? (mm), height? (mm), model?, weight? (kg), rotation? (degrees), \
                 labelHeight? (mm)} → {handle, name} (its ducts and terminals say supply or extract)",
            )
            .interactive(|s| Ok(Box::new(AhuAddM::new(s)))),
        CommandSpec::new("ahuedit", "Edit Air Handling Unit", run_ahuedit)
            .params(
                "{handle | handles, name?, type?: doubleFlow|supplyUnit|extractUnit|vmcBox|stackedDoubleFlow, width? (mm), depth? (mm), height? (mm or null), model?, weight? (kg or null), altitude? (m: the underside of the unit, or null), coils? ({chilledWater?, hotWater?, changeOver?, directExpansion?, electric?, electricPhases? (single|three; three by default)}: the power of each coil, kW, or null for none; an electric coil is a load of the unit's circuit; the water pipes that end at the unit take their load from it), portSizes? (one per collar: a diameter, 800x400 or null: the duct that starts on it takes that section), portLevels? (one per collar: an altitude in metres or null, [] for none: the duct that starts on it is at that altitude, and a step joins it to the run after it where they differ), \
                 ports? (the direction of the air at each collar, supply|extract|fresh|exhaust, one per collar: four for a double-flux unit or a box, two for a single-flux unit; [] restores the kind's), \
                 flow? | extractFlow? | freshFlow? | exhaustFlow? (m³/h typed for that direction: sizing keeps it and sizes the run that leaves the unit for it; null or auto gives it back)} → {changed}",
            ),
        CommandSpec::new("ductadd", "Duct", run_ductadd)
            .menu(&["Draw", "HVAC", "Duct"])
            .params(
                "{points (2 or more; one round duct per segment), diameter? (mm), kind?: auto|main|branch|connection, \
                 width? + height? (mm; a rectangular duct), air?: auto|supply|extract|fresh|exhaust (auto: from the terminals it serves), flexible? (true: a flexible duct), insulation? (outside, inside or none) + insulationThickness? (mm, default 30), labelHeight? (mm)} → {handles}",
            )
            .interactive(|s| Ok(Box::new(DuctAddM::new(s)))),
        CommandSpec::new("ductedit", "Edit Duct", run_ductedit)
            .params("{handle | handles, diameter? (mm, or auto: sizing chooses the size) | width? and/or height? (mm; a rectangular section: a side alone changes that side), kind?, air?, flexible? (true or false), shape? (round or rectangular: each duct keeps its size and takes the other shape), insulation? (outside, inside or none), insulationThickness? (mm, default 30), coneAuto? (true: a reducer's cone goes back to the symmetric 15 degree one, after its corners were set by hand), cutStart? / cutEnd? (degrees, -75 to 75, or 0 or null: the end of the run cut on the slant, a whistle; not drawn at an end with an elbow or a cone), flow? (m³/h: sizing keeps it and sizes the run for it; auto gives it back), altitude? (m: the underside, or null), endAltitude? (m, or null for a level run: the altitude of the underside at the far end, so that the run slopes from its altitude to this one; its true length, longer than its length in plan, is the one that loses pressure and is counted), label? (false: the label is not drawn), extra? (m: the run is longer than drawn by this, rises and drops, 0 to 1000), subnetwork? (a name for the ducts given, \"\" to take them out: listed with its length in the take-off), subnetworkColour? (#rrggbb for every duct of the sub-network, or null)} → {changed}"),
        CommandSpec::new("ductconnect", "Connect Terminals", run_ductconnect).menu(&["Draw", "HVAC", "Connect Terminals"]).params(
            "{terminals? (handles; default every terminal no duct reaches), maxDistance? (mm, default 3000)} → {created (flexible ducts), skipped}",
        ),
        CommandSpec::new("ductsize", "Size Ducts", run_ductsize).menu(&["Draw", "HVAC", "Size Ducts"]).params(
            "{ahu? (handle; default every unit), maxVelocityMain? (m/s), maxVelocityBranch?, maxVelocityConnection?, \
             autoCross? (false: the lower of two ducts that cross is not drawn broken by itself), balanceTerminals? (false keeps the terminals' flows; default true: a space's flow is shared between its terminals first), maxGradient? (Pa/m), margin? (mm kept back from the height available, default 100), maxAspect? (width / height of a rectangular duct, default 4), sideStep? (mm, default 50), \
             minSide? (mm, default 100), maxSide? (mm, default 2000)} → {networks: [{ahu, totalFlow (m³/h), requiredPressure (Pa), criticalPath, segments, warnings}]}",
        ),
    ]
}

/// What AHUADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AhuLastUsed {
    pub kind: AhuKind,
    /// Width and depth the user gave, mm; the kind's own when none.
    pub size: Option<(f64, f64)>,
}

impl Default for AhuLastUsed {
    fn default() -> Self {
        AhuLastUsed { kind: AhuKind::DoubleFlow, size: None }
    }
}

impl AhuLastUsed {
    /// Width and depth to draw, mm.
    pub fn size_mm(&self) -> (f64, f64) {
        self.size.unwrap_or_else(|| self.kind.default_size_mm())
    }
}

/// What DUCTADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctLastUsed {
    pub diameter_mm: f64,
    pub kind: DuctKind,
    /// `None`: not fixed, sizing takes it from the terminals the duct serves.
    pub air: Option<AirDirection>,
    pub flexible: bool,
    /// Which edge of the duct the points clicked follow.
    pub justification: DuctJust,
}

/// Which edge of a duct the points clicked follow (Tab while DUCTADD asks for a point): the middle (its axis), the bottom edge (the duct is above
/// the clicked line, as the line runs to the right) or the top one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DuctJust {
    #[default]
    Middle,
    Bottom,
    Top,
}

impl DuctJust {
    fn next(self) -> DuctJust {
        match self {
            DuctJust::Middle => DuctJust::Bottom,
            DuctJust::Bottom => DuctJust::Top,
            DuctJust::Top => DuctJust::Middle,
        }
    }
    fn name(self) -> &'static str {
        match self {
            DuctJust::Middle => "Middle",
            DuctJust::Bottom => "Bottom",
            DuctJust::Top => "Top",
        }
    }
    /// Which side of the clicked line the axis is on: +1 to its left (as it runs), -1 to its right, 0 on it.
    fn side(self) -> f64 {
        match self {
            DuctJust::Middle => 0.0,
            DuctJust::Bottom => 1.0,
            DuctJust::Top => -1.0,
        }
    }
}

/// A size the user gives a duct: a diameter, or width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Imposed {
    Round(f64),
    Rect(f64, f64),
}

impl Imposed {
    /// Reads `250` as a diameter and `400x250` (or `400×250`, `400*250`) as width and height, mm.
    fn parse(text: &str) -> Option<Imposed> {
        let ok = |v: f64| v.is_finite() && (cadcraft_doc::DUCT_DIAMETER_MM.0..=cadcraft_doc::DUCT_DIAMETER_MM.1).contains(&v);
        match text.trim().split_once(['x', 'X', '×', '*']) {
            Some((w, h)) => {
                let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
                (ok(w) && ok(h)).then_some(Imposed::Rect(w, h))
            }
            None => text.trim().parse::<f64>().ok().filter(|d| ok(*d)).map(Imposed::Round),
        }
    }

    /// `params` with this size, kept by sizing.
    fn apply(self, p: &mut DuctParams) {
        p.imposed = true;
        match self {
            Imposed::Round(d) => (p.shape, p.diameter_mm, p.width_mm, p.height_mm) = (DuctShape::Round, d, 0.0, 0.0),
            Imposed::Rect(w, h) => {
                let equivalent =
                    cadcraft_hvac::equivalent_diameter_mm(w, h).clamp(cadcraft_doc::DUCT_DIAMETER_MM.0, cadcraft_doc::DUCT_DIAMETER_MM.1);
                (p.shape, p.diameter_mm, p.width_mm, p.height_mm) = (DuctShape::Rectangular, equivalent, w, h);
            }
        }
    }
}

impl Default for DuctLastUsed {
    fn default() -> Self {
        DuctLastUsed { diameter_mm: 200.0, kind: DuctKind::Auto, air: None, flexible: false, justification: DuctJust::Middle }
    }
}

/// Most ducts one DUCTEDIT changes.
const MAX_EDITED: usize = 10_000;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

fn positive(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a positive number"))),
    }
}

fn non_negative(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number, 0 or more")))
        }
    }
}

/// A duct's `air`: absent leaves it, `"auto"` lets sizing decide, else supply or extract.
fn duct_air_param(cmd: &str, p: &Value) -> Result<Option<Option<AirDirection>>> {
    match str_param(p, "air") {
        None => Ok(None),
        Some(t) if t.trim().eq_ignore_ascii_case("auto") => Ok(Some(None)),
        Some(t) => AirDirection::parse(t).map(|a| Some(Some(a))).ok_or_else(|| bad(cmd, "`air` must be auto, supply, extract, fresh or exhaust")),
    }
}

fn ahu_kind_param(cmd: &str, p: &Value) -> Result<Option<AhuKind>> {
    match str_param(p, "type") {
        Some(t) => {
            AhuKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "`type` must be doubleFlow, supplyUnit, extractUnit, vmcBox or stackedDoubleFlow"))
        }
        None => Ok(None),
    }
}

fn bool_param(cmd: &str, p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false"))),
    }
}

fn kind_param(cmd: &str, p: &Value) -> Result<Option<DuctKind>> {
    match str_param(p, "kind") {
        Some(t) => DuctKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "`kind` must be auto, main, branch or connection")),
        None => Ok(None),
    }
}

// =====================================================================================
// AHUADD
// =====================================================================================

fn units_of(s: &Session) -> Result<HvacUnits> {
    Ok(HvacUnits::of(s.doc()?))
}

fn next_ahu_name(d: &Drawing) -> String {
    let n = d.model.iter().filter(|e| cadcraft_doc::ahu_of(e).is_some()).count() + 1;
    format!("CTA{n}")
}

fn add_ahu(s: &mut Session, p: &AhuParams, at: Vec2, rotation: f64) -> Result<Handle> {
    p.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    s.doc_mut()?.add_ahu(&space, common, p, at, rotation).map_err(doc_err)
}

fn run_ahuadd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ahuadd";
    let at = super::point_req(CMD, p, "at")?;
    let kind = ahu_kind_param(CMD, p)?.unwrap_or_default();
    let (default_width, default_depth) = kind.default_size_mm();
    let params = AhuParams {
        name: str_param(p, "name").map(str::to_string).unwrap_or_else(|| s.doc().map(next_ahu_name).unwrap_or_else(|_| "CTA".into())),
        kind,
        ports: vec![],
        width_mm: positive(CMD, p, "width")?.unwrap_or(default_width),
        depth_mm: positive(CMD, p, "depth")?.unwrap_or(default_depth),
        flow_m3h: None,
        pressure_pa: None,
        extract_flow_m3h: None,
        extract_pressure_pa: None,
        fresh_flow_m3h: None,
        fresh_pressure_pa: None,
        exhaust_flow_m3h: None,
        exhaust_pressure_pa: None,
        typed_flows: Vec::new(),
        port_sizes: Vec::new(),
        port_levels_mm: Vec::new(),
        height_mm: positive(CMD, p, "height")?,
        model: str_param(p, "model").unwrap_or_default().to_string(),
        weight_kg: positive(CMD, p, "weight")?,
        coils: coils_param(CMD, p, cadcraft_doc::AhuCoils::default())?,
        electric_kw: None,
        circuit: String::new(),
        address: String::new(),
        show_address: true,
        label_height_mm: positive(CMD, p, "labelHeight")?.unwrap_or(100.0),
        // The altitude of the underside: the one given (null for none), else the kind's own (a horizontal double-flow unit hangs at 2.70 m).
        bottom_mm: super::altitude_param(CMD, p)?.unwrap_or_else(|| kind.default_bottom_mm()),
        units: units_of(s)?,
    };
    let h = add_ahu(s, &params, at, f64_or(p, "rotation", 0.0).to_radians())?;
    Ok(json!({ "handle": h.hex(), "name": params.name }))
}

/// The `coils` of AHUADD and AHUEDIT: `{chilledWater?, hotWater?, changeOver?, directExpansion?}` in kW, each a number or null
/// (no such coil); what is not named stays as in `base`.
fn coils_param(cmd: &str, p: &Value, base: cadcraft_doc::AhuCoils) -> Result<cadcraft_doc::AhuCoils> {
    let Some(c) = p.get("coils").filter(|v| !v.is_null()) else { return Ok(base) };
    let pick = |key: &str, old: Option<f64>| -> Result<Option<f64>> {
        match c.get(key) {
            None => Ok(old),
            Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|x| x.is_finite() && *x > 0.0)
                .map(Some)
                .ok_or_else(|| bad(cmd, format!("`coils.{key}` must be a power in kW or null"))),
        }
    };
    Ok(cadcraft_doc::AhuCoils {
        chilled_kw: pick("chilledWater", base.chilled_kw)?,
        hot_kw: pick("hotWater", base.hot_kw)?,
        change_over_kw: pick("changeOver", base.change_over_kw)?,
        direct_expansion_kw: pick("directExpansion", base.direct_expansion_kw)?,
        electric_kw: pick("electric", base.electric_kw)?,
        electric_single_phase: match c.get("electricPhases").and_then(Value::as_str).map(|t| t.trim().to_ascii_lowercase()).as_deref() {
            None => base.electric_single_phase,
            Some("single" | "mono" | "monophase" | "1") => true,
            Some("three" | "tri" | "triphase" | "3") => false,
            Some(_) => return Err(bad(cmd, "`coils.electricPhases` is single or three")),
        },
    })
}

/// The `ports` of AHUEDIT: a direction for each collar.
fn ports_param(cmd: &str, p: &Value) -> Result<Option<Vec<AirDirection>>> {
    let Some(v) = p.get("ports") else { return Ok(None) };
    let list = v.as_array().filter(|a| a.len() <= 8).ok_or_else(|| bad(cmd, "`ports` is a list of directions"))?;
    let roles = list.iter().map(|x| x.as_str().and_then(AirDirection::parse)).collect::<Option<Vec<_>>>();
    roles.map(Some).ok_or_else(|| bad(cmd, "each port is supply, extract, fresh or exhaust"))
}

fn run_ahuedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ahuedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} units")));
    }
    let (width, depth) = (positive(CMD, p, "width")?, positive(CMD, p, "depth")?);
    // Height and weight: a number sets them, null clears them.
    let clearable = |key: &str| -> Result<Option<Option<f64>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(_) => Ok(Some(positive(CMD, p, key)?)),
        }
    };
    let (height, weight) = (clearable("height")?, clearable("weight")?);
    let electric = clearable("electricKw")?;
    // The size of each connection's orifice, one per collar: a diameter, "800x400", or null for none.
    let port_sizes: Option<Vec<Option<cadcraft_doc::RiserSize>>> = match p.get("portSizes") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) if list.len() <= 8 => Some(list.iter().map(|v| super::riser::size_value(CMD, v)).collect::<Result<_>>()?),
        Some(_) => return Err(bad(CMD, "`portSizes` is a list of sizes, one per collar")),
    };
    // The altitude of each connection, metres (null for none), one per collar: the duct that starts on it is at that altitude unless it has its own.
    let port_levels: Option<Vec<Option<f64>>> = match p.get("portLevels") {
        None => None,
        Some(Value::Null) => Some(Vec::new()),
        Some(Value::Array(list)) if list.len() <= 8 => Some(
            list.iter()
                .map(|v| match v {
                    Value::Null => Ok(None),
                    v => v
                        .as_f64()
                        .filter(|m| m.is_finite())
                        .map(|m| Some(m * 1000.0))
                        .ok_or_else(|| bad(CMD, "`portLevels` are altitudes in metres or null")),
                })
                .collect::<Result<_>>()?,
        ),
        Some(_) => return Err(bad(CMD, "`portLevels` is a list of altitudes in metres, one per collar")),
    };
    let kind = ahu_kind_param(CMD, p)?;
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut a = d.entity(*h).and_then(|e| cadcraft_doc::ahu_of(e)).ok_or_else(|| other("Select air handling units only."))?;
        if let Some(n) = str_param(p, "name") {
            a.name = n.to_string();
        }
        if kind.is_some_and(|k| k != a.kind) {
            a.ports.clear();
        }
        a.kind = kind.unwrap_or(a.kind);
        if let Some(ports) = ports_param(CMD, p)? {
            a.ports = ports;
        }
        a.width_mm = width.unwrap_or(a.width_mm);
        a.depth_mm = depth.unwrap_or(a.depth_mm);
        if let Some(v) = height {
            a.height_mm = v;
        }
        if let Some(v) = weight {
            a.weight_kg = v;
        }
        if let Some(m) = str_param(p, "model") {
            a.model = m.trim().to_string();
        }
        if let Some(v) = electric {
            a.electric_kw = v;
        }
        if let Some(c) = str_param(p, "circuit") {
            a.circuit = c.trim().to_string();
        }
        if let Some(t) = str_param(p, "address") {
            a.address = t.trim().to_string();
        }
        a.coils = coils_param(CMD, p, a.coils)?;
        if let Some(v) = super::altitude_param(CMD, p)? {
            a.bottom_mm = v;
        }
        // The flow of each direction typed on the unit: sizing keeps it, and sizes the run that leaves the unit for it.
        for (key, air) in [
            ("flow", AirDirection::Supply),
            ("extractFlow", AirDirection::Extract),
            ("freshFlow", AirDirection::Fresh),
            ("exhaustFlow", AirDirection::Exhaust),
        ] {
            if let Some(f) = typed_flow_param(CMD, p, key)? {
                a.set_typed_flow(air, f);
            }
        }
        if let Some(sizes) = &port_sizes {
            a.port_sizes = sizes.clone();
        }
        if let Some(levels) = &port_levels {
            a.port_levels_mm = levels.clone();
        }
        a.validate().map_err(doc_err)?;
        next.push((*h, a));
    }
    let d = s.doc_mut()?;
    for (h, a) in &next {
        d.regenerate_ahu(*h, a).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq)]
enum AhuAsk {
    Location,
    Type,
    Rotation(Vec2),
    Name,
    Width,
    Depth,
}

struct AhuAddM {
    last: AhuLastUsed,
    name: String,
    ask: AhuAsk,
}

impl AhuAddM {
    fn new(s: &Session) -> Self {
        AhuAddM { last: s.last_used.ahu.clone(), name: s.doc().map(next_ahu_name).unwrap_or_else(|_| "CTA".into()), ask: AhuAsk::Location }
    }
    fn settings(&self, s: &mut Session) {
        let (w, d) = self.last.size_mm();
        s.echo(format!("Current settings: Type = {}, Name = {}, Width = {w}, Depth = {d}", self.last.kind.name(), self.name));
    }
    fn size(t: &str) -> Result<f64> {
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= cadcraft_doc::MAX_AHU_SIZE_MM)
            .ok_or_else(|| other("Requires a size in millimetres."))
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, &i, text.as_deref()) {
            (AhuAsk::Name, _, Some(t)) if !t.is_empty() => self.name = t.to_string(),
            (AhuAsk::Width, _, Some(t)) => self.last.size = Some((Self::size(t)?, self.last.size_mm().1)),
            (AhuAsk::Depth, _, Some(t)) => self.last.size = Some((self.last.size_mm().0, Self::size(t)?)),
            (AhuAsk::Type, _, Some(t)) => {
                self.last =
                    AhuLastUsed { kind: AhuKind::parse(t).ok_or_else(|| other("Enter DoubleFlow, Vertical, Supply, Extract or Vmc."))?, size: None };
            }
            (AhuAsk::Rotation(at), _, _) => {
                let angle = match (&i, text.as_deref()) {
                    (Input::Point(p), _) => (*p - at).angle(),
                    (_, Some(t)) => {
                        t.parse::<f64>().ok().filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle in degrees."))?.to_radians()
                    }
                    _ => 0.0,
                };
                let (width_mm, depth_mm) = self.last.size_mm();
                let params = AhuParams {
                    name: self.name.clone(),
                    kind: self.last.kind,
                    ports: vec![],
                    width_mm,
                    depth_mm,
                    flow_m3h: None,
                    pressure_pa: None,
                    extract_flow_m3h: None,
                    extract_pressure_pa: None,
                    fresh_flow_m3h: None,
                    fresh_pressure_pa: None,
                    exhaust_flow_m3h: None,
                    exhaust_pressure_pa: None,
                    typed_flows: Vec::new(),
                    port_sizes: Vec::new(),
                    port_levels_mm: Vec::new(),
                    height_mm: None,
                    model: String::new(),
                    weight_kg: None,
                    coils: Default::default(),
                    electric_kw: None,
                    circuit: String::new(),
                    address: String::new(),
                    show_address: true,
                    label_height_mm: 100.0,
                    bottom_mm: self.last.kind.default_bottom_mm(),
                    units: units_of(s)?,
                };
                add_ahu(s, &params, at, angle)?;
                s.last_used.ahu = self.last.clone();
                self.name = s.doc().map(next_ahu_name).unwrap_or_else(|_| "CTA".into());
            }
            (_, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = AhuAsk::Location;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for AhuAddM {
    fn name(&self) -> &'static str {
        "AHUADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            AhuAsk::Location => Prompt::new("Specify location of air handling unit", Accept::POINT).kw(&["Type", "Name", "Width", "Depth"]),
            AhuAsk::Rotation(at) => Prompt::new("Specify rotation angle", Accept::POINT_OR_NUMBER).base(at).default("0"),
            AhuAsk::Name => Prompt::new("Enter unit name", Accept::TEXT).default(self.name.clone()),
            AhuAsk::Width => Prompt::new("Enter unit width (mm)", Accept::NUMBER).default(format!("{}", self.last.size_mm().0)),
            AhuAsk::Depth => Prompt::new("Enter unit depth (mm)", Accept::NUMBER).default(format!("{}", self.last.size_mm().1)),
            AhuAsk::Type => Prompt::new("Enter unit type", super::curves::KW)
                .kw(&["DoubleFlow", "Vertical", "Supply", "Extract", "Vmc"])
                .default(self.last.kind.name()),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != AhuAsk::Location {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => {
                self.ask = match k.as_str() {
                    "Name" => AhuAsk::Name,
                    "Type" => AhuAsk::Type,
                    "Width" => AhuAsk::Width,
                    "Depth" => AhuAsk::Depth,
                    _ => AhuAsk::Location,
                };
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                self.ask = AhuAsk::Rotation(p);
                Ok(Step::Continue)
            }
            Input::Enter | Input::Cancel => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<cadcraft_doc::Entity> {
        let Ok(units) = units_of(s) else { return Vec::new() };
        let (width_mm, depth_mm) = self.last.size_mm();
        let p = AhuParams {
            name: self.name.clone(),
            kind: self.last.kind,
            ports: vec![],
            width_mm,
            depth_mm,
            flow_m3h: None,
            pressure_pa: None,
            extract_flow_m3h: None,
            extract_pressure_pa: None,
            fresh_flow_m3h: None,
            fresh_pressure_pa: None,
            exhaust_flow_m3h: None,
            exhaust_pressure_pa: None,
            typed_flows: Vec::new(),
            port_sizes: Vec::new(),
            port_levels_mm: Vec::new(),
            height_mm: None,
            model: String::new(),
            weight_kg: None,
            coils: Default::default(),
            electric_kw: None,
            circuit: String::new(),
            address: String::new(),
            show_address: true,
            label_height_mm: 100.0,
            bottom_mm: self.last.kind.default_bottom_mm(),
            units,
        };
        match self.ask {
            AhuAsk::Location => p.preview(c, 0.0),
            AhuAsk::Rotation(at) if !c.near(at, 1e-9) => p.preview(at, (c - at).angle()),
            AhuAsk::Rotation(at) => p.preview(at, 0.0),
            _ => Vec::new(),
        }
    }
}

// =====================================================================================
// DUCTADD
// =====================================================================================

/// Distance, in millimetres, within which a point is on a duct.
const ON_DUCT_MM: f64 = 5.0;

/// Adds a duct from `a` to `b` with `params`. A run that starts or ends on the inside of another
/// cuts it there, so the two meet at their ends and form a tee.
fn add_run(s: &mut Session, params: &DuctParams, a: Vec2, b: Vec2) -> Result<Handle> {
    if a.near(b, 1e-9) || !a.is_finite() || !b.is_finite() {
        return Err(other("A duct needs two different points."));
    }
    DuctParams { length: a.dist(b), ..params.clone() }.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    let d = s.doc_mut()?;
    let tol = params.units.to_units(ON_DUCT_MM);
    for p in [a, b] {
        let hit = d.model.iter().filter_map(|e| d.duct_ends(e.handle).map(|_| e.handle)).collect::<Vec<_>>();
        for h in hit {
            if d.split_duct_at(&space, h, p, tol).map_err(doc_err)?.is_some() {
                break;
            }
        }
    }
    d.add_duct(&space, common, params.clone(), a, b).map_err(doc_err)
}

fn duct_params(s: &Session, last: &DuctLastUsed, label_mm: f64) -> Result<DuctParams> {
    Ok(DuctParams {
        length: 1.0,
        diameter_mm: last.diameter_mm,
        imposed: false,
        kind: last.kind,
        kind_found: None,
        air: last.air.unwrap_or_default(),
        air_set: last.air.is_some(),
        flow_m3h: None,
        velocity: None,
        shape: DuctShape::Round,
        width_mm: 0.0,
        height_mm: 0.0,
        flexible: last.flexible,
        reducer: None,
        reducer_len_mm: None,
        reducer_shape: None,
        label_shift_mm: None,
        kept_edge: None,
        hide_label: false,
        extra_m: 0.0,
        subnetwork: String::new(),
        subnetwork_colour: None,
        cut_deg: [0.0, 0.0],
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_set: false,
        insulation: None,
        cap_start: false,
        cap_end: false,
        flow_forward: None,
        open_flow_m3h: None,
        gaps_mm: Vec::new(),
        covers: Vec::new(),
        jogs: Vec::new(),
        level_mm: None,
        end_level_mm: None,
        steps: Vec::new(),
        bottom_mm: None,
        label_height_mm: label_mm,
        units: units_of(s)?,
    })
}

fn run_ductadd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductadd";
    let pts = super::points_param(p, "points").filter(|v| v.len() >= 2).ok_or_else(|| bad(CMD, "`points` (two or more [x, y]) is required"))?;
    if pts.len() > MAX_DUCTS {
        return Err(bad(CMD, format!("at most {MAX_DUCTS} points")));
    }
    let mut last = DuctLastUsed::default();
    if let Some(d) = positive(CMD, p, "diameter")? {
        last.diameter_mm = d;
    }
    last.kind = kind_param(CMD, p)?.unwrap_or(last.kind);
    last.air = duct_air_param(CMD, p)?.unwrap_or(last.air);
    last.flexible = bool_param(CMD, p, "flexible")?.unwrap_or(last.flexible);
    let mut params = duct_params(s, &last, positive(CMD, p, "labelHeight")?.unwrap_or(100.0))?;
    params.imposed = p.get("diameter").is_some_and(|v| !v.is_null());
    params.insulation = insulation_param(CMD, p, None)?.flatten();
    params.level_mm = match p.get("altitude") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(CMD, "`altitude` must be a number of metres"))? * 1000.0),
    };
    // Width and height together give a rectangular duct; one alone is a mistake.
    match (positive(CMD, p, "width")?, positive(CMD, p, "height")?) {
        (Some(w), Some(h)) => Imposed::Rect(w, h).apply(&mut params),
        (None, None) => {}
        _ => return Err(bad(CMD, "`width` and `height` go together")),
    }
    let mut handles = Vec::new();
    for w in pts.windows(2) {
        if let [a, b] = w {
            handles.push(add_run(s, &params, *a, *b)?.hex());
        }
    }
    Ok(json!({ "handles": handles }))
}

#[derive(Clone, Copy, PartialEq)]
enum DuctAsk {
    Point,
    Diameter,
    Kind,
    Air,
    Material,
}

struct DuctAddM {
    last: DuctLastUsed,
    /// The size the user typed, if any: sizing then leaves it.
    imposed: Option<Imposed>,
    /// The points clicked: the line the duct follows, by its middle or by one of its edges.
    pts: Vec<Vec2>,
    /// How far each run's axis lies to the left of that line (drawing units, negative to its right), one per run.
    offs: Vec<f64>,
    handles: Vec<Handle>,
    ask: DuctAsk,
}

/// The axis of run `k` along the clicked line `pts`, each run moved sideways by its offset; where two runs meet with the same offset their axes meet at the
/// corner of the offset lines.
fn shifted(pts: &[Vec2], offs: &[f64], k: usize) -> (Vec2, Vec2) {
    let n = |i: usize| (pts[i + 1] - pts[i]).normalized().perp();
    let same = |i: usize, j: usize| (offs[i] - offs[j]).abs() < 1e-9;
    let (nk, o) = (n(k), offs[k]);
    let corner = |at: Vec2, other: Vec2| {
        let d = 1.0 + nk.dot(other);
        // A turn back on itself has no corner: the run is just moved.
        if d < 0.2 { at + nk * o } else { at + (nk + other) * (o / d) }
    };
    let a = if k > 0 && same(k - 1, k) { corner(pts[k], n(k - 1)) } else { pts[k] + nk * o };
    let b = if k + 1 < offs.len() && same(k, k + 1) { corner(pts[k + 1], n(k + 1)) } else { pts[k + 1] + nk * o };
    (a, b)
}

impl DuctAddM {
    fn new(s: &Session) -> Self {
        DuctAddM { last: s.last_used.duct.clone(), imposed: None, pts: Vec::new(), offs: Vec::new(), handles: Vec::new(), ask: DuctAsk::Point }
    }
    /// How far the axis of a run laid now lies to the left of the line clicked.
    fn offset(&self, s: &Session) -> f64 {
        self.params(s).map_or(0.0, |p| p.units.to_units(p.plan_width_mm()) / 2.0 * self.last.justification.side())
    }
    /// Lays run `k` again (or for the first time) from the clicked line, in place of the one there was.
    fn lay(&mut self, s: &mut Session, k: usize) -> Result<()> {
        let (a, b) = shifted(&self.pts, &self.offs, k);
        let mut params = self.params(s)?;
        // Laid by an edge: the run keeps that edge on the line when its width changes later (sizing, Properties).
        params.kept_edge = self.offs.get(k).copied().filter(|o| *o != 0.0).map(f64::signum);
        if let Some(old) = self.handles.get(k).copied() {
            s.doc_mut()?.remove_entity(old);
        }
        let h = add_run(s, &params, a, b)?;
        match self.handles.get_mut(k) {
            Some(slot) => *slot = h,
            None => self.handles.push(h),
        }
        Ok(())
    }
    fn settings(&self, s: &mut Session) {
        let d = self.imposed_text();
        let material = if self.last.flexible { "Flexible" } else { "Rigid" };
        s.echo(format!(
            "Current settings: Diameter = {d}, Kind = {}, Air = {}, Material = {material}",
            self.last.kind.name(),
            self.last.air.map_or("auto", AirDirection::name)
        ));
    }
    /// The size to show: what was typed, or Auto.
    fn imposed_text(&self) -> String {
        match self.imposed {
            Some(Imposed::Round(d)) => format!("{d}"),
            Some(Imposed::Rect(w, h)) => format!("{w}x{h}"),
            None => "Auto".to_string(),
        }
    }
    fn params(&self, s: &Session) -> Result<DuctParams> {
        let mut p = duct_params(s, &self.last, 100.0)?;
        if let Some(size) = self.imposed {
            size.apply(&mut p);
        }
        Ok(p)
    }
    fn point(&mut self, s: &mut Session, p: Vec2) -> Result<Step> {
        if let Some(last) = self.pts.last().copied() {
            if last.near(p, 1e-9) {
                return Ok(Step::Continue);
            }
            let off = self.offset(s);
            let k = self.offs.len();
            self.pts.push(p);
            self.offs.push(off);
            // The run before takes the corner it now shares with this one.
            let laid =
                self.lay(s, k).and_then(|()| if k > 0 && off != 0.0 && (self.offs[k - 1] - off).abs() < 1e-9 { self.lay(s, k - 1) } else { Ok(()) });
            match laid {
                Ok(()) => s.last_used.duct = self.last.clone(),
                Err(e) => {
                    self.pts.pop();
                    self.offs.pop();
                    s.echo(e.to_string());
                    return Ok(Step::Continue);
                }
            }
            s.last_point = p;
            return Ok(Step::Continue);
        }
        self.pts.push(p);
        s.last_point = p;
        Ok(Step::Continue)
    }
    fn keyword(&mut self, s: &mut Session, k: &str) -> Result<Step> {
        match k {
            "Diameter" => self.ask = DuctAsk::Diameter,
            "Kind" => self.ask = DuctAsk::Kind,
            "Air" => self.ask = DuctAsk::Air,
            "Material" => self.ask = DuctAsk::Material,
            "Undo" => {
                if self.pts.len() >= 2
                    && let Some(h) = self.handles.pop()
                {
                    s.doc_mut()?.remove_entity(h);
                    self.offs.pop();
                }
                self.pts.pop();
                // The run before gives back the corner it shared.
                if let Some(k) = self.offs.len().checked_sub(1)
                    && self.offs[k] != 0.0
                    && let Err(e) = self.lay(s, k)
                {
                    s.echo(e.to_string());
                }
            }
            _ => {}
        }
        Ok(Step::Continue)
    }
    fn option(&mut self, s: &mut Session, i: &Input) -> Result<Step> {
        let text = match i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, text.as_deref()) {
            (DuctAsk::Diameter, Some(t)) if t.eq_ignore_ascii_case("auto") => self.imposed = None,
            (DuctAsk::Diameter, Some(t)) => {
                let size = Imposed::parse(t).ok_or_else(|| other("Requires a diameter in millimetres, a size such as 400x250, or Auto."))?;
                if let Imposed::Round(d) = size {
                    self.last.diameter_mm = d;
                }
                self.imposed = Some(size);
            }
            (DuctAsk::Kind, Some(t)) => self.last.kind = DuctKind::parse(t).ok_or_else(|| other("Enter Auto, Main, Branch or Connection."))?,
            (DuctAsk::Air, Some(t)) if t.eq_ignore_ascii_case("auto") => self.last.air = None,
            (DuctAsk::Air, Some(t)) => {
                self.last.air = Some(AirDirection::parse(t).ok_or_else(|| other("Enter Auto, Supply, Extract, Fresh or Exhaust."))?)
            }
            (DuctAsk::Material, Some(t)) => {
                self.last.flexible = match t.to_ascii_lowercase().as_str() {
                    "rigid" | "r" => false,
                    "flexible" | "flex" | "f" => true,
                    _ => return Err(other("Enter Rigid or Flexible.")),
                }
            }
            _ => {}
        }
        self.ask = DuctAsk::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for DuctAddM {
    fn name(&self) -> &'static str {
        "DUCTADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn cycle_alignment(&mut self, s: &mut Session) -> Option<String> {
        if self.ask != DuctAsk::Point {
            return None;
        }
        self.last.justification = self.last.justification.next();
        s.last_used.duct.justification = self.last.justification;
        Some(format!("Justification = {}", self.last.justification.name()))
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            DuctAsk::Diameter => Prompt::new("Enter duct diameter (mm), width x height (mm) or [Auto]", Accept::TEXT).default(self.imposed_text()),
            DuctAsk::Kind => {
                Prompt::new("Enter duct kind", super::curves::KW).kw(&["Auto", "Main", "Branch", "Connection"]).default(self.last.kind.name())
            }
            DuctAsk::Air => Prompt::new("Enter air direction", super::curves::KW)
                .kw(&["Auto", "Supply", "Extract", "Fresh", "Exhaust"])
                .default(self.last.air.map_or("auto", AirDirection::name)),
            DuctAsk::Material => Prompt::new("Enter duct material", super::curves::KW).kw(&["Rigid", "Flexible"]).default(if self.last.flexible {
                "Flexible"
            } else {
                "Rigid"
            }),
            DuctAsk::Point => match self.pts.last().copied() {
                None => Prompt::new("Specify start point of duct", Accept::POINT).kw(&["Diameter", "Kind", "Air", "Material"]),
                last => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base_opt(last),
            },
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != DuctAsk::Point {
            return self.option(s, &i);
        }
        match i {
            Input::Keyword(k) => self.keyword(s, &k),
            Input::Point(p) => self.point(s, p),
            Input::Enter | Input::Cancel => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        self.pts.last().map(|l| vec![EntityKind::Line(cadcraft_doc::Line { a: l.to3(0.0), b: c.to3(0.0) })]).unwrap_or_default()
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<cadcraft_doc::Entity> {
        // The run under way, as wide as it will be, with the edge Tab chose on the line followed.
        let (DuctAsk::Point, Ok(p)) = (self.ask, self.params(s)) else { return Vec::new() };
        let off = self.offset(s);
        match self.pts.last().copied() {
            // Before the first point: a stretch of duct on the cursor, to choose where it hangs.
            None => {
                let at = c + Vec2::new(0.0, off);
                p.preview(at, at + Vec2::new(4.0 * p.units.to_units(p.plan_width_mm()), 0.0))
            }
            Some(last) if !last.near(c, 1e-9) => {
                let (mut pts, mut offs) = (self.pts.clone(), self.offs.clone());
                pts.push(c);
                offs.push(off);
                let (a, b) = shifted(&pts, &offs, offs.len() - 1);
                p.preview(a, b)
            }
            _ => Vec::new(),
        }
    }
}

// =====================================================================================
// DUCTEDIT
// =====================================================================================

/// A flow a call types, m³/h: a number sets it, null or "auto" gives it back to sizing (`Some(None)`), nothing leaves it (`None`).
fn typed_flow_param(cmd: &str, p: &Value, key: &str) -> Result<Option<Option<f64>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => Ok(Some(None)),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite() && *f > 0.0 && *f <= 10_000_000.0)
            .map(|f| Some(Some(f)))
            .ok_or_else(|| bad(cmd, format!("`{key}` must be a flow above 0 (m\u{b3}/h) or auto"))),
    }
}

/// The insulation a call gives: `insulation` is "outside", "inside" or "none" (or null), `insulationThickness` its thickness in mm (the current one, else
/// 30, when only the side is given). `None` when the call says nothing about it.
fn insulation_param(cmd: &str, p: &Value, current: Option<DuctInsulation>) -> Result<Option<Option<DuctInsulation>>> {
    let thickness = match p.get("insulationThickness") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|t| t.is_finite()).ok_or_else(|| bad(cmd, "`insulationThickness` must be a number of millimetres"))?),
    };
    match p.get("insulation") {
        None => Ok(thickness.zip(current).map(|(t, c)| Some(DuctInsulation { thickness_mm: t, ..c }))),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) if matches!(t.trim().to_ascii_lowercase().as_str(), "none" | "aucune" | "") => Ok(Some(None)),
        Some(Value::String(t)) => {
            let side = InsulationSide::parse(t).ok_or_else(|| bad(cmd, "`insulation` is outside, inside or none"))?;
            Ok(Some(Some(DuctInsulation { side, thickness_mm: thickness.or(current.map(|c| c.thickness_mm)).unwrap_or(30.0) })))
        }
        Some(_) => Err(bad(cmd, "`insulation` is outside, inside or none")),
    }
}

fn run_ductedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductedit";
    let hs = super::arch::handles_param(CMD, p)?;
    let label = super::elec::label_param(CMD, p)?;
    let sub = super::subnet::SubnetEdit::read(CMD, p)?;
    // The sub-networks of the ducts and their colours, for a duct named into one.
    let known: Vec<(String, Option<String>)> =
        s.doc()?.model.iter().filter_map(|e| cadcraft_doc::duct_of(e)).map(|x| (x.subnetwork, x.subnetwork_colour)).collect();
    let extra = match p.get("extra") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| bad(CMD, "`extra` must be a number of metres, 0 or more"))?),
    };
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} ducts")));
    }
    // A number imposes the diameter, `width` and `height` together a rectangular section; "auto" gives
    // the size back to sizing.
    // `width` or `height` alone changes that side: the other is the duct's own (or its diameter when it is round), for each duct selected.
    let size: Option<Option<Imposed>> = match (p.get("diameter"), positive(CMD, p, "width")?, positive(CMD, p, "height")?) {
        (_, Some(w), Some(h)) => Some(Some(Imposed::Rect(w, h))),
        (_, Some(w), None) => Some(Some(Imposed::Rect(w, 0.0))),
        (_, None, Some(h)) => Some(Some(Imposed::Rect(0.0, h))),
        (None | Some(Value::Null), None, None) => None,
        (Some(Value::String(t)), None, None) if t.trim().eq_ignore_ascii_case("auto") => Some(None),
        (Some(v), None, None) => Some(Some(Imposed::Round(
            v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| bad(CMD, "`diameter` must be a number of millimetres or auto"))?,
        ))),
    };
    // The shape of the section: a round duct becomes rectangular (its diameter wide, three quarters of it high, to 50 mm) or the other way round.
    let shape = match str_param(p, "shape").map(|t| t.trim().to_ascii_lowercase()) {
        None => None,
        Some(t) if t == "round" || t == "rectangular" => Some(t == "rectangular"),
        Some(_) => return Err(bad(CMD, "`shape` is round or rectangular")),
    };
    let (kind, air) = (kind_param(CMD, p)?, duct_air_param(CMD, p)?);
    let flexible = bool_param(CMD, p, "flexible")?;
    // The altitude of the underside, metres: a number sets it, null gives it back to the height zone.
    let altitude: Option<Option<f64>> = match p.get("altitude") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => {
            Some(Some(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(CMD, "`altitude` must be a number of metres or null"))? * 1000.0))
        }
    };
    // The altitude of the far end, metres: the run slopes from its altitude to this one (null: level).
    let end_altitude: Option<Option<f64>> = match p.get("endAltitude") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => {
            Some(Some(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(CMD, "`endAltitude` must be a number of metres or null"))? * 1000.0))
        }
    };
    // The flow of an open end (a duct on a unit's collar that leads to no terminal); null gives it back to the balance.
    let open_flow: Option<Option<f64>> = match p.get("openFlow") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => {
            Some(Some(v.as_f64().filter(|f| f.is_finite() && *f > 0.0).ok_or_else(|| bad(CMD, "`openFlow` must be a flow above 0 (m³/h) or null"))?))
        }
    };
    // The cut on the slant at each end (a whistle), degrees: 0 or null makes it square again.
    let cut = |key: &str| -> Result<Option<f64>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(0.0)),
            Some(v) => {
                Ok(Some(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(CMD, format!("`{key}` must be a number of degrees or null")))?))
            }
        }
    };
    let (cut_start, cut_end) = (cut("cutStart")?, cut("cutEnd")?);
    let cone_auto = bool_param(CMD, p, "coneAuto")? == Some(true);
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let (mut dp, ..) = d.duct_ends(*h).ok_or_else(|| other("Select ducts only."))?;
        let rectangular = dp.shape == DuctShape::Rectangular;
        match size {
            Some(Some(Imposed::Rect(w, h))) => {
                let (own_w, own_h) = if rectangular { (dp.width_mm, dp.height_mm) } else { (dp.diameter_mm, dp.diameter_mm) };
                Imposed::Rect(if w > 0.0 { w } else { own_w }, if h > 0.0 { h } else { own_h }).apply(&mut dp);
            }
            Some(Some(imposed)) => imposed.apply(&mut dp),
            Some(None) => dp.imposed = false,
            None => {}
        }
        // (the size typed above may have made it rectangular already: it is kept as typed)
        let rectangular = dp.shape == DuctShape::Rectangular;
        match shape {
            Some(true) if !rectangular => {
                let w = dp.diameter_mm;
                Imposed::Rect(w, ((w * 0.75 / 50.0).round() * 50.0).max(100.0)).apply(&mut dp);
            }
            Some(false) if rectangular => Imposed::Round(dp.diameter_mm).apply(&mut dp),
            _ => {}
        }
        if let Some(k) = kind {
            (dp.kind, dp.kind_found) = (k, None);
        }
        match air {
            Some(Some(a)) => (dp.air, dp.air_set) = (a, true),
            Some(None) => dp.air_set = false,
            None => {}
        }
        dp.flexible = flexible.unwrap_or(dp.flexible);
        dp.open_flow_m3h = open_flow.unwrap_or(dp.open_flow_m3h);
        if let Some(ins) = insulation_param(CMD, p, dp.insulation)? {
            dp.insulation = ins;
        }
        // A flow typed on the run: sizing keeps it and sizes the run for it; auto gives it back.
        match typed_flow_param(CMD, p, "flow")? {
            Some(Some(f)) => (dp.flow_m3h, dp.flow_set) = (Some(f), true),
            Some(None) => dp.flow_set = false,
            None => {}
        }
        dp.cut_deg = [cut_start.unwrap_or(dp.cut_deg[0]), cut_end.unwrap_or(dp.cut_deg[1])];
        if cone_auto {
            (dp.reducer_len_mm, dp.reducer_shape) = (None, None);
        }
        if let Some(e) = end_altitude {
            dp.end_level_mm = e;
        }
        if let Some(a) = altitude {
            dp.level_mm = a;
        }
        if let Some(show) = label {
            dp.hide_label = !show;
        }
        dp.extra_m = extra.unwrap_or(dp.extra_m);
        sub.name_run(&mut dp.subnetwork, &mut dp.subnetwork_colour, |n| {
            super::subnet::known_colour(known.iter().map(|(a, b)| (a.as_str(), b.as_deref())), n)
        });
        dp.validate().map_err(doc_err)?;
        next.push((*h, dp));
    }
    // A colour given to a sub-network goes to all of its runs, selected or not.
    if let Some(c) = &sub.colour {
        let names: Vec<String> = next.iter().map(|(_, x)| x.subnetwork.clone()).filter(|n| !n.is_empty()).collect();
        if names.is_empty() {
            return Err(bad(CMD, "name a sub-network before giving it a colour"));
        }
        for (_, x) in &mut next {
            if !x.subnetwork.is_empty() {
                x.subnetwork_colour.clone_from(c);
            }
        }
        let d = s.doc()?;
        let picked: Vec<Handle> = next.iter().map(|(h, _)| *h).collect();
        let others: Vec<(Handle, DuctParams)> = d
            .model
            .iter()
            .filter(|e| !picked.contains(&e.handle))
            .filter_map(|e| cadcraft_doc::duct_of(e).map(|x| (e.handle, x)))
            .filter(|(_, x)| names.iter().any(|n| n.eq_ignore_ascii_case(&x.subnetwork)))
            .map(|(h, mut x)| {
                x.subnetwork_colour.clone_from(c);
                (h, x)
            })
            .collect();
        next.extend(others);
    }
    let d = s.doc_mut()?;
    for (h, dp) in &next {
        d.regenerate_duct(*h, dp).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

// =====================================================================================
// DUCTCONNECT
// =====================================================================================

/// Most terminals one DUCTCONNECT connects.
const MAX_CONNECTED: usize = 2_000;
/// How far, in millimetres, DUCTCONNECT looks for a rigid duct.
const CONNECT_REACH_MM: f64 = 3_000.0;

/// The terminals a DUCTCONNECT works on: the ones named, else every one no duct reaches.
fn terminals_to_connect(d: &Drawing, only: Option<&[Handle]>) -> Result<Vec<(Handle, cadcraft_doc::TerminalParams, Vec2)>> {
    let all: Vec<_> = d
        .model
        .iter()
        .filter_map(|e| match (&e.kind, cadcraft_doc::air_terminal_of(e)) {
            (EntityKind::Insert(i), Some(t)) => Some((e.handle, t, Vec2::new(i.insert.x, i.insert.y))),
            _ => None,
        })
        .collect();
    let wanted: Vec<Handle> = match only {
        Some(hs) => {
            if let Some(h) = hs.iter().find(|h| !all.iter().any(|t| t.0 == **h)) {
                return Err(other(format!("{} is not an air terminal.", h.hex())));
            }
            hs.to_vec()
        }
        None => {
            let nets = d.duct_networks().map_err(doc_err)?;
            match nets.first() {
                Some(first) => first.unconnected.clone(),
                None => all.iter().map(|t| t.0).collect(),
            }
        }
    };
    let mut out: Vec<_> = all.into_iter().filter(|t| wanted.contains(&t.0)).collect();
    out.truncate(MAX_CONNECTED);
    Ok(out)
}

/// The nearest rigid duct of `air` to `at`: its foot on the axis, the distance in drawing units, the width it shows in plan (mm) and the way it runs.
fn nearest_rigid(d: &Drawing, air: AirDirection, at: Vec2) -> Option<(Vec2, f64, f64, Vec2)> {
    d.model
        .iter()
        .filter_map(|e| d.duct_ends(e.handle))
        .filter(|(p, ..)| !p.flexible && (!p.air_set || p.air == air))
        .map(|(p, a, b)| {
            let along = (b - a).normalized();
            let foot = a + along * (at - a).dot(along).clamp(0.0, a.dist(b));
            (foot, foot.dist(at), p.plan_width_mm(), along)
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
}

/// How long the rigid collar that joins a flexible duct `neck_mm` across to a duct `trunk_mm` wide is, mm, drawn at `angle` to the duct: the cone of the
/// reduction (15°) at least, from the axis of the duct and a little straight run to put the flexible on.
fn collar_length_mm(trunk_mm: f64, neck_mm: f64, angle: f64) -> f64 {
    let cone = ((trunk_mm - neck_mm) / 2.0 / 15f64.to_radians().tan()).max(0.0);
    let to_edge = trunk_mm / 2.0 / angle.sin().abs().max(0.3);
    (2.5 * cone).max(to_edge + 200.0).clamp(300.0, 1000.0)
}

fn run_ductconnect(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductconnect";
    let reach_mm = positive(CMD, p, "maxDistance")?.unwrap_or(CONNECT_REACH_MM);
    let only: Option<Vec<Handle>> = match p.get("terminals") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => Some(
            a.iter().map(|v| v.as_str().and_then(Handle::parse_hex).ok_or_else(|| bad(CMD, "`terminals` must be handles"))).collect::<Result<_>>()?,
        ),
        Some(_) => return Err(bad(CMD, "`terminals` must be a list of handles")),
    };
    let terminals = terminals_to_connect(s.doc()?, only.as_deref())?;
    let mm = units_of(s)?.mm_per_unit;
    let (mut created, mut skipped) = (Vec::new(), Vec::new());
    for (h, t, at) in terminals {
        let Some((foot, dist, trunk_mm, along)) = nearest_rigid(s.doc()?, t.air, at).filter(|(_, dist, ..)| dist * mm <= reach_mm) else {
            skipped.push(json!({ "terminal": h.hex(), "reason": "no rigid duct of its air direction within reach" }));
            continue;
        };
        if dist * mm < 1.0 {
            skipped.push(json!({ "terminal": h.hex(), "reason": "already on a duct" }));
            continue;
        }
        // A flexible duct as wide as the terminal's neck, from the rigid duct's axis to the terminal.
        let params = DuctParams {
            length: 1.0,
            diameter_mm: t.model.neck_mm,
            imposed: true,
            kind: DuctKind::Connection,
            kind_found: None,
            air: t.air,
            air_set: true,
            flow_m3h: None,
            velocity: None,
            shape: DuctShape::Round,
            width_mm: 0.0,
            height_mm: 0.0,
            flexible: true,
            reducer: None,
            reducer_len_mm: None,
            reducer_shape: None,
            label_shift_mm: None,
            kept_edge: None,
            hide_label: false,
            extra_m: 0.0,
            subnetwork: String::new(),
            subnetwork_colour: None,
            cut_deg: [0.0, 0.0],
            elbow: None,
            trim_start_mm: 0.0,
            trim_end_mm: 0.0,
            flow_set: false,
            insulation: None,
            cap_start: false,
            cap_end: false,
            flow_forward: None,
            open_flow_m3h: None,
            gaps_mm: Vec::new(),
            covers: Vec::new(),
            jogs: Vec::new(),
            level_mm: None,
            end_level_mm: None,
            steps: Vec::new(),
            bottom_mm: None,
            label_height_mm: 100.0,
            units: units_of(s)?,
        };
        // A duct wider than the neck gets a rigid collar of the neck's size, which the cone of the reduction is drawn on (as it is for any run that starts on
        // the side of a duct of another width); the flexible duct goes on from its end. Too near the terminal for it, the flexible leaves the duct itself.
        let dir = (at - foot).normalized();
        let collar_mm = collar_length_mm(trunk_mm, t.model.neck_mm, dir.angle() - along.angle());
        if trunk_mm - t.model.neck_mm > 5.0 && dist * mm >= collar_mm + 150.0 {
            let end = foot + dir * (collar_mm / mm);
            let collar = DuctParams { flexible: false, ..params.clone() };
            created.push(add_run(s, &collar, foot, end)?.hex());
            created.push(add_run(s, &params, end, at)?.hex());
        } else {
            created.push(add_run(s, &params, foot, at)?.hex());
        }
    }
    Ok(json!({ "created": created, "skipped": skipped }))
}

// =====================================================================================
// DUCTSIZE
// =====================================================================================

fn section_name(s: cadcraft_hvac::Section) -> &'static str {
    match s {
        cadcraft_hvac::Section::Round => "round",
        cadcraft_hvac::Section::Rectangular => "rectangular",
    }
}

fn governing_name(g: Governing) -> &'static str {
    match g {
        Governing::Velocity => "velocity",
        Governing::Friction => "friction",
        Governing::Imposed => "imposed",
        Governing::Largest => "largest",
    }
}

/// Sizes `net`, then adds a reducer to each run whose diameter differs from the run it continues
/// from and sizes again for the losses. The fittings do not change a diameter, so the second pass
/// only adds their loss.
pub(super) fn size_with_reducers(
    net: &cadcraft_hvac::Network,
    criteria: &DesignCriteria,
) -> std::result::Result<cadcraft_hvac::NetworkReport, cadcraft_hvac::HvacError> {
    let first = size_network(net, criteria)?;
    let mut with = net.clone();
    for (i, seg) in with.segments.iter_mut().enumerate() {
        let parent = seg.parent.and_then(|p| first.segments.get(p));
        if let (Some(parent), Some(own)) = (parent, first.segments.get(i))
            && (parent.width_mm - own.width_mm).abs() > 0.5
        {
            seg.fittings.push(cadcraft_hvac::Fitting::Reducer);
        }
    }
    size_network(&with, criteria)
}

/// For a message about object `h` that no duct reaches: how far the nearest end of a duct of any
/// air direction is, when it is within a few metres (the duct may be of the wrong direction, or
/// stop short).
fn nearest_end(d: &Drawing, h: Handle) -> String {
    let Some(EntityKind::Insert(i)) = d.entity(h).map(|e| &e.kind) else { return String::new() };
    let at = Vec2::new(i.insert.x, i.insert.y);
    let mm = HvacUnits::of(d).mm_per_unit;
    let nearest = d
        .model
        .iter()
        .filter_map(|e| d.duct_ends(e.handle))
        .flat_map(|(p, a, b)| [(p.air, p.air_set, a), (p.air, p.air_set, b)])
        .map(|(air, set, end)| (end.dist(at) * mm, air, set))
        .min_by(|x, y| x.0.total_cmp(&y.0));
    match nearest {
        Some((dist, air, fixed)) if dist <= 5000.0 => {
            let hint = if fixed { ", whose air direction was set by hand: set its Air to Auto" } else { "" };
            format!(" (the nearest duct end is {dist:.0} mm away, a {} duct{hint})", air.name())
        }
        _ => String::new(),
    }
}

/// For a message about duct `h` no unit reaches: how far the nearest end of another duct is from
/// either of its ends, when under a metre (the ends probably should meet).
fn nearest_duct_end(d: &Drawing, h: Handle) -> String {
    let Some((_, a, b)) = d.duct_ends(h) else { return String::new() };
    let mm = HvacUnits::of(d).mm_per_unit;
    let nearest = d
        .model
        .iter()
        .filter(|e| e.handle != h)
        .filter_map(|e| d.duct_ends(e.handle))
        .flat_map(|(_, c, e)| [c, e])
        .flat_map(|end| [a, b].map(|mine| end.dist(mine) * mm))
        .filter(|dist| *dist > 1.0)
        .min_by(f64::total_cmp);
    match nearest {
        Some(dist) if dist <= 1000.0 => format!(" (the nearest end of another duct is {dist:.0} mm away)"),
        _ => String::new(),
    }
}

/// The units whose fresh air does not match their supply, or whose exhaust does not match their
/// extract (by more than 5 %): a double-flux unit takes in what it blows out and rejects what it extracts.
fn unit_balance(sized: &[(&cadcraft_doc::DuctNetwork, cadcraft_hvac::NetworkReport)]) -> Vec<Value> {
    let flow = |ahu: Handle, air: AirDirection| sized.iter().find(|(n, _)| n.ahu == ahu && n.air == air).map(|(_, r)| r.total_flow_m3h);
    let mut units: Vec<Handle> = sized.iter().map(|(n, _)| n.ahu).collect();
    units.dedup();
    let mut out = Vec::new();
    for ahu in units {
        for (a, b) in [(AirDirection::Supply, AirDirection::Fresh), (AirDirection::Extract, AirDirection::Exhaust)] {
            if let (Some(fa), Some(fb)) = (flow(ahu, a), flow(ahu, b))
                && (fa - fb).abs() > 0.05 * fa.max(fb)
            {
                out.push(json!({ "ahu": ahu.hex(), "left": a.name(), "leftFlow": fa, "right": b.name(), "rightFlow": fb }));
            }
        }
    }
    out
}

/// Where object `h` is, for a message.
fn place(d: &Drawing, h: Handle) -> String {
    match d.entity(h).map(|e| &e.kind) {
        Some(EntityKind::Insert(i)) => format!("({:.0}, {:.0})", i.insert.x, i.insert.y),
        _ => h.hex(),
    }
}

/// The spaces whose terminals of an air direction do not carry the flow the space needs: name,
/// air direction, needed and carried (m³/h). A double-flux unit supplies and extracts as much.
fn space_mismatches(d: &Drawing) -> Vec<Value> {
    let terminals: Vec<(Vec2, AirDirection, f64)> = d
        .model
        .iter()
        .filter_map(|e| match (&e.kind, cadcraft_doc::air_terminal_of(e)) {
            (EntityKind::Insert(i), Some(t)) => Some((Vec2::new(i.insert.x, i.insert.y), t.air, t.flow_m3h)),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for e in d.model.iter() {
        let Some((sp, outline)) = d.space_outline(e.handle) else { continue };
        let Ok(need) = sp.flow() else { continue };
        for air in [AirDirection::Supply, AirDirection::Extract] {
            let carried: f64 = terminals.iter().filter(|(p, a, _)| *a == air && cadcraft_geom::point_in_polygon(&outline, *p)).map(|(.., f)| f).sum();
            if carried > 0.0 && (carried - need).abs() > 0.05 * need.max(1.0) {
                out.push(json!({ "space": sp.name, "air": air.name(), "needed": need, "supplied": carried }));
            }
        }
    }
    out
}

pub(super) fn criteria(p: &Value) -> Result<DesignCriteria> {
    const CMD: &str = "ductsize";
    let mut c = DesignCriteria::default();
    for (key, slot) in [
        ("maxVelocityMain", &mut c.max_velocity_main),
        ("maxVelocityBranch", &mut c.max_velocity_branch),
        ("maxVelocityConnection", &mut c.max_velocity_connection),
        ("maxGradient", &mut c.max_gradient_pa_m),
        ("maxAspect", &mut c.max_aspect),
        ("sideStep", &mut c.side_step_mm),
        ("minSide", &mut c.min_side_mm),
        ("maxSide", &mut c.max_side_mm),
    ] {
        if let Some(v) = positive(CMD, p, key)? {
            *slot = v;
        }
    }
    Ok(c)
}

fn run_ductsize(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductsize";
    let criteria = criteria(p)?;
    let margin_mm = non_negative(CMD, p, "margin")?.unwrap_or(cadcraft_doc::DEFAULT_MARGIN_MM);
    let only = match p.get("ahu") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().and_then(Handle::parse_hex).ok_or_else(|| bad(CMD, "`ahu` must be a handle"))?),
    };
    // The flow a space asks is shared between its terminals first (`balanceTerminals: false` leaves them).
    let rebalanced =
        if p.get("balanceTerminals").and_then(Value::as_bool).unwrap_or(true) { super::hvac::rebalance_terminals(s)? } else { Vec::new() };
    // Ducts drawn across one another, or a terminal on a run, are cut so their ends meet.
    let space = s.space();
    let cuts = s.doc_mut()?.split_duct_taps(&space).map_err(doc_err)?;
    let nets: Vec<_> =
        s.doc()?.duct_networks_with_margin(margin_mm).map_err(doc_err)?.into_iter().filter(|n| only.is_none_or(|h| h == n.ahu)).collect();
    if nets.is_empty() {
        return Err(other("There is no air handling unit or riser to size: place one with AHUADD or RISERADD."));
    }
    let mut out = Vec::new();
    let mut sized = Vec::new();
    for net in &nets {
        let mut warnings: Vec<String> = Vec::new();
        let d = s.doc()?;
        warnings.extend(net.dangling.iter().map(|h| format!("the duct at {} leads to no terminal and was left out", place(d, *h))));
        warnings.extend(
            net.unconnected
                .iter()
                .map(|h| format!("the terminal at {} is reached by no duct of its air direction{}", place(d, *h), nearest_end(d, *h))),
        );
        warnings.extend(net.unplaced.iter().map(|h| format!("the accessory at {} is on no duct", place(d, *h))));
        // Ducts no unit reaches: where the chain from the unit breaks. A few are named, the rest counted.
        warnings.extend(
            net.isolated.iter().take(8).map(|h| format!("the duct at {} is not connected to a unit{}", place(d, *h), nearest_duct_end(d, *h))),
        );
        if net.isolated.len() > 8 {
            warnings.push(format!("{} more ducts are not connected to a unit", net.isolated.len() - 8));
        }
        if net.network.segments.is_empty() {
            warnings.push("no duct leads from this unit to a terminal: a duct must start on the unit, and the ducts meet end to end".into());
            out.push(
                json!({ "ahu": net.ahu.hex(), "air": net.air.name(), "totalFlow": 0.0, "requiredPressure": 0.0, "criticalPath": [], "segments": [], "warnings": warnings }),
            );
            continue;
        }
        let report = size_with_reducers(&net.network, &criteria).map_err(|e| other(e.to_string()))?;
        warnings.extend(report.warnings.iter().cloned());
        let segments: Vec<Value> = net
            .ducts
            .iter()
            .zip(&report.segments)
            .map(|(h, r)| {
                json!({
                    "handle": h.hex(), "flow": r.flow_m3h, "diameter": r.diameter_mm, "section": section_name(r.section),
                    "width": r.width_mm, "height": r.height_mm, "velocity": r.velocity,
                    "gradient": r.gradient_pa_m, "totalPa": r.total_pa, "governing": governing_name(r.governing),
                })
            })
            .collect();
        let path: Vec<String> = report.critical_path.iter().filter_map(|i| net.ducts.get(*i)).map(|h| h.hex()).collect();
        out.push(json!({
            "ahu": net.ahu.hex(), "air": net.air.name(), "totalFlow": report.total_flow_m3h, "requiredPressure": report.required_pressure_pa,
            "criticalPath": path, "segments": segments, "warnings": warnings,
        }));
        sized.push((net, report));
    }
    let mismatches = space_mismatches(s.doc()?);
    let balance = unit_balance(&sized);
    let d = s.doc_mut()?;
    let mut step_warnings = Vec::new();
    for (net, report) in &sized {
        step_warnings.extend(d.apply_duct_report(net, report).map_err(doc_err)?);
    }
    // The lower duct of two that cross passes under, where their altitudes are known (`autoCross: false` leaves them).
    let auto_crossed =
        if p.get("autoCross").and_then(Value::as_bool).unwrap_or(true) { d.cross_ducts_by_altitude(30.0).map_err(doc_err)? } else { 0 };
    // Where the ducts one over the other have their sides drawn straight is known once all are sized: the dashes go on the rest.
    d.refresh_cover_straights().map_err(doc_err)?;
    // The air of the ducts is known once sized: the hoods that follow them take it, and their size.
    d.refresh_hoods().map_err(doc_err)?;
    let ducts = nets.first().map_or(0, |n| n.duct_count);
    let isolated = nets.first().map_or(0, |n| n.isolated.len());
    Ok(json!({ "networks": out, "cuts": cuts, "rebalanced": rebalanced, "stepWarnings": step_warnings, "autoCrossed": auto_crossed,
        "crossings": super::duct_cross::crossings_json(&s.doc()?.duct_crossings().into_iter().filter(|c| !c.resolved).collect::<Vec<_>>()), "spaces": mismatches, "balance": balance, "ducts": ducts, "reached": ducts - isolated }))
}
