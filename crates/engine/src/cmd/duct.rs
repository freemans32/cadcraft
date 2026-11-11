//! Ventilation ducts in plan: AHUADD places an air handling unit, DUCTADD draws round duct runs
//! one segment at a time (a run started on another one cuts it, so branches join), DUCTEDIT
//! changes runs already drawn and DUCTSIZE sizes every network from its terminals back to its unit.
//! The network is read from the drawing each time (`Drawing::duct_networks`), never stored.

use cadcraft_doc::{AhuParams, AirDirection, Drawing, DuctKind, DuctParams, EntityKind, Handle, HvacUnits, MAX_DUCTS};
use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Governing, size_network};
use serde_json::{Value, json};

use super::{CommandSpec, bad, f64_or, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("ahuadd", "Air Handling Unit", run_ahuadd)
            .menu(&["Draw", "HVAC", "Air Handling Unit"])
            .params("{at, name?, width? (mm), depth? (mm), air?: supply|extract, rotation? (degrees), labelHeight? (mm)} → {handle, name}")
            .interactive(|s| Ok(Box::new(AhuAddM::new(s)))),
        CommandSpec::new("ductadd", "Duct", run_ductadd)
            .menu(&["Draw", "HVAC", "Duct"])
            .params(
                "{points (2 or more; one round duct per segment), diameter? (mm), kind?: auto|main|branch|connection, \
                 air?: supply|extract, labelHeight? (mm)} → {handles}",
            )
            .interactive(|s| Ok(Box::new(DuctAddM::new(s)))),
        CommandSpec::new("ductedit", "Edit Duct", run_ductedit)
            .params("{handle | handles, diameter? (mm, or auto: sizing chooses it), kind?, air?} → {changed}"),
        CommandSpec::new("ductsize", "Size Ducts", run_ductsize).menu(&["Draw", "HVAC", "Size Ducts"]).params(
            "{ahu? (handle; default every unit), maxVelocityMain? (m/s), maxVelocityBranch?, maxVelocityConnection?, \
             maxGradient? (Pa/m)} → {networks: [{ahu, totalFlow (m³/h), requiredPressure (Pa), criticalPath, segments, warnings}]}",
        ),
    ]
}

/// What AHUADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AhuLastUsed {
    pub width_mm: f64,
    pub depth_mm: f64,
    pub air: AirDirection,
}

impl Default for AhuLastUsed {
    fn default() -> Self {
        AhuLastUsed { width_mm: 1500.0, depth_mm: 800.0, air: AirDirection::Supply }
    }
}

/// What DUCTADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct DuctLastUsed {
    pub diameter_mm: f64,
    pub kind: DuctKind,
    pub air: AirDirection,
}

impl Default for DuctLastUsed {
    fn default() -> Self {
        DuctLastUsed { diameter_mm: 200.0, kind: DuctKind::Auto, air: AirDirection::Supply }
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

fn air_param(cmd: &str, p: &Value) -> Result<Option<AirDirection>> {
    match str_param(p, "air") {
        Some(t) => AirDirection::parse(t).map(Some).ok_or_else(|| bad(cmd, "`air` must be supply or extract")),
        None => Ok(None),
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
    let last = AhuLastUsed::default();
    let params = AhuParams {
        name: str_param(p, "name").map(str::to_string).unwrap_or_else(|| s.doc().map(next_ahu_name).unwrap_or_else(|_| "CTA".into())),
        width_mm: positive(CMD, p, "width")?.unwrap_or(last.width_mm),
        depth_mm: positive(CMD, p, "depth")?.unwrap_or(last.depth_mm),
        air: air_param(CMD, p)?.unwrap_or(last.air),
        flow_m3h: None,
        pressure_pa: None,
        label_height_mm: positive(CMD, p, "labelHeight")?.unwrap_or(100.0),
        units: units_of(s)?,
    };
    let h = add_ahu(s, &params, at, f64_or(p, "rotation", 0.0).to_radians())?;
    Ok(json!({ "handle": h.hex(), "name": params.name }))
}

#[derive(Clone, Copy, PartialEq)]
enum AhuAsk {
    Location,
    Rotation(Vec2),
    Name,
    Width,
    Depth,
    Air,
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
        s.echo(format!(
            "Current settings: Name = {}, Width = {}, Depth = {}, Air = {}",
            self.name,
            self.last.width_mm,
            self.last.depth_mm,
            self.last.air.name()
        ));
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
            (AhuAsk::Width, _, Some(t)) => self.last.width_mm = Self::size(t)?,
            (AhuAsk::Depth, _, Some(t)) => self.last.depth_mm = Self::size(t)?,
            (AhuAsk::Air, _, Some(t)) => self.last.air = AirDirection::parse(t).ok_or_else(|| other("Enter Supply or Extract."))?,
            (AhuAsk::Rotation(at), _, _) => {
                let angle = match (&i, text.as_deref()) {
                    (Input::Point(p), _) => (*p - at).angle(),
                    (_, Some(t)) => {
                        t.parse::<f64>().ok().filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle in degrees."))?.to_radians()
                    }
                    _ => 0.0,
                };
                let params = AhuParams {
                    name: self.name.clone(),
                    width_mm: self.last.width_mm,
                    depth_mm: self.last.depth_mm,
                    air: self.last.air,
                    flow_m3h: None,
                    pressure_pa: None,
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
            AhuAsk::Location => Prompt::new("Specify location of air handling unit", Accept::POINT).kw(&["Name", "Width", "Depth", "Air"]),
            AhuAsk::Rotation(at) => Prompt::new("Specify rotation angle", Accept::POINT_OR_NUMBER).base(at).default("0"),
            AhuAsk::Name => Prompt::new("Enter unit name", Accept::TEXT).default(self.name.clone()),
            AhuAsk::Width => Prompt::new("Enter unit width (mm)", Accept::NUMBER).default(format!("{}", self.last.width_mm)),
            AhuAsk::Depth => Prompt::new("Enter unit depth (mm)", Accept::NUMBER).default(format!("{}", self.last.depth_mm)),
            AhuAsk::Air => Prompt::new("Enter air direction", super::curves::KW).kw(&["Supply", "Extract"]).default(self.last.air.name()),
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
                    "Width" => AhuAsk::Width,
                    "Depth" => AhuAsk::Depth,
                    "Air" => AhuAsk::Air,
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
        air: last.air,
        flow_m3h: None,
        velocity: None,
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
    last.air = air_param(CMD, p)?.unwrap_or(last.air);
    let mut params = duct_params(s, &last, positive(CMD, p, "labelHeight")?.unwrap_or(100.0))?;
    params.imposed = p.get("diameter").is_some_and(|v| !v.is_null());
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
}

struct DuctAddM {
    last: DuctLastUsed,
    /// Whether the user typed the diameter: sizing then leaves it.
    imposed: bool,
    pts: Vec<Vec2>,
    handles: Vec<Handle>,
    ask: DuctAsk,
}

impl DuctAddM {
    fn new(s: &Session) -> Self {
        DuctAddM { last: s.last_used.duct.clone(), imposed: false, pts: Vec::new(), handles: Vec::new(), ask: DuctAsk::Point }
    }
    fn settings(&self, s: &mut Session) {
        let d = if self.imposed { format!("{}", self.last.diameter_mm) } else { "Auto".to_string() };
        s.echo(format!("Current settings: Diameter = {d}, Kind = {}, Air = {}", self.last.kind.name(), self.last.air.name()));
    }
    fn params(&self, s: &Session) -> Result<DuctParams> {
        let mut p = duct_params(s, &self.last, 100.0)?;
        p.imposed = self.imposed;
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
            (DuctAsk::Diameter, Some(t)) if t.eq_ignore_ascii_case("auto") => self.imposed = false,
            (DuctAsk::Diameter, Some(t)) => {
                self.last.diameter_mm = t
                    .parse::<f64>()
                    .ok()
                    .filter(|d| d.is_finite() && (cadcraft_doc::DUCT_DIAMETER_MM.0..=cadcraft_doc::DUCT_DIAMETER_MM.1).contains(d))
                    .ok_or_else(|| other("Requires a diameter in millimetres, or Auto."))?;
                self.imposed = true;
            }
            (DuctAsk::Kind, Some(t)) => self.last.kind = DuctKind::parse(t).ok_or_else(|| other("Enter Auto, Main, Branch or Connection."))?,
            (DuctAsk::Air, Some(t)) => self.last.air = AirDirection::parse(t).ok_or_else(|| other("Enter Supply or Extract."))?,
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
            DuctAsk::Diameter => Prompt::new("Enter duct diameter (mm) or [Auto]", Accept::TEXT).default(if self.imposed {
                format!("{}", self.last.diameter_mm)
            } else {
                "Auto".to_string()
            }),
            DuctAsk::Kind => {
                Prompt::new("Enter duct kind", super::curves::KW).kw(&["Auto", "Main", "Branch", "Connection"]).default(self.last.kind.name())
            }
            DuctAsk::Air => Prompt::new("Enter air direction", super::curves::KW).kw(&["Supply", "Extract"]).default(self.last.air.name()),
            DuctAsk::Point => match self.pts.last().copied() {
                None => Prompt::new("Specify start point of duct", Accept::POINT).kw(&["Diameter", "Kind", "Air"]),
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
    // A number imposes the diameter; "auto" gives it back to sizing.
    let diameter: Option<Option<f64>> = match p.get("diameter") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => Some(None),
        Some(v) => Some(Some(v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| bad(CMD, "`diameter` must be a number of millimetres or auto"))?)),
    };
    let (kind, air) = (kind_param(CMD, p)?, air_param(CMD, p)?);
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let (mut dp, ..) = d.duct_ends(*h).ok_or_else(|| other("Select ducts only."))?;
        match diameter {
            Some(Some(mm)) => {
                dp.diameter_mm = mm;
                dp.imposed = true;
            }
            Some(None) => dp.imposed = false,
            None => {}
        }
        dp.kind = kind.unwrap_or(dp.kind);
        dp.air = air.unwrap_or(dp.air);
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
// DUCTSIZE
// =====================================================================================

fn governing_name(g: Governing) -> &'static str {
    match g {
        Governing::Velocity => "velocity",
        Governing::Friction => "friction",
        Governing::Imposed => "imposed",
        Governing::Largest => "largest",
    }
}

fn criteria(p: &Value) -> Result<DesignCriteria> {
    const CMD: &str = "ductsize";
    let mut c = DesignCriteria::default();
    for (key, slot) in [
        ("maxVelocityMain", &mut c.max_velocity_main),
        ("maxVelocityBranch", &mut c.max_velocity_branch),
        ("maxVelocityConnection", &mut c.max_velocity_connection),
        ("maxGradient", &mut c.max_gradient_pa_m),
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
    let only = match p.get("ahu") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_str().and_then(Handle::parse_hex).ok_or_else(|| bad(CMD, "`ahu` must be a handle"))?),
    };
    let nets: Vec<_> = s.doc()?.duct_networks().map_err(doc_err)?.into_iter().filter(|n| only.is_none_or(|h| h == n.ahu)).collect();
    if nets.is_empty() {
        return Err(other("There is no air handling unit to size: place one with AHUADD."));
    }
    let mut out = Vec::new();
    let mut sized = Vec::new();
    for net in &nets {
        let mut warnings: Vec<String> = Vec::new();
        warnings.extend(net.dangling.iter().map(|h| format!("duct {} leads to no terminal and was left out", h.hex())));
        warnings.extend(net.unconnected.iter().map(|h| format!("terminal {} is reached by no duct", h.hex())));
        if net.network.segments.is_empty() {
            warnings.push("no duct leads from this unit to a terminal".into());
            out.push(
                json!({ "ahu": net.ahu.hex(), "totalFlow": 0.0, "requiredPressure": 0.0, "criticalPath": [], "segments": [], "warnings": warnings }),
            );
            continue;
        }
        let report = size_network(&net.network, &criteria).map_err(|e| other(e.to_string()))?;
        warnings.extend(report.warnings.iter().cloned());
        let segments: Vec<Value> = net
            .ducts
            .iter()
            .zip(&report.segments)
            .map(|(h, r)| {
                json!({
                    "handle": h.hex(), "flow": r.flow_m3h, "diameter": r.diameter_mm, "velocity": r.velocity,
                    "gradient": r.gradient_pa_m, "totalPa": r.total_pa, "governing": governing_name(r.governing),
                })
            })
            .collect();
        let path: Vec<String> = report.critical_path.iter().filter_map(|i| net.ducts.get(*i)).map(|h| h.hex()).collect();
        out.push(json!({
            "ahu": net.ahu.hex(), "totalFlow": report.total_flow_m3h, "requiredPressure": report.required_pressure_pa,
            "criticalPath": path, "segments": segments, "warnings": warnings,
        }));
        sized.push((net, report));
    }
    let d = s.doc_mut()?;
    for (net, report) in &sized {
        d.apply_duct_report(net, report).map_err(doc_err)?;
    }
    Ok(json!({ "networks": out }))
}
