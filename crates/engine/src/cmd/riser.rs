//! Risers (colonnes): RISERADD draws the shaft outline of a riser where a network goes up or down
//! through the floors, with its symbol, arrow and label; RISEREDIT changes one already drawn. Each
//! floor is its own drawing, so the risers of two floors meet by name.

use cadcraft_doc::{AirDirection, Drawing, EntityKind, Handle, HvacUnits, MAX_RISER_VERTICES, RiserParams, RiserSize, riser_of};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("riseradd", "Riser", run_add)
            .menu(&["Draw", "HVAC", "Riser"])
            .params(
                "{points | handle (closed polyline) | at + width + depth (mm), name?, air? (supply|extract|fresh|exhaust), direction? (up|down), \\
                 incomingFlow? (m³/h from the other floors), flow? (m³/h), size? (diameter, \"400x250\" or \"auto\"), labelHeight? (mm)} → {handle, name}",
            )
            .interactive(|s| Ok(Box::new(RiserAddM::new(s)))),
        CommandSpec::new("riseredit", "Edit Riser", run_edit).params(
            "{handle | handles, name?, air?, direction?, width? + depth? (the shaft, mm), incomingFlow? (or null), flow? (or null), size? (or \"auto\")} → {changed}",
        ),
    ]
}

/// What RISERADD remembers between uses: the air direction and the way.
#[derive(Clone, Debug, PartialEq)]
pub struct RiserLastUsed {
    pub air: AirDirection,
    pub up: bool,
}

impl Default for RiserLastUsed {
    fn default() -> Self {
        RiserLastUsed { air: AirDirection::Extract, up: true }
    }
}

/// Most risers one RISEREDIT changes.
const MAX_EDITED: usize = 10_000;

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

fn number(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a number"))),
    }
}

/// A key that may be a number, or null to clear it: None when the key is absent.
fn clearable(cmd: &str, p: &Value, key: &str) -> Result<Option<Option<f64>>> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(_) => Ok(Some(number(cmd, p, key)?)),
    }
}

fn air_param(cmd: &str, p: &Value) -> Result<Option<AirDirection>> {
    match str_param(p, "air") {
        None => Ok(None),
        Some(t) => AirDirection::parse(t).map(Some).ok_or_else(|| bad(cmd, "`air` is supply, extract, fresh or exhaust")),
    }
}

fn direction_param(cmd: &str, p: &Value) -> Result<Option<bool>> {
    match str_param(p, "direction").map(|t| t.trim().to_ascii_lowercase()).as_deref() {
        None => Ok(None),
        Some("up" | "u" | "monte") => Ok(Some(true)),
        Some("down" | "d" | "descend") => Ok(Some(false)),
        Some(_) => Err(bad(cmd, "`direction` is up or down")),
    }
}

/// "200" or 200 (a diameter), "400x250" or [400, 250] (a rectangle); "auto" or null gives none.
fn size_param(cmd: &str, p: &Value) -> Result<Option<Option<RiserSize>>> {
    let sides = |t: &str| -> Option<(f64, f64)> {
        let (a, b) = t.split_once(['x', 'X', '×', '*'])?;
        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
    };
    let err = || bad(cmd, "`size` is a diameter, \"400x250\" or \"auto\"");
    match p.get("size") {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) if t.trim().eq_ignore_ascii_case("auto") || t.trim().is_empty() => Ok(Some(None)),
        Some(Value::String(t)) => match (sides(t), t.trim().parse::<f64>().ok()) {
            (Some((w, h)), _) => Ok(Some(Some(RiserSize::Rectangular { width_mm: w, height_mm: h }))),
            (None, Some(d)) => Ok(Some(Some(RiserSize::Round { diameter_mm: d }))),
            _ => Err(err()),
        },
        Some(Value::Number(n)) => Ok(Some(Some(RiserSize::Round { diameter_mm: n.as_f64().ok_or_else(err)? }))),
        Some(Value::Array(a)) => match (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64), a.len()) {
            (Some(w), Some(h), 2) => Ok(Some(Some(RiserSize::Rectangular { width_mm: w, height_mm: h }))),
            _ => Err(err()),
        },
        Some(_) => Err(err()),
    }
}

fn risers_count(d: &Drawing) -> usize {
    d.model.iter().filter(|e| riser_of(e).is_some()).count()
}

fn add_riser(s: &mut Session, p: RiserParams) -> Result<Handle> {
    p.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    s.doc_mut()?.add_riser(&space, common, p).map_err(doc_err)
}

fn run_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "riseradd";
    let units = HvacUnits::of(s.doc()?);
    let outline = match (super::points_param(p, "points"), super::curves::handle_param(p, "handle"), super::point_param(p, "at")) {
        (Some(pts), ..) => pts,
        (None, Some(h), _) => super::hvac::polyline_outline(s.doc()?, h)?,
        (None, None, Some(c)) => {
            let (w, d) = (number(CMD, p, "width")?, number(CMD, p, "depth")?);
            let (Some(w), Some(d)) = (w, d) else { return Err(bad(CMD, "`width` and `depth` (mm) go with `at`")) };
            let mut r = RiserParams {
                name: String::new(),
                air: AirDirection::Extract,
                up: true,
                outline: vec![c; 4],
                incoming_flow_m3h: None,
                flow_m3h: None,
                size: None,
                size_set: false,
                label_height_mm: 100.0,
                units,
            };
            r.set_shaft_mm(w, d).map_err(doc_err)?;
            r.outline
        }
        _ => return Err(bad(CMD, "`points` (three or more [x, y]), `handle` (a closed polyline) or `at` + `width` + `depth` is required")),
    };
    let last = s.last_used.riser.clone();
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| format!("Riser {}", s.doc().map(risers_count).unwrap_or(0) + 1));
    let params = RiserParams {
        name,
        air: air_param(CMD, p)?.unwrap_or(last.air),
        up: direction_param(CMD, p)?.unwrap_or(last.up),
        outline,
        incoming_flow_m3h: number(CMD, p, "incomingFlow")?,
        flow_m3h: number(CMD, p, "flow")?,
        size: size_param(CMD, p)?.flatten(),
        size_set: size_param(CMD, p)?.flatten().is_some(),
        label_height_mm: number(CMD, p, "labelHeight")?.unwrap_or(100.0),
        units,
    };
    let name = params.name.clone();
    let h = add_riser(s, params)?;
    Ok(json!({ "handle": h.hex(), "name": name }))
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "riseredit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} risers")));
    }
    let (air, up, size) = (air_param(CMD, p)?, direction_param(CMD, p)?, size_param(CMD, p)?);
    let (incoming, flow) = (clearable(CMD, p, "incomingFlow")?, clearable(CMD, p, "flow")?);
    let shaft = (number(CMD, p, "width")?, number(CMD, p, "depth")?);
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut r = d.entity(*h).and_then(|e| riser_of(e)).ok_or_else(|| other("Select risers only."))?;
        if let Some(n) = str_param(p, "name") {
            r.name = n.to_string();
        }
        r.air = air.unwrap_or(r.air);
        r.up = up.unwrap_or(r.up);
        if let Some(v) = incoming {
            r.incoming_flow_m3h = v;
        }
        if let Some(v) = flow {
            r.flow_m3h = v;
        }
        if let Some(v) = size {
            (r.size, r.size_set) = (v, v.is_some());
        }
        match shaft {
            (None, None) => {}
            (w, d) => {
                let (cw, cd) = r.shaft_mm();
                r.set_shaft_mm(w.unwrap_or(cw), d.unwrap_or(cd)).map_err(doc_err)?;
            }
        }
        r.validate().map_err(doc_err)?;
        next.push((*h, r));
    }
    let d = s.doc_mut()?;
    for (h, r) in &next {
        d.regenerate_riser(*h, r).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Point,
    Object,
    Name,
    Air,
}

struct RiserAddM {
    pts: Vec<Vec2>,
    name: String,
    last: RiserLastUsed,
    ask: Ask,
}

impl RiserAddM {
    fn new(s: &Session) -> Self {
        let n = s.doc().map(risers_count).unwrap_or(0) + 1;
        RiserAddM { pts: Vec::new(), name: format!("Riser {n}"), last: s.last_used.riser.clone(), ask: Ask::Point }
    }
    fn settings(&self, s: &mut Session) {
        let way = if self.last.up { "Up" } else { "Down" };
        s.echo(format!("Current settings: Name = {}, Air = {}, Direction = {way}", self.name, self.last.air.name()));
    }
    fn params(&self, s: &Session, outline: Vec<Vec2>) -> Result<RiserParams> {
        Ok(RiserParams {
            name: self.name.clone(),
            air: self.last.air,
            up: self.last.up,
            outline,
            incoming_flow_m3h: None,
            flow_m3h: None,
            size: None,
            size_set: false,
            label_height_mm: 100.0,
            units: HvacUnits::of(s.doc()?),
        })
    }
    /// Adds the riser; the points stay when it can't be added, with the reason.
    fn finish(&mut self, s: &mut Session, outline: Vec<Vec2>) -> Result<Step> {
        let p = self.params(s, outline)?;
        if let Err(e) = p.validate() {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        add_riser(s, p)?;
        s.last_used.riser = self.last.clone();
        Ok(Step::Done)
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, i, text.as_deref()) {
            (Ask::Object, Input::Pick(hs), _) => {
                let Some(&h) = hs.first() else { return Ok(Step::Continue) };
                return match super::hvac::polyline_outline(s.doc()?, h) {
                    Ok(outline) => self.finish(s, outline),
                    Err(e) => {
                        s.echo(e.to_string());
                        Ok(Step::Continue)
                    }
                };
            }
            (Ask::Name, _, Some(t)) if !t.is_empty() => self.name = t.to_string(),
            (Ask::Air, _, Some(t)) => {
                self.last.air = AirDirection::parse(t).ok_or_else(|| other("Requires supply, extract, fresh or exhaust."))?;
            }
            (Ask::Object, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = Ask::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for RiserAddM {
    fn name(&self) -> &'static str {
        "RISERADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Object => return Prompt::new("Select a closed polyline", Accept::SELECT),
            Ask::Name => return Prompt::new("Enter riser name", Accept::TEXT).default(self.name.clone()),
            Ask::Air => {
                return Prompt::new("Enter the air direction [supply/extract/fresh/exhaust]", Accept::TEXT).default(self.last.air.name().to_string());
            }
            Ask::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify first point of the shaft", Accept::POINT).kw(&["OBject", "Name", "Air", "Up", "Down"]),
            1 | 2 => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base_opt(last),
            _ => Prompt::new("Specify next point", Accept::POINT).kw(&["Close", "Undo"]).base_opt(last),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != Ask::Point {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => {
                match k.as_str() {
                    "OBject" => self.ask = Ask::Object,
                    "Name" => self.ask = Ask::Name,
                    "Air" => self.ask = Ask::Air,
                    "Up" | "Down" => {
                        self.last.up = k == "Up";
                        self.settings(s);
                    }
                    "Undo" => {
                        self.pts.pop();
                    }
                    "Close" if self.pts.len() >= 3 => return self.finish(s, self.pts.clone()),
                    _ => {}
                }
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if self.pts.len() < MAX_RISER_VERTICES && !self.pts.last().is_some_and(|l| l.near(p, 1e-9)) {
                    self.pts.push(p);
                }
                Ok(Step::Continue)
            }
            Input::Enter if self.pts.len() >= 3 => self.finish(s, self.pts.clone()),
            Input::Enter | Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        let mut pts = self.pts.clone();
        if pts.is_empty() || self.ask != Ask::Point {
            return Vec::new();
        }
        pts.push(c);
        if let Some(first) = self.pts.first() {
            pts.push(*first);
        }
        pts.windows(2).filter_map(|w| Some(EntityKind::Line(cadcraft_doc::Line { a: w.first()?.to3(0.0), b: w.get(1)?.to3(0.0) }))).collect()
    }
}
