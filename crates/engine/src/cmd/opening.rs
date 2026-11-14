//! Openings in walls: OPENINGADD places a door, double door, window or bay on a wall, by a click or
//! by a distance typed from the end of the wall; OPENINGEDIT changes one (its size, its swing, or
//! its place along the wall by a distance). The wall under an opening is cut when the walls are
//! joined, which both commands do.

use cadcraft_doc::{Drawing, EntityKind, Handle, OpeningKind, OpeningParams, OpeningStyle, WallHit, WallUnits, opening_of, preview_entities};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("openingadd", "Opening", run_add)
            .menu(&["Draw", "Architecture", "Opening"])
            .params(
                "{at ([x, y], put on the nearest wall) | wall (handle) + distance (mm from `from` to the nearest edge of the opening) + from?: start|end \\
                 + segment? (default 0), type?: door|doubleDoor|window|bay, style?: standard|sliding|pocket|folding|tilting|centreParting|revolving|casement|doubleCasement|fixed|skylight, width? | height? | sill? (mm), flipSide?, flipHand?} → {handle, type}",
            )
            .interactive(|s| Ok(Box::new(OpeningAddM::new(s)))),
        CommandSpec::new("openingedit", "Edit Opening", run_edit).params(
            "{handle | handles, type?, style?, width? | height? | sill? (mm), flipSide?, flipHand?, distance? (mm from `from` to the nearest edge), \
             from?: start|end} → {changed}",
        ),
    ]
}

/// What OPENINGADD remembers between uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpeningLastUsed {
    pub kind: OpeningKind,
    pub style: OpeningStyle,
    /// Width, height and sill the user gave, mm; the kind's own when none.
    pub size: Option<(f64, f64, f64)>,
    pub flip_side: bool,
    pub flip_hand: bool,
}

impl OpeningLastUsed {
    fn size_mm(&self) -> (f64, f64, f64) {
        self.size.unwrap_or_else(|| self.kind.default_size_mm())
    }
}

/// Most openings one OPENINGEDIT changes.
const MAX_EDITED: usize = 10_000;
/// How far from a wall a click still means that wall, mm.
const CLICK_REACH_MM: f64 = 500.0;

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

fn flag(cmd: &str, p: &Value, key: &str) -> Result<Option<bool>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(cmd, format!("`{key}` must be true or false"))),
    }
}

fn kind_param(cmd: &str, p: &Value) -> Result<Option<OpeningKind>> {
    match str_param(p, "type") {
        Some(t) => OpeningKind::parse(t).map(Some).ok_or_else(|| bad(cmd, "`type` must be door, doubleDoor, window or bay")),
        None => Ok(None),
    }
}

fn style_param(cmd: &str, p: &Value) -> Result<Option<OpeningStyle>> {
    match str_param(p, "style") {
        Some(t) => OpeningStyle::parse(t).map(Some).ok_or_else(|| {
            bad(
                cmd,
                "`style` must be standard, sliding, pocket, folding, tilting, centreParting, revolving, casement, doubleCasement, fixed or skylight",
            )
        }),
        None => Ok(None),
    }
}

fn units_of(d: &Drawing) -> WallUnits {
    WallUnits::of(d)
}

/// Whether the opening of width `width_mm` fits in the segment of `hit`.
fn fits(hit: &WallHit, width_mm: f64, mm_per_unit: f64) -> bool {
    width_mm / mm_per_unit <= hit.length + 1e-9
}

/// The centre of an opening whose nearest edge is `distance_mm` from the start (or the end) of the segment.
fn centre_along(hit: &WallHit, width_mm: f64, distance_mm: f64, from_end: bool, mm_per_unit: f64) -> f64 {
    let (w, d) = (width_mm / mm_per_unit, distance_mm / mm_per_unit);
    if from_end { hit.length - d - w / 2.0 } else { d + w / 2.0 }
}

/// Puts the opening on `hit`'s segment with its centre `along` from the segment's start, kept inside it.
fn place(s: &mut Session, mut p: OpeningParams, hit: &WallHit, along: f64) -> Result<Handle> {
    let units = p.units.mm_per_unit;
    if !fits(hit, p.width_mm, units) {
        return Err(other("The opening is wider than the wall segment."));
    }
    let half = p.width() / 2.0;
    let along = along.clamp(half, (hit.length - half).max(half));
    let d = Vec2::from_angle(hit.angle);
    let centre = hit.foot - d * hit.along + d * along;
    (p.left_mm, p.right_mm) = (hit.left_mm, hit.right_mm);
    p.validate().map_err(doc_err)?;
    let space = s.space();
    let common = s.current_common();
    let h = s.doc_mut()?.add_opening(&space, common, &p, centre, hit.angle).map_err(doc_err)?;
    super::arch::join_walls(s.doc_mut()?, &space);
    Ok(h)
}

fn params_of(last: &OpeningLastUsed, units: WallUnits) -> OpeningParams {
    let (width_mm, height_mm, sill_mm) = last.size_mm();
    OpeningParams {
        style: last.style,
        width_mm,
        height_mm,
        sill_mm,
        flip_side: last.flip_side,
        flip_hand: last.flip_hand,
        ..OpeningParams::new(last.kind, units)
    }
}

fn run_add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "openingadd";
    let kind = kind_param(CMD, p)?.unwrap_or_default();
    let (dw, dh, ds) = kind.default_size_mm();
    let mut params = OpeningParams::new(kind, units_of(s.doc()?));
    params.style = style_param(CMD, p)?.unwrap_or_default();
    params.width_mm = number(CMD, p, "width")?.unwrap_or(dw);
    params.height_mm = number(CMD, p, "height")?.unwrap_or(dh);
    params.sill_mm = number(CMD, p, "sill")?.unwrap_or(ds);
    params.flip_side = flag(CMD, p, "flipSide")?.unwrap_or(false);
    params.flip_hand = flag(CMD, p, "flipHand")?.unwrap_or(false);
    let units = params.units.mm_per_unit;
    let h = if let Some(at) = super::point_param(p, "at") {
        let hit = s.doc()?.wall_under(at, CLICK_REACH_MM / units).ok_or_else(|| other("Place the opening on a wall."))?;
        let along = hit.along;
        place(s, params, &hit, along)?
    } else if let (Some(wall), Some(distance)) = (super::curves::handle_param(p, "wall"), number(CMD, p, "distance")?) {
        let segment = number(CMD, p, "segment")?.map_or(0, |v| v.max(0.0) as usize);
        let from_end = match str_param(p, "from") {
            None | Some("start") => false,
            Some("end") => true,
            Some(_) => return Err(bad(CMD, "`from` must be start or end")),
        };
        let hit = s.doc()?.wall_segment(wall, segment).ok_or_else(|| other("Cannot find that wall segment."))?;
        let along = centre_along(&hit, params.width_mm, distance, from_end, units);
        // A distance that puts the opening off the wall is refused, not moved.
        if along - params.width() / 2.0 < -1e-9 || along + params.width() / 2.0 > hit.length + 1e-9 {
            return Err(other("The opening does not fit there: the distance puts it off the wall."));
        }
        place(s, params, &hit, along)?
    } else {
        return Err(bad(CMD, "`at` ([x, y]) or `wall` + `distance` is required"));
    };
    Ok(json!({ "handle": h.hex(), "type": kind.name() }))
}

fn run_edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "openingedit";
    let hs = super::arch::handles_param(CMD, p)?;
    if hs.is_empty() || hs.len() > MAX_EDITED {
        return Err(bad(CMD, format!("select 1 to {MAX_EDITED} openings")));
    }
    let kind = kind_param(CMD, p)?;
    let style = style_param(CMD, p)?;
    let (width, height, sill) = (number(CMD, p, "width")?, number(CMD, p, "height")?, number(CMD, p, "sill")?);
    let (flip_side, flip_hand) = (flag(CMD, p, "flipSide")?, flag(CMD, p, "flipHand")?);
    let distance = number(CMD, p, "distance")?;
    let from_end = match str_param(p, "from") {
        None | Some("start") => false,
        Some("end") => true,
        Some(_) => return Err(bad(CMD, "`from` must be start or end")),
    };
    let d = s.doc()?;
    let mut next: Vec<(Handle, OpeningParams, Option<Vec2>)> = Vec::with_capacity(hs.len());
    for h in &hs {
        let (mut o, hit) = match d.opening_host(*h) {
            Some(x) => x,
            None => {
                // An opening off every wall can still change its own parameters, not its place.
                let o = d.entity(*h).and_then(|e| opening_of(e)).ok_or_else(|| other("Select openings only."))?;
                if distance.is_some() {
                    return Err(other("The opening is not on a wall: it can't be placed by a distance."));
                }
                o.validate().map_err(doc_err)?;
                let mut o = apply(o, kind, width, height, sill, flip_side, flip_hand)?;
                o.style = style.unwrap_or(o.style);
                next.push((*h, o, None));
                continue;
            }
        };
        o = apply(o, kind, width, height, sill, flip_side, flip_hand)?;
        o.style = style.unwrap_or(o.style);
        let units = o.units.mm_per_unit;
        let seg = d.wall_segment(hit.wall, hit.segment).ok_or_else(|| other("Cannot find the wall of the opening."))?;
        let along = match distance {
            Some(dist) => centre_along(&seg, o.width_mm, dist, from_end, units),
            None => hit.along,
        };
        if !fits(&seg, o.width_mm, units) || along - o.width() / 2.0 < -1e-9 || along + o.width() / 2.0 > seg.length + 1e-9 {
            return Err(other("The opening does not fit there: it would go off the wall."));
        }
        let dir = Vec2::from_angle(seg.angle);
        next.push((*h, o, Some(seg.foot + dir * along)));
    }
    let space = s.space();
    let d = s.doc_mut()?;
    for (h, o, centre) in &next {
        d.regenerate_opening(*h, o).map_err(doc_err)?;
        if let Some(c) = centre {
            d.modify_entity(*h, |e| {
                if let EntityKind::Insert(i) = &mut e.kind {
                    i.insert.x = c.x;
                    i.insert.y = c.y;
                }
            })
            .map_err(doc_err)?;
        }
    }
    super::arch::join_walls(d, &space);
    Ok(json!({ "changed": next.len() }))
}

fn apply(
    mut o: OpeningParams,
    kind: Option<OpeningKind>,
    width: Option<f64>,
    height: Option<f64>,
    sill: Option<f64>,
    flip_side: Option<bool>,
    flip_hand: Option<bool>,
) -> Result<OpeningParams> {
    o.kind = kind.unwrap_or(o.kind);
    o.width_mm = width.unwrap_or(o.width_mm);
    o.height_mm = height.unwrap_or(o.height_mm);
    o.sill_mm = sill.unwrap_or(o.sill_mm);
    o.flip_side = flip_side.unwrap_or(o.flip_side);
    o.flip_hand = flip_hand.unwrap_or(o.flip_hand);
    o.validate().map_err(doc_err)?;
    Ok(o)
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {
    Location,
    Type,
    Width,
    Height,
    Sill,
    /// A point on a wall near the end to measure from.
    DistanceWall,
    /// The distance from that end, for this wall segment.
    Distance(Handle, usize, bool),
}

struct OpeningAddM {
    last: OpeningLastUsed,
    ask: Ask,
}

impl OpeningAddM {
    fn new(s: &Session) -> Self {
        OpeningAddM { last: s.last_used.opening.clone(), ask: Ask::Location }
    }
    fn settings(&self, s: &mut Session) {
        let (w, h, sill) = self.last.size_mm();
        s.echo(format!(
            "Current settings: Type = {}, Style = {}, Width = {w}, Height = {h}, Sill = {sill}, Swing = {}, Hinge = {}",
            self.last.kind.name(),
            self.last.style.name(),
            if self.last.flip_side { "right" } else { "left" },
            if self.last.flip_hand { "end" } else { "start" }
        ));
    }
    fn size(t: &str) -> Result<f64> {
        t.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= cadcraft_doc::MAX_OPENING_MM)
            .ok_or_else(|| other("Requires a size in millimetres."))
    }
    fn option(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, &i, text.as_deref()) {
            (Ask::Type, _, Some(t)) => {
                let kind = OpeningKind::parse(t).ok_or_else(|| other("Enter Door, DoubleDoor, Window or Bay."))?;
                self.last = OpeningLastUsed { kind, style: OpeningStyle::Standard, size: None, ..self.last.clone() };
            }
            (Ask::Width, _, Some(t)) => {
                let (_, h, sill) = self.last.size_mm();
                self.last.size = Some((Self::size(t)?, h, sill));
            }
            (Ask::Height, _, Some(t)) => {
                let (w, _, sill) = self.last.size_mm();
                self.last.size = Some((w, Self::size(t)?, sill));
            }
            (Ask::Sill, _, Some(t)) => {
                let (w, h, _) = self.last.size_mm();
                let sill = t
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .ok_or_else(|| other("Requires a height in millimetres, 0 or more."))?;
                self.last.size = Some((w, h, sill));
            }
            (Ask::DistanceWall, Input::Point(p), _) => {
                let units = units_of(s.doc()?);
                let Some(hit) = s.doc()?.wall_under(*p, CLICK_REACH_MM / units.mm_per_unit) else {
                    s.echo("Place the point on a wall.");
                    return Ok(Step::Continue);
                };
                // Measured from the end of the wall segment the point is nearer to.
                self.ask = Ask::Distance(hit.wall, hit.segment, hit.along > hit.length / 2.0);
                return Ok(Step::Continue);
            }
            (Ask::Distance(wall, segment, from_end), _, Some(t)) => {
                let dist =
                    t.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| other("Requires a distance in millimetres."))?;
                let units = units_of(s.doc()?);
                let params = params_of(&self.last, units);
                let hit = s.doc()?.wall_segment(wall, segment).ok_or_else(|| other("Cannot find that wall."))?;
                let along = centre_along(&hit, params.width_mm, dist, from_end, units.mm_per_unit);
                if along - params.width() / 2.0 < -1e-9 || along + params.width() / 2.0 > hit.length + 1e-9 {
                    s.echo("The opening does not fit there: the distance puts it off the wall.");
                    return Ok(Step::Continue);
                }
                match place(s, params, &hit, along) {
                    Ok(_) => s.last_used.opening = self.last.clone(),
                    Err(e) => s.echo(e.to_string()),
                }
            }
            (_, Input::Enter | Input::Cancel, _) => {}
            _ => return Ok(Step::Continue),
        }
        self.ask = Ask::Location;
        self.settings(s);
        Ok(Step::Continue)
    }
}

impl Interactive for OpeningAddM {
    fn name(&self) -> &'static str {
        "OPENINGADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Location => Prompt::new("Specify the place of the opening on a wall", Accept::POINT)
                .kw(&["Type", "Width", "Height", "Sill", "Flip", "Hinge", "Distance"]),
            Ask::Type => {
                Prompt::new("Enter opening type", super::curves::KW).kw(&["Door", "DoubleDoor", "Window", "Bay"]).default(self.last.kind.name())
            }
            Ask::Width => Prompt::new("Enter opening width (mm)", Accept::NUMBER).default(format!("{}", self.last.size_mm().0)),
            Ask::Height => Prompt::new("Enter opening height (mm)", Accept::NUMBER).default(format!("{}", self.last.size_mm().1)),
            Ask::Sill => Prompt::new("Enter sill height (mm)", Accept::NUMBER).default(format!("{}", self.last.size_mm().2)),
            Ask::DistanceWall => Prompt::new("Pick the wall near the end to measure from", Accept::POINT),
            Ask::Distance(..) => Prompt::new("Enter the distance from that end to the edge of the opening (mm)", Accept::NUMBER),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if self.ask != Ask::Location {
            return self.option(s, i);
        }
        match i {
            Input::Keyword(k) => {
                match k.as_str() {
                    "Type" => self.ask = Ask::Type,
                    "Width" => self.ask = Ask::Width,
                    "Height" => self.ask = Ask::Height,
                    "Sill" => self.ask = Ask::Sill,
                    "Distance" => self.ask = Ask::DistanceWall,
                    "Flip" => {
                        self.last.flip_side = !self.last.flip_side;
                        self.settings(s);
                    }
                    "Hinge" => {
                        self.last.flip_hand = !self.last.flip_hand;
                        self.settings(s);
                    }
                    _ => {}
                }
                Ok(Step::Continue)
            }
            Input::Point(at) => {
                let units = units_of(s.doc()?);
                match s.doc()?.wall_under(at, CLICK_REACH_MM / units.mm_per_unit) {
                    Some(hit) => match place(s, params_of(&self.last, units), &hit, hit.along) {
                        Ok(_) => s.last_used.opening = self.last.clone(),
                        Err(e) => s.echo(e.to_string()),
                    },
                    None => s.echo("Place the opening on a wall."),
                }
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
        // The opening where it would go, on the wall under the cursor, across that wall.
        let (Ask::Location, Ok(d)) = (self.ask, s.doc()) else { return Vec::new() };
        let units = units_of(d);
        let Some(hit) = d.wall_under(c, CLICK_REACH_MM / units.mm_per_unit) else { return Vec::new() };
        let mut p = params_of(&self.last, units);
        (p.left_mm, p.right_mm) = (hit.left_mm, hit.right_mm);
        let half = p.width() / 2.0;
        if hit.length < 2.0 * half {
            return Vec::new();
        }
        let along = hit.along.clamp(half, hit.length - half);
        let dir = Vec2::from_angle(hit.angle);
        p.generate().map(|pieces| preview_entities(pieces, hit.foot - dir * hit.along + dir * along, hit.angle)).unwrap_or_default()
    }
}
