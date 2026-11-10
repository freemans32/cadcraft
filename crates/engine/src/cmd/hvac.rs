//! Ventilation in plan: SPACEADD draws a space (a room's outline, its use and occupants, the
//! fresh air it needs), AIRTERMINALADD places diffusers and grilles one by one or spread over a
//! space, AIRTERMINALMODELEDIT lists, creates, changes and deletes the drawing's air terminal
//! models.

use cadcraft_doc::{
    AirDirection, Distribution, Drawing, EntityKind, Handle, HvacUnits, MAX_DISTRIBUTED, MAX_SPACE_VERTICES, SpaceParams, TerminalKind,
    TerminalModel, TerminalParams, distribute_in_polygon, smallest_model_for, space_of, standard_air_terminal_models,
};
use cadcraft_geom::Vec2;
use cadcraft_hvac::RoomUse;
use serde_json::{Value, json};

use super::{CommandSpec, bad, bool_or, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("spaceadd", "Space", run_spaceadd)
            .menu(&["Draw", "HVAC", "Space"])
            .params(
                "{points | handle (closed polyline), name?, use?: office|meeting|lightWorkshop|workshop|toilet|shower|\
                 showerToilet|sanitary|washbasins, occupants?, fixtures?, collective?, flow? (m³/h), labelHeight? (mm)} \
                 → {handle, area (m²), flow (m³/h)}",
            )
            .interactive(|s| Ok(Box::new(SpaceAddM::new(s)))),
        CommandSpec::new("airterminaladd", "Air Terminal", run_airterminaladd)
            .menu(&["Draw", "HVAC", "Air Terminal"])
            .params(
                "{at | space (handle) with count? or rows?+cols?, model? | type?: squareCeiling|round|rectangular|linear|extract, \
                 flow? (m³/h per terminal; spread over a space: its flow shared), air?: supply|extract, rotation? (degrees), \
                 labelHeight? (mm)} → {handles, model, flow}",
            )
            .interactive(|s| Ok(Box::new(TerminalAddM::new(s)))),
        CommandSpec::new("airterminalmodeledit", "Air Terminal Models", run_modeledit).menu(&["Draw", "HVAC", "Air Terminal Models"]).params(
            "{} → {models} | {name, type?, width? (mm), depth? (mm), slots?, neck? (mm), maxFlow? (m³/h), pressureDrop? (Pa), copyFrom?} \
             → {model} | {delete: name}",
        ),
    ]
}

/// What AIRTERMINALADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalLastUsed {
    pub kind: TerminalKind,
    /// The model chosen, or empty for the smallest that carries the flow.
    pub model: String,
    pub flow_m3h: f64,
    pub air: AirDirection,
}

impl Default for TerminalLastUsed {
    fn default() -> Self {
        TerminalLastUsed { kind: TerminalKind::SquareCeiling, model: String::new(), flow_m3h: 150.0, air: AirDirection::Supply }
    }
}

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

fn doc_err(e: cadcraft_doc::DocError) -> EngineError {
    other(e.to_string())
}

// =====================================================================================
// SPACEADD
// =====================================================================================

fn room_use(text: &str, fixtures: u32) -> Option<RoomUse> {
    Some(match text.trim().to_ascii_lowercase().as_str() {
        "office" => RoomUse::Office,
        "meeting" | "dining" | "sales" => RoomUse::DiningSalesMeeting,
        "lightworkshop" | "light" => RoomUse::LightWorkshop,
        "workshop" => RoomUse::OtherWorkshop,
        "toilet" => RoomUse::Toilet,
        "shower" => RoomUse::Shower,
        "showertoilet" => RoomUse::ShowerWithToilet,
        "sanitary" => RoomUse::GroupedSanitary { fixtures },
        "washbasins" => RoomUse::GroupedWashbasins { fixtures },
        _ => return None,
    })
}

fn room_name(r: RoomUse) -> &'static str {
    match r {
        RoomUse::Office => "Office",
        RoomUse::DiningSalesMeeting => "Meeting",
        RoomUse::LightWorkshop => "LIght",
        RoomUse::OtherWorkshop => "Workshop",
        RoomUse::Toilet => "Toilet",
        RoomUse::Shower => "SHower",
        RoomUse::ShowerWithToilet => "ShowerToilet",
        RoomUse::GroupedSanitary { .. } => "SAnitary",
        RoomUse::GroupedWashbasins { .. } => "WAshbasins",
    }
}

const ROOM_KEYWORDS: &[&str] = &["Office", "Meeting", "LIght", "Workshop", "Toilet", "SHower", "ShowerToilet", "SAnitary", "WAshbasins"];

fn spaces_count(d: &Drawing) -> usize {
    d.model.iter().filter(|e| space_of(e).is_some()).count()
}

/// The vertices of closed polyline `h`, refusing arcs: a space's outline is straight.
fn polyline_outline(d: &Drawing, h: Handle) -> Result<Vec<Vec2>> {
    let e = d.entity(h).ok_or_else(|| other("Cannot find that object."))?;
    match &e.kind {
        EntityKind::LwPolyline(p) if p.closed => {
            if p.vertices.iter().any(|v| v.bulge.abs() > 1e-9) {
                return Err(other("The polyline has arcs: a space's outline is made of straight segments."));
            }
            Ok(p.vertices.iter().map(|v| v.p).collect())
        }
        _ => Err(other("Select a closed polyline.")),
    }
}

fn add_space(s: &mut Session, p: SpaceParams) -> Result<(Handle, f64, f64)> {
    let area = p.area_m2();
    let flow = p.flow().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    let h = s.doc_mut()?.add_space(&space, common, p).map_err(doc_err)?;
    Ok((h, area, flow))
}

fn u32_param(cmd: &str, p: &Value, key: &str, default: u32) -> Result<u32> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad(cmd, format!("`{key}` must be a whole number"))),
    }
}

fn positive_param(cmd: &str, p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0).map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be a positive number"))),
    }
}

fn run_spaceadd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "spaceadd";
    let mut outline = match (super::curves::handle_param(p, "handle"), p.get("points")) {
        (Some(h), _) => polyline_outline(s.doc()?, h)?,
        (None, Some(Value::Array(raw))) => {
            if raw.len() > MAX_SPACE_VERTICES + 1 {
                return Err(bad(CMD, format!("a space has at most {MAX_SPACE_VERTICES} points")));
            }
            let pts: Option<Vec<Vec2>> = raw.iter().map(super::point_value).collect();
            super::distinct_points(&pts.ok_or_else(|| bad(CMD, "`points` must be [x, y] points"))?)
        }
        _ => return Err(bad(CMD, "`points` (3 or more) or `handle` (a closed polyline) is required")),
    };
    if outline.len() > 3 && outline.first().zip(outline.last()).is_some_and(|(a, b)| a.near(*b, 1e-12)) {
        outline.pop();
    }
    let fixtures = u32_param(CMD, p, "fixtures", 1)?;
    let room = match str_param(p, "use") {
        Some(t) => room_use(t, fixtures).ok_or_else(|| bad(CMD, "unknown `use`"))?,
        None => RoomUse::Office,
    };
    let d = s.doc()?;
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| format!("Space {}", spaces_count(d) + 1));
    let space = SpaceParams {
        name,
        room,
        occupants: u32_param(CMD, p, "occupants", 1)?,
        collective: bool_or(p, "collective", true),
        flow_m3h: positive_param(CMD, p, "flow")?,
        outline,
        label_height_mm: positive_param(CMD, p, "labelHeight")?.unwrap_or(200.0),
        units: HvacUnits::of(d),
    };
    let (h, area, flow) = add_space(s, space)?;
    Ok(json!({ "handle": h.hex(), "area": area, "flow": flow }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpaceAsk {
    Point,
    Object,
    Name,
    Use,
    Occupants,
    Fixtures,
    Flow,
}

struct SpaceAddM {
    pts: Vec<Vec2>,
    name: String,
    room: RoomUse,
    occupants: u32,
    /// A flow the user set, m³/h.
    flow: Option<f64>,
    ask: SpaceAsk,
}

impl SpaceAddM {
    fn new(s: &Session) -> Self {
        let n = s.doc().map(spaces_count).unwrap_or(0) + 1;
        SpaceAddM { pts: Vec::new(), name: format!("Space {n}"), room: RoomUse::Office, occupants: 1, flow: None, ask: SpaceAsk::Point }
    }
    fn params(&self, s: &Session, outline: Vec<Vec2>) -> Result<SpaceParams> {
        Ok(SpaceParams {
            name: self.name.clone(),
            room: self.room,
            occupants: self.occupants,
            collective: true,
            flow_m3h: self.flow,
            outline,
            label_height_mm: 200.0,
            units: HvacUnits::of(s.doc()?),
        })
    }
    fn settings(&self, s: &mut Session) {
        let flow = self.flow.map(|f| format!("{f:.0} m³/h")).unwrap_or_else(|| "labour code".into());
        s.echo(format!("Current settings: Name = {}, Use = {}, Occupants = {}, Flow = {flow}", self.name, room_name(self.room), self.occupants));
    }
    /// Adds the space; the points stay when it can't be added, with the reason.
    fn finish(&mut self, s: &mut Session, outline: Vec<Vec2>) -> Result<Step> {
        let p = self.params(s, outline)?;
        if let Err(e) = p.validate() {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        let (_, area, flow) = add_space(s, p)?;
        s.echo(format!("{}: {area:.1} m², {flow:.0} m³/h", self.name));
        Ok(Step::Done)
    }
    fn keyword(&mut self, s: &mut Session, k: &str) -> Result<Step> {
        match k {
            "OBject" => self.ask = SpaceAsk::Object,
            "Name" => self.ask = SpaceAsk::Name,
            "Use" => self.ask = SpaceAsk::Use,
            "OCcupants" => self.ask = SpaceAsk::Occupants,
            "Flow" => self.ask = SpaceAsk::Flow,
            "Undo" => {
                self.pts.pop();
            }
            "Close" if self.pts.len() >= 3 => return self.finish(s, self.pts.clone()),
            _ => {}
        }
        Ok(Step::Continue)
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, i, text.as_deref()) {
            (SpaceAsk::Object, Input::Pick(hs), _) => {
                let Some(&h) = hs.first() else { return Ok(Step::Continue) };
                match polyline_outline(s.doc()?, h) {
                    Ok(outline) => return self.finish(s, outline),
                    Err(e) => {
                        s.echo(e.to_string());
                        return Ok(Step::Continue);
                    }
                }
            }
            (SpaceAsk::Name, _, Some(t)) if !t.is_empty() => self.name = t.to_string(),
            (SpaceAsk::Use, _, Some(t)) => {
                let fixtures = 1;
                self.room = room_use(t, fixtures).ok_or_else(|| other("Choose one of the listed uses."))?;
                if matches!(self.room, RoomUse::GroupedSanitary { .. } | RoomUse::GroupedWashbasins { .. }) {
                    self.ask = SpaceAsk::Fixtures;
                    return Ok(Step::Continue);
                }
            }
            (SpaceAsk::Occupants, _, Some(t)) => {
                self.occupants = t
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n <= cadcraft_doc::MAX_OCCUPANTS)
                    .ok_or_else(|| other("Requires a whole number of occupants."))?;
            }
            (SpaceAsk::Fixtures, _, Some(t)) => {
                let n = t.parse::<u32>().ok().filter(|n| *n >= 1 && *n <= 10_000).ok_or_else(|| other("Requires a whole number of fixtures."))?;
                self.room = match self.room {
                    RoomUse::GroupedWashbasins { .. } => RoomUse::GroupedWashbasins { fixtures: n },
                    _ => RoomUse::GroupedSanitary { fixtures: n },
                };
            }
            (SpaceAsk::Flow, _, Some(t)) => {
                // 0 goes back to the labour code's flow.
                let f = t.parse::<f64>().ok().filter(|f| f.is_finite() && *f >= 0.0 && *f <= cadcraft_doc::MAX_SPACE_FLOW_M3H);
                self.flow = match f.ok_or_else(|| other("Requires a flow in m³/h (0 for the labour code's)."))? {
                    0.0 => None,
                    f => Some(f),
                };
            }
            (SpaceAsk::Object, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = SpaceAsk::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for SpaceAddM {
    fn name(&self) -> &'static str {
        "SPACEADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            SpaceAsk::Object => return Prompt::new("Select a closed polyline", Accept::SELECT),
            SpaceAsk::Name => return Prompt::new("Enter space name", Accept::TEXT).default(self.name.clone()),
            SpaceAsk::Use => return Prompt::new("Enter space use", super::curves::KW).kw(ROOM_KEYWORDS).default(room_name(self.room)),
            SpaceAsk::Occupants => return Prompt::new("Enter number of occupants", Accept::NUMBER).default(self.occupants.to_string()),
            SpaceAsk::Fixtures => return Prompt::new("Enter number of fixtures", Accept::NUMBER).default("1"),
            SpaceAsk::Flow => {
                return Prompt::new("Enter flow (m³/h, 0 for the labour code)", Accept::NUMBER).default(format!("{:.0}", self.flow.unwrap_or(0.0)));
            }
            SpaceAsk::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify first point of space", Accept::POINT).kw(&["OBject", "Name", "Use", "OCcupants", "Flow"]),
            1 | 2 => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base_opt(last),
            _ => Prompt::new("Specify next point", Accept::POINT).kw(&["Close", "Undo"]).base_opt(last),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != SpaceAsk::Point {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => self.keyword(s, &k),
            Input::Point(p) => {
                if self.pts.len() < MAX_SPACE_VERTICES && !self.pts.last().is_some_and(|l| l.near(p, 1e-9)) {
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
        if pts.is_empty() || self.ask != SpaceAsk::Point {
            return Vec::new();
        }
        pts.push(c);
        if let Some(first) = self.pts.first() {
            pts.push(*first);
        }
        pts.windows(2).filter_map(|w| Some(EntityKind::Line(cadcraft_doc::Line { a: w.first()?.to3(0.0), b: w.get(1)?.to3(0.0) }))).collect()
    }
}

// =====================================================================================
// AIRTERMINALADD
// =====================================================================================

fn model_named(d: &Drawing, name: &str) -> Result<TerminalModel> {
    d.available_air_terminal_models()
        .into_iter()
        .find(|m| m.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| other(format!("Cannot find air terminal model \"{}\".", name.trim())))
}

/// The model to place: `model` when one is named, else the smallest of `kind` for `flow`.
fn choose_model(d: &Drawing, model: &str, kind: TerminalKind, flow: f64) -> Result<TerminalModel> {
    if !model.trim().is_empty() {
        return model_named(d, model);
    }
    smallest_model_for(&d.available_air_terminal_models(), kind, flow)
        .ok_or_else(|| other(format!("No {} model carries {flow:.0} m³/h: place more terminals, or add a larger model.", kind.name())))
}

/// Places one terminal of `model` per point; a standard model used is copied into the drawing.
fn place(s: &mut Session, model: TerminalModel, flow: f64, air: AirDirection, label_mm: f64, at: &[Vec2], rotation: f64) -> Result<Vec<Handle>> {
    let p = TerminalParams { model, flow_m3h: flow, air, label_height_mm: label_mm, units: HvacUnits::of(s.doc()?) };
    p.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    let d = s.doc_mut()?;
    let mut handles = Vec::with_capacity(at.len());
    for pt in at {
        handles.push(d.add_air_terminal(&space, common.clone(), &p, *pt, rotation).map_err(doc_err)?);
    }
    if d.air_terminal_model(&p.model.name).is_none() {
        d.air_terminal_models.push(p.model.clone());
    }
    Ok(handles)
}

/// Points spread over space `h` and the flow per terminal (the space's flow shared, unless
/// `flow` is given).
fn spread(d: &Drawing, h: Handle, how: Distribution, flow: Option<f64>) -> Result<(Vec<Vec2>, f64)> {
    let (sp, outline) = d.space_outline(h).ok_or_else(|| other("Select a space."))?;
    let pts = distribute_in_polygon(&outline, how).map_err(doc_err)?;
    let total = sp.flow().map_err(doc_err)?;
    let each = flow.unwrap_or(total / pts.len().max(1) as f64);
    Ok((pts, each))
}

fn run_airterminaladd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "airterminaladd";
    let air = match str_param(p, "air") {
        Some(t) => AirDirection::parse(t).ok_or_else(|| bad(CMD, "`air` must be supply or extract"))?,
        None => AirDirection::Supply,
    };
    let kind = match str_param(p, "type") {
        Some(t) => TerminalKind::parse(t).ok_or_else(|| bad(CMD, "unknown `type`"))?,
        None if air == AirDirection::Extract => TerminalKind::Extract,
        None => TerminalKind::SquareCeiling,
    };
    let flow = positive_param(CMD, p, "flow")?;
    let rotation = super::f64_or(p, "rotation", 0.0).to_radians();
    let label_mm = positive_param(CMD, p, "labelHeight")?.unwrap_or(100.0);
    let (points, each) = match (super::curves::handle_param(p, "space"), super::point_param(p, "at")) {
        (Some(h), _) => {
            let count = |k| u32_param(CMD, p, k, 0).map(|n| n as usize);
            let how = match (count("rows")?, count("cols")?, count("count")?) {
                (r, c, _) if r > 0 && c > 0 => Distribution::Grid { rows: r, cols: c },
                (_, _, n) => Distribution::Count(n.max(1)),
            };
            spread(s.doc()?, h, how, flow)?
        }
        (None, Some(at)) => (vec![at], flow.unwrap_or(TerminalLastUsed::default().flow_m3h)),
        _ => return Err(bad(CMD, "`at` ([x, y]) or `space` (a space's handle) is required")),
    };
    let model = choose_model(s.doc()?, str_param(p, "model").unwrap_or(""), kind, each)?;
    let name = model.name.clone();
    let handles = place(s, model, each, air, label_mm, &points, rotation)?;
    Ok(json!({ "handles": handles.iter().map(|h| h.hex()).collect::<Vec<_>>(), "model": name, "flow": each }))
}

#[derive(Clone, Copy, PartialEq)]
enum TermAsk {
    Location,
    Rotation(Vec2),
    Type,
    Size,
    Flow,
    Air,
    Space,
    Count(Handle),
}

struct TerminalAddM {
    last: TerminalLastUsed,
    ask: TermAsk,
}

impl TerminalAddM {
    fn new(s: &Session) -> Self {
        TerminalAddM { last: s.last_used.air_terminal.clone(), ask: TermAsk::Location }
    }
    fn settings(&self, s: &mut Session) {
        let size = if self.last.model.is_empty() { "Auto".to_string() } else { self.last.model.clone() };
        s.echo(format!(
            "Current settings: Type = {}, Size = {size}, Flow = {:.0} m³/h, Air = {}",
            self.last.kind.keyword(),
            self.last.flow_m3h,
            self.last.air.name()
        ));
    }
    fn remember(&self, s: &mut Session) {
        s.last_used.air_terminal = self.last.clone();
    }
    fn place_at(&mut self, s: &mut Session, at: &[Vec2], flow: f64, rotation: f64) -> Result<()> {
        let model = choose_model(s.doc()?, &self.last.model, self.last.kind, flow)?;
        let name = model.name.clone();
        let n = place(s, model, flow, self.last.air, 100.0, at, rotation)?.len();
        s.echo(format!("{n} × {name}, {flow:.0} m³/h"));
        self.remember(s);
        Ok(())
    }
    fn text(i: &Input) -> Option<String> {
        match i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        }
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = Self::text(&i);
        match (self.ask, &i, text.as_deref()) {
            (TermAsk::Type, _, Some(t)) => {
                self.last.kind = TerminalKind::parse(t).ok_or_else(|| other("Choose one of the listed types."))?;
                self.last.model.clear();
            }
            (TermAsk::Size, _, Some("?")) => {
                let kind = self.last.kind;
                let names: Vec<String> = s
                    .doc()?
                    .available_air_terminal_models()
                    .into_iter()
                    .filter(|m| m.kind == kind)
                    .map(|m| format!("{} ({:.0} m³/h)", m.name, m.max_flow_m3h))
                    .collect();
                s.echo(format!("Models: {}", names.join(", ")));
                return Ok(Step::Continue);
            }
            (TermAsk::Size, _, Some(t)) if t.eq_ignore_ascii_case("auto") => self.last.model.clear(),
            (TermAsk::Size, _, Some(t)) if !t.is_empty() => {
                let m = model_named(s.doc()?, t)?;
                self.last.kind = m.kind;
                self.last.model = m.name;
            }
            (TermAsk::Flow, _, Some(t)) => {
                self.last.flow_m3h = t
                    .parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite() && *f > 0.0 && *f <= cadcraft_doc::MAX_TERMINAL_FLOW_M3H)
                    .ok_or_else(|| other("Requires a positive flow in m³/h."))?;
            }
            (TermAsk::Air, _, Some(t)) => {
                self.last.air = AirDirection::parse(t).ok_or_else(|| other("Enter Supply or Extract."))?;
                if self.last.air == AirDirection::Extract && self.last.model.is_empty() {
                    self.last.kind = TerminalKind::Extract;
                }
            }
            (TermAsk::Space, Input::Pick(hs), _) => {
                let d = s.doc()?;
                match hs.iter().find(|h| d.space_outline(**h).is_some()) {
                    Some(&h) => self.ask = TermAsk::Count(h),
                    None => s.echo("Select a space."),
                }
                return Ok(Step::Continue);
            }
            (TermAsk::Count(h), _, Some(t)) => return self.distribute(s, h, t),
            (TermAsk::Count(h), Input::Enter, _) => return self.distribute(s, h, "1"),
            (TermAsk::Rotation(at), _, _) => {
                let angle = match (&i, text.as_deref()) {
                    (Input::Point(p), _) => (*p - at).y.atan2((*p - at).x),
                    (_, Some(t)) => {
                        t.parse::<f64>().ok().filter(|a| a.is_finite()).ok_or_else(|| other("Requires an angle in degrees."))?.to_radians()
                    }
                    _ => 0.0,
                };
                let flow = self.last.flow_m3h;
                self.place_at(s, &[at], flow, angle)?;
            }
            (_, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = TermAsk::Location;
        self.settings(s);
        Ok(Step::Continue)
    }
    /// Spreads terminals over space `h`: `t` is a count, or rows × columns ("2x3").
    fn distribute(&mut self, s: &mut Session, h: Handle, t: &str) -> Result<Step> {
        let parse = |x: &str| x.trim().parse::<usize>().ok().filter(|n| (1..=MAX_DISTRIBUTED).contains(n));
        let how = match t.split_once(['x', 'X', '×', '*']) {
            Some((r, c)) => Distribution::Grid {
                rows: parse(r).ok_or_else(|| other("Enter rows x columns, as 2x3."))?,
                cols: parse(c).ok_or_else(|| other("Enter rows x columns, as 2x3."))?,
            },
            None => Distribution::Count(parse(t).ok_or_else(|| other(format!("Enter 1 to {MAX_DISTRIBUTED} terminals, or rows x columns.")))?),
        };
        let (pts, each) = spread(s.doc()?, h, how, None)?;
        self.place_at(s, &pts, each, 0.0)?;
        Ok(Step::Done)
    }
}

impl Interactive for TerminalAddM {
    fn name(&self) -> &'static str {
        "AIRTERMINALADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            TermAsk::Location => Prompt::new("Specify location of air terminal", Accept::POINT).kw(&["Type", "Size", "Flow", "Air", "Distribute"]),
            TermAsk::Rotation(at) => Prompt::new("Specify rotation angle", Accept::POINT_OR_NUMBER).base(at).default("0"),
            TermAsk::Type => Prompt::new("Enter air terminal type", super::curves::KW)
                .kw(&["Square", "Round", "Grille", "Linear", "Extract"])
                .default(self.last.kind.keyword()),
            TermAsk::Size => Prompt::new("Enter model name or [Auto/?]", Accept::TEXT).default(if self.last.model.is_empty() {
                "Auto".to_string()
            } else {
                self.last.model.clone()
            }),
            TermAsk::Flow => Prompt::new("Enter flow per terminal (m³/h)", Accept::NUMBER).default(format!("{:.0}", self.last.flow_m3h)),
            TermAsk::Air => Prompt::new("Enter air direction", super::curves::KW)
                .kw(&["Supply", "Extract"])
                .default(if self.last.air == AirDirection::Supply { "Supply" } else { "Extract" }),
            TermAsk::Space => Prompt::new("Select space", Accept::SELECT),
            TermAsk::Count(_) => Prompt::new("Enter number of terminals or rows x columns", Accept::TEXT).default("1"),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != TermAsk::Location {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => {
                self.ask = match k.as_str() {
                    "Type" => TermAsk::Type,
                    "Size" => TermAsk::Size,
                    "Flow" => TermAsk::Flow,
                    "Air" => TermAsk::Air,
                    "Distribute" => TermAsk::Space,
                    _ => TermAsk::Location,
                };
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                self.ask = TermAsk::Rotation(p);
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
// AIRTERMINALMODELEDIT
// =====================================================================================

fn model_value(m: &TerminalModel, standard: bool) -> Value {
    let mut v = serde_json::to_value(m).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("standard".into(), json!(standard));
    }
    v
}

fn run_modeledit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "airterminalmodeledit";
    if let Some(name) = str_param(p, "delete") {
        let d = s.doc_mut()?;
        let before = d.air_terminal_models.len();
        d.air_terminal_models.retain(|m| !m.name.eq_ignore_ascii_case(name.trim()));
        if d.air_terminal_models.len() == before {
            return Err(other(format!("The drawing has no air terminal model \"{}\".", name.trim())));
        }
        return Ok(json!({ "deleted": name.trim() }));
    }
    let Some(name) = str_param(p, "name") else {
        let d = s.doc()?;
        let models: Vec<Value> = d.available_air_terminal_models().iter().map(|m| model_value(m, d.air_terminal_model(&m.name).is_none())).collect();
        return Ok(json!({ "models": models }));
    };
    let d = s.doc()?;
    let base = match str_param(p, "copyFrom") {
        Some(from) => model_named(d, from)?,
        None => model_named(d, name).ok().unwrap_or_else(|| {
            standard_air_terminal_models().into_iter().next().unwrap_or(TerminalModel {
                name: String::new(),
                kind: TerminalKind::SquareCeiling,
                width_mm: 600.0,
                depth_mm: 600.0,
                slots: 0,
                neck_mm: 160.0,
                max_flow_m3h: 250.0,
                pressure_drop_pa: 15.0,
            })
        }),
    };
    let mut m = TerminalModel { name: name.trim().to_string(), ..base };
    if let Some(t) = str_param(p, "type") {
        m.kind = TerminalKind::parse(t).ok_or_else(|| bad(CMD, "unknown `type`"))?;
        if m.kind == TerminalKind::Linear && m.slots == 0 {
            m.slots = 2;
        }
    }
    if let Some(w) = positive_param(CMD, p, "width")? {
        m.width_mm = w;
        if matches!(m.kind, TerminalKind::SquareCeiling | TerminalKind::Round | TerminalKind::Extract) {
            m.depth_mm = w;
        }
    }
    m.depth_mm = positive_param(CMD, p, "depth")?.unwrap_or(m.depth_mm);
    m.neck_mm = positive_param(CMD, p, "neck")?.unwrap_or(m.neck_mm);
    m.max_flow_m3h = positive_param(CMD, p, "maxFlow")?.unwrap_or(m.max_flow_m3h);
    if let Some(v) = p.get("pressureDrop").filter(|v| !v.is_null()) {
        m.pressure_drop_pa = v.as_f64().filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| bad(CMD, "`pressureDrop` must be zero or positive"))?;
    }
    if let Some(v) = p.get("slots").filter(|v| !v.is_null()) {
        m.slots = v.as_u64().and_then(|n| u8::try_from(n).ok()).ok_or_else(|| bad(CMD, "`slots` must be a whole number"))?;
    }
    m.validate().map_err(doc_err)?;
    let d = s.doc_mut()?;
    match d.air_terminal_models.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&m.name)) {
        Some(x) => *x = m.clone(),
        None => d.air_terminal_models.push(m.clone()),
    }
    Ok(json!({ "model": model_value(&m, false) }))
}
