//! Slabs in plan: SLABADD draws a floor, roof or foundation slab over a closed outline (points, or a closed
//! polyline), SLABEDIT changes slabs already drawn.

use cadcraft_doc::{Handle, HvacUnits, MAX_SLAB_VERTICES, SlabKind, SlabParams, WallUnits, slab_of};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

/// Most slabs one SLABEDIT changes.
const MAX_EDITED: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("slabadd", "Slab", run_slabadd)
            .menu(&["Draw", "Architecture", "Slab"])
            .params(
                "{points (3 or more) | handle (closed polyline), kind?: floor|roof|foundation, name?, thickness? (mm), elevation? (m, the top), \
                 pattern? (hatch name, empty for none), labelHeight? (mm)} → {handle, area (m²), volume (m³)}",
            )
            .interactive(|s| Ok(Box::new(SlabAddM::new(s)))),
        CommandSpec::new("slabedit", "Edit Slab", run_slabedit)
            .params("{handle | handles, kind?, name?, thickness? (mm), elevation? (m), pattern?} → {changed}"),
    ]
}

/// What SLABADD remembers between uses.
#[derive(Clone, Debug, PartialEq)]
pub struct SlabLastUsed {
    pub kind: SlabKind,
    /// The thickness the user typed, mm; `None`: the kind's own.
    pub thickness_mm: Option<f64>,
    pub elevation_mm: f64,
}

impl Default for SlabLastUsed {
    fn default() -> Self {
        SlabLastUsed { kind: SlabKind::Floor, thickness_mm: None, elevation_mm: 0.0 }
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

fn kind_param(cmd: &str, p: &Value) -> Result<Option<SlabKind>> {
    match str_param(p, "kind") {
        Some(t) => SlabKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "`kind` must be floor, roof or foundation")),
        None => Ok(None),
    }
}

/// A new slab over `outline`, with the hatch scale of the drawing's patterns.
fn new_slab(s: &Session, last: &SlabLastUsed, outline: Vec<Vec2>) -> Result<SlabParams> {
    let d = s.doc()?;
    let n = d.model.iter().filter(|e| slab_of(e).is_some()).count() + 1;
    let units = HvacUnits::of(d);
    let mut p = SlabParams::new(&format!("Slab {n}"), last.kind, outline, units);
    p.thickness_mm = last.thickness_mm.unwrap_or_else(|| last.kind.default_thickness_mm());
    p.elevation_mm = last.elevation_mm;
    let w = WallUnits::of(d);
    p.hatch_scale = (if w.metric_patterns { 1.0 } else { 25.4 }) / w.mm_per_unit;
    Ok(p)
}

fn add_slab(s: &mut Session, p: SlabParams) -> Result<(Handle, f64, f64)> {
    let (area, volume) = (p.area_m2(), p.volume_m3());
    let space = s.space();
    let common = s.current_common();
    let h = s.doc_mut()?.add_slab(&space, common, p).map_err(doc_err)?;
    Ok((h, area, volume))
}

fn run_slabadd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "slabadd";
    let mut outline = match (super::curves::handle_param(p, "handle"), p.get("points")) {
        (Some(h), _) => super::hvac::polyline_outline(s.doc()?, h)?,
        (None, Some(Value::Array(raw))) => {
            if raw.len() > MAX_SLAB_VERTICES + 1 {
                return Err(bad(CMD, format!("a slab has at most {MAX_SLAB_VERTICES} points")));
            }
            let pts: Option<Vec<Vec2>> = raw.iter().map(super::point_value).collect();
            super::distinct_points(&pts.ok_or_else(|| bad(CMD, "`points` must be [x, y] points"))?)
        }
        _ => return Err(bad(CMD, "`points` (3 or more) or `handle` (a closed polyline) is required")),
    };
    if outline.len() > 3 && outline.first().zip(outline.last()).is_some_and(|(a, b)| a.near(*b, 1e-12)) {
        outline.pop();
    }
    let mut last = SlabLastUsed::default();
    last.kind = kind_param(CMD, p)?.unwrap_or(last.kind);
    last.thickness_mm = number(CMD, p, "thickness", 1.0)?;
    last.elevation_mm = p.get("elevation").and_then(Value::as_f64).filter(|e| e.is_finite()).map_or(0.0, |e| e * 1000.0);
    let mut slab = new_slab(s, &last, outline)?;
    if let Some(n) = str_param(p, "name") {
        slab.name = n.to_string();
    }
    if let Some(pat) = p.get("pattern").and_then(Value::as_str) {
        slab.pattern = pat.to_string();
    }
    slab.label_height_mm = number(CMD, p, "labelHeight", 1.0)?.unwrap_or(slab.label_height_mm);
    let (h, area, volume) = add_slab(s, slab)?;
    Ok(json!({ "handle": h.hex(), "area": area, "volume": volume }))
}

fn run_slabedit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "slabedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} slabs")));
    }
    let kind = kind_param(CMD, p)?;
    let thickness = number(CMD, p, "thickness", 1.0)?;
    let elevation = p.get("elevation").and_then(Value::as_f64).filter(|e| e.is_finite()).map(|e| e * 1000.0);
    let d = s.doc()?;
    let mut next = Vec::with_capacity(hs.len());
    for h in &hs {
        let mut sp = d.entity(*h).and_then(|e| slab_of(e)).ok_or_else(|| other("Select slabs only."))?;
        sp.kind = kind.unwrap_or(sp.kind);
        sp.thickness_mm = thickness.unwrap_or(sp.thickness_mm);
        sp.elevation_mm = elevation.unwrap_or(sp.elevation_mm);
        if let Some(n) = str_param(p, "name") {
            sp.name = n.to_string();
        }
        if let Some(pat) = p.get("pattern").and_then(Value::as_str) {
            sp.pattern = pat.to_string();
        }
        sp.validate().map_err(doc_err)?;
        next.push((*h, sp));
    }
    let d = s.doc_mut()?;
    for (h, sp) in &next {
        d.regenerate_slab(*h, sp).map_err(doc_err)?;
    }
    Ok(json!({ "changed": next.len() }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Point,
    Object,
    Kind,
    Thickness,
    Elevation,
}

struct SlabAddM {
    pts: Vec<Vec2>,
    last: SlabLastUsed,
    ask: Ask,
}

impl SlabAddM {
    fn new(s: &Session) -> Self {
        SlabAddM { pts: Vec::new(), last: s.last_used.slab.clone(), ask: Ask::Point }
    }
    fn settings(&self, s: &mut Session) {
        let t = self.last.thickness_mm.unwrap_or_else(|| self.last.kind.default_thickness_mm());
        s.echo(format!(
            "Current settings: Kind = {}, Thickness = {t:.0} mm, Elevation = {:.2} m",
            self.last.kind.name(),
            self.last.elevation_mm / 1000.0
        ));
    }
    fn finish(&mut self, s: &mut Session, outline: Vec<Vec2>) -> Result<Step> {
        let p = new_slab(s, &self.last, outline)?;
        if let Err(e) = p.validate() {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        s.last_used.slab = self.last.clone();
        let (_, area, volume) = add_slab(s, p)?;
        s.echo(format!("Slab: {area:.1} m², {volume:.2} m³"));
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
                match super::hvac::polyline_outline(s.doc()?, h) {
                    Ok(outline) => return self.finish(s, outline),
                    Err(e) => {
                        s.echo(e.to_string());
                        return Ok(Step::Continue);
                    }
                }
            }
            (Ask::Kind, _, Some(t)) => {
                self.last.kind = SlabKind::parse(t).ok_or_else(|| other("Choose Floor, Roof or Foundation."))?;
                self.last.thickness_mm = None;
            }
            (Ask::Thickness, _, Some(t)) => {
                self.last.thickness_mm =
                    Some(t.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 10.0).ok_or_else(|| other("Requires a thickness in mm."))?);
            }
            (Ask::Elevation, _, Some(t)) => {
                self.last.elevation_mm =
                    t.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| other("Requires an elevation in metres."))? * 1000.0;
            }
            (Ask::Object, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = Ask::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for SlabAddM {
    fn name(&self) -> &'static str {
        "SLABADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Object => return Prompt::new("Select a closed polyline", Accept::SELECT),
            Ask::Kind => {
                return Prompt::new("Enter slab kind", super::curves::KW).kw(&["Floor", "Roof", "Foundation"]).default(self.last.kind.name());
            }
            Ask::Thickness => {
                let t = self.last.thickness_mm.unwrap_or_else(|| self.last.kind.default_thickness_mm());
                return Prompt::new("Enter thickness (mm)", Accept::NUMBER).default(format!("{t:.0}"));
            }
            Ask::Elevation => {
                return Prompt::new("Enter elevation of the top (m)", Accept::NUMBER).default(format!("{:.2}", self.last.elevation_mm / 1000.0));
            }
            Ask::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify first point of slab", Accept::POINT).kw(&["OBject", "Kind", "Thickness", "Elevation"]),
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
                    "Kind" => self.ask = Ask::Kind,
                    "Thickness" => self.ask = Ask::Thickness,
                    "Elevation" => self.ask = Ask::Elevation,
                    "Undo" => {
                        self.pts.pop();
                    }
                    "Close" if self.pts.len() >= 3 => return self.finish(s, self.pts.clone()),
                    _ => {}
                }
                Ok(Step::Continue)
            }
            Input::Point(p) => {
                if self.pts.len() < MAX_SLAB_VERTICES && !self.pts.last().is_some_and(|l| l.near(p, 1e-9)) {
                    self.pts.push(p);
                }
                Ok(Step::Continue)
            }
            Input::Enter if self.pts.len() >= 3 => self.finish(s, self.pts.clone()),
            Input::Enter | Input::Cancel => Ok(Step::Cancel),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        if self.pts.is_empty() || self.ask != Ask::Point {
            return Vec::new();
        }
        let mut pts = self.pts.clone();
        pts.push(c);
        if let Some(first) = self.pts.first() {
            pts.push(*first);
        }
        pts.windows(2)
            .filter_map(|w| Some(cadcraft_doc::EntityKind::Line(cadcraft_doc::Line { a: w.first()?.to3(0.0), b: w.get(1)?.to3(0.0) })))
            .collect()
    }
}
