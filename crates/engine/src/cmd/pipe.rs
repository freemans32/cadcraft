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
        CommandSpec::new("colourconvention", "Pipe and Duct Colours", run_colour_convention)
            .params(
                "{convention?: plan|nfx08100} → {convention, layers, pipes}: draws the pipes and ducts in the plan's own colours or in those of NF X 08-100 (water yellow-green with a band for the state, air light blue, \
                 the polluted air brown, the refrigerants orange-yellow); without `convention` it only says which is in use. The standard layers change colour (a layer you made is left alone) and the pipes are drawn again",
            ),
        CommandSpec::new("pipeadd", "Pipe", run_pipeadd)
            .menu(&["Draw", "HVAC", "Pipe"])
            .params(
                "{points (2 or more; one pipe per segment), service?: chilledWater|hotWater|changeOver|condensate|refrigerantLiquid|refrigerantGas|refrigerantPair (the liquid and the gas lines drawn as one run)|refrigerantTriple (heat recovery: liquid, suction gas and high/low-pressure gas drawn as one run)|wasteWater (EU)|soilWater (EV)|rainWater (EP)|coldWater (EF)|domesticHotWater (ECS)|hotWaterLoop (its return loop; the plumbing sizes are the tube's outer diameter, mm), slope? (% of a drain, written on it), \
                 leg?: supply|return (the water networks), dn? (mm: the nominal diameter of a steel pipe; a refrigerant tube takes its outer diameter in mm or in inches, \"3/8\", \"1 1/8\"; for a pair, the liquid tube), gas? (the gas tube of a link, as dn; for a heat-recovery link, the suction gas), hpGas? (the high/low-pressure gas tube of a heat-recovery link), \
                 insulation? (mm), altitude? (m, the underside), load? (kW at the free end; l/h for a condensate drain), source? (true: the run starts at the source of its network), \
                 labelHeight? (mm), network? (a name: the pipes are on that network; one connected to a named network takes its name by itself)} → {handles}",
            )
            .interactive(|s| Ok(Box::new(PipeAddM::new(s)))),
        CommandSpec::new("pipesize", "Size Pipes", run_pipesize).menu(&["Draw", "HVAC", "Size Pipes"]).params(
            "{deltaT? ({chilledWater?, hotWater?, changeOver?}: the temperature difference of each water network, K; default 5, 20 and 5), maxVelocity? (m/s; default: by size, 0.7 to 2), \
             maxGradient? (Pa/m, default 300), minDn? (mm, default 15), slope? (% of a condensate drain, default 1)} → {risers: [{handle, name, lines: [{tag, dn, imposed, load, flowM3h, velocity, incoming}]}] (the lines of the pipe risers, sized for what they carry here: the load of the networks that start in the shaft and `incoming`), networks: [{service, leg, source, totalLoad, totalLoss (kPa from the source to the far end of the most unfavourable run, pipes and parts), critical, runs: [{handle, dn, load, flowM3h, velocity, gradient, imposed, governing, runLoss, partLoss, parts}], warnings}], warnings}: \
             each network starts at the run marked as its source, the loads typed at the free ends (kW; l/h for a drain) are added up towards it",
        ),
        CommandSpec::new("pipefittings", "Pipe Fittings", run_pipefittings).menu(&["Draw", "HVAC", "Pipe Fittings"]).params(
            "{} → {fittings: [{kind, name, at: [x, y], service, leg, size, pipes: [handle]}], counts: {name: n}}: the elbows, tees, crosses, reducers and crossovers read from where the pipes meet and cross",
        ),
        CommandSpec::new("pipeedit", "Edit Pipe", run_pipeedit).params(
            "{handle | handles, service?, leg?, dn? (mm, inches for a refrigerant tube, or auto: sizing chooses the size), gas? (mm or inches: the gas tube of a link), hpGas? (the high/low-pressure gas tube of a heat-recovery link), insulation? (mm), altitude? (m or null), load? (kW, l/h for a condensate drain, or null), \
             source? (true or false), network? (a name: it is the name of the whole network of each pipe), slope? (% of a drain, or null), label? (false: the label is not drawn)} → {changed}",
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
    /// The gas tube of a refrigerant pair, mm; `None`: the default.
    pub gas_mm: Option<f64>,
    /// The high/low-pressure gas tube of a heat-recovery link, mm; `None`: the default.
    pub gas2_mm: Option<f64>,
    /// The liquid tube of a refrigerant link when none was typed, mm; `None`: the default. Set in the Options.
    pub liquid_mm: Option<f64>,
    pub insulation_mm: f64,
}

impl Default for PipeLastUsed {
    fn default() -> Self {
        PipeLastUsed {
            service: PipeService::ChilledWater,
            leg: PipeLeg::Supply,
            dn_mm: None,
            gas_mm: None,
            gas2_mm: None,
            liquid_mm: None,
            insulation_mm: 0.0,
        }
    }
}

impl PipeLastUsed {
    /// The size a new pipe starts with, mm: the one typed, else the liquid tube chosen in the Options for a refrigerant link, else the service's own.
    pub fn start_dn_mm(&self) -> f64 {
        self.dn_mm.or(self.liquid_mm.filter(|_| self.service.is_refrigerant())).unwrap_or_else(|| default_dn_mm(self.service))
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
        // The liquid tube of a link.
        PipeService::RefrigerantPair | PipeService::RefrigerantTriple => 9.52,
        // A collector of waste water, soil water (a water closet's Ø100) and rain water; copper or multilayer tube for domestic water.
        PipeService::WasteWater => 50.0,
        PipeService::SoilWater | PipeService::RainWater => 100.0,
        PipeService::ColdWater | PipeService::DomesticHotWater => 20.0,
        PipeService::HotWaterLoop => 16.0,
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

/// A size from a command: millimetres as a number, or text: millimetres, or inches for a refrigerant tube ("3/8", "1 1/8").
fn size_value(cmd: &str, key: &str, v: &Value, refrigerant: bool) -> Result<f64> {
    let mm = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(t) if refrigerant => cadcraft_doc::parse_refrigerant_size(t),
        Value::String(t) => t.trim().parse::<f64>().ok(),
        _ => None,
    };
    mm.filter(|x| x.is_finite() && *x > 0.0)
        .ok_or_else(|| bad(cmd, format!("`{key}` must be a size in millimetres{}", if refrigerant { " or inches (3/8, 1 1/8)" } else { "" })))
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
    let mut p = PipeParams::new(last.service, last.start_dn_mm(), units);
    p.leg = if last.service.has_legs() { last.leg } else { PipeLeg::Supply };
    p.imposed = last.dn_mm.is_some();
    if last.service.has_gas_tube() {
        p.gas_dn_mm = last.gas_mm.unwrap_or(15.88);
    }
    if last.service == PipeService::RefrigerantTriple {
        p.gas2_dn_mm = last.gas2_mm.unwrap_or(19.05);
    }
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
    let h = s.doc_mut()?.add_pipe(&space, common, params.clone(), a, b).map_err(doc_err)?;
    // A pipe drawn on a named network takes its name.
    super::network::propagate(s);
    Ok(h)
}

fn run_colour_convention(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "colourconvention";
    if let Some(t) = str_param(p, "convention") {
        let c = cadcraft_doc::ColourConvention::parse(t).ok_or_else(|| bad(CMD, "`convention` is plan or nfx08100"))?;
        let (layers, pipes) = s.doc_mut()?.set_colour_convention(c).map_err(|e| EngineError::Other(e.to_string()))?;
        return Ok(json!({ "convention": c.name(), "layers": layers, "pipes": pipes }));
    }
    Ok(json!({ "convention": s.doc()?.colour_convention().name(), "layers": 0, "pipes": 0 }))
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
    let refrigerant = last.service.is_refrigerant();
    last.dn_mm = p.get("dn").filter(|v| !v.is_null()).map(|v| size_value(CMD, "dn", v, refrigerant)).transpose()?;
    last.gas_mm = p.get("gas").filter(|v| !v.is_null()).map(|v| size_value(CMD, "gas", v, true)).transpose()?;
    last.gas2_mm = p.get("hpGas").filter(|v| !v.is_null()).map(|v| size_value(CMD, "hpGas", v, true)).transpose()?;
    last.insulation_mm = number(CMD, p, "insulation", 0.0)?.unwrap_or(0.0);
    let mut params = pipe_params(s, &last, number(CMD, p, "labelHeight", 1.0)?.unwrap_or(100.0))?;
    if let Some(a) = altitude_param(CMD, p)? {
        params.level_mm = a;
    }
    params.load_kw = number(CMD, p, "load", 0.0)?;
    params.source = flag(CMD, p, "source")?.unwrap_or(false);
    params.slope_pct = number(CMD, p, "slope", 0.0)?;
    if let Some(n) = str_param(p, "network") {
        params.network = n.trim().to_string();
    }
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
    let label = super::elec::label_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} pipes")));
    }
    let (service, leg) = (service_param(CMD, p)?, leg_param(CMD, p)?);
    // A number imposes the size; "auto" gives it back to sizing.
    let dn: Option<Option<Value>> = match p.get("dn") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") => Some(None),
        Some(v) => Some(Some(v.clone())),
    };
    let gas: Option<f64> = p.get("gas").filter(|v| !v.is_null()).map(|v| size_value(CMD, "gas", v, true)).transpose()?;
    let hp_gas: Option<f64> = p.get("hpGas").filter(|v| !v.is_null()).map(|v| size_value(CMD, "hpGas", v, true)).transpose()?;
    let insulation = number(CMD, p, "insulation", 0.0)?;
    let altitude = altitude_param(CMD, p)?;
    // Null clears the slope.
    let slope: Option<Option<f64>> = match p.get("slope") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(_) => Some(number(CMD, p, "slope", 0.0)?),
    };
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
        match &dn {
            Some(Some(v)) => (pp.dn_mm, pp.imposed) = (size_value(CMD, "dn", v, pp.service.is_refrigerant())?, true),
            Some(None) => pp.imposed = false,
            None => {}
        }
        if pp.service.has_gas_tube() {
            if pp.gas_dn_mm <= 0.0 {
                pp.gas_dn_mm = 15.88;
            }
            pp.gas_dn_mm = gas.unwrap_or(pp.gas_dn_mm);
        }
        if pp.service == PipeService::RefrigerantTriple {
            if pp.gas2_dn_mm <= 0.0 {
                pp.gas2_dn_mm = 19.05;
            }
            pp.gas2_dn_mm = hp_gas.unwrap_or(pp.gas2_dn_mm);
        }
        pp.insulation_mm = insulation.unwrap_or(pp.insulation_mm);
        if let Some(i) = slope {
            pp.slope_pct = i;
        }
        if let Some(a) = altitude {
            pp.level_mm = a;
        }
        if let Some(l) = load {
            pp.load_kw = l;
        }
        pp.source = source.unwrap_or(pp.source);
        if let Some(show) = label {
            pp.hide_label = !show;
        }
        pp.validate().map_err(doc_err)?;
        next.push((*h, pp));
    }
    let d = s.doc_mut()?;
    for (h, pp) in &next {
        d.regenerate_pipe(*h, pp).map_err(doc_err)?;
    }
    // A name typed is the name of the whole network of each pipe.
    if let Some(n) = str_param(p, "network") {
        s.doc_mut()?.set_pipe_network_name(&hs, n).map_err(doc_err)?;
    }
    super::network::propagate(s);
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
                "totalLoss": r.total_loss_kpa,
                "critical": r.critical.hex(),
                "runs": r.runs.iter().map(|x| json!({
                    "handle": x.handle.hex(), "dn": x.dn_mm, "load": x.load, "flowM3h": x.flow_m3h, "velocity": x.velocity_ms,
                    "gradient": x.gradient_pa_m, "imposed": x.imposed, "governing": governing(x.governing),
                    "runLoss": x.run_loss_kpa, "partLoss": x.part_loss_kpa,
                    "parts": x.parts.iter().map(|(h, k)| json!({ "handle": h.hex(), "loss": k })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "warnings": r.warnings,
            })
        })
        .collect();
    // The lines of the pipe risers, sized for what they carry at this level.
    let risers: Vec<Value> = s
        .doc()?
        .pipe_risers()
        .into_iter()
        .map(|(h, r, _)| {
            json!({
                "handle": h.hex(), "name": r.name,
                "lines": r.lines.iter().map(|l| json!({
                    "tag": l.pipe(r.units).tag(), "dn": l.dn_mm, "imposed": l.imposed, "load": l.load, "flowM3h": l.flow_m3h, "velocity": l.velocity, "incoming": l.incoming,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "networks": networks, "risers": risers, "warnings": warnings }))
}

#[derive(Clone, Copy, PartialEq)]
enum PipeAsk {
    Point,
    Service,
    Leg,
    Diameter,
    Gas,
    HpGas,
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
    /// The diameter as the prompt shows it: the one typed, the refrigerant default of the Options, or Auto.
    fn shown_dn(&self) -> String {
        match (self.last.dn_mm, self.last.liquid_mm) {
            (Some(d), _) => self.size_text(d),
            (None, Some(l)) if self.last.service.is_refrigerant() => self.size_text(l),
            _ => "Auto".to_string(),
        }
    }
    fn settings(&self, s: &mut Session) {
        let dn = self.shown_dn();
        let mut gas = String::new();
        if self.last.service.has_gas_tube() {
            gas = format!(", Gas = {}", self.size_text(self.last.gas_mm.unwrap_or(15.88)));
        }
        if self.last.service == PipeService::RefrigerantTriple {
            gas.push_str(&format!(", HPgas = {}", self.size_text(self.last.gas2_mm.unwrap_or(19.05))));
        }
        s.echo(format!(
            "Current settings: Service = {}, Leg = {}, {} = {dn}{gas}, Insulation = {}",
            self.last.service.name(),
            if self.last.service.has_legs() { self.last.leg.name() } else { "-" },
            if self.last.service.has_gas_tube() { "Liquid" } else { "Diameter" },
            self.last.insulation_mm
        ));
    }
    /// A size as it is spoken: inches for a refrigerant tube, else millimetres.
    fn size_text(&self, mm: f64) -> String {
        if self.last.service.is_refrigerant() { cadcraft_doc::refrigerant_size_label(mm) } else { format!("{mm}") }
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
            "Diameter" | "Liquid" => self.ask = PipeAsk::Diameter,
            "Gas" if self.last.service.has_gas_tube() => self.ask = PipeAsk::Gas,
            "HPgas" if self.last.service == PipeService::RefrigerantTriple => self.ask = PipeAsk::HpGas,
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
                self.last.service = PipeService::parse(t).ok_or_else(|| {
                    other("Enter ChilledWater, HotWater, ChangeOver, Condensate, RefrigerantLiquid, RefrigerantGas, RefrigerantPair or RefrigerantTriple.")
                })?;
            }
            (PipeAsk::Leg, Some(t)) => self.last.leg = PipeLeg::parse(t).ok_or_else(|| other("Enter Supply or Return."))?,
            (PipeAsk::Diameter, Some(t)) if t.eq_ignore_ascii_case("auto") => self.last.dn_mm = None,
            (PipeAsk::Diameter, Some(t)) => {
                self.last.dn_mm = Some(size_value("pipeadd", "diameter", &Value::String(t.to_string()), self.last.service.is_refrigerant())?)
            }
            (PipeAsk::Gas, Some(t)) => self.last.gas_mm = Some(size_value("pipeadd", "gas", &Value::String(t.to_string()), true)?),
            (PipeAsk::HpGas, Some(t)) => self.last.gas2_mm = Some(size_value("pipeadd", "hpGas", &Value::String(t.to_string()), true)?),
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
                .kw(&[
                    "ChilledWater",
                    "HotWater",
                    "ChangeOver",
                    "Condensate",
                    "RefrigerantLiquid",
                    "RefrigerantGas",
                    "RefrigerantPair",
                    "RefrigerantTriple",
                ])
                .default(self.last.service.name()),
            PipeAsk::Leg => Prompt::new("Enter pipe leg", super::curves::KW).kw(&["Supply", "Return"]).default(self.last.leg.name()),
            PipeAsk::Diameter => Prompt::new(
                if self.last.service.has_gas_tube() {
                    "Enter liquid tube diameter (inches, e.g. 3/8, or mm) or [Auto]"
                } else if self.last.service.is_refrigerant() {
                    "Enter tube diameter (inches, e.g. 3/8, or mm) or [Auto]"
                } else {
                    "Enter pipe diameter (mm) or [Auto]"
                },
                Accept::TEXT,
            )
            .default(self.shown_dn()),
            PipeAsk::Gas => Prompt::new("Enter gas tube diameter (inches, e.g. 5/8, or mm)", Accept::TEXT)
                .default(self.size_text(self.last.gas_mm.unwrap_or(15.88))),
            PipeAsk::HpGas => Prompt::new("Enter high/low-pressure gas tube diameter (inches, e.g. 3/4, or mm)", Accept::TEXT)
                .default(self.size_text(self.last.gas2_mm.unwrap_or(19.05))),
            PipeAsk::Insulation => Prompt::new("Enter insulation thickness (mm)", Accept::NUMBER).default(format!("{}", self.last.insulation_mm)),
            PipeAsk::Point => match self.pts.last().copied() {
                None => Prompt::new("Specify start point of pipe", Accept::POINT).kw(match self.last.service {
                    PipeService::RefrigerantPair => &["Service", "Leg", "Liquid", "Gas", "Insulation"][..],
                    PipeService::RefrigerantTriple => &["Service", "Leg", "Liquid", "Gas", "HPgas", "Insulation"][..],
                    _ => &["Service", "Leg", "Diameter", "Insulation"][..],
                }),
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

/// PIPEFITTINGS: what the pipes of the drawing need where they meet and cross, as a list and as counts.
fn run_pipefittings(s: &mut Session, _p: &Value) -> Result<Value> {
    let fittings = s.doc()?.pipe_fittings();
    let mut counts = serde_json::Map::new();
    for f in &fittings {
        let n = counts.get(f.kind.name()).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(f.kind.name().into(), json!(n + 1));
    }
    let list: Vec<Value> = fittings
        .iter()
        .map(|f| {
            json!({
                "kind": format!("{:?}", f.kind), "name": f.kind.name(), "at": [f.at.x, f.at.y], "service": f.service.name(),
                "leg": f.leg.name(), "size": f.size, "pipes": f.pipes.iter().map(|h| h.hex()).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({ "fittings": list, "counts": counts }))
}
