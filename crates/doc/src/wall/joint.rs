//! Junctions between walls (L, T and X): where two walls meet, each component is cut by the
//! components of the other wall that have a stronger priority (a lower number), merges with
//! those of the same priority, and cuts through weaker ones.
//!
//! A wall that meets others keeps what it needs to redraw itself in its own data, in its block's
//! coordinates: the cut lines its free ends were extended or trimmed to, and the other walls'
//! component outlines that overlap it. Its geometry then depends on nothing but its own data,
//! so a wall still recognises its block as the one it generates, and the drawing reads the same
//! in any program. [`crate::Drawing::cleanup_walls`] works these out from the walls in a space.
//!
//! Each component of each segment is a convex quadrilateral; a stronger outline is subtracted
//! from it as convex pieces. A line is drawn along a piece's edge where the material changes:
//! nothing beyond it, a weaker component of another wall, or another component of the same
//! wall (drawn once). Edges against the same component, or another wall's component of the same
//! priority, are left out, so merging walls read as one.

use serde::{Deserialize, Serialize};

use super::{ComponentFunction, Join, WALL_LAYER, WALL_PATTERN_LAYER, WallParams, WallPiece, invalid, v3};
use cadcraft_color::Color;
use cadcraft_geom::{PolyVertex, Vec2};

use crate::{EntityKind, Hatch, HatchLoop, Line, Result};

/// Most outlines of other walls one wall keeps.
pub const MAX_FOREIGN_BANDS: usize = 96;
/// Most components a wall's per-component end cuts cover.
const MAX_LAYER_CUTS: usize = 64;
/// Most corners of one outline.
const MAX_BAND_CORNERS: usize = 8;
/// Most convex pieces one component of one segment is cut into.
const MAX_PIECES: usize = 256;

/// A line a free end of a wall stops on: every component boundary ends where it crosses it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndCut {
    pub point: Vec2,
    pub dir: Vec2,
}

/// A component outline of another wall, in this wall's block coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignBand {
    pub corners: Vec<Vec2>,
    pub priority: u16,
    /// Whether this wall leaves the overlap to the other one when their priorities are equal
    /// (the wall drawn later yields), so the shared corner is hatched once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub yields: bool,
}

/// What a wall keeps about the walls it meets.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallJoints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<EndCut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<EndCut>,
    /// Where each component of the start ends, by component, when the start meets the end of
    /// another wall (an L corner): each layer stops on the far edge of the matching layer of the
    /// other wall, so the two walls' layers wrap around the corner. Empty: every component
    /// stops on `start`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start_layers: Vec<EndCut>,
    /// The same for the end.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_layers: Vec<EndCut>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign: Vec<ForeignBand>,
}

impl WallJoints {
    pub fn is_empty(&self) -> bool {
        self.start.is_none() && self.end.is_none() && self.start_layers.is_empty() && self.end_layers.is_empty() && self.foreign.is_empty()
    }

    /// Refuses numbers that could not have been computed (from a file or a script).
    pub fn validate(&self) -> Result<()> {
        let ok = |p: Vec2| p.is_finite() && p.x.abs() <= 1e12 && p.y.abs() <= 1e12;
        if self.start_layers.len() > MAX_LAYER_CUTS || self.end_layers.len() > MAX_LAYER_CUTS {
            return Err(invalid("A wall junction is not usable."));
        }
        for c in self.start.iter().chain(self.end.iter()).chain(&self.start_layers).chain(&self.end_layers) {
            if !ok(c.point) || !ok(c.dir) || c.dir.len() < 1e-12 {
                return Err(invalid("A wall junction is not usable."));
            }
        }
        if self.foreign.len() > MAX_FOREIGN_BANDS {
            return Err(invalid(format!("A wall meets at most {MAX_FOREIGN_BANDS} outlines of other walls.")));
        }
        if self.foreign.iter().any(|b| b.corners.len() < 3 || b.corners.len() > MAX_BAND_CORNERS || !b.corners.iter().all(|p| ok(*p))) {
            return Err(invalid("A wall junction outline is not usable."));
        }
        Ok(())
    }
}

// =====================================================================================
// Convex polygons
// =====================================================================================

fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Twice the signed area (positive counterclockwise).
fn area2(p: &[Vec2]) -> f64 {
    let n = p.len();
    (0..n).map(|i| cross(p[i], p[(i + 1) % n])).sum()
}

/// `p` counterclockwise.
fn ccw(mut p: Vec<Vec2>) -> Vec<Vec2> {
    if area2(&p) < 0.0 {
        p.reverse();
    }
    p
}

/// The part of convex `p` on the left of the line through `a` along `d` (`keep_left`), or on
/// its right.
fn clip(p: &[Vec2], a: Vec2, d: Vec2, keep_left: bool) -> Vec<Vec2> {
    let side = |q: Vec2| {
        let s = cross(d, q - a);
        if keep_left { s } else { -s }
    };
    let n = p.len();
    let mut out = Vec::with_capacity(n + 2);
    for i in 0..n {
        let (cur, next) = (p[i], p[(i + 1) % n]);
        let (sc, sn) = (side(cur), side(next));
        if sc >= 0.0 {
            out.push(cur);
        }
        if (sc >= 0.0) != (sn >= 0.0) {
            let t = sc / (sc - sn);
            out.push(cur + (next - cur) * t);
        }
    }
    out
}

fn substantial(p: &[Vec2], tol: f64) -> bool {
    p.len() >= 3 && area2(p).abs() > tol * tol
}

/// The convex pieces of `a` outside convex `b` (both counterclockwise).
fn subtract(a: &[Vec2], b: &[Vec2], tol: f64) -> Vec<Vec<Vec2>> {
    // No overlap: `a` stays whole.
    let mut inter = a.to_vec();
    let n = b.len();
    for i in 0..n {
        inter = clip(&inter, b[i], b[(i + 1) % n] - b[i], true);
        if inter.is_empty() {
            break;
        }
    }
    if !substantial(&inter, tol) {
        return vec![a.to_vec()];
    }
    let mut out = Vec::new();
    let mut rest = a.to_vec();
    for i in 0..n {
        let (p, d) = (b[i], b[(i + 1) % n] - b[i]);
        let outside = clip(&rest, p, d, false);
        if substantial(&outside, tol) {
            out.push(outside);
        }
        rest = clip(&rest, p, d, true);
        if rest.is_empty() {
            break;
        }
    }
    out
}

/// Whether `q` is inside convex counterclockwise `p`, at least `tol` from its edges.
fn strictly_inside(p: &[Vec2], q: Vec2, tol: f64) -> bool {
    let n = p.len();
    n >= 3
        && (0..n).all(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            let len = a.dist(b);
            len > 0.0 && cross(b - a, q - a) / len > tol
        })
}

/// The parameters along `a`–`b` where it crosses or meets segment `c`–`d`.
fn crossings(a: Vec2, b: Vec2, c: Vec2, d: Vec2, tol: f64, out: &mut Vec<f64>) {
    let r = b - a;
    let s = d - c;
    let den = cross(r, s);
    let len2 = r.dot(r);
    if len2 <= 0.0 {
        return;
    }
    if den.abs() > 1e-12 * r.len() * s.len() {
        let t = cross(c - a, s) / den;
        let u = cross(c - a, r) / den;
        if (-1e-9..=1.0 + 1e-9).contains(&u) {
            out.push(t);
        }
    } else if cross(c - a, r).abs() / r.len() <= tol {
        // Along the same line: where the other segment's ends fall.
        out.push((c - a).dot(r) / len2);
        out.push((d - a).dot(r) / len2);
    }
}

// =====================================================================================
// Generation
// =====================================================================================

/// The point where the line through `p` along `d` meets cut `c`.
fn meet(p: Vec2, d: Vec2, c: &EndCut) -> Option<Vec2> {
    let den = cross(d, c.dir);
    if den.abs() < 1e-9 * d.len() * c.dir.len() {
        return None;
    }
    let t = cross(c.point - p, c.dir) / den;
    let q = p + d * t;
    q.is_finite().then_some(q)
}

/// What is on the far side of an edge.
#[derive(Clone, Copy, PartialEq)]
enum Material {
    Nothing,
    /// One of this wall's components, by index.
    Own(usize),
    /// Another wall's component, by priority.
    Foreign(u16),
}

struct Piece {
    component: usize,
    corners: Vec<Vec2>,
}

impl WallParams {
    /// The component outlines of each segment, in the block's coordinates, with the free ends
    /// on their cut lines: `(component, segment, corners)`, corners counterclockwise.
    pub(super) fn bands(&self, joins: &[Join]) -> Result<Vec<(usize, Vec<Vec2>)>> {
        let bounds = self.boundaries();
        let n = joins.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        let mut out = Vec::new();
        for i in 0..segs {
            let (Some(a), Some(b)) = (joins.get(i), joins.get((i + 1) % n)) else { continue };
            let d = b.at - a.at;
            let first = !self.closed && i == 0;
            let last = !self.closed && i + 1 == segs;
            // The cut of component `k` at one end: its own if it has one, else the end's.
            let cut_of = |on: bool, cut: Option<EndCut>, layers: &[EndCut], k: usize| -> Option<EndCut> {
                if !on {
                    return None;
                }
                cut.map(|c| layers.get(k).copied().unwrap_or(c))
            };
            let corner = |j: &Join, o: f64, cut: Option<EndCut>| -> Result<Vec2> {
                let on = j.at + j.miter * o;
                match cut {
                    Some(c) => meet(on, d, &c).ok_or_else(|| invalid("A wall junction is parallel to the wall.")),
                    None => Ok(on),
                }
            };
            for (k, (c, pair)) in self.style.components.iter().zip(bounds.windows(2)).enumerate() {
                let (Some(&outer), Some(&inner)) = (pair.first(), pair.get(1)) else { continue };
                if c.thickness <= 0.0 {
                    continue;
                }
                let start = cut_of(first, self.joints.start, &self.joints.start_layers, k);
                let end = cut_of(last, self.joints.end, &self.joints.end_layers, k);
                let q = vec![corner(a, outer, start)?, corner(b, outer, end)?, corner(b, inner, end)?, corner(a, inner, start)?];
                // A cut that turns the segment inside out is not a junction this wall can take.
                let s0 = (q[1] - q[0]).dot(d);
                let s1 = (q[2] - q[3]).dot(d);
                if s0 <= 0.0 || s1 <= 0.0 {
                    return Err(invalid("A wall junction leaves a segment with no length."));
                }
                out.push((k, ccw(q)));
            }
        }
        Ok(out)
    }

    /// The geometry of a wall that meets others: hatches of the cut components, lines where the
    /// material changes.
    pub(super) fn generate_jointed(&self, joins: &[Join]) -> Result<Vec<WallPiece>> {
        let tol = 1e-7 * self.thickness().max(1.0);
        let foreign: Vec<(Vec<Vec2>, u16)> = self.joints.foreign.iter().map(|b| (ccw(b.corners.clone()), b.priority)).collect();
        let yields: Vec<bool> = self.joints.foreign.iter().map(|b| b.yields).collect();
        let mut pieces: Vec<Piece> = Vec::new();
        for (k, quad) in self.bands(joins)? {
            let p = self.style.components.get(k).map_or(u16::MAX, |c| c.priority);
            let mut parts = vec![quad];
            for ((f, fp), y) in foreign.iter().zip(&yields) {
                if *fp < p || (*fp == p && *y) {
                    parts = parts.iter().flat_map(|q| subtract(q, f, tol)).collect();
                    if parts.len() > MAX_PIECES {
                        return Err(invalid("The walls meet in too complicated a way to draw."));
                    }
                }
            }
            pieces.extend(parts.into_iter().map(|corners| Piece { component: k, corners }));
        }
        let mut out = self.joint_lines(&pieces, &foreign, tol);
        out.extend(self.joint_hatches(&pieces));
        Ok(out)
    }

    fn material(&self, pieces: &[Piece], foreign: &[(Vec<Vec2>, u16)], q: Vec2, tol: f64) -> Material {
        if let Some(p) = pieces.iter().find(|p| strictly_inside(&p.corners, q, tol)) {
            return Material::Own(p.component);
        }
        foreign.iter().filter(|(f, _)| strictly_inside(f, q, tol)).map(|(_, p)| *p).min().map_or(Material::Nothing, Material::Foreign)
    }

    fn joint_lines(&self, pieces: &[Piece], foreign: &[(Vec<Vec2>, u16)], tol: f64) -> Vec<WallPiece> {
        let probe = 1e-4 * self.thickness().max(1e-6);
        let mut out = Vec::new();
        for piece in pieces {
            let mine = self.style.components.get(piece.component).map_or(u16::MAX, |c| c.priority);
            let n = piece.corners.len();
            for i in 0..n {
                let (a, b) = (piece.corners[i], piece.corners[(i + 1) % n]);
                let len = a.dist(b);
                if len <= tol {
                    continue;
                }
                // Where the edge may change what is beyond it.
                let mut ts = vec![0.0, 1.0];
                for other in pieces.iter().map(|p| &p.corners).chain(foreign.iter().map(|(f, _)| f)) {
                    let m = other.len();
                    for j in 0..m {
                        crossings(a, b, other[j], other[(j + 1) % m], tol, &mut ts);
                    }
                }
                ts.retain(|t| t.is_finite() && (0.0..=1.0).contains(t));
                ts.sort_by(f64::total_cmp);
                ts.dedup_by(|x, y| (*x - *y).abs() * len <= tol);
                // Counterclockwise corners: the outside is on the right.
                let out_n = (b - a).perp() * (-1.0 / len);
                let mut run: Option<(Vec2, Vec2)> = None;
                for w in ts.windows(2) {
                    let (t0, t1) = (w[0], w[1]);
                    let mid = a + (b - a) * ((t0 + t1) / 2.0);
                    let draw = match self.material(pieces, foreign, mid + out_n * probe, tol) {
                        Material::Nothing => true,
                        Material::Own(k) => k != piece.component && piece.component < k,
                        Material::Foreign(p) => p > mine,
                    };
                    let (p0, p1) = (a + (b - a) * t0, a + (b - a) * t1);
                    match (draw, run) {
                        (true, Some((s, _))) => run = Some((s, p1)),
                        (true, None) => run = Some((p0, p1)),
                        (false, Some((s, e))) => {
                            out.push(line(s, e));
                            run = None;
                        }
                        (false, None) => {}
                    }
                }
                if let Some((s, e)) = run {
                    out.push(line(s, e));
                }
            }
        }
        out
    }

    fn joint_hatches(&self, pieces: &[Piece]) -> Vec<WallPiece> {
        let mut out = Vec::new();
        for (k, c) in self.style.components.iter().enumerate() {
            let pattern = c.pattern.trim();
            if pattern.is_empty() || c.thickness <= 0.0 || matches!(c.function, ComponentFunction::Membrane | ComponentFunction::AirGap) {
                continue;
            }
            let loops: Vec<HatchLoop> = pieces
                .iter()
                .filter(|p| p.component == k)
                .map(|p| HatchLoop { vertices: p.corners.iter().copied().map(PolyVertex::new).collect(), outer: true, sources: Vec::new() })
                .collect();
            if loops.is_empty() {
                continue;
            }
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
            out.push(WallPiece { layer: WALL_PATTERN_LAYER, color: c.color, kind: EntityKind::Hatch(hatch) });
        }
        out
    }
}

fn line(a: Vec2, b: Vec2) -> WallPiece {
    WallPiece { layer: WALL_LAYER, color: Color::ByLayer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) }
}

// =====================================================================================
// Working the junctions out
// =====================================================================================

/// A wall placed in its space: block coordinates to drawing coordinates and back.
#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub insert: Vec2,
    pub rotation: f64,
    /// ±1 on each axis (a mirrored wall).
    pub scale: Vec2,
}

impl Placement {
    pub fn to_world(self, p: Vec2) -> Vec2 {
        let (s, c) = self.rotation.sin_cos();
        let (x, y) = (p.x * self.scale.x, p.y * self.scale.y);
        self.insert + Vec2::new(x * c - y * s, x * s + y * c)
    }
    pub fn to_local(self, p: Vec2) -> Vec2 {
        let (s, c) = self.rotation.sin_cos();
        let q = p - self.insert;
        let (x, y) = (q.x * c + q.y * s, -q.x * s + q.y * c);
        Vec2::new(x / self.scale.x, y / self.scale.y)
    }
    fn dir_to_local(self, d: Vec2) -> Vec2 {
        self.to_local(self.insert + d) - self.to_local(self.insert)
    }
}

/// A wall taking part in a cleanup: its data and placement.
pub struct Placed {
    pub params: WallParams,
    pub at: Placement,
}

/// The smallest angle, in degrees, between a wall end and a wall it meets for them to join.
const MIN_JOIN_DEGREES: f64 = 15.0;

/// A segment of a wall in drawing coordinates, with the offsets of its two faces.
#[derive(Clone)]
struct Seg {
    a: Vec2,
    b: Vec2,
    faces: (f64, f64),
    /// Each component's two offsets (lower first) and priority.
    layers: Vec<(f64, f64, u16)>,
    /// Whether `a` and `b` are free ends of their wall.
    free: (bool, bool),
}

fn segs(w: &Placed) -> Vec<Seg> {
    let bounds = w.params.boundaries();
    let lo = bounds.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = bounds.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    // A mirrored placement turns the left of the path to the right.
    let flip = if w.at.scale.x * w.at.scale.y < 0.0 { -1.0 } else { 1.0 };
    let faces = if flip > 0.0 { (lo, hi) } else { (-hi, -lo) };
    let layers: Vec<(f64, f64, u16)> = w
        .params
        .style
        .components
        .iter()
        .zip(bounds.windows(2))
        .filter_map(|(c, pair)| {
            let (o0, o1) = (*pair.first()? * flip, *pair.get(1)? * flip);
            Some((o0.min(o1), o0.max(o1), c.priority))
        })
        .collect();
    let p = &w.params.path;
    let n = p.len();
    let count = if w.params.closed { n } else { n.saturating_sub(1) };
    let open = !w.params.closed;
    (0..count)
        .filter_map(|i| {
            Some(Seg {
                a: w.at.to_world(*p.get(i)?),
                b: w.at.to_world(*p.get((i + 1) % n)?),
                faces,
                layers: layers.clone(),
                free: (open && i == 0, open && i + 1 == count),
            })
        })
        .collect()
}

/// The wall segment a free end meets, and how.
struct Hit<'a> {
    cut: EndCut,
    seg: &'a Seg,
    /// Where the end is across the segment, and the cosine between the end's direction and the
    /// segment's normal.
    h: f64,
    across: f64,
    /// When the end meets a free end of the other wall (an L corner): the direction that wall
    /// leaves the corner along.
    corner: Option<Vec2>,
}

/// The cut a free end at `p`, leaving its wall along `d` (unit, drawing coordinates), takes from
/// the walls it meets: the far face of the nearest wall whose band it ends in or near.
fn end_cut<'a>(p: Vec2, d: Vec2, reach: f64, others: &'a [Vec<Seg>]) -> Option<Hit<'a>> {
    let min_sin = MIN_JOIN_DEGREES.to_radians().sin();
    let mut best: Option<(f64, Hit<'a>)> = None;
    for s in others.iter().flatten() {
        let len = s.a.dist(s.b);
        if len <= 0.0 {
            continue;
        }
        let u = (s.b - s.a) / len;
        let nv = u.perp();
        let across = d.dot(nv);
        if across.abs() < min_sin {
            continue;
        }
        let h = (p - s.a).dot(nv);
        let t = (p - s.a).dot(u);
        let (lo, hi) = s.faces;
        let tol = reach.max(hi - lo);
        if h < lo - tol || h > hi + tol || t < -tol || t > len + tol {
            continue;
        }
        // The face beyond, along the end's direction.
        let far = [lo, hi].into_iter().map(|o| ((o - h) / across, o)).max_by(|x, y| x.0.total_cmp(&y.0));
        let Some((dist, o)) = far else { continue };
        let corner = if s.free.0 && t.abs() <= tol {
            Some(u)
        } else if s.free.1 && (t - len).abs() <= tol {
            Some(-u)
        } else {
            None
        };
        // An end already on the far face still meets a corner (its layers may need cutting).
        let touching = corner.is_some() && dist > -1e-9 * (reach + (hi - lo)).max(1.0);
        if (dist <= 0.0 && !touching) || dist > 2.0 * (reach + (hi - lo)) {
            continue;
        }
        let cut = EndCut { point: s.a + nv * o, dir: u };
        if best.as_ref().is_none_or(|(bd, _)| dist < *bd) {
            best = Some((dist, Hit { cut, seg: s, h, across, corner }));
        }
    }
    best.map(|(_, c)| c)
}

/// At an L corner, where each component of the ending wall stops: on the far edge of the other
/// wall's component of the same priority that matches it, counting from the outside of the
/// corner (the outermost layer meets the other wall's farthest one, the innermost its nearest),
/// so the layers wrap around the corner like nested Ls. A component with no match stops on the
/// wall's cut. `layers` are the ending wall's components at the end: where each one's middle
/// is, and its priority (`None` for one with no thickness).
fn layer_cuts(hit: &Hit, layers: &[(Option<Vec2>, u16)]) -> Vec<EndCut> {
    let Some(away) = hit.corner else { return Vec::new() };
    let s = hit.seg;
    let len = s.a.dist(s.b);
    if len <= 0.0 {
        return Vec::new();
    }
    let u = (s.b - s.a) / len;
    let nv = u.perp();
    // The other wall's components by priority, nearest first along `d`, as the far edge's offset.
    let far_edge = |(o0, o1): (f64, f64)| -> (f64, f64) {
        [o0, o1].into_iter().map(|o| ((o - hit.h) / hit.across, o)).fold((f64::NEG_INFINITY, o0), |m, x| if x.0 > m.0 { x } else { m })
    };
    let mut out = Vec::with_capacity(layers.len());
    for (k, (mid, pri)) in layers.iter().enumerate() {
        let Some(mid) = mid else {
            out.push(hit.cut);
            continue;
        };
        // How far inside the corner this layer is, and its rank among the ending wall's
        // layers of the same priority, innermost first.
        let inner = mid.dot(away);
        let rank = layers.iter().enumerate().filter(|(j, (m, p))| *j != k && p == pri && m.is_some_and(|m| m.dot(away) > inner)).count();
        let mut theirs: Vec<(f64, f64)> = s.layers.iter().filter(|(o0, o1, p)| p == pri && o1 > o0).map(|(o0, o1, _)| far_edge((*o0, *o1))).collect();
        theirs.sort_by(|x, y| x.0.total_cmp(&y.0));
        let cut = match theirs.get(rank).or(theirs.last()) {
            Some((_, o)) if o.is_finite() => EndCut { point: s.a + nv * *o, dir: u },
            _ => hit.cut,
        };
        out.push(cut);
    }
    out
}

fn bbox(p: &[Vec2]) -> (Vec2, Vec2) {
    p.iter().fold((Vec2::new(f64::INFINITY, f64::INFINITY), Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY)), |(lo, hi), q| {
        (Vec2::new(lo.x.min(q.x), lo.y.min(q.y)), Vec2::new(hi.x.max(q.x), hi.y.max(q.y)))
    })
}

fn overlaps(a: &(Vec2, Vec2), b: &(Vec2, Vec2), tol: f64) -> bool {
    a.0.x <= b.1.x + tol && b.0.x <= a.1.x + tol && a.0.y <= b.1.y + tol && b.0.y <= a.1.y + tol
}

/// The junctions of every wall in `walls` with the others: each wall's new joints, in the
/// order given. A wall that can't take a junction keeps none at that end.
pub fn work_out_joints(walls: &[Placed]) -> Vec<WallJoints> {
    let all_segs: Vec<Vec<Seg>> = walls.iter().map(segs).collect();
    // 1. The end cuts.
    type Cut = Option<(EndCut, Vec<EndCut>)>;
    let mut cuts: Vec<(Cut, Cut)> = Vec::with_capacity(walls.len());
    for (i, w) in walls.iter().enumerate() {
        let others: Vec<Vec<Seg>> = all_segs.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, s)| segs_clone(s)).collect();
        let p = &w.params.path;
        let reach = w.params.thickness();
        let bounds = w.params.boundaries();
        let local = |c: EndCut| EndCut { point: w.at.to_local(c.point), dir: w.at.dir_to_local(c.dir) };
        // `along`: the direction of the segment the end is on, in the block's coordinates.
        let cut_at = |at: Option<Vec2>, toward: Option<Vec2>, along: f64| -> Cut {
            let (at, toward) = (at?, toward?);
            let (pw, tw) = (w.at.to_world(at), w.at.to_world(toward));
            let len = pw.dist(tw);
            let llen = at.dist(toward);
            if len <= 0.0 || llen <= 0.0 {
                return None;
            }
            let hit = end_cut(pw, (pw - tw) / len, reach, &others)?;
            let normal = ((toward - at) * (along / llen)).perp();
            let layers: Vec<(Option<Vec2>, u16)> = w
                .params
                .style
                .components
                .iter()
                .zip(bounds.windows(2))
                .map(|(c, pair)| match (pair.first(), pair.get(1)) {
                    (Some(o0), Some(o1)) if c.thickness > 0.0 => (Some(w.at.to_world(at + normal * ((o0 + o1) / 2.0)) - pw), c.priority),
                    _ => (None, c.priority),
                })
                .collect();
            Some((local(hit.cut), layer_cuts(&hit, &layers).into_iter().map(local).collect()))
        };
        if w.params.closed {
            cuts.push((None, None));
        } else {
            let start = cut_at(p.first().copied(), p.get(1).copied(), 1.0);
            let end = cut_at(p.last().copied(), p.len().checked_sub(2).and_then(|k| p.get(k)).copied(), -1.0);
            cuts.push((start, end));
        }
    }
    // 2. Each wall's component outlines with those cuts, in drawing coordinates. A cut the wall
    //    can't take is dropped.
    let mut world_bands: Vec<Vec<(Vec<Vec2>, u16)>> = Vec::with_capacity(walls.len());
    for (w, (start, end)) in walls.iter().zip(cuts.iter_mut()) {
        let bands = |s: &Cut, e: &Cut| {
            let mut p = w.params.clone();
            p.joints = WallJoints {
                start: s.as_ref().map(|c| c.0),
                end: e.as_ref().map(|c| c.0),
                start_layers: s.as_ref().map(|c| c.1.clone()).unwrap_or_default(),
                end_layers: e.as_ref().map(|c| c.1.clone()).unwrap_or_default(),
                foreign: Vec::new(),
            };
            let joins = super::joins(&p.path, p.closed, p.boundaries().iter().fold(0.0_f64, |m, b| m.max(b.abs()))).ok()?;
            p.bands(&joins).ok()
        };
        let plain = |c: &Cut| c.as_ref().map(|c| (c.0, Vec::new()));
        let got = bands(start, end)
            .or_else(|| {
                // Without the per-layer cuts.
                *start = plain(start);
                *end = plain(end);
                bands(start, end)
            })
            .or_else(|| {
                *start = None;
                *end = None;
                bands(start, end)
            });
        let pri = |k: usize| w.params.style.components.get(k).map_or(u16::MAX, |c| c.priority);
        world_bands.push(got.unwrap_or_default().into_iter().map(|(k, q)| (q.iter().map(|p| w.at.to_world(*p)).collect(), pri(k))).collect());
    }
    // 3. The other walls' outlines that touch each wall's.
    let boxes: Vec<Vec<(Vec2, Vec2)>> = world_bands.iter().map(|bs| bs.iter().map(|(q, _)| bbox(q)).collect()).collect();
    let mut out = Vec::with_capacity(walls.len());
    for (i, w) in walls.iter().enumerate() {
        let tol = 1e-6 * w.params.thickness().max(1.0);
        let mut foreign = Vec::new();
        for (j, bands) in world_bands.iter().enumerate() {
            if j == i {
                continue;
            }
            for (q, (corners, pri)) in bands.iter().enumerate() {
                let Some(b) = boxes.get(j).and_then(|v| v.get(q)) else { continue };
                if boxes.get(i).is_some_and(|mine| mine.iter().any(|m| overlaps(m, b, tol))) && foreign.len() < MAX_FOREIGN_BANDS {
                    foreign.push(ForeignBand { corners: corners.iter().map(|p| w.at.to_local(*p)).collect(), priority: *pri, yields: j < i });
                }
            }
        }
        let (start, end) = cuts.get(i).cloned().unwrap_or((None, None));
        let (start, start_layers) = start.map_or((None, Vec::new()), |(c, l)| (Some(c), l));
        let (end, end_layers) = end.map_or((None, Vec::new()), |(c, l)| (Some(c), l));
        out.push(WallJoints { start, end, start_layers, end_layers, foreign });
    }
    out
}

fn segs_clone(s: &[Seg]) -> Vec<Seg> {
    s.to_vec()
}

#[cfg(test)]
mod tests;
