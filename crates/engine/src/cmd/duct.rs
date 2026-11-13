//! Ventilation ducts in plan: AHUADD places an air handling unit, DUCTADD draws round duct runs
//! one segment at a time (a run started on another one cuts it, so branches join), DUCTEDIT
//! changes runs already drawn and DUCTSIZE sizes every network from its terminals back to its unit.
//! The network is read from the drawing each time (`Drawing::duct_networks`), never stored.

use cadcraft_doc::{AhuKind, AhuParams, AirDirection, Drawing, DuctKind, DuctParams, DuctShape, EntityKind, Handle, HvacUnits, MAX_DUCTS};
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
                "{handle | handles, name?, type?: doubleFlow|supplyUnit|extractUnit|vmcBox|stackedDoubleFlow, width? (mm), depth? (mm), height? (mm or null), model?, weight? (kg or null), \
                 ports? (the direction of the air at each collar, supply|extract|fresh|exhaust, one per collar: four for a double-flux unit or a box, two for a single-flux unit; [] restores the kind's)} → {changed}",
            ),
        CommandSpec::new("ductadd", "Duct", run_ductadd)
            .menu(&["Draw", "HVAC", "Duct"])
            .params(
                "{points (2 or more; one round duct per segment), diameter? (mm), kind?: auto|main|branch|connection, \
                 width? + height? (mm; a rectangular duct), air?: auto|supply|extract|fresh|exhaust (auto: from the terminals it serves), flexible? (true: a flexible duct), labelHeight? (mm)} → {handles}",
            )
            .interactive(|s| Ok(Box::new(DuctAddM::new(s)))),
        CommandSpec::new("ductedit", "Edit Duct", run_ductedit)
            .params("{handle | handles, diameter? (mm, or auto: sizing chooses the size) | width? + height? (mm; a rectangular section), kind?, air?, flexible? (true or false)} → {changed}"),
        CommandSpec::new("ductconnect", "Connect Terminals", run_ductconnect).menu(&["Draw", "HVAC", "Connect Terminals"]).params(
            "{terminals? (handles; default every terminal no duct reaches), maxDistance? (mm, default 3000)} → {created (flexible ducts), skipped}",
        ),
        CommandSpec::new("ductsize", "Size Ducts", run_ductsize).menu(&["Draw", "HVAC", "Size Ducts"]).params(
            "{ahu? (handle; default every unit), maxVelocityMain? (m/s), maxVelocityBranch?, maxVelocityConnection?, \
             balanceTerminals? (false keeps the terminals' flows; default true: a space's flow is shared between its terminals first), maxGradient? (Pa/m), margin? (mm kept back from the height available, default 100), maxAspect? (width / height of a rectangular duct, default 4), sideStep? (mm, default 50), \
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
        DuctLastUsed { diameter_mm: 200.0, kind: DuctKind::Auto, air: None, flexible: false }
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
        height_mm: positive(CMD, p, "height")?,
        model: str_param(p, "model").unwrap_or_default().to_string(),
        weight_kg: positive(CMD, p, "weight")?,
        label_height_mm: positive(CMD, p, "labelHeight")?.unwrap_or(100.0),
        units: units_of(s)?,
    };
    let h = add_ahu(s, &params, at, f64_or(p, "rotation", 0.0).to_radians())?;
    Ok(json!({ "handle": h.hex(), "name": params.name }))
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
                self.last = AhuLastUsed { kind: AhuKind::parse(t).ok_or_else(|| other("Enter DoubleFlow, Supply, Extract or Vmc."))?, size: None };
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
                    height_mm: None,
                    model: String::new(),
                    weight_kg: None,
                    label_height_mm: 100.0,
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
            AhuAsk::Type => {
                Prompt::new("Enter unit type", super::curves::KW).kw(&["DoubleFlow", "Supply", "Extract", "Vmc"]).default(self.last.kind.name())
            }
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
            height_mm: None,
            model: String::new(),
            weight_kg: None,
            label_height_mm: 100.0,
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
        air: last.air.unwrap_or_default(),
        air_set: last.air.is_some(),
        flow_m3h: None,
        velocity: None,
        shape: DuctShape::Round,
        width_mm: 0.0,
        height_mm: 0.0,
        flexible: last.flexible,
        reducer: None,
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_forward: None,
        open_flow_m3h: None,
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
    pts: Vec<Vec2>,
    handles: Vec<Handle>,
    ask: DuctAsk,
}

impl DuctAddM {
    fn new(s: &Session) -> Self {
        DuctAddM { last: s.last_used.duct.clone(), imposed: None, pts: Vec::new(), handles: Vec::new(), ask: DuctAsk::Point }
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
            match self.params(s).and_then(|params| add_run(s, &params, last, p)) {
                Ok(h) => {
                    self.handles.push(h);
                    s.last_used.duct = self.last.clone();
                }
                Err(e) => {
                    s.echo(e.to_string());
                    return Ok(Step::Continue);
                }
            }
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
                }
                self.pts.pop();
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
        // The run under way, as wide as it will be.
        match (self.ask, self.pts.last().copied(), self.params(s)) {
            (DuctAsk::Point, Some(last), Ok(p)) if !last.near(c, 1e-9) => p.preview(last, c),
            _ => Vec::new(),
        }
    }
}

// =====================================================================================
// DUCTEDIT
// =====================================================================================

fn run_ductedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "ductedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} ducts")));
    }
    // A number imposes the diameter, `width` and `height` together a rectangular section; "auto" gives
    // the size back to sizing.
    let size: Option<Option<Imposed>> = match (p.get("diameter"), positive(CMD, p, "width")?, positive(CMD, p, "height")?) {
        (_, Some(w), Some(h)) => Some(Some(Imposed::Rect(w, h))),
        (_, Some(_), None) | (_, None, Some(_)) => return Err(bad(CMD, "`width` and `height` go together")),
        (None | Some(Value::Null), None, None) => None,
        (Some(Value::String(t)), None, None) if t.trim().eq_ignore_ascii_case("auto") => Some(None),
        (Some(v), None, None) => Some(Some(Imposed::Round(
            v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| bad(CMD, "`diameter` must be a number of millimetres or auto"))?,
        ))),
    };
    let (kind, air) = (kind_param(CMD, p)?, duct_air_param(CMD, p)?);
    let flexible = bool_param(CMD, p, "flexible")?;
    // The flow of an open end (a duct on a unit's collar that leads to no terminal); null gives it back to the balance.
    let open_flow: Option<Option<f64>> = match p.get("openFlow") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => {
            Some(Some(v.as_f64().filter(|f| f.is_finite() && *f > 0.0).ok_or_else(|| bad(CMD, "`openFlow` must be a flow above 0 (m³/h) or null"))?))
        }
    };
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let (mut dp, ..) = d.duct_ends(*h).ok_or_else(|| other("Select ducts only."))?;
        match size {
            Some(Some(imposed)) => imposed.apply(&mut dp),
            Some(None) => dp.imposed = false,
            None => {}
        }
        dp.kind = kind.unwrap_or(dp.kind);
        match air {
            Some(Some(a)) => (dp.air, dp.air_set) = (a, true),
            Some(None) => dp.air_set = false,
            None => {}
        }
        dp.flexible = flexible.unwrap_or(dp.flexible);
        dp.open_flow_m3h = open_flow.unwrap_or(dp.open_flow_m3h);
        dp.validate().map_err(doc_err)?;
        next.push((*h, dp));
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

/// The nearest rigid duct of `air` to `at`: its foot on the axis and the distance, in drawing units.
fn nearest_rigid(d: &Drawing, air: AirDirection, at: Vec2) -> Option<(Vec2, f64)> {
    d.model
        .iter()
        .filter_map(|e| d.duct_ends(e.handle))
        .filter(|(p, ..)| !p.flexible && (!p.air_set || p.air == air))
        .map(|(_, a, b)| {
            let along = (b - a).normalized();
            let foot = a + along * (at - a).dot(along).clamp(0.0, a.dist(b));
            (foot, foot.dist(at))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
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
        let Some((foot, dist)) = nearest_rigid(s.doc()?, t.air, at).filter(|(_, dist)| dist * mm <= reach_mm) else {
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
            air: t.air,
            air_set: true,
            flow_m3h: None,
            velocity: None,
            shape: DuctShape::Round,
            width_mm: 0.0,
            height_mm: 0.0,
            flexible: true,
            reducer: None,
            elbow: None,
            trim_start_mm: 0.0,
            trim_end_mm: 0.0,
            flow_forward: None,
            open_flow_m3h: None,
            bottom_mm: None,
            label_height_mm: 100.0,
            units: units_of(s)?,
        };
        created.push(add_run(s, &params, foot, at)?.hex());
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
fn size_with_reducers(
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

fn criteria(p: &Value) -> Result<DesignCriteria> {
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
    for (net, report) in &sized {
        d.apply_duct_report(net, report).map_err(doc_err)?;
    }
    let ducts = nets.first().map_or(0, |n| n.duct_count);
    let isolated = nets.first().map_or(0, |n| n.isolated.len());
    Ok(
        json!({ "networks": out, "cuts": cuts, "rebalanced": rebalanced, "spaces": mismatches, "balance": balance, "ducts": ducts, "reached": ducts - isolated }),
    )
}
