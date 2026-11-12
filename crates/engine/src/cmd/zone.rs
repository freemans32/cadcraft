//! Height zones: ZONEADD draws a hatched outline where the height available for ducts is less than
//! elsewhere (under a beam, a drop, a low false ceiling), with the altitude of the underside of the
//! ducts there if it is given; ZONEEDIT changes one already drawn. DUCTSIZE cuts the ducts that cross
//! a zone at its edges and sizes the piece inside for the height it leaves.

use cadcraft_doc::{Drawing, EntityKind, Handle, HvacUnits, MAX_ZONE_VERTICES, ZoneParams, zone_of};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("zoneadd", "Height Zone", run_add)
            .menu(&["Draw", "HVAC", "Height Zone"])
            .params(
                "{points | handle (closed polyline), voidHeight (mm available for ducts), name?, altitude? (metres: the underside of the ducts, \\
                 written on their labels), labelHeight? (mm)} → {handle, name}",
            )
            .interactive(|s| Ok(Box::new(ZoneAddM::new(s)))),
        CommandSpec::new("zoneedit", "Edit Height Zone", run_edit)
            .params("{handle | handles, name?, voidHeight? (mm), altitude? (metres, or null for none)} → {changed}"),
    ]
}

/// What ZONEADD remembers between uses: the height and the altitude.
#[derive(Clone, Debug, PartialEq)]
pub struct ZoneLastUsed {
    pub void_height_mm: f64,
    pub bottom_m: Option<f64>,
}

impl Default for ZoneLastUsed {
    fn default() -> Self {
        ZoneLastUsed { void_height_mm: 250.0, bottom_m: None }
    }
}

/// Most zones one ZONEEDIT changes.
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

fn zones_count(d: &Drawing) -> usize {
    d.model.iter().filter(|e| zone_of(e).is_some()).count()
}

fn add_zone(s: &mut Session, p: ZoneParams) -> Result<Handle> {
    p.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    s.doc_mut()?.add_zone(&space, common, p).map_err(doc_err)
}

fn run_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "zoneadd";
    let outline = match (super::points_param(p, "points"), super::curves::handle_param(p, "handle")) {
        (Some(pts), _) => pts,
        (None, Some(h)) => super::hvac::polyline_outline(s.doc()?, h)?,
        _ => return Err(bad(CMD, "`points` (three or more [x, y]) or `handle` (a closed polyline) is required")),
    };
    let void = number(CMD, p, "voidHeight")?.ok_or_else(|| bad(CMD, "`voidHeight` (mm) is required"))?;
    let name = str_param(p, "name").map(str::to_string).unwrap_or_else(|| format!("Zone {}", s.doc().map(zones_count).unwrap_or(0) + 1));
    let params = ZoneParams {
        name,
        void_height_mm: void,
        bottom_mm: number(CMD, p, "altitude")?.map(|m| m * 1000.0),
        outline,
        label_height_mm: number(CMD, p, "labelHeight")?.unwrap_or(100.0),
        units: HvacUnits::of(s.doc()?),
    };
    let name = params.name.clone();
    let h = add_zone(s, params)?;
    Ok(json!({ "handle": h.hex(), "name": name }))
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "zoneedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} zones")));
    }
    let void = number(CMD, p, "voidHeight")?;
    // Null clears the altitude; a number is metres.
    let bottom: Option<Option<f64>> = match p.get("altitude") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(_) => Some(number(CMD, p, "altitude")?.map(|m| m * 1000.0)),
    };
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut z = d.entity(*h).and_then(|e| zone_of(e)).ok_or_else(|| other("Select height zones only."))?;
        if let Some(n) = str_param(p, "name") {
            z.name = n.to_string();
        }
        z.void_height_mm = void.unwrap_or(z.void_height_mm);
        if let Some(b) = bottom {
            z.bottom_mm = b;
        }
        z.validate().map_err(doc_err)?;
        next.push((*h, z));
    }
    let d = s.doc_mut()?;
    for (h, z) in &next {
        d.regenerate_zone(*h, z).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Point,
    Object,
    Name,
    Height,
    Altitude,
}

struct ZoneAddM {
    pts: Vec<Vec2>,
    name: String,
    last: ZoneLastUsed,
    ask: Ask,
}

impl ZoneAddM {
    fn new(s: &Session) -> Self {
        let n = s.doc().map(zones_count).unwrap_or(0) + 1;
        ZoneAddM { pts: Vec::new(), name: format!("Zone {n}"), last: s.last_used.zone.clone(), ask: Ask::Point }
    }
    fn settings(&self, s: &mut Session) {
        let alt = self.last.bottom_m.map_or("none".to_string(), |m| format!("{m:.2} m"));
        s.echo(format!("Current settings: Name = {}, Height = {} mm, Altitude = {alt}", self.name, self.last.void_height_mm));
    }
    fn params(&self, s: &Session, outline: Vec<Vec2>) -> Result<ZoneParams> {
        Ok(ZoneParams {
            name: self.name.clone(),
            void_height_mm: self.last.void_height_mm,
            bottom_mm: self.last.bottom_m.map(|m| m * 1000.0),
            outline,
            label_height_mm: 100.0,
            units: HvacUnits::of(s.doc()?),
        })
    }
    /// Adds the zone; the points stay when it can't be added, with the reason.
    fn finish(&mut self, s: &mut Session, outline: Vec<Vec2>) -> Result<Step> {
        let p = self.params(s, outline)?;
        if let Err(e) = p.validate() {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        add_zone(s, p)?;
        s.last_used.zone = self.last.clone();
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
            (Ask::Height, _, Some(t)) => {
                self.last.void_height_mm = t
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && (20.0..=cadcraft_doc::MAX_VOID_MM).contains(v))
                    .ok_or_else(|| other("Requires a height in millimetres, 20 or more."))?;
            }
            // Nothing or "none" clears the altitude.
            (Ask::Altitude, _, Some(t)) if t.is_empty() || t.eq_ignore_ascii_case("none") => self.last.bottom_m = None,
            (Ask::Altitude, _, Some(t)) => {
                self.last.bottom_m =
                    Some(t.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| other("Requires an altitude in metres, or None."))?);
            }
            (Ask::Object, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = Ask::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for ZoneAddM {
    fn name(&self) -> &'static str {
        "ZONEADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Object => return Prompt::new("Select a closed polyline", Accept::SELECT),
            Ask::Name => return Prompt::new("Enter zone name", Accept::TEXT).default(self.name.clone()),
            Ask::Height => {
                return Prompt::new("Enter the height available for ducts (mm)", Accept::NUMBER).default(format!("{}", self.last.void_height_mm));
            }
            Ask::Altitude => {
                return Prompt::new("Enter the altitude of the underside of the ducts (m) or [None]", Accept::TEXT)
                    .default(self.last.bottom_m.map_or("None".to_string(), |m| format!("{m}")));
            }
            Ask::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify first point of zone", Accept::POINT).kw(&["OBject", "Name", "Height", "Altitude"]),
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
                    "Height" => self.ask = Ask::Height,
                    "Altitude" => self.ask = Ask::Altitude,
                    "Undo" => {
                        self.pts.pop();
                    }
                    "Close" if self.pts.len() >= 3 => return self.finish(s, self.pts.clone()),
                    _ => {}
                }
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if self.pts.len() < MAX_ZONE_VERTICES && !self.pts.last().is_some_and(|l| l.near(p, 1e-9)) {
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
