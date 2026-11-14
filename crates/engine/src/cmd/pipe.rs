//! Pipework in plan: PIPEADD draws pipe runs of a service (chilled water, hot water, change-over,
//! condensate drain, refrigerant liquid or gas) one segment at a time, PIPEEDIT changes runs already
//! drawn. The network the runs make is read from the drawing when it is sized, never stored.

use cadcraft_doc::{Drawing, EntityKind, Handle, HvacUnits, PipeLeg, PipeParams, PipeService};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

/// Most points one PIPEADD takes.
const MAX_PIPE_POINTS: usize = 512;
/// Most pipes one PIPEEDIT changes.
const MAX_EDITED: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pipeadd", "Pipe", run_pipeadd)
            .menu(&["Draw", "HVAC", "Pipe"])
            .params(
                "{points (2 or more; one pipe per segment), service?: chilledWater|hotWater|changeOver|condensate|refrigerantLiquid|refrigerantGas, \
                 leg?: supply|return (the water networks), dn? (mm: the nominal diameter of a steel pipe, the outer diameter of a refrigerant tube; default by service), \
                 insulation? (mm), altitude? (m, the underside), load? (kW at the free end; l/h for a condensate drain), source? (true: the run starts at the source of its network), \
                 labelHeight? (mm)} → {handles}",
            )
            .interactive(|s| Ok(Box::new(PipeAddM::new(s)))),
        CommandSpec::new("pipesize", "Size Pipes", run_pipesize).menu(&["Draw", "HVAC", "Size Pipes"]).params(
            "{deltaT? ({chilledWater?, hotWater?, changeOver?}: the temperature difference of each water network, K; default 5, 20 and 5), maxVelocity? (m/s; default: by size, 0.7 to 2), \
             maxGradient? (Pa/m, default 300), minDn? (mm, default 15), slope? (% of a condensate drain, default 1)} → {networks: [{service, leg, source, totalLoad, runs: [{handle, dn, load, flowM3h, velocity, gradient, imposed, governing}], warnings}], warnings}: \
             each network starts at the run marked as its source, the loads typed at the free ends (kW; l/h for a drain) are added up towards it",
        ),
        CommandSpec::new("pipeedit", "Edit Pipe", run_pipeedit).params(
            "{handle | handles, service?, leg?, dn? (mm, or auto: sizing chooses the size), insulation? (mm), altitude? (m or null), load? (kW, l/h for a condensate drain, or null), \
             source? (true or false)} → {changed}",
        ),
    ]
}

/// What PIPEADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct PipeLastUsed {
    pub service: PipeService,
    pub leg: PipeLeg,
    /// The size the user typed, mm; `None`: the service's own, and sizing may change it.
    pub dn_mm: Option<f64>,
    pub insulation_mm: f64,
}

impl Default for PipeLastUsed {
    fn default() -> Self {
        PipeLastUsed { service: PipeService::ChilledWater, leg: PipeLeg::Supply, dn_mm: None, insulation_mm: 0.0 }
    }
}

/// The size a new pipe of `service` starts with, mm.
pub fn default_dn_mm(service: PipeService) -> f64 {
    match service {
        PipeService::ChilledWater | PipeService::ChangeOver => 50.0,
        PipeService::HotWater => 40.0,
        PipeService::Condensate => 25.0,
        PipeService::RefrigerantLiquid => 9.52,
        PipeService::RefrigerantGas => 15.88,
    }
}

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

fn number(cmd: &str, p: &Value, key: &str, min: f64) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            v.as_f64().filter(|x| x.is_finite() && *x >= min).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number of {min} or more")))
        }
    }
}

fn flag(cmd: &str, p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false"))),
    }
}

fn service_param(cmd: &str, p: &Value) -> Result<Option<PipeService>> {
    match str_param(p, "service") {
        Some(t) => PipeService::parse(t)
            .map(Some)
            .ok_or_else(|| bad(cmd, "`service` must be chilledWater, hotWater, changeOver, condensate, refrigerantLiquid or refrigerantGas")),
        None => Ok(None),
    }
}

fn leg_param(cmd: &str, p: &Value) -> Result<Option<PipeLeg>> {
    match str_param(p, "leg") {
        Some(t) => PipeLeg::parse(t).map(Some).ok_or_else(|| bad(cmd, "`leg` must be supply or return")),
        None => Ok(None),
    }
}

/// The altitude of the underside in millimetres from a value in metres: absent keeps, null clears.
fn altitude_param(cmd: &str, p: &Value) -> Result<Option<Option<f64>>> {
    match p.get("altitude") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(v) => {
            Ok(Some(Some(v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| bad(cmd, "`altitude` must be a number of metres or null"))? * 1000.0)))
        }
    }
}

fn pipe_params(s: &Session, last: &PipeLastUsed, label_mm: f64) -> Result<PipeParams> {
    let units = HvacUnits::of(s.doc()?);
    let mut p = PipeParams::new(last.service, last.dn_mm.unwrap_or_else(|| default_dn_mm(last.service)), units);
    p.leg = if last.service.has_legs() { last.leg } else { PipeLeg::Supply };
    p.imposed = last.dn_mm.is_some();
    p.insulation_mm = last.insulation_mm;
    p.label_height_mm = label_mm;
    Ok(p)
}

fn add_run(s: &mut Session, params: &PipeParams, a: Vec2, b: Vec2) -> Result<Handle> {
    if a.near(b, 1e-9) || !a.is_finite() || !b.is_finite() {
        return Err(other("A pipe needs two different points."));
    }
    PipeParams { length: a.dist(b), ..params.clone() }.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    s.doc_mut()?.add_pipe(&space, common, params.clone(), a, b).map_err(doc_err)
}

fn run_pipeadd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "pipeadd";
    let pts = super::points_param(p, "points").filter(|v| v.len() >= 2).ok_or_else(|| bad(CMD, "`points` (two or more [x, y]) is required"))?;
    if pts.len() > MAX_PIPE_POINTS {
        return Err(bad(CMD, format!("at most {MAX_PIPE_POINTS} points")));
    }
    let mut last = PipeLastUsed::default();
    last.service = service_param(CMD, p)?.unwrap_or(last.service);
    last.leg = leg_param(CMD, p)?.unwrap_or(last.leg);
    last.dn_mm = number(CMD, p, "dn", 1.0)?;
    last.insulation_mm = number(CMD, p, "insulation", 0.0)?.unwrap_or(0.0);
    let mut params = pipe_params(s, &last, number(CMD, p, "labelHeight", 1.0)?.unwrap_or(100.0))?;
    if let Some(a) = altitude_param(CMD, p)? {
        params.level_mm = a;
    }
    params.load_kw = number(CMD, p, "load", 0.0)?;
    params.source = flag(CMD, p, "source")?.unwrap_or(false);
    let mut handles = Vec::new();
    for w in pts.windows(2) {
        if let [a, b] = w {
            handles.push(add_run(s, &params, *a, *b)?.hex());
        }
    }
    Ok(json!({ "handles": handles }))
}

fn run_pipeedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "pipeedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} pipes")));
    }
    let (service, leg) = (service_param(CMD, p)?, leg_param(CMD, p)?);
    // A number imposes the size; "auto" gives it back to sizing.
    let dn: Option<Option<f64>> = match p.get("dn") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => Some(None),
        Some(v) => {
            Some(Some(v.as_f64().filter(|d| d.is_finite() && *d > 0.0).ok_or_else(|| bad(CMD, "`dn` must be a number of millimetres or auto"))?))
        }
    };
    let insulation = number(CMD, p, "insulation", 0.0)?;
    let altitude = altitude_param(CMD, p)?;
    let load: Option<Option<f64>> = match p.get("load") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(_) => Some(number(CMD, p, "load", 0.0)?),
    };
    let source = flag(CMD, p, "source")?;
    let d: &Drawing = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let (mut pp, ..) = d.pipe_ends(*h).ok_or_else(|| other("Select pipes only."))?;
        if let Some(sv) = service {
            // A new service takes its own size, unless one was imposed or is given.
            if sv != pp.service && !pp.imposed && dn.is_none() {
                pp.dn_mm = default_dn_mm(sv);
            }
            pp.service = sv;
        }
        if let Some(l) = leg {
            pp.leg = l;
        }
        if !pp.service.has_legs() {
            pp.leg = PipeLeg::Supply;
        }
        match dn {
            Some(Some(v)) => (pp.dn_mm, pp.imposed) = (v, true),
            Some(None) => pp.imposed = false,
            None => {}
        }
        pp.insulation_mm = insulation.unwrap_or(pp.insulation_mm);
        if let Some(a) = altitude {
            pp.level_mm = a;
        }
        if let Some(l) = load {
            pp.load_kw = l;
        }
        pp.source = source.unwrap_or(pp.source);
        pp.validate().map_err(doc_err)?;
        next.push((*h, pp));
    }
    let d = s.doc_mut()?;
    for (h, pp) in &next {
        d.regenerate_pipe(*h, pp).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

fn run_pipesize(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "pipesize";
    let mut sizing = cadcraft_doc::PipeSizingParams::default();
    if let Some(dt) = p.get("deltaT").filter(|v| !v.is_null()) {
        let get = |key: &str| number(CMD, dt, key, 0.5);
        sizing.delta_t_chilled = get("chilledWater")?.unwrap_or(sizing.delta_t_chilled);
        sizing.delta_t_hot = get("hotWater")?.unwrap_or(sizing.delta_t_hot);
        sizing.delta_t_change_over = get("changeOver")?.unwrap_or(sizing.delta_t_change_over);
    }
    sizing.criteria.max_velocity_ms = number(CMD, p, "maxVelocity", 0.1)?.or(sizing.criteria.max_velocity_ms);
    sizing.criteria.max_gradient_pa_m = number(CMD, p, "maxGradient", 1.0)?.unwrap_or(sizing.criteria.max_gradient_pa_m);
    sizing.criteria.min_dn_mm = number(CMD, p, "minDn", 1.0)?.unwrap_or(sizing.criteria.min_dn_mm);
    sizing.drain_slope_percent = number(CMD, p, "slope", 0.1)?.unwrap_or(sizing.drain_slope_percent);
    let (reports, warnings) = s.doc_mut()?.size_pipes(&sizing).map_err(doc_err)?;
    let governing = |g: cadcraft_hvac::PipeGoverning| match g {
        cadcraft_hvac::PipeGoverning::Minimum => "minimum",
        cadcraft_hvac::PipeGoverning::Velocity => "velocity",
        cadcraft_hvac::PipeGoverning::Gradient => "gradient",
        cadcraft_hvac::PipeGoverning::Imposed => "imposed",
    };
    let networks: Vec<Value> = reports
        .iter()
        .map(|r| {
            json!({
                "service": r.service.name(),
                "leg": r.leg.name(),
                "source": r.source.hex(),
                "totalLoad": r.total_load,
                "runs": r.runs.iter().map(|x| json!({
                    "handle": x.handle.hex(), "dn": x.dn_mm, "load": x.load, "flowM3h": x.flow_m3h, "velocity": x.velocity_ms,
                    "gradient": x.gradient_pa_m, "imposed": x.imposed, "governing": governing(x.governing),
                })).collect::<Vec<_>>(),
                "warnings": r.warnings,
            })
        })
        .collect();
    Ok(json!({ "networks": networks, "warnings": warnings }))
}

#[derive(Clone, Copy, PartialEq)]
enum PipeAsk {
    Point,
    Service,
    Leg,
    Diameter,
    Insulation,
}

struct PipeAddM {
    last: PipeLastUsed,
    pts: Vec<Vec2>,
    handles: Vec<Handle>,
    ask: PipeAsk,
}

impl PipeAddM {
    fn new(s: &Session) -> Self {
        PipeAddM { last: s.last_used.pipe.clone(), pts: Vec::new(), handles: Vec::new(), ask: PipeAsk::Point }
    }
    fn settings(&self, s: &mut Session) {
        let dn = self.last.dn_mm.map_or("Auto".to_string(), |d| format!("{d}"));
        s.echo(format!(
            "Current settings: Service = {}, Leg = {}, Diameter = {dn}, Insulation = {}",
            self.last.service.name(),
            if self.last.service.has_legs() { self.last.leg.name() } else { "-" },
            self.last.insulation_mm
        ));
    }
    fn params(&self, s: &Session) -> Result<PipeParams> {
        pipe_params(s, &self.last, 100.0)
    }
    fn point(&mut self, s: &mut Session, p: Vec2) -> Result<Step> {
        if let Some(last) = self.pts.last().copied() {
            if last.near(p, 1e-9) {
                return Ok(Step::Continue);
            }
            match self.params(s).and_then(|params| add_run(s, &params, last, p)) {
                Ok(h) => {
                    self.handles.push(h);
                    s.last_used.pipe = self.last.clone();
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
            "Service" => self.ask = PipeAsk::Service,
            "Leg" => self.ask = PipeAsk::Leg,
            "Diameter" => self.ask = PipeAsk::Diameter,
            "Insulation" => self.ask = PipeAsk::Insulation,
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
            (PipeAsk::Service, Some(t)) => {
                self.last.service = PipeService::parse(t)
                    .ok_or_else(|| other("Enter ChilledWater, HotWater, ChangeOver, Condensate, RefrigerantLiquid or RefrigerantGas."))?;
            }
            (PipeAsk::Leg, Some(t)) => self.last.leg = PipeLeg::parse(t).ok_or_else(|| other("Enter Supply or Return."))?,
            (PipeAsk::Diameter, Some(t)) if t.eq_ignore_ascii_case("auto") => self.last.dn_mm = None,
            (PipeAsk::Diameter, Some(t)) => {
                self.last.dn_mm = Some(
                    t.parse::<f64>()
                        .ok()
                        .filter(|d| d.is_finite() && *d > 0.0)
                        .ok_or_else(|| other("Requires a diameter in millimetres, or Auto."))?,
                )
            }
            (PipeAsk::Insulation, Some(t)) => {
                self.last.insulation_mm = t
                    .parse::<f64>()
                    .ok()
                    .filter(|d| d.is_finite() && *d >= 0.0)
                    .ok_or_else(|| other("Requires a thickness in millimetres, 0 or more."))?
            }
            _ => {}
        }
        self.ask = PipeAsk::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for PipeAddM {
    fn name(&self) -> &'static str {
        "PIPEADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            PipeAsk::Service => Prompt::new("Enter pipe service", super::curves::KW)
                .kw(&["ChilledWater", "HotWater", "ChangeOver", "Condensate", "RefrigerantLiquid", "RefrigerantGas"])
                .default(self.last.service.name()),
            PipeAsk::Leg => Prompt::new("Enter pipe leg", super::curves::KW).kw(&["Supply", "Return"]).default(self.last.leg.name()),
            PipeAsk::Diameter => Prompt::new("Enter pipe diameter (mm) or [Auto]", Accept::TEXT)
                .default(self.last.dn_mm.map_or("Auto".to_string(), |d| format!("{d}"))),
            PipeAsk::Insulation => Prompt::new("Enter insulation thickness (mm)", Accept::NUMBER).default(format!("{}", self.last.insulation_mm)),
            PipeAsk::Point => match self.pts.last().copied() {
                None => Prompt::new("Specify start point of pipe", Accept::POINT).kw(&["Service", "Leg", "Diameter", "Insulation"]),
                last => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base_opt(last),
            },
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != PipeAsk::Point {
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
        let (PipeAsk::Point, Some(last), Ok(p)) = (self.ask, self.pts.last().copied(), self.params(s)) else { return Vec::new() };
        if last.near(c, 1e-9) {
            return Vec::new();
        }
        let p = PipeParams { length: last.dist(c), ..p };
        let rotation = (c - last).angle();
        p.generate(false).map(|pieces| cadcraft_doc::preview_entities(pieces, last, rotation)).unwrap_or_default()
    }
}
