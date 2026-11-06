//! Architectural walls: a wall style lists the wall's components (brick, insulation,
//! plasterboard…) with their thicknesses, from the exterior face to the interior face; a wall is
//! a block reference to an anonymous block holding the lines and hatches generated from its
//! style and its path.
//!
//! The wall's parameters ([`WallParams`]) travel as extended data of their own registered
//! application ([`WALL_APP`]) on the block reference. Entity xdata is what a DWG save keeps
//! (xdata on block records is dropped there, see `docs/file-format-parity.md`), and copying the
//! reference (COPY, MIRROR, paste) copies the wall with it, as xdata is part of the object.
//! Other programs see an ordinary block reference with lines and hatches.
//!
//! Units: styles measure their components in millimetres; a wall keeps the factor it was drawn
//! with ([`WallUnits`]), so its geometry is the same however the drawing's units change later.
//!
//! The path is in the block's coordinates: moving, rotating or mirroring the reference moves
//! the wall without regenerating it.

use cadcraft_color::Color;
use cadcraft_geom::{PolyVertex, Vec2, Vec3, shoelace};
use serde::{Deserialize, Serialize};

use crate::{DocError, EntityKind, Hatch, HatchLoop, Line, LwPolyline, RawTag, Result};

/// The registered application name of a wall's extended data.
pub const WALL_APP: &str = "CADCRAFT_WALL";
/// Most components a wall style has.
pub const MAX_WALL_COMPONENTS: usize = 16;
/// Most vertices of one wall's path; a longer run is drawn as several walls.
pub const MAX_WALL_VERTICES: usize = 64;
/// Thickest component and highest wall, in millimetres (100 m).
pub const MAX_WALL_SIZE_MM: f64 = 100_000.0;
/// Longest style or component name, in characters.
pub const MAX_WALL_NAME_CHARS: usize = 64;
/// Longest wall style description, in characters.
pub const MAX_WALL_DESCRIPTION_CHARS: usize = 255;
/// Most bytes of a wall's parameters in its extended data, each non-ASCII character counted as
/// the 7-byte `\U+XXXX` escape an older DXF file writes it as: within the 16 KB other programs
/// allow per object whatever the file version.
pub const MAX_WALL_XDATA_BYTES: usize = 12_000;
/// The layer of a wall's block reference and lines.
pub const WALL_LAYER: &str = "A-WALL";
/// The layer of a wall's hatches.
pub const WALL_PATTERN_LAYER: &str = "A-WALL-PATT";
/// The smallest angle between two segments of a wall: a sharper corner has no clean miter (its
/// point would run far past the wall's faces), so the path is refused.
const MIN_CORNER_DEGREES: f64 = 30.0;
/// The first string of a wall's extended data, naming the format of the JSON that follows.
const XDATA_FORMAT: &str = "WALL1";
/// Bytes per xdata string, each non-ASCII character counted as its 7-byte `\U+XXXX` escape: a
/// string stays under the 255-byte limit in any file version (see `json_strings`).
const XDATA_CHUNK_COST: usize = 240;

fn one() -> f64 {
    1.0
}

fn default_priority() -> u16 {
    500
}

fn default_height() -> f64 {
    2500.0
}

/// What a component is for: it sets the default cleanup priority, and how the style manager
/// groups components.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentFunction {
    /// Load-bearing material: concrete, brick, block, studs.
    #[default]
    Structure,
    Insulation,
    /// Plasterboard, render, cladding.
    Finish,
    /// A membrane (vapour control layer, breather membrane): drawn as a line, no thickness.
    Membrane,
    /// An air gap or a service void: never hatched.
    AirGap,
}

/// One layer of a wall style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallComponent {
    pub name: String,
    /// Millimetres; 0 for a membrane.
    pub thickness: f64,
    /// The hatch pattern name (`SOLID` for a solid fill); empty for no hatch.
    #[serde(default)]
    pub pattern: String,
    /// The pattern scale for a metric drawing in millimetres (or an imperial one in inches).
    #[serde(default = "one")]
    pub pattern_scale: f64,
    /// The pattern angle in degrees.
    #[serde(default)]
    pub pattern_angle: f64,
    #[serde(default)]
    pub color: Color,
    /// Cleanup priority, kept with the style for joining walls (not used yet): where two walls
    /// meet, a component with a lower number will cut through components with a higher one.
    #[serde(default = "default_priority")]
    pub priority: u16,
    #[serde(default)]
    pub function: ComponentFunction,
    /// Whether the component wraps into the reveal of an opening, as a lining around a window
    /// does; kept with the style for openings (not used yet).
    #[serde(default)]
    pub returns_at_openings: bool,
}

impl WallComponent {
    /// A component with the default priority of its function.
    pub fn new(name: &str, thickness: f64, function: ComponentFunction) -> Self {
        let priority = match function {
            ComponentFunction::Structure => 300,
            ComponentFunction::Insulation => 500,
            ComponentFunction::AirGap => 600,
            ComponentFunction::Membrane => 700,
            ComponentFunction::Finish => 800,
        };
        WallComponent {
            name: name.into(),
            thickness,
            pattern: String::new(),
            pattern_scale: 1.0,
            pattern_angle: 0.0,
            color: Color::ByLayer,
            priority,
            function,
            returns_at_openings: false,
        }
    }
    /// The same component hatched with `pattern` at `scale`.
    pub fn hatched(mut self, pattern: &str, scale: f64) -> Self {
        self.pattern = pattern.into();
        self.pattern_scale = scale;
        self
    }
    /// The same component wrapping into openings.
    pub fn returning(mut self) -> Self {
        self.returns_at_openings = true;
        self
    }
}

/// A wall style: the components from the exterior face to the interior face.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallStyle {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub components: Vec<WallComponent>,
    /// The height of new walls, in millimetres.
    #[serde(default = "default_height")]
    pub default_height: f64,
}

impl WallStyle {
    /// The total thickness, in millimetres.
    pub fn thickness(&self) -> f64 {
        self.components.iter().map(|c| c.thickness).sum()
    }

    /// Checks a style from a file, a command or a script; the message says what is wrong.
    pub fn validate(&self) -> Result<()> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > MAX_WALL_NAME_CHARS {
            return Err(invalid(format!("A wall style name has 1 to {MAX_WALL_NAME_CHARS} characters.")));
        }
        if self.description.chars().count() > MAX_WALL_DESCRIPTION_CHARS {
            return Err(invalid(format!("A wall style description has at most {MAX_WALL_DESCRIPTION_CHARS} characters.")));
        }
        if self.components.is_empty() || self.components.len() > MAX_WALL_COMPONENTS {
            return Err(invalid(format!("A wall style has 1 to {MAX_WALL_COMPONENTS} components.")));
        }
        for c in &self.components {
            let n = c.name.trim();
            if n.is_empty() || n.chars().count() > MAX_WALL_NAME_CHARS {
                return Err(invalid(format!("A wall component name has 1 to {MAX_WALL_NAME_CHARS} characters.")));
            }
            if !c.thickness.is_finite() || c.thickness < 0.0 || c.thickness > MAX_WALL_SIZE_MM {
                return Err(invalid(format!("Component \"{n}\": the thickness must be 0 to {MAX_WALL_SIZE_MM} mm.")));
            }
            if (c.thickness == 0.0) != (c.function == ComponentFunction::Membrane) {
                return Err(invalid(format!("Component \"{n}\": only a membrane has no thickness, and a membrane has none.")));
            }
            if c.pattern.chars().count() > MAX_WALL_NAME_CHARS {
                return Err(invalid(format!("Component \"{n}\": the pattern name is too long.")));
            }
            if !c.pattern_scale.is_finite() || c.pattern_scale <= 0.0 || c.pattern_scale > 1e6 {
                return Err(invalid(format!("Component \"{n}\": the pattern scale must be above 0.")));
            }
            if !c.pattern_angle.is_finite() {
                return Err(invalid(format!("Component \"{n}\": the pattern angle must be a number.")));
            }
        }
        if self.thickness() <= 0.0 {
            return Err(invalid("A wall style needs at least one component with a thickness."));
        }
        if !self.default_height.is_finite() || self.default_height <= 0.0 || self.default_height > MAX_WALL_SIZE_MM {
            return Err(invalid(format!("The default wall height must be above 0 and at most {MAX_WALL_SIZE_MM} mm.")));
        }
        Ok(())
    }
}

/// Where the wall's path runs, looking along the drawing direction: the exterior face is on
/// the left. Drawing a building's outline clockwise puts the exterior outside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WallJustification {
    /// The path is the exterior face; the wall lies to its right.
    #[default]
    Left,
    /// The path is the middle of the wall.
    Center,
    /// The path is the interior face; the wall lies to its left.
    Right,
}

impl WallJustification {
    pub fn name(self) -> &'static str {
        match self {
            WallJustification::Left => "Left",
            WallJustification::Center => "Center",
            WallJustification::Right => "Right",
        }
    }
    /// The justification a command option or a JSON value names.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "l" | "left" => Some(WallJustification::Left),
            "c" | "center" | "centre" => Some(WallJustification::Center),
            "r" | "right" => Some(WallJustification::Right),
            _ => None,
        }
    }
}

/// How a wall's millimetres become drawing units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallUnits {
    /// Millimetres per drawing unit (1 for millimetres, 25.4 for inches).
    pub mm_per_unit: f64,
    /// Whether the drawing's hatch patterns are the metric ones (`MEASUREMENT` 1), measured in
    /// millimetres, rather than the imperial ones, measured in inches.
    pub metric_patterns: bool,
}

impl Default for WallUnits {
    fn default() -> Self {
        WallUnits { mm_per_unit: 1.0, metric_patterns: true }
    }
}

impl WallUnits {
    /// The units of drawing `d`: its `INSUNITS`, or millimetres in a metric drawing and inches in
    /// an imperial one when it has none.
    pub fn of(d: &crate::Drawing) -> Self {
        let metric_patterns = d.header.i64("MEASUREMENT", 0) == 1;
        let mm_per_unit = match d.header.i64("INSUNITS", 0) {
            1 => 25.4,   // inches
            2 => 304.8,  // feet
            4 => 1.0,    // millimetres
            5 => 10.0,   // centimetres
            6 => 1000.0, // metres
            14 => 100.0, // decimetres
            _ if metric_patterns => 1.0,
            _ => 25.4,
        };
        WallUnits { mm_per_unit, metric_patterns }
    }
    fn valid(self) -> bool {
        self.mm_per_unit.is_finite() && self.mm_per_unit > 0.0 && self.mm_per_unit <= 1e6
    }
    /// Millimetres as drawing units.
    pub fn to_units(self, mm: f64) -> f64 {
        mm / self.mm_per_unit
    }
    /// A pattern scale defined for millimetres (or inches) as the scale for this drawing.
    fn pattern_scale(self, scale: f64) -> f64 {
        let pattern_unit_mm = if self.metric_patterns { 1.0 } else { 25.4 };
        scale * pattern_unit_mm / self.mm_per_unit
    }
}

/// A wall: what its block reference keeps to regenerate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallParams {
    /// A copy of the style the wall was drawn with, so the wall stands on its own.
    pub style: WallStyle,
    /// The path in the block's coordinates (drawing units); no repeated point.
    pub path: Vec<Vec2>,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub justification: WallJustification,
    /// Millimetres.
    #[serde(default = "default_height")]
    pub height: f64,
    #[serde(default)]
    pub units: WallUnits,
}

/// One object of a wall's generated geometry: lines go on [`WALL_LAYER`], hatches on
/// [`WALL_PATTERN_LAYER`].
#[derive(Clone, Debug, PartialEq)]
pub struct WallPiece {
    pub layer: &'static str,
    pub color: Color,
    pub kind: EntityKind,
}

impl WallParams {
    /// Checks parameters from a file, a command or a script.
    pub fn validate(&self) -> Result<()> {
        self.style.validate()?;
        if !self.units.valid() {
            return Err(invalid("The wall's units are not valid."));
        }
        if !self.height.is_finite() || self.height <= 0.0 || self.height > MAX_WALL_SIZE_MM {
            return Err(invalid(format!("The wall height must be above 0 and at most {MAX_WALL_SIZE_MM} mm.")));
        }
        let min = if self.closed { 3 } else { 2 };
        if self.path.len() < min || self.path.len() > MAX_WALL_VERTICES {
            return Err(invalid(format!("A wall has {min} to {MAX_WALL_VERTICES} points.")));
        }
        if self.path.iter().any(|p| !p.is_finite() || p.x.abs() > 1e12 || p.y.abs() > 1e12) {
            return Err(invalid("A wall point is not a finite coordinate."));
        }
        // Thickness and segment lengths are checked when the geometry is built, the data size
        // when the extended data is.
        self.generate()?;
        self.to_xdata().map(|_| ())
    }

    /// The total thickness in drawing units.
    pub fn thickness(&self) -> f64 {
        self.units.to_units(self.style.thickness())
    }

    /// The offsets of the component boundaries from the path (left positive), exterior face
    /// first: one more than there are components.
    fn boundaries(&self) -> Vec<f64> {
        let total = self.thickness();
        let mut at = match self.justification {
            WallJustification::Left => 0.0,
            WallJustification::Center => total / 2.0,
            WallJustification::Right => total,
        };
        let mut v = Vec::with_capacity(self.style.components.len() + 1);
        v.push(at);
        for c in &self.style.components {
            at -= self.units.to_units(c.thickness);
            v.push(at);
        }
        v
    }

    /// The wall's lines and hatches in the block's coordinates: one line along each component
    /// boundary (a membrane's included), the two end lines of an open wall, and a hatch per
    /// component with a pattern.
    pub fn generate(&self) -> Result<Vec<WallPiece>> {
        let bounds = self.boundaries();
        let reach = bounds.iter().fold(0.0_f64, |m, b| m.max(b.abs()));
        let joins = joins(&self.path, self.closed, reach)?;
        let mut pieces = boundary_lines(&joins, &bounds, self.closed, reach);
        if !self.closed {
            pieces.extend(end_lines(&joins, &bounds));
        }
        for (c, pair) in self.style.components.iter().zip(bounds.windows(2)) {
            if let &[outer, inner] = pair
                && let Some(h) = self.hatch(c, &joins, outer, inner)
            {
                pieces.push(h);
            }
        }
        Ok(pieces)
    }

    /// The hatch of component `c` between offsets `outer` and `inner`, when it has a pattern.
    fn hatch(&self, c: &WallComponent, joins: &[Join], outer: f64, inner: f64) -> Option<WallPiece> {
        let pattern = c.pattern.trim();
        if pattern.is_empty() || c.thickness <= 0.0 || matches!(c.function, ComponentFunction::Membrane | ComponentFunction::AirGap) {
            return None;
        }
        let a = offset(joins, outer);
        let b = offset(joins, inner);
        let loops = if self.closed {
            // The larger ring is the outer boundary, whichever way the wall was drawn.
            let a_outer = shoelace(&a).abs() >= shoelace(&b).abs();
            vec![hatch_loop(a, a_outer), hatch_loop(b, !a_outer)]
        } else {
            let mut ring = a;
            ring.extend(b.into_iter().rev());
            vec![hatch_loop(ring, true)]
        };
        let solid = pattern.eq_ignore_ascii_case("SOLID");
        let hatch = Hatch {
            pattern: pattern.to_ascii_uppercase(),
            solid,
            loops,
            scale: if solid { 1.0 } else { self.units.pattern_scale(c.pattern_scale) },
            angle: c.pattern_angle.to_radians(),
            associative: false,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
            pattern_lines: Vec::new(),
        };
        Some(WallPiece { layer: WALL_PATTERN_LAYER, color: c.color, kind: EntityKind::Hatch(hatch) })
    }

    /// Whether `entities` are still the objects these parameters generate, with their layers and
    /// colours: a wall whose block was edited elsewhere (another program, BEDIT) no longer
    /// matches, and is not regenerated, so those edits are never overwritten.
    pub fn matches<'a>(&self, entities: impl IntoIterator<Item = &'a crate::Entity>) -> bool {
        let Ok(pieces) = self.generate() else { return false };
        let tol = 1e-6 * self.path.iter().fold(1.0_f64, |m, p| m.max(p.x.abs()).max(p.y.abs())).max(self.thickness());
        let mut it = entities.into_iter();
        for p in &pieces {
            let Some(e) = it.next() else { return false };
            if !e.common.layer.eq_ignore_ascii_case(p.layer) || e.common.color != p.color || !same_shape(&p.kind, &e.kind, tol) {
                return false;
            }
        }
        it.next().is_none()
    }

    /// The wall's extended data: `1001` [`WALL_APP`], `1000 WALL1`, then the parameters as JSON
    /// in `1000` strings.
    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        let json = serde_json::to_string(self).map_err(|e| invalid(format!("The wall can't be saved: {e}")))?;
        let strings = json_strings(&json);
        if strings.iter().map(|s| xdata_cost(s)).sum::<usize>() > MAX_WALL_XDATA_BYTES {
            return Err(invalid("The wall has too much data to save: use fewer points, or shorter style and component names."));
        }
        let mut tags = vec![tag(1001, WALL_APP), tag(1000, XDATA_FORMAT)];
        tags.extend(strings.into_iter().map(|s| RawTag { code: 1000, value: s }));
        Ok(tags)
    }

    /// The wall parameters in an object's extended data, when they are there and usable.
    pub fn from_xdata(xdata: &[RawTag]) -> Option<WallParams> {
        let start = xdata.iter().position(|t| t.code == 1001 && t.value.trim().eq_ignore_ascii_case(WALL_APP))?;
        let ours = xdata.get(start + 1..)?;
        let ours = ours.get(..ours.iter().position(|t| t.code == 1001).unwrap_or(ours.len()))?;
        let mut strings = ours.iter().filter(|t| t.code == 1000);
        if strings.next()?.value != XDATA_FORMAT {
            return None;
        }
        let mut json = String::new();
        let mut cost = 0usize;
        for t in strings {
            cost = cost.saturating_add(xdata_cost(&t.value));
            if cost > MAX_WALL_XDATA_BYTES {
                return None;
            }
            json.push_str(&t.value);
        }
        let p: WallParams = serde_json::from_str(&json).ok()?;
        p.validate().is_ok().then_some(p)
    }

    /// `xdata` with this wall's data in place of any wall data it had (other applications'
    /// data kept).
    pub fn replace_xdata(&self, xdata: &[RawTag]) -> Result<Vec<RawTag>> {
        let mut out = without_wall_xdata(xdata);
        out.extend(self.to_xdata()?);
        Ok(out)
    }
}

impl crate::Drawing {
    /// The drawing's wall style `name` (case-insensitive).
    pub fn wall_style(&self, name: &str) -> Option<&WallStyle> {
        self.wall_styles.iter().find(|s| s.name.eq_ignore_ascii_case(name.trim()))
    }
    /// Wall style `name` from the drawing, else from the standard styles.
    pub fn wall_style_or_standard(&self, name: &str) -> Option<WallStyle> {
        let n = name.trim();
        self.wall_style(n).cloned().or_else(|| crate::standard_wall_styles().into_iter().find(|s| s.name.eq_ignore_ascii_case(n)))
    }
    /// Every wall style a new wall can use: the drawing's, then the standard ones it doesn't
    /// replace (a drawing style of the same name wins).
    pub fn available_wall_styles(&self) -> Vec<WallStyle> {
        let mut v = self.wall_styles.clone();
        for s in crate::standard_wall_styles() {
            if self.wall_style(&s.name).is_none() {
                v.push(s);
            }
        }
        v
    }
}

impl crate::Drawing {
    /// Adds wall `p` to `space`, its path given in drawing coordinates: an anonymous block holding
    /// the wall's geometry, and a block reference to it on [`WALL_LAYER`] at the path's first
    /// point carrying the wall's data. `common` gives the reference's other properties.
    pub fn add_wall(&mut self, space: &crate::Space, mut common: crate::Common, mut p: WallParams) -> Result<crate::Handle> {
        let origin = p.path.first().copied().ok_or_else(|| invalid("A wall needs at least two points."))?;
        for v in &mut p.path {
            *v -= origin;
        }
        // Everything that can fail comes before the drawing changes.
        p.validate()?;
        common.xdata = p.replace_xdata(&common.xdata)?;
        common.layer = WALL_LAYER.into();
        let block = self.wall_block(&p)?;
        self.ensure_wall_layers();
        let insert = crate::Insert { block, insert: v3(origin), ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives wall `h` the parameters `p` (path in the block's coordinates): a new block with the
    /// new geometry, the old block removed when nothing else refers to it.
    pub fn regenerate_wall(&mut self, h: crate::Handle, p: &WallParams) -> Result<()> {
        let e = self.entity(h).ok_or(DocError::NoEntity(h))?;
        let EntityKind::Insert(old) = &e.kind else { return Err(invalid("The object is not a wall.")) };
        let old = old.block.clone();
        p.validate()?;
        let xdata = p.replace_xdata(&e.common.xdata)?;
        let block = self.wall_block(p)?;
        self.ensure_wall_layers();
        self.modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.block = block;
            }
            e.common.xdata = xdata;
        })?;
        self.drop_unreferenced_block(&old);
        Ok(())
    }

    /// Whether wall `h`'s block still holds the geometry its parameters generate, and the
    /// reference keeps the wall's size (no scale but a mirror): only then may it be regenerated.
    pub fn wall_is_intact(&self, h: crate::Handle) -> bool {
        let Some(e) = self.entity(h) else { return false };
        let (EntityKind::Insert(i), Some(p)) = (&e.kind, wall_of(e)) else { return false };
        let unit = |s: f64| (s.abs() - 1.0).abs() <= 1e-9;
        let Some(b) = self.block(&i.block) else { return false };
        unit(i.scale.x) && unit(i.scale.y) && i.cols <= 1 && i.rows <= 1 && p.matches(b.entities.iter().map(AsRef::as_ref))
    }

    /// A new anonymous block holding wall `p`'s geometry; returns its name.
    fn wall_block(&mut self, p: &WallParams) -> Result<String> {
        let pieces = p.generate()?;
        let name = self.anonymous_block_name("*U");
        let mut b = crate::Block::new(&name);
        b.anonymous = true;
        for piece in pieces {
            let common = crate::Common { layer: piece.layer.into(), color: piece.color, ..Default::default() };
            b.entities.push(crate::Entity { handle: self.new_handle(), common, kind: piece.kind });
        }
        self.blocks.insert(name.clone(), std::sync::Arc::new(b));
        Ok(name)
    }

    /// The wall layers, created when missing: lines in the default colour, hatches grey.
    fn ensure_wall_layers(&mut self) {
        self.ensure_layer(WALL_LAYER);
        if self.ensure_layer(WALL_PATTERN_LAYER)
            && let Some(l) = self.layer_mut(WALL_PATTERN_LAYER)
        {
            l.color = Color::Index(8);
        }
    }

    /// Removes anonymous block `name` when no object refers to it.
    fn drop_unreferenced_block(&mut self, name: &str) {
        let refers = |e: &std::sync::Arc<crate::Entity>| matches!(&e.kind, EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(name));
        let used = self.model.iter().any(refers)
            || self.layouts.iter().any(|l| l.entities.iter().any(refers))
            || self.blocks.values().any(|b| b.entities.iter().any(refers) || b.array.as_ref().is_some_and(|a| a.source.eq_ignore_ascii_case(name)));
        if !used && self.block(name).is_some_and(|b| b.anonymous) {
            self.blocks.retain(|k, _| !k.eq_ignore_ascii_case(name));
        }
    }
}

/// The wall an object is, when it is a block reference carrying usable wall data.
pub fn wall_of(e: &crate::Entity) -> Option<WallParams> {
    match &e.kind {
        EntityKind::Insert(_) => WallParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// `xdata` without a wall's application data.
fn without_wall_xdata(xdata: &[RawTag]) -> Vec<RawTag> {
    let mut out = Vec::with_capacity(xdata.len());
    let mut ours = false;
    for t in xdata {
        if t.code == 1001 {
            ours = t.value.trim().eq_ignore_ascii_case(WALL_APP);
        }
        if !ours {
            out.push(t.clone());
        }
    }
    out
}

/// A path vertex and the direction its offset points move in: an offset point is
/// `at + miter * offset`, on the line parallel to each adjacent segment at that distance.
#[derive(Clone, Copy, Debug)]
struct Join {
    at: Vec2,
    miter: Vec2,
}

/// The joins of `path`, checking that every offset up to `reach` from the path draws cleanly:
/// no zero-length segment, no fold back, no corner sharper than the miter limit, and no segment
/// too short for the corners at its two ends.
fn joins(path: &[Vec2], closed: bool, reach: f64) -> Result<Vec<Join>> {
    let n = path.len();
    let segs = if closed { n } else { n.saturating_sub(1) };
    if n < 2 || (closed && n < 3) {
        return Err(invalid("A wall needs at least two points (three when closed)."));
    }
    // Unit directions of the segments.
    let mut dirs = Vec::with_capacity(segs);
    for i in 0..segs {
        let (Some(&a), Some(&b)) = (path.get(i), path.get((i + 1) % n)) else {
            return Err(invalid("A wall point is missing."));
        };
        let len = a.dist(b);
        if len.is_nan() || len <= 1e-9 * reach.max(1.0) {
            return Err(invalid("A wall can't have two points in the same place."));
        }
        dirs.push((b - a) / len);
    }
    let mut out = Vec::with_capacity(n);
    for (i, &at) in path.iter().enumerate() {
        let incoming = if i == 0 { closed.then(|| dirs.last().copied()).flatten() } else { dirs.get(i - 1).copied() };
        let outgoing = if closed || i + 1 < n { dirs.get(i).copied() } else { None };
        let miter = match (incoming, outgoing) {
            (Some(a), Some(b)) => {
                let m = a.perp() + b.perp();
                let half = m.len() / 2.0; // cos of half the turn
                // `half` is the sine of half the angle between the two segments.
                if half < (MIN_CORNER_DEGREES / 2.0).to_radians().sin() {
                    return Err(invalid(format!(
                        "A wall can't turn this sharply: the angle between two segments must be at least {MIN_CORNER_DEGREES} degrees."
                    )));
                }
                m.normalized() / half
            }
            (Some(d), None) | (None, Some(d)) => d.perp(),
            (None, None) => return Err(invalid("A wall needs at least two points.")),
        };
        out.push(Join { at, miter });
    }
    // Each segment must be long enough for the miters at both of its ends at the widest offset.
    for i in 0..segs {
        let (Some(a), Some(b), Some(d)) = (out.get(i), out.get((i + 1) % n), dirs.get(i)) else { continue };
        let along = |j: &Join| (j.miter.dot(*d)).abs() * reach;
        let len = a.at.dist(b.at);
        if along(a) + along(b) > len + 1e-9 * len.max(1.0) {
            return Err(invalid("A wall segment is too short for the wall's thickness at its corners."));
        }
    }
    Ok(out)
}

/// The points at `o` from the path.
fn offset(joins: &[Join], o: f64) -> Vec<Vec2> {
    joins.iter().map(|j| j.at + j.miter * o).collect()
}

/// A line along each component boundary, once per distinct offset (a membrane shares its
/// position with the boundary next to it).
fn boundary_lines(joins: &[Join], bounds: &[f64], closed: bool, reach: f64) -> Vec<WallPiece> {
    let mut pieces = Vec::with_capacity(bounds.len());
    let mut last: Option<f64> = None;
    for &b in bounds {
        if last.is_some_and(|l| (l - b).abs() <= f64::EPSILON * reach.max(1.0)) {
            continue;
        }
        last = Some(b);
        pieces.push(line_piece(offset(joins, b), closed));
    }
    pieces
}

/// The two end lines of an open wall, across all its components.
fn end_lines(joins: &[Join], bounds: &[f64]) -> Vec<WallPiece> {
    let (Some(&ext), Some(&int)) = (bounds.first(), bounds.last()) else { return Vec::new() };
    [joins.first(), joins.last()]
        .into_iter()
        .flatten()
        .map(|j| {
            let kind = EntityKind::Line(Line { a: v3(j.at + j.miter * ext), b: v3(j.at + j.miter * int) });
            WallPiece { layer: WALL_LAYER, color: Color::ByLayer, kind }
        })
        .collect()
}

fn line_piece(points: Vec<Vec2>, closed: bool) -> WallPiece {
    let vertices = points.into_iter().map(PolyVertex::new).collect();
    let kind = EntityKind::LwPolyline(LwPolyline { vertices, closed, const_width: 0.0, elevation: 0.0, plinegen: false });
    WallPiece { layer: WALL_LAYER, color: Color::ByLayer, kind }
}

fn hatch_loop(points: Vec<Vec2>, outer: bool) -> HatchLoop {
    HatchLoop { vertices: points.into_iter().map(PolyVertex::new).collect(), outer, sources: Vec::new() }
}

fn v3(p: Vec2) -> Vec3 {
    p.to3(0.0)
}

fn tag(code: i32, value: &str) -> RawTag {
    RawTag { code, value: value.into() }
}

fn invalid(m: impl Into<String>) -> DocError {
    DocError::Invalid(m.into())
}

/// Whether two generated objects have the same shape within `tol`.
fn same_shape(a: &EntityKind, b: &EntityKind, tol: f64) -> bool {
    let pts = |x: &[PolyVertex], y: &[PolyVertex]| x.len() == y.len() && x.iter().zip(y).all(|(p, q)| p.p.near(q.p, tol) && p.bulge == q.bulge);
    match (a, b) {
        (EntityKind::LwPolyline(x), EntityKind::LwPolyline(y)) => x.closed == y.closed && pts(&x.vertices, &y.vertices),
        (EntityKind::Line(x), EntityKind::Line(y)) => x.a.xy().near(y.a.xy(), tol) && x.b.xy().near(y.b.xy(), tol),
        (EntityKind::Hatch(x), EntityKind::Hatch(y)) => {
            x.pattern.eq_ignore_ascii_case(&y.pattern)
                && x.loops.len() == y.loops.len()
                && x.loops.iter().zip(&y.loops).all(|(l, m)| pts(&l.vertices, &m.vertices))
                && (x.scale - y.scale).abs() <= 1e-9 * x.scale.abs().max(1.0)
                && (x.angle - y.angle).abs() <= 1e-9
        }
        _ => false,
    }
}

/// JSON text in strings short enough for a DXF xdata string. A JSON escaped backslash (two
/// characters) is written as the six characters `\u005c`, so no string contains a DXF `\U+`
/// escape; a string costs at most [`XDATA_CHUNK_COST`] bytes ([`xdata_cost`]) and never begins
/// or ends with a space.
fn json_strings(json: &str) -> Vec<String> {
    let mut safe = String::with_capacity(json.len());
    let mut it = json.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('\\') => safe.push_str("\\u005c"),
                Some(n) => {
                    safe.push('\\');
                    safe.push(n);
                }
                None => safe.push('\\'),
            }
        } else {
            safe.push(c);
        }
    }
    let chars: Vec<char> = safe.chars().collect();
    let cost = |c: &char| char_cost(*c);
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        // The longest run from `start` within the cost limit (always at least one character).
        let mut end = start;
        let mut total = 0;
        while let Some(c) = chars.get(end) {
            if end > start && total + cost(c) > XDATA_CHUNK_COST {
                break;
            }
            total += cost(c);
            end += 1;
        }
        // A string never begins or ends with a space: some readers trim them. Back off to a
        // boundary between two other characters when there is one in the run.
        let edge = |i: usize| chars.get(i - 1).is_some_and(|c| *c == ' ') || chars.get(i).is_some_and(|c| *c == ' ');
        if end < chars.len()
            && let Some(cut) = (start + 1..=end).rev().find(|&i| !edge(i))
        {
            end = cut;
        }
        out.push(chars.get(start..end).unwrap_or_default().iter().collect());
        start = end;
    }
    out
}

/// The bytes `c` takes in an xdata string in any file version: an older DXF writes a non-ASCII
/// character as a 7-byte `\U+XXXX` escape (two of them outside the Basic Multilingual Plane).
fn char_cost(c: char) -> usize {
    if c.is_ascii() { 1 } else { 7 * c.len_utf16() }
}

/// The bytes `s` takes in an xdata string in any file version ([`char_cost`]).
fn xdata_cost(s: &str) -> usize {
    s.chars().map(char_cost).sum()
}

#[cfg(test)]
mod tests;
