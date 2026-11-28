//! Architectural walls (AutoCAD Architecture's names): WALLADD draws a wall of a wall style
//! along picked points, WALLSTYLEEDIT lists, creates, changes, renames and deletes the
//! drawing's wall styles and regenerates the walls that use a changed style, WALLEDIT gives
//! walls already drawn another style, justification or height, WALLCLEANUP joins the walls
//! where they meet (the other wall commands do it after their change).

use cadcraft_doc::{
    Common, Drawing, Entity, EntityKind, Handle, Line, MAX_WALL_VERTICES, WallComponent, WallJustification, WallParams, WallStyle, WallUnits, wall_of,
};
use cadcraft_geom::Vec2;
use serde_json::{Value, json};

use super::{CommandSpec, bad, bool_or, str_param};

use crate::{Accept, EngineError, Input, Interactive, Prompt, Result, Session, Step};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("walladd", "Wall", run_walladd)
            .params("{points, bulges? (one per segment, as in a polyline: 0 straight, the tangent of a quarter of the arc's angle, at most 1 in absolute value, positive turning left) | centres? (one per segment: [x, y], the centre of its arc, the arc turning the short way round, or null for a straight one), closed?, style?, justification?: left|center|right, height? (mm)} → {handle}")
            .interactive(|s| Ok(Box::new(WallAddM::new(s)))),
        CommandSpec::new("wallstyleedit", "Wall Styles", run_wallstyleedit).params(
            "{} → {styles, current} | {name, description?, components?: [{name, thickness (mm), pattern?, patternScale?, \
             patternAngle?, color?, priority?, function?: structure|insulation|finish|membrane|airGap, returnsAtOpenings?}], \
             defaultHeight? (mm), copyFrom?} → {style, regenerated, kept} | {delete: name} | {name, rename: newName}",
        ),
        CommandSpec::new("wallelevation", "Wall Elevation", run_wallelevation)
            .params(
                "{handle | handles, at? ([x, y]: the lower left corner of the first, default below the drawing), side?: right|left (the side the wall is \
                 seen from, default right: the start of the wall on the left)} → {created: [handles], width}: draws the face of each wall straight on, with \
                 its openings at their sills and heights and a chain of dimensions below",
            )
            .interactive(|s| Ok(Box::new(WallElevationM::new(s)))),
        CommandSpec::new("wallcleanup", "Wall Junctions", run_wallcleanup)
            .params("{} → {regenerated}: joins the walls where they meet (L, T, X), after walls were moved, copied or erased"),
        CommandSpec::new("walledit", "Edit Wall", run_walledit)
            .params(
                "{handle | handles, style?, justification?: left|center|right, height? (mm), layers? ({layer name: thickness in mm}, or [{name | index, thickness}]: these walls only), \
                 applyToStyle? (true: the style and every wall that uses it take the thicknesses too), \
                 freeStart? | freeEnd? (true: that end of an open wall is left as drawn, not joined to the walls it meets), \
                 flip? (true: the walls are turned round, their layers in the opposite order on the same footprint: the face that was outside is inside), \
                 force? (true: walls edited by hand or scaled are regenerated from their data too; else they are left as they are and counted in `kept`), \
                 bulges? (one per segment, as WALLADD) | segment? + bulge? (one segment's, the others kept; 0 makes it straight)} → {regenerated}",
            )
            .interactive(|s| Ok(Box::new(WallEditM::new(s)))),
    ]
}

fn other(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// The style new walls use: the last one used this session, else the drawing's first, else the
/// first standard style.
fn current_style(s: &Session) -> Result<WallStyle> {
    let d = s.doc()?;
    let last = &s.last_used.wall_style;
    let found = (!last.is_empty()).then(|| d.wall_style_or_standard(last)).flatten();
    found.or_else(|| d.available_wall_styles().into_iter().next()).ok_or_else(|| other("No wall style is available."))
}

fn style_named(d: &Drawing, name: &str) -> Result<WallStyle> {
    d.wall_style_or_standard(name).ok_or_else(|| other(format!("Cannot find wall style \"{}\".", name.trim())))
}

/// The parameters of a wall along `points` (drawing coordinates).
fn params(d: &Drawing, style: WallStyle, points: Vec<Vec2>, closed: bool, justification: WallJustification, height: f64) -> WallParams {
    WallParams {
        style,
        path: points,
        closed,
        justification,
        height,
        units: WallUnits::of(d),
        joints: Default::default(),
        start_free: false,
        end_free: false,
        bulges: Vec::new(),
    }
}

/// Adds wall `p` (drawing coordinates) to the current space; a standard style it uses is then
/// copied into the drawing, so the drawing keeps it and it can be changed. A wall that can't be
/// added changes nothing.
fn add(s: &mut Session, p: WallParams) -> Result<Handle> {
    let space = s.space();
    let common = s.current_common();
    let style = p.style.clone();
    let d = s.doc_mut()?;
    let h = d.add_wall(&space, common, p).map_err(|e| other(e.to_string()))?;
    if d.wall_style(&style.name).is_none() {
        d.wall_styles.push(style);
    }
    join_walls(d, &space);
    Ok(h)
}

/// The post-command pass for walls and openings: when a command added, changed (moved, rotated,
/// stretched, copied) or erased a wall or an opening, the walls are joined again, so a wall moved
/// against another meets it, and an opening moved, copied or erased cuts its wall (or stops cutting
/// it), without WALLCLEANUP. Cheap when the command changed nothing or touched no wall.
pub(crate) fn after_command(s: &mut Session, before: Option<&std::sync::Arc<Drawing>>) {
    let space = s.space();
    let Ok(st) = s.state_mut() else { return };
    if before.is_none_or(|b| std::sync::Arc::ptr_eq(b, &st.doc)) {
        return;
    }
    let Some(b) = before else { return };
    if st.doc.walls_touched_since(b, &space) {
        join_walls(std::sync::Arc::make_mut(&mut st.doc), &space);
    }
    // The elbows of the duct runs are drawn where two runs meet and turn, as soon as they are laid (sizing draws the same ones later).
    if st.doc.ducts_touched_since(b, &space) {
        // A run laid by an edge whose width changed keeps that edge where it was laid, before its elbows are drawn again.
        let _ = std::sync::Arc::make_mut(&mut st.doc).keep_duct_edges_since(b, &space);
        // The height zones first (the ducts under a beam are cut, made flat and given the altitude of the zone), then the bends, cones and steps that follow.
        let _ = std::sync::Arc::make_mut(&mut st.doc).draw_height_zones(&space);
        let _ = std::sync::Arc::make_mut(&mut st.doc).draw_duct_elbows(&space);
        // The hoods covering the end of a duct follow its size and its air.
        let _ = std::sync::Arc::make_mut(&mut st.doc).refresh_hoods();
    }
    // The same for the pipes: elbows where two pipes of one service turn, cones where their sizes differ.
    if st.doc.pipes_touched_since(b, &space) {
        let _ = std::sync::Arc::make_mut(&mut st.doc).draw_pipe_elbows(&space);
    }
    // The principle diagrams of the controls follow the control devices.
    if st.doc.controls_touched_since(b, &space) && st.doc.model.iter().any(|e| cadcraft_doc::control_schema_of(e).is_some()) {
        std::sync::Arc::make_mut(&mut st.doc).refresh_control_schemas();
    }
    // Every label of an object added, changed or turned reads left to right or from the bottom up, never upside down.
    // The labels moved by their grip are moved again on the objects drawn again.
    if st.doc.labels_to_move_since(b, &space) {
        std::sync::Arc::make_mut(&mut st.doc).keep_moved_labels_since(b, &space);
    }
    let upside_down = st.doc.upside_down_labels_since(b, &space);
    if !upside_down.is_empty() {
        let d = std::sync::Arc::make_mut(&mut st.doc);
        for h in upside_down {
            d.upright_labels(h);
        }
    }
}

/// Joins the walls of `space` where they meet; a drawing with too many walls to join at once
/// keeps them as drawn.
pub(super) fn join_walls(d: &mut Drawing, space: &cadcraft_doc::Space) -> usize {
    d.cleanup_walls(space).unwrap_or(0)
}

fn justification_param(cmd: &str, p: &Value, default: WallJustification) -> Result<WallJustification> {
    match str_param(p, "justification") {
        Some(t) => WallJustification::parse(t).ok_or_else(|| bad(cmd, "justification must be left, center or right")),
        None => Ok(default),
    }
}

fn run_walladd(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("points").and_then(Value::as_array).ok_or_else(|| bad("walladd", "`points` (2 or more) is required"))?;
    if raw.len() > MAX_WALL_VERTICES {
        return Err(bad("walladd", format!("a wall has at most {MAX_WALL_VERTICES} points")));
    }
    let mut points: Vec<Vec2> = super::distinct_points(
        &raw.iter().map(super::point_value).collect::<Option<Vec<_>>>().ok_or_else(|| bad("walladd", "`points` must be [x, y] points"))?,
    );
    let closed = bool_or(p, "closed", false);
    // A closed path may repeat its first point at the end.
    if closed && points.len() > 3 && points.first().zip(points.last()).is_some_and(|(a, b)| a.near(*b, 1e-12)) {
        points.pop();
    }
    let style = match str_param(p, "style") {
        Some(n) => style_named(s.doc()?, n)?,
        None => current_style(s)?,
    };
    let justification = justification_param("walladd", p, WallJustification::Left)?;
    let height = match p.get("height") {
        None | Some(Value::Null) => style.default_height,
        Some(v) => v.as_f64().filter(|h| h.is_finite() && *h > 0.0).ok_or_else(|| bad("walladd", "`height` must be a positive number (mm)"))?,
    };
    let mut wall = params(s.doc()?, style, points, closed, justification, height);
    // The bulge of each segment (a polyline's: 0 straight, the tangent of a quarter of the arc's angle).
    if let Some(v) = p.get("bulges").filter(|v| !v.is_null()) {
        let list = v.as_array().ok_or_else(|| bad("walladd", "`bulges` must be a list of numbers"))?;
        if raw.len() != wall.path.len() {
            return Err(bad("walladd", "`bulges` needs points that are all apart"));
        }
        wall.bulges = list
            .iter()
            .map(|b| b.as_f64().filter(|b| b.is_finite()).ok_or_else(|| bad("walladd", "`bulges` must be a list of numbers")))
            .collect::<Result<_>>()?;
    }
    // Or the centre of each arc: the bulge is worked out from it, the arc turning the short way round.
    if let Some(v) = p.get("centres").filter(|v| !v.is_null()) {
        if p.get("bulges").is_some_and(|b| !b.is_null()) {
            return Err(bad("walladd", "give `bulges` or `centres`, not both"));
        }
        if raw.len() != wall.path.len() {
            return Err(bad("walladd", "`centres` needs points that are all apart"));
        }
        wall.bulges =
            bulges_from_centres(&wall.path, wall.closed, v.as_array().ok_or_else(|| bad("walladd", "`centres` must be a list of [x, y] or null"))?)?;
    }
    Ok(json!({ "handle": add(s, wall)?.hex() }))
}

/// The bulge of each segment of `points` from the centre of its arc (`null` or missing: straight). The centre must be as far from the two
/// ends of its segment, within a thousandth, and not on either.
fn bulges_from_centres(points: &[Vec2], closed: bool, centres: &[Value]) -> Result<Vec<f64>> {
    let n = points.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    if centres.len() > segs {
        return Err(bad("walladd", "`centres` has one entry per segment at most"));
    }
    let mut out = Vec::with_capacity(centres.len());
    for (i, c) in centres.iter().enumerate() {
        if c.is_null() {
            out.push(0.0);
            continue;
        }
        let centre = super::point_value(c).ok_or_else(|| bad("walladd", "`centres` must be a list of [x, y] or null"))?;
        let (a, b) = (points.get(i).copied().unwrap_or(Vec2::ZERO), points.get((i + 1) % n).copied().unwrap_or(Vec2::ZERO));
        let r = a.dist(centre);
        if (b.dist(centre) - r).abs() > 1e-3 * r.max(1e-9) {
            return Err(bad("walladd", format!("the centre of segment {i} is not as far from its two ends")));
        }
        let (bulge, _) =
            cadcraft_doc::bulge_from_centre(a, centre, b).ok_or_else(|| bad("walladd", format!("segment {i} has no arc about that centre")))?;
        out.push(bulge);
    }
    Ok(out)
}

/// What WALLADD is asking for besides points.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Point,
    Style,
    Justification,
    Height,
    /// A point on the arc the next wall is, then its end.
    ArcThrough,
    ArcEnd,
    /// The centre of the arc the next wall is, then its end.
    ArcCentre,
    ArcCentreEnd,
}

struct WallAddM {
    /// The point picked on the arc under way, or its centre.
    arc_mid: Option<Vec2>,
    arc_centre: Option<Vec2>,
    pts: Vec<Vec2>,
    /// The walls added so far, one per segment.
    handles: Vec<Handle>,
    style: WallStyle,
    justification: WallJustification,
    /// Millimetres.
    height: f64,
    ask: Ask,
    /// The walls are drawn turned round: their layers in the opposite order on the same side of the path.
    flip: bool,
}

impl WallAddM {
    fn new(s: &Session) -> Self {
        let style = current_style(s).unwrap_or_else(|_| cadcraft_doc::standard_wall_styles().into_iter().next().unwrap_or_else(fallback_style));
        let height = style.default_height;
        WallAddM {
            arc_mid: None,
            arc_centre: None,
            pts: Vec::new(),
            handles: Vec::new(),
            style,
            justification: s.last_used.wall_justification,
            height,
            ask: Ask::Point,
            flip: false,
        }
    }
    /// The bulge of an arc as the wall has it: the path of a turned wall is walked the other way.
    fn bulge_of(&self, bulge: f64) -> Vec<f64> {
        vec![if self.flip { -bulge } else { bulge }]
    }
    fn settings(&self, s: &mut Session) {
        s.echo(format!("Current settings: Style = {}, Justification = {}, Height = {}", self.style.name, self.justification.name(), self.height));
    }
    fn wall(&self, s: &Session, pts: Vec<Vec2>, closed: bool) -> Option<WallParams> {
        let d = s.doc().ok()?;
        let w = params(d, self.style.clone(), pts, closed, self.justification, self.height);
        Some(if self.flip { w.flipped() } else { w })
    }
    /// Remembers the settings for the next WALLADD.
    fn remember(&self, s: &mut Session) {
        s.last_used.wall_style = self.style.name.clone();
        s.last_used.wall_justification = self.justification;
    }
    /// The reason the wall along `pts` can't be drawn, if any.
    fn problem(&self, s: &Session, pts: Vec<Vec2>, closed: bool) -> Option<String> {
        self.wall(s, pts, closed).and_then(|w| w.validate().err()).map(|e| e.to_string())
    }
    /// Adds the wall from `a` to `b` at once, as AutoCAD Architecture does, so the walls drawn
    /// so far snap and join while the next one is drawn. False when it can't be drawn there.
    /// `corner` is the path around the corners the new wall makes with its neighbours, checked
    /// as one wall so a turn too sharp is refused as before.
    fn segment(&mut self, s: &mut Session, a: Vec2, b: Vec2, corner: Vec<Vec2>) -> Result<bool> {
        if let Some(why) = self.problem(s, corner, false) {
            s.echo(why);
            return Ok(false);
        }
        if let Some(w) = self.wall(s, vec![a, b], false) {
            self.handles.push(add(s, w)?);
            self.remember(s);
        }
        Ok(true)
    }
    /// The arc wall from the last point through the picked one to `end`.
    fn arc_to(&mut self, s: &mut Session, end: Vec2) -> Result<Step> {
        self.ask = Ask::Point;
        let (Some(start), Some(mid)) = (self.pts.last().copied(), self.arc_mid.take()) else { return Ok(Step::Continue) };
        let Some(bulge) = cadcraft_doc::bulge_through(start, mid, end) else {
            s.echo("The three points are on a line, or the arc is more than a half circle.");
            return Ok(Step::Continue);
        };
        self.add_arc(s, start, end, bulge)
    }
    /// The arc wall from the last point about `centre` to the direction of `end`: the short way round, its end brought onto the circle.
    fn arc_about(&mut self, s: &mut Session, end: Vec2) -> Result<Step> {
        self.ask = Ask::Point;
        let (Some(start), Some(centre)) = (self.pts.last().copied(), self.arc_centre.take()) else { return Ok(Step::Continue) };
        let Some((bulge, end)) = cadcraft_doc::bulge_from_centre(start, centre, end) else {
            s.echo("The centre is on the start or on the end of the arc.");
            return Ok(Step::Continue);
        };
        self.add_arc(s, start, end, bulge)
    }
    /// Adds the arc wall from `start` to `end` with `bulge`, and goes on from its end; the command ends when the arc comes back to the first
    /// point.
    fn add_arc(&mut self, s: &mut Session, start: Vec2, end: Vec2, bulge: f64) -> Result<Step> {
        let Some(mut w) = self.wall(s, vec![start, end], false) else { return Ok(Step::Continue) };
        w.bulges = self.bulge_of(bulge);
        if let Err(e) = w.validate() {
            s.echo(e.to_string());
            return Ok(Step::Continue);
        }
        self.handles.push(add(s, w)?);
        self.remember(s);
        let closes = self.pts.len() >= 2 && self.pts.first().is_some_and(|f| f.near(end, 1e-6));
        self.pts.push(end);
        s.last_point = end;
        // An arc that comes back to the first point closes the outline: nothing is left to draw.
        Ok(if closes { Step::Done } else { Step::Continue })
    }
    /// Draws the last wall back to the start point and ends the command.
    fn close(&mut self, s: &mut Session) -> Result<Step> {
        let n = self.pts.len();
        let corner: Vec<Vec2> =
            [n.checked_sub(2), n.checked_sub(1), Some(0), Some(1)].into_iter().flatten().filter_map(|i| self.pts.get(i).copied()).collect();
        if let (Some(a), Some(b)) = (self.pts.last().copied(), self.pts.first().copied())
            && !self.segment(s, a, b, corner)?
        {
            return Ok(Step::Continue);
        }
        Ok(Step::Done)
    }
    fn point(&mut self, s: &mut Session, p: Vec2) -> Result<Step> {
        if self.pts.last().is_some_and(|l| l.near(p, 1e-9)) {
            return Ok(Step::Continue);
        }
        // A click back on the start point (snapped to it) closes the outline.
        if self.pts.len() >= 3 && self.pts.first().is_some_and(|f| f.near(p, 1e-6)) {
            return self.close(s);
        }
        let mut corner: Vec<Vec2> = self.pts.iter().rev().take(2).rev().copied().collect();
        corner.push(p);
        if let Some(last) = self.pts.last().copied()
            && !self.segment(s, last, p, corner)?
        {
            return Ok(Step::Continue);
        }
        self.pts.push(p);
        s.last_point = p;
        Ok(Step::Continue)
    }
    fn keyword(&mut self, s: &mut Session, k: &str) -> Result<Step> {
        match k {
            "STyle" => self.ask = Ask::Style,
            "Justification" => self.ask = Ask::Justification,
            "Height" => self.ask = Ask::Height,
            "Flip" => {
                self.flip = !self.flip;
                s.echo(if self.flip {
                    "Walls are drawn turned round: the layers are in the opposite order."
                } else {
                    "Walls are drawn as the style has them."
                });
            }
            "Arc" if !self.pts.is_empty() => self.ask = Ask::ArcThrough,
            "CEnter" if !self.pts.is_empty() => self.ask = Ask::ArcCentre,
            "Undo" => {
                if self.pts.len() >= 2
                    && let Some(h) = self.handles.pop()
                {
                    let space = s.space();
                    let d = s.doc_mut()?;
                    d.remove_entity(h);
                    join_walls(d, &space);
                }
                self.pts.pop();
            }
            "Close" if self.pts.len() >= 3 => return self.close(s),
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
            (Ask::Style, Some("?")) => {
                let names: Vec<String> = s.doc()?.available_wall_styles().into_iter().map(|st| st.name).collect();
                s.echo(format!("Wall styles: {}", names.join(", ")));
                return Ok(Step::Continue);
            }
            (Ask::Style, Some(t)) if !t.is_empty() => {
                self.style = style_named(s.doc()?, t)?;
                self.height = self.style.default_height;
            }
            (Ask::Justification, Some(t)) => {
                self.justification = WallJustification::parse(t).ok_or_else(|| other("Enter Left, Center or Right."))?;
            }
            (Ask::Height, Some(t)) => {
                // Millimetres whatever the drawing's units: a plain number, not a distance.
                self.height = t
                    .parse::<f64>()
                    .ok()
                    .filter(|h| h.is_finite() && *h > 0.0 && *h <= cadcraft_doc::MAX_WALL_SIZE_MM)
                    .ok_or_else(|| other("Requires a positive height in millimetres."))?;
            }
            _ => {}
        }
        self.ask = Ask::Point;
        self.settings(s);
        Ok(Step::Continue)
    }
}

fn fallback_style() -> WallStyle {
    WallStyle {
        name: "Wall".into(),
        description: String::new(),
        components: vec![WallComponent::new("Wall", 200.0, Default::default())],
        default_height: 2500.0,
        glazed_above_mm: None,
    }
}

impl Interactive for WallAddM {
    fn name(&self) -> &'static str {
        "WALLADD"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        self.settings(s);
        Ok(Step::Continue)
    }
    fn cycle_alignment(&mut self, s: &mut Session) -> Option<String> {
        if self.ask != Ask::Point {
            return None;
        }
        self.justification = match self.justification {
            WallJustification::Left => WallJustification::Center,
            WallJustification::Center => WallJustification::Right,
            WallJustification::Right => WallJustification::Left,
        };
        s.last_used.wall_justification = self.justification;
        Some(format!("Justification = {}", self.justification.name()))
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Style => return Prompt::new("Enter wall style name or [?]", Accept::TEXT).default(self.style.name.clone()),
            Ask::Justification => {
                return Prompt::new("Enter justification", super::curves::KW).kw(&["Left", "Center", "Right"]).default(self.justification.name());
            }
            Ask::Height => return Prompt::new("Enter wall height (mm)", Accept::NUMBER).default(format!("{}", self.height)),
            Ask::ArcThrough => return Prompt::new("Specify a point on the arc or [CEnter]", Accept::POINT).kw(&["CEnter"]),
            Ask::ArcEnd => return Prompt::new("Specify end point of the arc", Accept::POINT).base_opt(self.pts.last().copied()),
            Ask::ArcCentre => return Prompt::new("Specify the centre of the arc", Accept::POINT),
            Ask::ArcCentreEnd => {
                return Prompt::new("Specify end point of the arc (the arc turns the short way round)", Accept::POINT).base_opt(self.arc_centre);
            }
            Ask::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify start point of wall", Accept::POINT).kw(&["STyle", "Justification", "Height", "Flip"]),
            1 | 2 => Prompt::new("Specify next point", Accept::POINT).kw(&["Arc", "CEnter", "Undo", "Flip"]).base_opt(last),
            _ => Prompt::new("Specify next point", Accept::POINT).kw(&["Arc", "CEnter", "Close", "Undo", "Flip"]).base_opt(last),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.ask, &i) {
            (Ask::ArcThrough, Input::Point(m)) => {
                self.arc_mid = Some(*m);
                self.ask = Ask::ArcEnd;
                return Ok(Step::Continue);
            }
            (Ask::ArcEnd, Input::Point(e)) => return self.arc_to(s, *e),
            // The centre of the arc instead of a point on it.
            (Ask::ArcThrough, Input::Keyword(k)) if k == "CEnter" => {
                self.ask = Ask::ArcCentre;
                return Ok(Step::Continue);
            }
            (Ask::ArcCentre, Input::Point(c)) => {
                self.arc_centre = Some(*c);
                self.ask = Ask::ArcCentreEnd;
                return Ok(Step::Continue);
            }
            (Ask::ArcCentreEnd, Input::Point(e)) => return self.arc_about(s, *e),
            (Ask::ArcThrough | Ask::ArcEnd | Ask::ArcCentre | Ask::ArcCentreEnd, _) => {
                // Anything else gives up the arc.
                self.arc_mid = None;
                self.arc_centre = None;
                self.ask = Ask::Point;
                return Ok(Step::Continue);
            }
            _ => {}
        }
        if self.ask != Ask::Point {
            return self.option(s, &i);
        }
        match i {
            Input::Keyword(k) => self.keyword(s, &k),
            Input::Point(p) => self.point(s, p),
            // Enter closes an outline of three points or more, else ends with what is drawn.
            Input::Enter if self.pts.len() >= 3 => self.close(s),
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<EntityKind> {
        // The segment under way, in case the wall itself can't be drawn there.
        self.pts.last().map(|l| vec![EntityKind::Line(Line { a: l.to3(0.0), b: c.to3(0.0) })]).unwrap_or_default()
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<Entity> {
        if !matches!(self.ask, Ask::Point | Ask::ArcEnd | Ask::ArcCentreEnd) || self.pts.is_empty() {
            return Vec::new();
        }
        // The walls drawn so far are in the drawing: only the one under way is previewed.
        let Some(last) = self.pts.last().copied().filter(|l| !l.near(c, 1e-9)) else { return Vec::new() };
        let mut wall = self.wall(s, vec![last, c], false);
        if self.ask == Ask::ArcEnd {
            // The arc through the point picked, to the cursor.
            let Some(bulge) = self.arc_mid.and_then(|m| cadcraft_doc::bulge_through(last, m, c)) else { return Vec::new() };
            if let Some(w) = wall.as_mut() {
                w.bulges = self.bulge_of(bulge);
            }
        } else if self.ask == Ask::ArcCentreEnd {
            // The arc about the centre, to where the cursor points.
            let Some((bulge, end)) = self.arc_centre.and_then(|m| cadcraft_doc::bulge_from_centre(last, m, c)) else { return Vec::new() };
            wall = self.wall(s, vec![last, end], false);
            if let Some(w) = wall.as_mut() {
                w.bulges = self.bulge_of(bulge);
            }
        }
        let Some(Ok(pieces)) = wall.map(|w| w.generate()) else { return Vec::new() };
        pieces
            .into_iter()
            .map(|p| Entity { handle: Handle(0), common: Common { layer: p.layer.into(), color: p.color, ..Common::default() }, kind: p.kind })
            .collect()
    }
}

// =====================================================================================
// WALLSTYLEEDIT
// =====================================================================================

fn style_value(st: &WallStyle, standard: bool) -> Value {
    let mut v = serde_json::to_value(st).unwrap_or(Value::Null);
    if let Some(o) = v.as_object_mut() {
        o.insert("standard".into(), json!(standard));
        o.insert("thickness".into(), json!(st.thickness()));
    }
    v
}

fn styles_value(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let styles: Vec<Value> = d.available_wall_styles().iter().map(|st| style_value(st, d.wall_style(&st.name).is_none())).collect();
    Ok(json!({ "styles": styles, "current": current_style(s).map(|st| st.name).unwrap_or_default() }))
}

/// Every wall of the drawing (model and layouts) whose style is `name`.
fn walls_of_style(d: &Drawing, name: &str) -> Vec<(Handle, WallParams)> {
    let spaces = std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities));
    spaces.flat_map(|st| st.iter()).filter_map(|e| wall_of(e).filter(|w| w.style.name.eq_ignore_ascii_case(name)).map(|w| (e.handle, w))).collect()
}

/// Gives the walls of style `name` the style `new`: regenerated when intact, else left as they
/// are (edited elsewhere, or scaled). Returns how many were regenerated and kept.
fn update_walls(d: &mut Drawing, name: &str, new: &WallStyle) -> Result<(usize, usize)> {
    let (mut done, mut kept) = (0, 0);
    for (h, mut w) in walls_of_style(d, name) {
        if !d.wall_is_intact(h) {
            kept += 1;
            continue;
        }
        w.style = new.clone();
        // Junctions are worked out again once every wall has its new style.
        w.joints = Default::default();
        if w.validate().is_err() {
            kept += 1;
            continue;
        }
        // A wall that can't take the new style keeps its old one rather than stop the rest.
        match d.regenerate_wall(h, &w) {
            Ok(()) => done += 1,
            Err(_) => kept += 1,
        }
    }
    Ok((done, kept))
}

fn components_param(p: &Value) -> Result<Option<Vec<WallComponent>>> {
    match p.get("components") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            if v.as_array().is_some_and(|a| a.len() > cadcraft_doc::MAX_WALL_COMPONENTS) {
                return Err(bad("wallstyleedit", format!("a wall style has 1 to {} components", cadcraft_doc::MAX_WALL_COMPONENTS)));
            }
            serde_json::from_value(v.clone()).map(Some).map_err(|e| bad("wallstyleedit", format!("`components`: {e}")))
        }
    }
}

fn run_wallstyleedit(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(name) = str_param(p, "delete") {
        return delete_style(s, name);
    }
    let Some(name) = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) else { return styles_value(s) };
    if let Some(to) = str_param(p, "rename") {
        return rename_style(s, name, to);
    }
    let d = s.doc()?;
    let changes = ["description", "components", "defaultHeight", "copyFrom"].iter().any(|k| p.get(*k).is_some_and(|v| !v.is_null()));
    if !changes {
        // Only a name: the style as it is, nothing changed.
        let st = style_named(d, name)?;
        return Ok(json!({ "style": style_value(&st, d.wall_style(name).is_none()), "regenerated": 0, "kept": 0 }));
    }
    let mut st = match (d.wall_style_or_standard(name), str_param(p, "copyFrom")) {
        (Some(_), Some(_)) => return Err(bad("wallstyleedit", format!("a wall style named \"{name}\" already exists: `copyFrom` makes a new one"))),
        (Some(st), None) => st,
        (None, Some(from)) => WallStyle { name: name.to_string(), ..style_named(d, from)? },
        (None, None) if p.get("components").is_some() => {
            WallStyle { name: name.to_string(), description: String::new(), components: Vec::new(), default_height: 2500.0, glazed_above_mm: None }
        }
        (None, None) => return Err(other(format!("Cannot find wall style \"{name}\"; give `components` or `copyFrom` to create it."))),
    };
    if let Some(t) = str_param(p, "description") {
        st.description = t.to_string();
    }
    if let Some(c) = components_param(p)? {
        st.components = c;
    }
    if let Some(h) = p.get("defaultHeight").filter(|v| !v.is_null()) {
        st.default_height = h.as_f64().ok_or_else(|| bad("wallstyleedit", "`defaultHeight` must be a number (mm)"))?;
    }
    st.validate().map_err(|e| bad("wallstyleedit", e.to_string()))?;
    let d = s.doc_mut()?;
    let old_name = d.wall_style(name).map(|o| o.name.clone()).unwrap_or_else(|| st.name.clone());
    match d.wall_styles.iter_mut().find(|o| o.name.eq_ignore_ascii_case(name)) {
        Some(o) => *o = st.clone(),
        None => d.wall_styles.push(st.clone()),
    }
    let (regenerated, kept) = update_walls(d, &old_name, &st)?;
    for space in wall_spaces(d) {
        join_walls(d, &space);
    }
    Ok(json!({ "style": style_value(&st, false), "regenerated": regenerated, "kept": kept }))
}

fn delete_style(s: &mut Session, name: &str) -> Result<Value> {
    let d = s.doc_mut()?;
    let n = name.trim();
    if d.wall_style(n).is_none() {
        return Err(other(format!("The drawing has no wall style \"{n}\" (standard styles can't be deleted).")));
    }
    // Walls keep their own copy of the style; only the drawing's entry goes.
    d.wall_styles.retain(|o| !o.name.eq_ignore_ascii_case(n));
    if s.last_used.wall_style.eq_ignore_ascii_case(n) {
        s.last_used.wall_style.clear();
    }
    styles_value(s)
}

fn rename_style(s: &mut Session, name: &str, to: &str) -> Result<Value> {
    let to = to.trim();
    let d = s.doc_mut()?;
    let Some(mut st) = d.wall_style(name).cloned() else { return Err(other(format!("The drawing has no wall style \"{}\".", name.trim()))) };
    if d.wall_style(to).is_some_and(|o| !o.name.eq_ignore_ascii_case(&st.name)) {
        return Err(other(format!("A wall style named \"{to}\" already exists.")));
    }
    let old = std::mem::replace(&mut st.name, to.to_string());
    st.validate().map_err(|e| bad("wallstyleedit", e.to_string()))?;
    if let Some(o) = d.wall_styles.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&old)) {
        *o = st.clone();
    }
    // The walls' copies take the new name: their geometry doesn't change.
    for (h, mut w) in walls_of_style(d, &old) {
        w.style.name = st.name.clone();
        let x = d.entity(h).map(|e| w.replace_xdata(&e.common.xdata)).transpose().map_err(|e| other(e.to_string()))?;
        if let Some(x) = x {
            d.modify_entity(h, |e| e.common.xdata = x).map_err(|e| other(e.to_string()))?;
        }
    }
    if s.last_used.wall_style.eq_ignore_ascii_case(&old) {
        s.last_used.wall_style = st.name.clone();
    }
    styles_value(s)
}

// =====================================================================================
// WALLEDIT
// =====================================================================================

/// What WALLEDIT changes; `None` keeps the wall's own value.
#[derive(Clone, Default)]
struct WallChange {
    style: Option<WallStyle>,
    justification: Option<WallJustification>,
    height: Option<f64>,
    /// New thicknesses by component name, mm: for these walls only, or, with `apply_to_style`, for
    /// the style the walls have (the drawing's, and every wall that uses it).
    layers: Vec<(LayerRef, f64)>,
    apply_to_style: bool,
    /// Whether the start (the end) of these walls is left as drawn, not joined to the walls it meets.
    start_free: Option<bool>,
    end_free: Option<bool>,
    /// New bulges for the walls' segments, as WALLADD takes them (a list of one per segment at most).
    bulges: Option<Vec<f64>>,
    /// Turns the walls round: their layers are in the opposite order on the same footprint.
    flip: bool,
    /// Regenerates the walls that were edited by hand or scaled too, from their data (what was done to them is lost).
    force: bool,
}

/// A layer of a wall style: by name (every layer of that name) or by its place from the exterior face.
#[derive(Clone, Debug)]
enum LayerRef {
    Name(String),
    Index(usize),
}

/// Gives the components of `style` the thicknesses in `layers`.
fn set_thicknesses(style: &mut WallStyle, layers: &[(LayerRef, f64)]) -> Result<()> {
    for (layer, t) in layers {
        let hits: Vec<usize> = match layer {
            LayerRef::Index(i) => (*i < style.components.len()).then_some(*i).into_iter().collect(),
            LayerRef::Name(n) => {
                style.components.iter().enumerate().filter(|(_, c)| c.name.trim().eq_ignore_ascii_case(n.trim())).map(|(i, _)| i).collect()
            }
        };
        if hits.is_empty() {
            let what = match layer {
                LayerRef::Index(i) => format!("no layer number {i}"),
                LayerRef::Name(n) => format!("no layer \"{}\"", n.trim()),
            };
            return Err(other(format!("The wall style \"{}\" has {what}.", style.name)));
        }
        for i in hits {
            if let Some(c) = style.components.get_mut(i) {
                if c.function == cadcraft_doc::ComponentFunction::Membrane {
                    return Err(other(format!("Layer \"{}\" is a membrane: it has no thickness to change.", c.name)));
                }
                c.thickness = *t;
            }
        }
    }
    style.validate().map_err(|e| other(e.to_string()))
}

/// The walls among `handles`, refusing an object that is not a wall. A wall edited by hand or scaled (or drawn by an older version) can't be regenerated
/// without losing what was done to it: it is left as it is and counted, unless `force` asks for it to be regenerated from its data; when none can be, that is an error.
fn editable_walls(d: &Drawing, handles: &[Handle], force: bool) -> Result<(Vec<(Handle, WallParams)>, usize)> {
    if handles.is_empty() || handles.len() > MAX_EDITED {
        return Err(other(format!("Select 1 to {MAX_EDITED} walls.")));
    }
    let mut out = Vec::with_capacity(handles.len());
    let mut kept = 0;
    for &h in handles {
        let w = d.entity(h).and_then(|e| wall_of(e)).ok_or_else(|| other("Select walls only."))?;
        if force || d.wall_is_intact(h) {
            out.push((h, w));
        } else {
            kept += 1;
        }
    }
    if out.is_empty() {
        return Err(other(
            "The selected walls were edited by hand or scaled (or drawn by an older version): they can't be regenerated without losing that. Use Regenerate in Properties (or `force`) to redraw them from their data.",
        ));
    }
    Ok((out, kept))
}

/// Most objects one edit changes.
const MAX_EDITED: usize = 10_000;

/// Gives every wall of `handles` the change; nothing changes when one of them can't take it.
fn edit_walls(s: &mut Session, handles: &[Handle], c: &WallChange) -> Result<(usize, usize)> {
    let (walls, kept) = editable_walls(s.doc()?, handles, c.force)?;
    let mut next = Vec::with_capacity(walls.len());
    for (h, mut w) in walls {
        if let Some(st) = &c.style {
            w.style = st.clone();
        }
        if let Some(j) = c.justification {
            w.justification = j;
        }
        if let Some(height) = c.height {
            w.height = height;
        }
        if !c.layers.is_empty() {
            set_thicknesses(&mut w.style, &c.layers)?;
        }
        if c.start_free.is_some() || c.end_free.is_some() {
            if w.closed {
                return Err(other("A closed wall has no ends to leave free."));
            }
            w.start_free = c.start_free.unwrap_or(w.start_free);
            w.end_free = c.end_free.unwrap_or(w.end_free);
        }
        if let Some(b) = &c.bulges {
            w.bulges = b.clone();
        }
        if c.flip {
            w = w.flipped();
        }
        // Junctions are worked out again after the change.
        w.joints = Default::default();
        w.validate().map_err(|e| other(e.to_string()))?;
        next.push((h, w));
    }
    let d = s.doc_mut()?;
    for (h, w) in &next {
        d.regenerate_wall(*h, w).map_err(|e| other(e.to_string()))?;
        // A wall that was scaled is drawn at its own size again.
        if c.force {
            d.modify_entity(*h, |e| {
                if let EntityKind::Insert(i) = &mut e.kind {
                    (i.scale.x, i.scale.y, i.scale.z, i.cols, i.rows) = (i.scale.x.signum(), i.scale.y.signum(), 1.0, 1, 1);
                }
            })
            .map_err(|e| other(e.to_string()))?;
        }
    }
    if let Some(st) = &c.style
        && d.wall_style(&st.name).is_none()
    {
        d.wall_styles.push(st.clone());
    }
    // The style itself takes the thicknesses too: the drawing's entry (added when the style was a
    // standard one) and every wall that uses it, the first of each name giving the thicknesses.
    if c.apply_to_style && !c.layers.is_empty() {
        let mut done: Vec<String> = Vec::new();
        for (_, w) in &next {
            if done.iter().any(|n| n.eq_ignore_ascii_case(&w.style.name)) {
                continue;
            }
            done.push(w.style.name.clone());
            match d.wall_styles.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&w.style.name)) {
                Some(o) => *o = w.style.clone(),
                None => d.wall_styles.push(w.style.clone()),
            }
            update_walls(d, &w.style.name.clone(), &w.style)?;
        }
    }
    for space in wall_spaces(d) {
        join_walls(d, &space);
    }
    Ok((next.len(), kept))
}

/// The spaces (model, layouts) holding walls.
fn wall_spaces(d: &Drawing) -> Vec<cadcraft_doc::Space> {
    let mut v = Vec::new();
    if d.model.iter().any(|e| wall_of(e).is_some()) {
        v.push(cadcraft_doc::Space::Model);
    }
    for l in &d.layouts {
        if l.entities.iter().any(|e| wall_of(e).is_some()) {
            v.push(cadcraft_doc::Space::Paper(l.name.clone()));
        }
    }
    v
}

/// Most walls one WALLELEVATION draws.
const MAX_ELEVATIONS: usize = 64;

fn side_param(cmd: &str, p: &Value) -> Result<cadcraft_doc::ElevationSide> {
    match str_param(p, "side") {
        Some(t) => cadcraft_doc::ElevationSide::parse(t).ok_or_else(|| bad(cmd, "`side` must be right or left")),
        None => Ok(Default::default()),
    }
}

fn run_wallelevation(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "wallelevation";
    let handles = handles_param(CMD, p)?;
    if handles.is_empty() || handles.len() > MAX_ELEVATIONS {
        return Err(bad(CMD, format!("select 1 to {MAX_ELEVATIONS} walls")));
    }
    let side = side_param(CMD, p)?;
    let space = s.space();
    let units = WallUnits::of(s.doc()?);
    let gap = units.to_units(2000.0);
    let mut at = match super::point_param(p, "at") {
        Some(a) => a,
        None => {
            // Below the drawing, at its left edge.
            let b = s.doc()?.extents(&space);
            if b.min.is_finite() && b.max.is_finite() { Vec2::new(b.min.x, b.min.y - units.to_units(5000.0)) } else { Vec2::ZERO }
        }
    };
    // Every wall is checked before anything is drawn.
    for h in &handles {
        s.doc()?.wall_elevation_pieces(&space, *h, side).map_err(|e| other(e.to_string()))?;
    }
    let mut created = Vec::with_capacity(handles.len());
    let mut width = 0.0;
    for h in &handles {
        let common = s.current_common();
        let (e, w) = s.doc_mut()?.add_wall_elevation(&space, common, *h, at, side).map_err(|e| other(e.to_string()))?;
        created.push(e.hex());
        width = w;
        at.x += w + gap;
    }
    Ok(json!({ "created": created, "width": width }))
}

/// What WALLELEVATION is asking for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ElevAsk {
    Select,
    Place,
}

struct WallElevationM {
    walls: Vec<Handle>,
    side: cadcraft_doc::ElevationSide,
    ask: ElevAsk,
}

impl WallElevationM {
    fn new(s: &Session) -> Self {
        let walls: Vec<Handle> =
            s.doc().map(|d| s.selection().into_iter().filter(|h| d.entity(*h).and_then(|e| wall_of(e)).is_some()).collect()).unwrap_or_default();
        let ask = if walls.is_empty() { ElevAsk::Select } else { ElevAsk::Place };
        WallElevationM { walls, side: Default::default(), ask }
    }
}

impl Interactive for WallElevationM {
    fn name(&self) -> &'static str {
        "WALLELEVATION"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            ElevAsk::Select => Prompt::new("Select walls", Accept::SELECT),
            ElevAsk::Place => Prompt::new("Specify the lower left corner of the elevation", Accept::POINT).kw(&["Side"]),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.ask, i) {
            (ElevAsk::Select, Input::Pick(hs)) => {
                let d = s.doc()?;
                let new: Vec<Handle> =
                    hs.into_iter().filter(|h| d.entity(*h).and_then(|e| wall_of(e)).is_some() && !self.walls.contains(h)).collect();
                self.walls.extend(new);
                Ok(Step::Continue)
            }
            (ElevAsk::Select, Input::Enter) if !self.walls.is_empty() => {
                self.ask = ElevAsk::Place;
                Ok(Step::Continue)
            }
            (ElevAsk::Place, Input::Keyword(k)) if k == "Side" => {
                self.side = match self.side {
                    cadcraft_doc::ElevationSide::Right => cadcraft_doc::ElevationSide::Left,
                    cadcraft_doc::ElevationSide::Left => cadcraft_doc::ElevationSide::Right,
                };
                s.echo(format!("Seen from the {}.", if self.side == cadcraft_doc::ElevationSide::Left { "left" } else { "right" }));
                Ok(Step::Continue)
            }
            (ElevAsk::Place, Input::Point(at)) => {
                let side = if self.side == cadcraft_doc::ElevationSide::Left { "left" } else { "right" };
                let hexes: Vec<String> = self.walls.iter().map(|h| h.hex()).collect();
                match run_wallelevation(s, &json!({ "handles": hexes, "at": [at.x, at.y], "side": side })) {
                    Ok(r) => s.echo(format!("{} elevation(s) drawn.", r["created"].as_array().map_or(0, Vec::len))),
                    Err(e) => s.echo(e.to_string()),
                }
                Ok(Step::Done)
            }
            (_, Input::Enter | Input::Cancel) => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
    fn preview_objects(&self, s: &Session, c: Vec2) -> Vec<Entity> {
        // The first wall's elevation under the cursor.
        let (ElevAsk::Place, Some(h), Ok(d)) = (self.ask, self.walls.first(), s.doc()) else { return Vec::new() };
        d.wall_elevation_pieces(&s.space(), *h, self.side).map(|(_, pieces)| cadcraft_doc::preview_entities(pieces, c, 0.0)).unwrap_or_default()
    }
}

fn run_wallcleanup(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = s.doc_mut()?;
    let mut n = 0;
    for space in wall_spaces(d) {
        n += d.cleanup_walls(&space).map_err(|e| other(e.to_string()))?;
    }
    Ok(json!({ "regenerated": n }))
}

/// `handle` or `handles` of a JSON form.
pub(crate) fn handles_param(cmd: &str, p: &Value) -> Result<Vec<Handle>> {
    if let Some(h) = super::curves::handle_param(p, "handle") {
        return Ok(vec![h]);
    }
    let Some(Value::Array(a)) = p.get("handles") else { return Err(bad(cmd, "`handle` or `handles` is required")) };
    if a.len() > MAX_EDITED {
        return Err(bad(cmd, format!("at most {MAX_EDITED} objects")));
    }
    a.iter().map(|v| v.as_str().and_then(Handle::parse_hex).ok_or_else(|| bad(cmd, "`handles` must be handles"))).collect()
}

/// `layers` of WALLEDIT: an object `{"Clay brick": 150}` (by name), or a list of `{name | index, thickness}`
/// (the index counts from the exterior face, from 0), in millimetres.
fn layers_param(p: &Value) -> Result<Vec<(LayerRef, f64)>> {
    const CMD: &str = "walledit";
    let number = |v: &Value| {
        v.as_f64().filter(|t| t.is_finite() && *t >= 0.0).ok_or_else(|| bad(CMD, "a layer's thickness is a number of millimetres, 0 or more"))
    };
    let out: Vec<(LayerRef, f64)> = match p.get("layers") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Object(o)) => o.iter().map(|(k, v)| Ok((LayerRef::Name(k.clone()), number(v)?))).collect::<Result<_>>()?,
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                let layer = match (x.get("index").and_then(Value::as_u64), x.get("name").and_then(Value::as_str)) {
                    (Some(i), _) => LayerRef::Index(usize::try_from(i).map_err(|_| bad(CMD, "a layer index is a small whole number"))?),
                    (None, Some(n)) => LayerRef::Name(n.to_string()),
                    _ => return Err(bad(CMD, "each layer has a `name` or an `index`")),
                };
                Ok((layer, number(x.get("thickness").unwrap_or(&Value::Null))?))
            })
            .collect::<Result<_>>()?,
        Some(_) => return Err(bad(CMD, "`layers` is an object of thicknesses by layer name, or a list")),
    };
    if out.len() > cadcraft_doc::MAX_WALL_COMPONENTS {
        return Err(bad(CMD, format!("at most {} layers", cadcraft_doc::MAX_WALL_COMPONENTS)));
    }
    Ok(out)
}

fn run_walledit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "walledit";
    let handles = handles_param(CMD, p)?;
    let style = match str_param(p, "style") {
        Some(n) => Some(style_named(s.doc()?, n)?),
        None => None,
    };
    let justification = match str_param(p, "justification") {
        Some(t) => Some(WallJustification::parse(t).ok_or_else(|| bad(CMD, "justification must be left, center or right"))?),
        None => None,
    };
    let height = match p.get("height") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|h| h.is_finite() && *h > 0.0).ok_or_else(|| bad(CMD, "`height` must be a positive number"))?),
    };
    let layers = layers_param(p)?;
    let apply_to_style = p.get("applyToStyle").and_then(Value::as_bool).unwrap_or(false);
    if apply_to_style && layers.is_empty() {
        return Err(bad(CMD, "`applyToStyle` needs `layers`"));
    }
    let flag = |key: &str| match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(CMD, format!("`{key}` must be true or false"))),
    };
    let (start_free, end_free) = (flag("freeStart")?, flag("freeEnd")?);
    // The bulges of the segments: a list, or one segment's (`segment`, `bulge`), the others kept.
    let bulges = match (p.get("bulges").filter(|v| !v.is_null()), p.get("bulge").filter(|v| !v.is_null())) {
        (Some(_), Some(_)) => return Err(bad(CMD, "give `bulges` or `bulge`, not both")),
        (Some(list), None) => Some(
            list.as_array()
                .ok_or_else(|| bad(CMD, "`bulges` must be a list of numbers"))?
                .iter()
                .map(|b| b.as_f64().filter(|b| b.is_finite()).ok_or_else(|| bad(CMD, "`bulges` must be a list of numbers")))
                .collect::<Result<Vec<f64>>>()?,
        ),
        (None, Some(b)) => {
            let bulge = b.as_f64().filter(|b| b.is_finite()).ok_or_else(|| bad(CMD, "`bulge` must be a number"))?;
            let segment =
                p.get("segment").and_then(Value::as_u64).ok_or_else(|| bad(CMD, "`bulge` needs the `segment` (from 0) it is for"))? as usize;
            if segment >= MAX_WALL_VERTICES {
                return Err(bad(CMD, "no such segment"));
            }
            // Resolved wall by wall below: only the one segment changes.
            if handles.len() > 1 {
                return Err(bad(CMD, "`segment` and `bulge` are for one wall; give `bulges` for several"));
            }
            let mut current = handles.first().and_then(|h| s.doc().ok()?.entity(*h).and_then(|e| wall_of(e))).map(|w| w.bulges).unwrap_or_default();
            current.resize(current.len().max(segment + 1), 0.0);
            if let Some(slot) = current.get_mut(segment) {
                *slot = bulge;
            }
            Some(current)
        }
        (None, None) => None,
    };
    let flip = flag("flip")?.unwrap_or(false);
    let force = flag("force")?.unwrap_or(false);
    let (n, kept) =
        edit_walls(s, &handles, &WallChange { style, justification, height, layers, apply_to_style, start_free, end_free, bulges, flip, force })?;
    Ok(json!({ "regenerated": n, "kept": kept }))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditAsk {
    Select,
    Option,
    Style,
    Justification,
    Height,
}

struct WallEditM {
    walls: Vec<Handle>,
    ask: EditAsk,
}

impl WallEditM {
    fn new(s: &Session) -> Self {
        let walls: Vec<Handle> =
            s.doc().map(|d| s.selection().into_iter().filter(|h| d.entity(*h).and_then(|e| wall_of(e)).is_some()).collect()).unwrap_or_default();
        let ask = if walls.is_empty() { EditAsk::Select } else { EditAsk::Option };
        WallEditM { walls, ask }
    }
    fn apply(&mut self, s: &mut Session, c: WallChange) -> Result<Step> {
        match edit_walls(s, &self.walls, &c) {
            Ok((n, 0)) => s.echo(format!("{n} wall(s) changed.")),
            Ok((n, kept)) => s.echo(format!("{n} wall(s) changed; {kept} edited by hand or scaled were left as they are.")),
            Err(e) => s.echo(e.to_string()),
        }
        self.ask = EditAsk::Option;
        Ok(Step::Continue)
    }
}

impl Interactive for WallEditM {
    fn name(&self) -> &'static str {
        "WALLEDIT"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            EditAsk::Select => Prompt::new("Select walls", Accept::SELECT),
            EditAsk::Option => {
                Prompt::new("Enter an option", super::curves::KW).kw(&["Style", "Justification", "Height", "FreeStart", "FreeEnd", "Flip"])
            }
            EditAsk::Style => Prompt::new("Enter wall style name", Accept::TEXT),
            EditAsk::Justification => Prompt::new("Enter justification", super::curves::KW).kw(&["Left", "Center", "Right"]),
            EditAsk::Height => Prompt::new("Enter wall height", Accept::NUMBER),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let text = match &i {
            Input::Text(t) | Input::Keyword(t) => Some(t.trim().to_string()),
            _ => None,
        };
        match (self.ask, i, text) {
            (EditAsk::Select, Input::Pick(hs), _) => {
                let d = s.doc()?;
                let new: Vec<Handle> =
                    hs.into_iter().filter(|h| d.entity(*h).and_then(|e| wall_of(e)).is_some() && !self.walls.contains(h)).collect();
                self.walls.extend(new);
                Ok(Step::Continue)
            }
            (EditAsk::Select, Input::Enter, _) if !self.walls.is_empty() => {
                self.ask = EditAsk::Option;
                Ok(Step::Continue)
            }
            (_, Input::Enter | Input::Cancel, _) if matches!(self.ask, EditAsk::Select | EditAsk::Option) => Ok(Step::Done),
            (EditAsk::Option, _, Some(k)) => {
                self.ask = match k.as_str() {
                    "Style" => EditAsk::Style,
                    "Justification" => EditAsk::Justification,
                    "Height" => EditAsk::Height,
                    "Flip" => return self.apply(s, WallChange { flip: true, ..WallChange::default() }),
                    // Toggles the join at that end, from the first wall's present state.
                    "FreeStart" | "FreeEnd" => {
                        let first = self.walls.first().and_then(|h| s.doc().ok()?.entity(*h).and_then(|e| wall_of(e)));
                        let start = k == "FreeStart";
                        let now = first.is_some_and(|w| if start { w.start_free } else { w.end_free });
                        let c = if start {
                            WallChange { start_free: Some(!now), ..WallChange::default() }
                        } else {
                            WallChange { end_free: Some(!now), ..WallChange::default() }
                        };
                        return self.apply(s, c);
                    }
                    _ => EditAsk::Option,
                };
                Ok(Step::Continue)
            }
            (EditAsk::Style, _, Some(t)) if !t.is_empty() => match style_named(s.doc()?, &t) {
                Ok(st) => self.apply(s, WallChange { style: Some(st), ..WallChange::default() }),
                Err(e) => {
                    s.echo(e.to_string());
                    Ok(Step::Continue)
                }
            },
            (EditAsk::Justification, _, Some(t)) => match WallJustification::parse(&t) {
                Some(j) => self.apply(s, WallChange { justification: Some(j), ..WallChange::default() }),
                None => Ok(Step::Continue),
            },
            (EditAsk::Height, _, Some(t)) => match t.parse::<f64>().ok().filter(|h| h.is_finite() && *h > 0.0) {
                Some(h) => self.apply(s, WallChange { height: Some(h), ..WallChange::default() }),
                None => {
                    s.echo("Requires a positive height.");
                    Ok(Step::Continue)
                }
            },
            (_, Input::Enter | Input::Cancel, _) => {
                self.ask = EditAsk::Option;
                Ok(Step::Continue)
            }
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, _c: Vec2) -> Vec<EntityKind> {
        Vec::new()
    }
}
