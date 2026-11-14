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
        CommandSpec::new("cornerwindow", "Corner Window", run_corner)
            .menu(&["Draw", "Architecture", "Corner Window"])
            .params(
                "{at ([x, y], near the corner two walls make), width? (mm, the length of the window along each wall from the corner, default 1200), \
                 height? | sill? (mm), post? (mm left between the corner and each window, default 0)} → {handles: [the opening on each wall]}",
            )
            .interactive(|s| Ok(Box::new(CornerM::new(s)))),
        CommandSpec::new("openingconvert", "Convert to Openings", run_convert)
            .params(
                "{handles? (the block references to look at, default all of model space), apply? (true: make the openings and set the old blocks aside on the frozen \
                 layer A-OPENING-OLD; default false: only list them), doors? | windows? (words that name a door or a window in a block or layer name, replacing the \
                 defaults door, porte / window, fenetre, vitr, glaz)} → {found: [{handle, type, width, wall, ...}], created?, skipped?}",
            )
            .interactive(|s| Ok(Box::new(ConvertM::new(s)))),
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

/// Whether the opening of width `width_mm` fits in the part of the segment of `hit` between the
/// faces of the walls at its ends.
fn fits(hit: &WallHit, width_mm: f64, mm_per_unit: f64) -> bool {
    let (lo, hi) = hit.room();
    width_mm / mm_per_unit <= hi - lo + 1e-9
}

/// Whether an opening of `width` (drawing units) centred `along` the segment of `hit` is between
/// the faces of the walls at its ends.
fn within(hit: &WallHit, along: f64, width: f64) -> bool {
    let (lo, hi) = hit.room();
    along - width / 2.0 >= lo - 1e-9 && along + width / 2.0 <= hi + 1e-9
}

/// The centre of an opening whose nearest edge is `distance_mm` from the face at the start (or the
/// end) of the segment: the face of the wall the segment meets there, or its end when it meets none.
fn centre_along(hit: &WallHit, width_mm: f64, distance_mm: f64, from_end: bool, mm_per_unit: f64) -> f64 {
    let (w, d) = (width_mm / mm_per_unit, distance_mm / mm_per_unit);
    if from_end { hit.end_face - d - w / 2.0 } else { hit.start_face + d + w / 2.0 }
}

/// Puts the opening on `hit`'s segment with its centre `along` from the segment's start, kept inside it.
fn place(s: &mut Session, mut p: OpeningParams, hit: &WallHit, along: f64) -> Result<Handle> {
    let units = p.units.mm_per_unit;
    if !fits(hit, p.width_mm, units) {
        return Err(other("The opening is wider than the wall segment."));
    }
    let half = p.width() / 2.0;
    let (lo, hi) = hit.room();
    let along = along.clamp(half + lo, (hi - half).max(half + lo));
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
        if !within(&hit, along, params.width()) {
            return Err(other("The opening does not fit there: the distance puts it off the wall."));
        }
        place(s, params, &hit, along)?
    } else {
        return Err(bad(CMD, "`at` ([x, y]) or `wall` + `distance` is required"));
    };
    Ok(json!({ "handle": h.hex(), "type": kind.name() }))
}

/// The layer the blocks of a converted plan are set aside on (frozen).
const OLD_LAYER: &str = cadcraft_doc::OLD_OPENING_LAYER;
/// Most openings one conversion makes.
const MAX_CONVERTED: usize = 2_000;

fn words_param(cmd: &str, p: &Value, key: &str) -> Result<Vec<String>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) if a.len() <= 64 => {
            a.iter().map(|w| w.as_str().map(str::to_string).ok_or_else(|| bad(cmd, format!("`{key}` must be a list of words")))).collect()
        }
        Some(_) => Err(bad(cmd, format!("`{key}` must be a list of at most 64 words"))),
    }
}

fn candidate_value(c: &cadcraft_doc::OpeningCandidate) -> Value {
    json!({
        "handle": c.block_ref.hex(),
        "type": c.kind.name(),
        "width": c.width_mm,
        "at": [c.centre.x, c.centre.y],
        "wall": c.wall.hex(),
    })
}

/// OPENINGCONVERT: the door and window blocks of a plan that lie on walls, as openings.
fn run_convert(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "openingconvert";
    let only = match p.get("handles") {
        None | Some(Value::Null) => None,
        Some(_) => Some(super::arch::handles_param(CMD, p)?),
    };
    let (doors, windows) = (words_param(CMD, p, "doors")?, words_param(CMD, p, "windows")?);
    let found = s.doc()?.recognise_openings(only.as_deref(), &doors, &windows);
    let list: Vec<Value> = found.iter().map(candidate_value).collect();
    if !flag(CMD, p, "apply")?.unwrap_or(false) {
        return Ok(json!({ "found": list }));
    }
    let units = units_of(s.doc()?);
    let (mut created, mut skipped) = (Vec::new(), Vec::new());
    for c in found.iter().take(MAX_CONVERTED) {
        let mut params = OpeningParams::new(c.kind, units);
        params.width_mm = c.width_mm;
        let Some(hit) = s.doc()?.wall_segment(c.wall, c.segment) else {
            skipped.push(c.block_ref.hex());
            continue;
        };
        match place(s, params, &hit, c.along) {
            Ok(h) => {
                created.push(h.hex());
                // The old block is set aside, not erased: on a frozen layer of its own.
                let d = s.doc_mut()?;
                if d.ensure_layer(OLD_LAYER)
                    && let Some(l) = d.layer_mut(OLD_LAYER)
                {
                    l.frozen = true;
                }
                d.modify_entity(c.block_ref, |e| e.common.layer = OLD_LAYER.into()).map_err(doc_err)?;
            }
            Err(_) => skipped.push(c.block_ref.hex()),
        }
    }
    Ok(json!({ "found": list, "created": created, "skipped": skipped }))
}

/// The prompt of OPENINGCONVERT: what was found, then whether to convert it.
struct ConvertM {
    only: Option<Vec<Handle>>,
}

impl ConvertM {
    fn new(s: &Session) -> Self {
        let selected = s.selection();
        ConvertM { only: (!selected.is_empty()).then_some(selected) }
    }
    fn handles_json(&self) -> Value {
        self.only.as_ref().map_or(Value::Null, |h| json!(h.iter().map(|x| x.hex()).collect::<Vec<_>>()))
    }
}

impl Interactive for ConvertM {
    fn name(&self) -> &'static str {
        "OPENINGCONVERT"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        let r = run_convert(s, &json!({ "handles": self.handles_json() }))?;
        let found = r["found"].as_array().map_or(0, Vec::len);
        if found == 0 {
            s.echo("No door or window block found on a wall.");
            return Ok(Step::Done);
        }
        let count = |t: &str| r["found"].as_array().map_or(0, |a| a.iter().filter(|c| c["type"] == t).count());
        s.echo(format!(
            "{found} block(s) found on walls: {} door(s), {} double door(s), {} window(s).",
            count("door"),
            count("doubleDoor"),
            count("window")
        ));
        Ok(Step::Continue)
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Convert them to openings? The old blocks are kept on a frozen layer", super::curves::KW).kw(&["Yes", "No"]).default("Yes")
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => self.input(s, Input::Keyword("Yes".into())),
            Input::Keyword(k) | Input::Text(k) if k.eq_ignore_ascii_case("yes") || k.eq_ignore_ascii_case("y") => {
                let r = run_convert(s, &json!({ "handles": self.handles_json(), "apply": true }))?;
                let n = |k: &str| r[k].as_array().map_or(0, Vec::len);
                s.echo(format!("{} opening(s) made, {} left as they were.", n("created"), n("skipped")));
                Ok(Step::Done)
            }
            _ => Ok(Step::Done),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
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
        if !fits(&seg, o.width_mm, units) || !within(&seg, along, o.width()) {
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
                if !within(&hit, along, params.width()) {
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
        let (lo, hi) = hit.room();
        if hi - lo < 2.0 * half {
            return Vec::new();
        }
        let along = hit.along.clamp(half + lo, hi - half);
        let dir = Vec2::from_angle(hit.angle);
        // The symbol, and the two dimensions from the faces at the ends of the wall to it.
        let flip = dir.x < 0.0;
        let to_start = along - half - hit.start_face;
        let to_end = hit.end_face - along - half;
        p.generate()
            .map(|mut pieces| {
                pieces.extend(p.provisional_dimensions(to_start, to_end, flip));
                preview_entities(pieces, hit.foot - dir * hit.along + dir * along, hit.angle)
            })
            .unwrap_or_default()
    }
}

/// The two wall segments that meet near `at`: for each, the hit at its start and whether the corner is at its end.
fn corner_segments(d: &Drawing, at: Vec2, reach: f64) -> Option<[(Handle, usize, bool); 2]> {
    let mut found: Vec<(f64, Handle, usize, bool, f64)> = Vec::new();
    for e in d.model.iter() {
        if cadcraft_doc::wall_of(e).is_none() {
            continue;
        }
        for k in 0..MAX_SEGMENTS {
            let Some(hit) = d.wall_segment(e.handle, k) else { break };
            let (a, b) = (hit.foot, hit.foot + Vec2::from_angle(hit.angle) * hit.length);
            for (end, v) in [(false, a), (true, b)] {
                let dist = v.dist(at);
                if dist <= reach {
                    found.push((dist, e.handle, k, end, if end { hit.angle + std::f64::consts::PI } else { hit.angle }));
                }
            }
        }
    }
    found.sort_by(|x, y| x.0.total_cmp(&y.0));
    // The nearest, then the nearest that leaves the corner in another direction (not along the same line).
    let first = found.first()?;
    let second = found.iter().skip(1).find(|c| {
        let d = (c.4 - first.4).rem_euclid(std::f64::consts::TAU);
        d.min(std::f64::consts::TAU - d) > 0.2 && (d - std::f64::consts::PI).abs() > 0.05
    })?;
    Some([(first.1, first.2, first.3), (second.1, second.2, second.3)])
}

/// Most segments of one wall looked at for a corner.
const MAX_SEGMENTS: usize = 4096;

/// A window on each of the two walls that meet at a corner, from the corner along each wall.
fn corner_window(s: &mut Session, at: Vec2, width: f64, height: f64, sill: f64, post: f64) -> Result<Vec<Handle>> {
    let units = units_of(s.doc()?);
    let mm = units.mm_per_unit;
    let legs = corner_segments(s.doc()?, at, CLICK_REACH_MM / mm).ok_or_else(|| other("Click near a corner of two walls."))?;
    // Both walls are checked before anything is placed: a corner window is two windows or none.
    for (wall, segment, _) in legs {
        let hit = s.doc()?.wall_segment(wall, segment).ok_or_else(|| other("Cannot find that wall segment."))?;
        if width + post > hit.length * mm {
            return Err(other("The window is longer than the wall."));
        }
    }
    let mut handles = Vec::new();
    for (wall, segment, at_end) in legs {
        // The hit is read again each time: the first window changed the walls.
        let hit = s.doc()?.wall_segment(wall, segment).ok_or_else(|| other("Cannot find that wall segment."))?;
        let mut params = OpeningParams::new(OpeningKind::Window, units);
        (params.width_mm, params.height_mm, params.sill_mm) = (width, height, sill);
        let along = centre_along(&hit, width, post, at_end, mm);
        handles.push(place(s, params, &hit, along)?);
    }
    Ok(handles)
}

fn run_corner(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "cornerwindow";
    let at = super::point_param(p, "at").ok_or_else(|| bad(CMD, "`at` ([x, y]) is required"))?;
    let (dw, dh, ds) = OpeningKind::Window.default_size_mm();
    let width = number(CMD, p, "width")?.unwrap_or(dw);
    let handles = corner_window(
        s,
        at,
        width,
        number(CMD, p, "height")?.unwrap_or(dh),
        number(CMD, p, "sill")?.unwrap_or(ds),
        number(CMD, p, "post")?.unwrap_or(0.0).max(0.0),
    )?;
    Ok(json!({ "handles": handles.iter().map(|h| h.hex()).collect::<Vec<_>>() }))
}

/// CORNERWINDOW at the command line: a click near a corner makes the two windows; Width, Height and Sill are asked first.
struct CornerM {
    width: f64,
    height: f64,
    sill: f64,
    ask: Option<char>,
}

impl CornerM {
    fn new(_s: &Session) -> Self {
        let (width, height, sill) = OpeningKind::Window.default_size_mm();
        CornerM { width, height, sill, ask: None }
    }
}

impl Interactive for CornerM {
    fn name(&self) -> &'static str {
        "CORNERWINDOW"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Some('w') => Prompt::new("Enter the length of the window along each wall (mm)", Accept::NUMBER).default(format!("{}", self.width)),
            Some('h') => Prompt::new("Enter window height (mm)", Accept::NUMBER).default(format!("{}", self.height)),
            Some(_) => Prompt::new("Enter sill height (mm)", Accept::NUMBER).default(format!("{}", self.sill)),
            None => Prompt::new("Click near the corner of two walls", Accept::POINT).kw(&["Width", "Height", "Sill"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        if let Some(which) = self.ask {
            let text = match &i {
                Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
                _ => None,
            };
            if let Some(v) = text.and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0) {
                match which {
                    'w' => self.width = v,
                    'h' => self.height = v,
                    _ => self.sill = v,
                }
            }
            self.ask = None;
            return Ok(Step::Continue);
        }
        match i {
            Input::Keyword(k) => {
                self.ask = match k.as_str() {
                    "Width" => Some('w'),
                    "Height" => Some('h'),
                    "Sill" => Some('s'),
                    _ => None,
                };
                Ok(Step::Continue)
            }
            Input::Point(at) => {
                if let Err(e) = corner_window(s, at, self.width, self.height, self.sill, 0.0) {
                    s.echo(e.to_string());
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
}
