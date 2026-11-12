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
            .params("{points, closed?, style?, justification?: left|center|right, height? (mm)} → {handle}")
            .interactive(|s| Ok(Box::new(WallAddM::new(s)))),
        CommandSpec::new("wallstyleedit", "Wall Styles", run_wallstyleedit).params(
            "{} → {styles, current} | {name, description?, components?: [{name, thickness (mm), pattern?, patternScale?, \
             patternAngle?, color?, priority?, function?: structure|insulation|finish|membrane|airGap, returnsAtOpenings?}], \
             defaultHeight? (mm), copyFrom?} → {style, regenerated, kept} | {delete: name} | {name, rename: newName}",
        ),
        CommandSpec::new("wallcleanup", "Wall Junctions", run_wallcleanup)
            .params("{} → {regenerated}: joins the walls where they meet (L, T, X), after walls were moved, copied or erased"),
        CommandSpec::new("walledit", "Edit Wall", run_walledit)
            .params(
                "{handle | handles, style?, justification?: left|center|right, height? (mm), layers? ({layer name: thickness in mm}, or [{name | index, thickness}]: these walls only), \
                 applyToStyle? (true: the style and every wall that uses it take the thicknesses too)} → {regenerated}",
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
    WallParams { style, path: points, closed, justification, height, units: WallUnits::of(d), joints: Default::default() }
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

/// The post-command pass for openings: a drawing that has openings, or had them before the command,
/// gets its walls joined again, so an opening moved, copied or erased cuts its wall (or stops cutting
/// it) without WALLCLEANUP. Cheap when the command changed nothing or there are no openings.
pub(crate) fn after_command(s: &mut Session, before: Option<&std::sync::Arc<Drawing>>) {
    let space = s.space();
    let Ok(st) = s.state_mut() else { return };
    if before.is_none_or(|b| std::sync::Arc::ptr_eq(b, &st.doc)) {
        return;
    }
    let has_openings = |d: &Drawing| d.space(&space).is_some_and(|sp| sp.iter().any(|e| cadcraft_doc::opening_of(e).is_some()));
    if before.is_some_and(|b| has_openings(b)) || has_openings(&st.doc) {
        join_walls(std::sync::Arc::make_mut(&mut st.doc), &space);
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
    let wall = params(s.doc()?, style, points, closed, justification, height);
    Ok(json!({ "handle": add(s, wall)?.hex() }))
}

/// What WALLADD is asking for besides points.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Point,
    Style,
    Justification,
    Height,
}

struct WallAddM {
    pts: Vec<Vec2>,
    /// The walls added so far, one per segment.
    handles: Vec<Handle>,
    style: WallStyle,
    justification: WallJustification,
    /// Millimetres.
    height: f64,
    ask: Ask,
}

impl WallAddM {
    fn new(s: &Session) -> Self {
        let style = current_style(s).unwrap_or_else(|_| cadcraft_doc::standard_wall_styles().into_iter().next().unwrap_or_else(fallback_style));
        let height = style.default_height;
        WallAddM { pts: Vec::new(), handles: Vec::new(), style, justification: s.last_used.wall_justification, height, ask: Ask::Point }
    }
    fn settings(&self, s: &mut Session) {
        s.echo(format!("Current settings: Style = {}, Justification = {}, Height = {}", self.style.name, self.justification.name(), self.height));
    }
    fn wall(&self, s: &Session, pts: Vec<Vec2>, closed: bool) -> Option<WallParams> {
        let d = s.doc().ok()?;
        Some(params(d, self.style.clone(), pts, closed, self.justification, self.height))
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
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.ask {
            Ask::Style => return Prompt::new("Enter wall style name or [?]", Accept::TEXT).default(self.style.name.clone()),
            Ask::Justification => {
                return Prompt::new("Enter justification", super::curves::KW).kw(&["Left", "Center", "Right"]).default(self.justification.name());
            }
            Ask::Height => return Prompt::new("Enter wall height (mm)", Accept::NUMBER).default(format!("{}", self.height)),
            Ask::Point => {}
        }
        let last = self.pts.last().copied();
        match self.pts.len() {
            0 => Prompt::new("Specify start point of wall", Accept::POINT).kw(&["STyle", "Justification", "Height"]),
            1 | 2 => Prompt::new("Specify next point", Accept::POINT).kw(&["Undo"]).base_opt(last),
            _ => Prompt::new("Specify next point", Accept::POINT).kw(&["Close", "Undo"]).base_opt(last),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
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
        if self.ask != Ask::Point || self.pts.is_empty() {
            return Vec::new();
        }
        // The walls drawn so far are in the drawing: only the one under way is previewed.
        let Some(last) = self.pts.last().copied().filter(|l| !l.near(c, 1e-9)) else { return Vec::new() };
        let Some(Ok(pieces)) = self.wall(s, vec![last, c], false).map(|w| w.generate()) else { return Vec::new() };
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
            WallStyle { name: name.to_string(), description: String::new(), components: Vec::new(), default_height: 2500.0 }
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

/// The walls among `handles`, refusing an object that is not a wall or that was edited by hand.
fn editable_walls(d: &Drawing, handles: &[Handle]) -> Result<Vec<(Handle, WallParams)>> {
    if handles.is_empty() || handles.len() > MAX_EDITED {
        return Err(other(format!("Select 1 to {MAX_EDITED} walls.")));
    }
    let mut out = Vec::with_capacity(handles.len());
    for &h in handles {
        let w = d.entity(h).and_then(|e| wall_of(e)).ok_or_else(|| other("Select walls only."))?;
        if !d.wall_is_intact(h) {
            return Err(other("A selected wall was edited by hand or scaled: it can't be regenerated."));
        }
        out.push((h, w));
    }
    Ok(out)
}

/// Most objects one edit changes.
const MAX_EDITED: usize = 10_000;

/// Gives every wall of `handles` the change; nothing changes when one of them can't take it.
fn edit_walls(s: &mut Session, handles: &[Handle], c: &WallChange) -> Result<usize> {
    let walls = editable_walls(s.doc()?, handles)?;
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
        // Junctions are worked out again after the change.
        w.joints = Default::default();
        w.validate().map_err(|e| other(e.to_string()))?;
        next.push((h, w));
    }
    let d = s.doc_mut()?;
    for (h, w) in &next {
        d.regenerate_wall(*h, w).map_err(|e| other(e.to_string()))?;
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
    Ok(next.len())
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
    let n = edit_walls(s, &handles, &WallChange { style, justification, height, layers, apply_to_style })?;
    Ok(json!({ "regenerated": n }))
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
            Ok(n) => s.echo(format!("{n} wall(s) changed.")),
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
            EditAsk::Option => Prompt::new("Enter an option", super::curves::KW).kw(&["Style", "Justification", "Height"]),
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
