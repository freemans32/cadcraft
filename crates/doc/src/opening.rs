//! Openings in walls: doors, double doors, windows and bays. An opening is a block reference of its
//! own, drawn across the wall it is in (its jambs, the leaf and arc of a door, the glazing of a
//! window), with its parameters in extended data. It sits on the wall's path, turned along it. The
//! wall under it is cut away when the walls are joined ([`crate::Drawing::cleanup_walls`]), so an
//! opening can be moved along the wall, copied or erased like any object, and the wall follows.
//!
//! The sizes are in millimetres; the height and the sill are kept for elevations and exports (a
//! plan shows only the width).

use cadcraft_geom::{PolyVertex, Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::ventilation::{HvacPiece, xdata};
use crate::{DocError, EntityKind, Line, LwPolyline, RawTag, Result, WallUnits};

/// The registered application name of an opening's extended data.
pub const OPENING_APP: &str = "CADCRAFT_OPENING";
/// The layers of doors and bays, and of windows.
pub const DOOR_LAYER: &str = "A-DOOR";
pub const WINDOW_LAYER: &str = "A-GLAZ";
/// Largest side of an opening, mm.
pub const MAX_OPENING_MM: f64 = 20_000.0;
const XDATA_FORMAT: &str = "OPENING1";
/// How thick the drawn leaf of a door is, mm.
const LEAF_MM: f64 = 40.0;

fn invalid(m: impl Into<String>) -> DocError {
    DocError::Invalid(m.into())
}

fn v3(p: Vec2) -> Vec3 {
    Vec3::new(p.x, p.y, 0.0)
}

/// What an opening is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpeningKind {
    /// A single-leaf door.
    #[default]
    Door,
    DoubleDoor,
    Window,
    /// A bay: an opening with no joinery.
    Bay,
}

impl OpeningKind {
    pub const ALL: [OpeningKind; 4] = [OpeningKind::Door, OpeningKind::DoubleDoor, OpeningKind::Window, OpeningKind::Bay];

    pub fn parse(text: &str) -> Option<OpeningKind> {
        match text.trim().to_ascii_lowercase().as_str() {
            "door" | "porte" | "d" => Some(OpeningKind::Door),
            "doubledoor" | "double" | "portedouble" | "dd" => Some(OpeningKind::DoubleDoor),
            "window" | "fenetre" | "fenêtre" | "w" => Some(OpeningKind::Window),
            "bay" | "baie" | "opening" | "b" => Some(OpeningKind::Bay),
            _ => None,
        }
    }

    /// The name a command takes.
    pub fn name(self) -> &'static str {
        match self {
            OpeningKind::Door => "door",
            OpeningKind::DoubleDoor => "doubleDoor",
            OpeningKind::Window => "window",
            OpeningKind::Bay => "bay",
        }
    }

    /// Width, height and sill a new opening of the kind starts with, mm.
    pub fn default_size_mm(self) -> (f64, f64, f64) {
        match self {
            OpeningKind::Door => (900.0, 2100.0, 0.0),
            OpeningKind::DoubleDoor => (1400.0, 2100.0, 0.0),
            OpeningKind::Window => (1200.0, 1200.0, 900.0),
            OpeningKind::Bay => (1000.0, 2100.0, 0.0),
        }
    }
}

/// How an opening is drawn in plan: its joinery, with the same kind (door, window...) and sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpeningStyle {
    /// The swing of a door, the glazing lines of a window.
    #[default]
    Standard,
    /// Two leaves sliding past each other (a sliding door or window, an elevator door opening to one side).
    Sliding,
    /// A leaf sliding into the thickness of the wall.
    Pocket,
    /// A folding door: a zigzag across the opening.
    Folding,
    /// A tilting (garage) door: a leaf across the opening, crossed.
    Tilting,
    /// Two leaves parting at the centre (an elevator door).
    CentreParting,
    /// A revolving door: a circle with its four wings.
    Revolving,
    /// A window with a leaf swinging on a hinge.
    Casement,
    /// A window with two leaves swinging.
    DoubleCasement,
    /// A window that does not open.
    Fixed,
    /// A roof window: the whole thickness of the wall, crossed.
    Skylight,
}

impl OpeningStyle {
    pub const ALL: [OpeningStyle; 11] = [
        OpeningStyle::Standard,
        OpeningStyle::Sliding,
        OpeningStyle::Pocket,
        OpeningStyle::Folding,
        OpeningStyle::Tilting,
        OpeningStyle::CentreParting,
        OpeningStyle::Revolving,
        OpeningStyle::Casement,
        OpeningStyle::DoubleCasement,
        OpeningStyle::Fixed,
        OpeningStyle::Skylight,
    ];

    /// The name a command takes.
    pub fn name(self) -> &'static str {
        match self {
            OpeningStyle::Standard => "standard",
            OpeningStyle::Sliding => "sliding",
            OpeningStyle::Pocket => "pocket",
            OpeningStyle::Folding => "folding",
            OpeningStyle::Tilting => "tilting",
            OpeningStyle::CentreParting => "centreParting",
            OpeningStyle::Revolving => "revolving",
            OpeningStyle::Casement => "casement",
            OpeningStyle::DoubleCasement => "doubleCasement",
            OpeningStyle::Fixed => "fixed",
            OpeningStyle::Skylight => "skylight",
        }
    }

    pub fn parse(text: &str) -> Option<OpeningStyle> {
        let t = text.trim().to_ascii_lowercase();
        OpeningStyle::ALL.into_iter().find(|s| s.name().to_ascii_lowercase() == t)
    }
}

/// An opening: what its block reference keeps. The block's origin is the opening's centre on the
/// wall's path; its x axis runs along the wall and its y axis to the left of the wall's direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningParams {
    pub kind: OpeningKind,
    /// How it is drawn; absent in drawings made before styles existed (the standard symbol).
    #[serde(default)]
    pub style: OpeningStyle,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Height of the sill above the floor, mm.
    pub sill_mm: f64,
    /// Whether a door opens to the right of the wall's direction instead of the left.
    #[serde(default)]
    pub flip_side: bool,
    /// Whether the hinge of a single door is at the end of the opening instead of the start.
    #[serde(default)]
    pub flip_hand: bool,
    /// How far the wall's two faces are from its path, mm, to the left (positive) and to the right
    /// (negative): the symbol is drawn across them. Set from the wall when the walls are joined.
    #[serde(default = "default_left")]
    pub left_mm: f64,
    #[serde(default = "default_right")]
    pub right_mm: f64,
    #[serde(default)]
    pub units: WallUnits,
}

fn default_left() -> f64 {
    100.0
}

fn default_right() -> f64 {
    -100.0
}

impl OpeningParams {
    /// A new opening of `kind` at its default size, across a wall of 200 mm.
    pub fn new(kind: OpeningKind, units: WallUnits) -> OpeningParams {
        let (width_mm, height_mm, sill_mm) = kind.default_size_mm();
        OpeningParams {
            kind,
            style: OpeningStyle::Standard,
            width_mm,
            height_mm,
            sill_mm,
            flip_side: false,
            flip_hand: false,
            left_mm: 100.0,
            right_mm: -100.0,
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let size = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_OPENING_MM;
        if !size(self.width_mm) || !size(self.height_mm) {
            return Err(invalid(format!("An opening's width and height are more than 0 and at most {MAX_OPENING_MM} mm.")));
        }
        if !(self.sill_mm.is_finite() && (0.0..=MAX_OPENING_MM).contains(&self.sill_mm)) {
            return Err(invalid(format!("An opening's sill is 0 to {MAX_OPENING_MM} mm.")));
        }
        if !(self.left_mm.is_finite()
            && self.right_mm.is_finite()
            && self.left_mm > self.right_mm
            && self.left_mm.abs() <= 1e6
            && self.right_mm.abs() <= 1e6)
        {
            return Err(invalid("The wall faces of an opening are not usable."));
        }
        if !(self.units.mm_per_unit.is_finite() && self.units.mm_per_unit > 0.0 && self.units.mm_per_unit <= 1e6) {
            return Err(invalid("The drawing units are not usable."));
        }
        Ok(())
    }

    /// Width in drawing units.
    pub fn width(&self) -> f64 {
        self.units.to_units(self.width_mm)
    }

    /// The symbol across the wall, in the block's coordinates.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let u = |mm: f64| self.units.to_units(mm);
        let (hw, left, right) = (u(self.width_mm) / 2.0, u(self.left_mm), u(self.right_mm));
        let line = |layer: &'static str, a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let rect = |layer: &'static str, corners: [Vec2; 4]| HvacPiece {
            layer,
            kind: EntityKind::LwPolyline(LwPolyline {
                vertices: corners.iter().map(|p| PolyVertex::new(*p)).collect(),
                closed: true,
                const_width: 0.0,
                elevation: 0.0,
                plinegen: false,
            }),
        };
        let arc = |layer: &'static str, centre: Vec2, radius: f64, start: f64, end: f64| HvacPiece {
            layer,
            kind: EntityKind::Arc(crate::Arc { center: v3(centre), radius, start, end }),
        };
        let mut v = vec![line(DOOR_LAYER, Vec2::new(-hw, right), Vec2::new(-hw, left)), line(DOOR_LAYER, Vec2::new(hw, right), Vec2::new(hw, left))];
        let lay = if self.kind == OpeningKind::Window { WINDOW_LAYER } else { DOOR_LAYER };
        let (pi, half_pi) = (std::f64::consts::PI, std::f64::consts::FRAC_PI_2);
        // A leaf of `len` hinged at x = `hinge_x`, open to the side the door swings to: the leaf
        // across the wall, the arc of its swing back to the closed position.
        let leaf = |hinge_x: f64, len: f64, toward_right_end: bool| -> Vec<HvacPiece> {
            let y_face = if self.flip_side { right } else { left };
            let s = if self.flip_side { -1.0 } else { 1.0 };
            let half = u(LEAF_MM) / 2.0;
            let tip = y_face + s * len;
            let leaf = rect(
                lay,
                [
                    Vec2::new(hinge_x - half, y_face),
                    Vec2::new(hinge_x + half, y_face),
                    Vec2::new(hinge_x + half, tip),
                    Vec2::new(hinge_x - half, tip),
                ],
            );
            // The closed leaf lies along the wall, toward the far jamb: +x when the hinge is at the
            // start, −x when it is at the end.
            let (a0, a1) = match (toward_right_end, self.flip_side) {
                (true, false) => (0.0, half_pi),
                (true, true) => (3.0 * half_pi, 2.0 * pi),
                (false, false) => (half_pi, pi),
                (false, true) => (pi, 3.0 * half_pi),
            };
            vec![leaf, arc(lay, Vec2::new(hinge_x, y_face), len, a0, a1)]
        };
        let mid = (left + right) / 2.0;
        let gap = 0.15 * (left - right);
        let leaf_h = u(LEAF_MM) / 2.0;
        // A leaf across the opening between `x0` and `x1`, centred on `y`.
        let slab = |x0: f64, x1: f64, y: f64| {
            rect(lay, [Vec2::new(x0, y - leaf_h), Vec2::new(x1, y - leaf_h), Vec2::new(x1, y + leaf_h), Vec2::new(x0, y + leaf_h)])
        };
        match self.style {
            OpeningStyle::Standard => {}
            OpeningStyle::Sliding => {
                v.push(slab(-hw, 0.1 * hw, mid + gap));
                v.push(slab(-0.1 * hw, hw, mid - gap));
                return Ok(v);
            }
            OpeningStyle::Pocket => {
                // The leaf drawn half drawn into the wall, on the side of the hinge.
                let (x0, x1) = if self.flip_hand { (-2.0 * hw, 0.0) } else { (0.0, 2.0 * hw) };
                v.push(slab(x0, x1, mid));
                return Ok(v);
            }
            OpeningStyle::Folding => {
                for k in 0..4 {
                    let (x0, x1) = (-hw + f64::from(k) * hw / 2.0, -hw + f64::from(k + 1) * hw / 2.0);
                    let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                    v.push(line(lay, Vec2::new(x0, mid - sign * gap), Vec2::new(x1, mid + sign * gap)));
                }
                return Ok(v);
            }
            OpeningStyle::Tilting => {
                v.push(slab(-hw, hw, mid));
                v.push(line(lay, Vec2::new(-hw, mid - gap), Vec2::new(hw, mid + gap)));
                v.push(line(lay, Vec2::new(-hw, mid + gap), Vec2::new(hw, mid - gap)));
                return Ok(v);
            }
            OpeningStyle::CentreParting => {
                v.push(slab(-hw, 0.0, mid));
                v.push(slab(0.0, hw, mid));
                return Ok(v);
            }
            OpeningStyle::Revolving => {
                v.push(HvacPiece { layer: lay, kind: EntityKind::Circle(crate::Circle { center: v3(Vec2::new(0.0, mid)), radius: hw }) });
                v.push(line(lay, Vec2::new(-hw, mid), Vec2::new(hw, mid)));
                v.push(line(lay, Vec2::new(0.0, mid - hw), Vec2::new(0.0, mid + hw)));
                return Ok(v);
            }
            OpeningStyle::Fixed => {
                v.push(line(WINDOW_LAYER, Vec2::new(-hw, mid), Vec2::new(hw, mid)));
                return Ok(v);
            }
            OpeningStyle::Skylight => {
                v.push(line(WINDOW_LAYER, Vec2::new(-hw, left), Vec2::new(hw, right)));
                v.push(line(WINDOW_LAYER, Vec2::new(-hw, right), Vec2::new(hw, left)));
                return Ok(v);
            }
            OpeningStyle::Casement => {
                v.push(line(WINDOW_LAYER, Vec2::new(-hw, mid), Vec2::new(hw, mid)));
                let (hinge_x, toward_right_end) = if self.flip_hand { (hw, false) } else { (-hw, true) };
                v.extend(leaf(hinge_x, 2.0 * hw, toward_right_end));
                return Ok(v);
            }
            OpeningStyle::DoubleCasement => {
                v.push(line(WINDOW_LAYER, Vec2::new(-hw, mid), Vec2::new(hw, mid)));
                v.extend(leaf(-hw, hw, true));
                v.extend(leaf(hw, hw, false));
                return Ok(v);
            }
        }
        match self.kind {
            OpeningKind::Door => {
                // The hinge at the start of the opening by default, the leaf swinging toward the end.
                let (hinge_x, toward_right_end) = if self.flip_hand { (hw, false) } else { (-hw, true) };
                v.extend(leaf(hinge_x, 2.0 * hw, toward_right_end));
            }
            OpeningKind::DoubleDoor => {
                v.extend(leaf(-hw, hw, true));
                v.extend(leaf(hw, hw, false));
            }
            OpeningKind::Window => {
                // Three lines of glazing across the width, the middle one on the path's side of the wall's middle.
                for y in [mid - gap, mid, mid + gap] {
                    v.push(line(WINDOW_LAYER, Vec2::new(-hw, y), Vec2::new(hw, y)));
                }
            }
            OpeningKind::Bay => {}
        }
        Ok(v)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(OPENING_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<OpeningParams> {
        let p: OpeningParams = xdata::from_xdata(OPENING_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The opening an object is, when it is a block reference carrying usable opening data.
pub fn opening_of(e: &crate::Entity) -> Option<OpeningParams> {
    match &e.kind {
        EntityKind::Insert(_) => OpeningParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// Where a point meets a wall: the wall and its segment, the foot of the point on the wall's path,
/// the segment's direction and the distances along it, in drawing units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallHit {
    pub wall: crate::Handle,
    pub segment: usize,
    /// The foot of the point on the path.
    pub foot: Vec2,
    /// The segment's direction, radians.
    pub angle: f64,
    /// From the segment's start to the foot, and the segment's length.
    pub along: f64,
    pub length: f64,
    /// How far the wall's faces are from its path, mm, to the left (positive) and to the right.
    pub left_mm: f64,
    pub right_mm: f64,
    /// How far the point is from the path.
    pub off: f64,
}

impl crate::Drawing {
    /// The wall of model space nearest to `at` within `reach` drawing units of its path and no
    /// farther than its thickness, with where `at` falls on it; `None` when no wall is that near.
    pub fn wall_under(&self, at: Vec2, reach: f64) -> Option<WallHit> {
        let mut best: Option<WallHit> = None;
        for e in self.model.iter() {
            let (EntityKind::Insert(i), Some(w)) = (&e.kind, crate::wall_of(e)) else { continue };
            let place = crate::Placement {
                insert: Vec2::new(i.insert.x, i.insert.y),
                rotation: i.rotation,
                scale: Vec2::new(i.scale.x.signum(), i.scale.y.signum()),
            };
            let bounds = w.boundaries();
            let to_mm = |v: f64| v / w.units.to_units(1.0);
            let (Some(left), Some(right)) = (bounds.first().copied().map(to_mm), bounds.last().copied().map(to_mm)) else { continue };
            let wide = bounds.iter().fold(0.0_f64, |m, b| m.max(b.abs())).max(reach);
            let n = w.path.len();
            let segs = if w.closed { n } else { n.saturating_sub(1) };
            for k in 0..segs {
                let (Some(&p0), Some(&p1)) = (w.path.get(k), w.path.get((k + 1) % n)) else { continue };
                let (a, b) = (place.to_world(p0), place.to_world(p1));
                let length = a.dist(b);
                if length <= 1e-9 {
                    continue;
                }
                let d = (b - a) / length;
                let (t, off) = ((at - a).dot(d), (at - a).cross(d).abs());
                if off > wide || t < -wide || t > length + wide {
                    continue;
                }
                let t = t.clamp(0.0, length);
                if best.as_ref().is_none_or(|x| off < x.off) {
                    best = Some(WallHit {
                        wall: e.handle,
                        segment: k,
                        foot: a + d * t,
                        angle: d.angle(),
                        along: t,
                        length,
                        left_mm: left,
                        right_mm: right,
                        off,
                    });
                }
            }
        }
        best
    }

    /// Segment `segment` of wall `wall` as a [`WallHit`] at its start: the base for placing an
    /// opening by a distance along it.
    pub fn wall_segment(&self, wall: crate::Handle, segment: usize) -> Option<WallHit> {
        let e = self.entity(wall)?;
        let (EntityKind::Insert(i), Some(w)) = (&e.kind, crate::wall_of(e)) else { return None };
        let place = crate::Placement {
            insert: Vec2::new(i.insert.x, i.insert.y),
            rotation: i.rotation,
            scale: Vec2::new(i.scale.x.signum(), i.scale.y.signum()),
        };
        let bounds = w.boundaries();
        let to_mm = |v: f64| v / w.units.to_units(1.0);
        let (left, right) = (bounds.first().copied().map(to_mm)?, bounds.last().copied().map(to_mm)?);
        let n = w.path.len();
        if segment >= if w.closed { n } else { n.saturating_sub(1) } {
            return None;
        }
        let (a, b) = (place.to_world(*w.path.get(segment)?), place.to_world(*w.path.get((segment + 1) % n)?));
        let length = a.dist(b);
        (length > 1e-9).then(|| WallHit {
            wall,
            segment,
            foot: a,
            angle: (b - a).angle(),
            along: 0.0,
            length,
            left_mm: left,
            right_mm: right,
            off: 0.0,
        })
    }

    /// Where opening `h` lies in its wall, when it is in one.
    pub fn opening_host(&self, h: crate::Handle) -> Option<(OpeningParams, WallHit)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = opening_of(e)?;
        let hit = self.wall_under(Vec2::new(i.insert.x, i.insert.y), 1e-6)?;
        Some((p, hit))
    }

    /// Adds opening `p` with its centre at `at`, on a wall's path, turned `rotation` radians along it.
    pub fn add_opening(
        &mut self,
        space: &crate::Space,
        mut common: crate::Common,
        p: &OpeningParams,
        at: Vec2,
        rotation: f64,
    ) -> Result<crate::Handle> {
        if !at.is_finite() || !rotation.is_finite() {
            return Err(invalid("An opening needs a usable position and angle."));
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(OPENING_APP, &common.xdata, p.to_xdata()?);
        common.layer = if p.kind == OpeningKind::Window { WINDOW_LAYER } else { DOOR_LAYER }.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(DOOR_LAYER);
        self.ensure_layer(WINDOW_LAYER);
        let insert = crate::Insert { block, insert: v3(at), rotation, ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives opening `h` the parameters `p`, redrawing it in place.
    pub fn regenerate_opening(&mut self, h: crate::Handle, p: &OpeningParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(OPENING_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = if p.kind == OpeningKind::Window { WINDOW_LAYER } else { DOOR_LAYER };
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
