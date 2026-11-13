//! Risers (colonnes): where a network goes up or down through the floors. A riser is its shaft
//! outline (the trémie), a circle that shows the way (a dot: the air comes up to this level; a
//! cross: it goes down), a leader arrow and a label "name Ø200-240 m³/h". Each floor is its own
//! drawing, so the risers of two floors meet by their name. Like the other ventilation objects it
//! is a block reference, its parameters in extended data and its text written out for other programs.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

use cadcraft_geom::{PolyVertex, Vec2, polygon_centroid};
use serde::{Deserialize, Serialize};

use super::terminal::air_layer;
use super::{AirDirection, DUCT_DIAMETER_MM, HvacPiece, HvacUnits, check_label_height, check_name, invalid, v3, xdata};
use crate::{Circle, EntityKind, HAlign, Line, LwPolyline, RawTag, Result, Text, VAlign};

/// The registered application name of a riser's extended data.
pub const RISER_APP: &str = "CADCRAFT_RISER";
/// The layer of a riser's shaft outline; its symbol and text are on the layer of its air direction.
pub const RISER_LAYER: &str = "M-HVAC-RISER";
/// Most vertices of a riser's shaft outline.
pub const MAX_RISER_VERTICES: usize = 256;
/// Most flow a riser may carry, m³/h.
pub const MAX_RISER_FLOW_M3H: f64 = 1_000_000.0;
const XDATA_FORMAT: &str = "RISER1";

fn default_label_mm() -> f64 {
    100.0
}

fn default_up() -> bool {
    true
}

/// The section the riser is given at this level.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "camelCase")]
pub enum RiserSize {
    #[serde(rename_all = "camelCase")]
    Round { diameter_mm: f64 },
    #[serde(rename_all = "camelCase")]
    Rectangular { width_mm: f64, height_mm: f64 },
}

impl RiserSize {
    /// The width a plan shows, mm.
    pub fn width_mm(self) -> f64 {
        match self {
            RiserSize::Round { diameter_mm } => diameter_mm,
            RiserSize::Rectangular { width_mm, .. } => width_mm,
        }
    }

    /// The size as written on the plan: "Ø200" or "400×250".
    pub fn label(self) -> String {
        match self {
            RiserSize::Round { diameter_mm } => format!("Ø{diameter_mm:.0}"),
            RiserSize::Rectangular { width_mm, height_mm } => format!("{width_mm:.0}×{height_mm:.0}"),
        }
    }

    fn validate(self) -> Result<()> {
        let ok = |v: f64| v.is_finite() && (DUCT_DIAMETER_MM.0..=DUCT_DIAMETER_MM.1).contains(&v);
        let fits = match self {
            RiserSize::Round { diameter_mm } => ok(diameter_mm),
            RiserSize::Rectangular { width_mm, height_mm } => ok(width_mm) && ok(height_mm),
        };
        if fits { Ok(()) } else { Err(invalid(format!("A riser's size is {} to {} mm.", DUCT_DIAMETER_MM.0, DUCT_DIAMETER_MM.1))) }
    }
}

/// A riser: what its block reference keeps to regenerate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiserParams {
    /// The name the risers of the other floors share ("VMC 3").
    pub name: String,
    #[serde(default)]
    pub air: AirDirection,
    /// Whether the network goes up from this level (`true`) or down (`false`).
    #[serde(default = "default_up")]
    pub up: bool,
    /// The shaft outline in the block's coordinates (drawing units), without repeating the first point.
    pub outline: Vec<Vec2>,
    /// The flow that comes from the other floors, m³/h, as typed.
    #[serde(default)]
    pub incoming_flow_m3h: Option<f64>,
    /// The flow the riser carries at this level, m³/h, once it is set or sized.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    #[serde(default)]
    pub size: Option<RiserSize>,
    /// Whether the size was typed: sizing leaves it as it is.
    #[serde(default)]
    pub size_set: bool,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl RiserParams {
    pub fn validate(&self) -> Result<()> {
        check_name("riser", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if !(3..=MAX_RISER_VERTICES).contains(&self.outline.len()) || self.outline.iter().any(|p| !p.is_finite()) {
            return Err(invalid(format!("A riser's shaft outline has 3 to {MAX_RISER_VERTICES} points.")));
        }
        let (w, d) = self.shaft_mm();
        if !(w > 0.0 && d > 0.0) {
            return Err(invalid("A riser's shaft outline has no area."));
        }
        for f in [self.incoming_flow_m3h, self.flow_m3h].into_iter().flatten() {
            if !(f.is_finite() && f > 0.0 && f <= MAX_RISER_FLOW_M3H) {
                return Err(invalid(format!("A riser's flow is more than 0 and at most {MAX_RISER_FLOW_M3H} m³/h.")));
            }
        }
        if let Some(s) = self.size {
            s.validate()?;
        }
        Ok(())
    }

    /// The width and the depth of the shaft (its bounding box), mm.
    pub fn shaft_mm(&self) -> (f64, f64) {
        let (mut lo, mut hi) = (Vec2::new(f64::MAX, f64::MAX), Vec2::new(f64::MIN, f64::MIN));
        for p in &self.outline {
            (lo, hi) = (Vec2::new(lo.x.min(p.x), lo.y.min(p.y)), Vec2::new(hi.x.max(p.x), hi.y.max(p.y)));
        }
        let m = self.units.mm_per_unit;
        (((hi.x - lo.x) * m).max(0.0), ((hi.y - lo.y) * m).max(0.0))
    }

    /// Redraws the shaft as a rectangle of `width_mm` by `depth_mm` around the centre it has now.
    pub fn set_shaft_mm(&mut self, width_mm: f64, depth_mm: f64) -> Result<()> {
        let ok = |v: f64| v.is_finite() && (10.0..=1.0e6).contains(&v);
        if !(ok(width_mm) && ok(depth_mm)) {
            return Err(invalid("A shaft is 10 mm to 1000 m on each side."));
        }
        let c = self.centre();
        let (hx, hy) = (self.units.to_units(width_mm) / 2.0, self.units.to_units(depth_mm) / 2.0);
        self.outline = vec![c + Vec2::new(-hx, -hy), c + Vec2::new(hx, -hy), c + Vec2::new(hx, hy), c + Vec2::new(-hx, hy)];
        Ok(())
    }

    fn centre(&self) -> Vec2 {
        polygon_centroid(&self.outline).or_else(|| self.outline.first().copied()).unwrap_or(Vec2::ZERO)
    }

    /// The text written at the end of the arrow, readable by any program: "VMC 3 Ø200-240 m³/h".
    pub fn text(&self) -> String {
        let size = self.size.map(RiserSize::label);
        let flow = self.flow_m3h.or(self.incoming_flow_m3h).map(|f| format!("{f:.0}"));
        let mut t = self.name.clone();
        match (size, flow) {
            (Some(s), Some(f)) => t.push_str(&format!(" {s}-{f} m³/h")),
            (Some(s), None) => t.push_str(&format!(" {s}")),
            (None, Some(f)) => t.push_str(&format!(" {f} m³/h")),
            (None, None) => {}
        }
        t
    }

    /// The shaft outline, the symbol with its leader arrow, and the text.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let h = self.units.to_units(self.label_height_mm);
        let vertices: Vec<PolyVertex> = self.outline.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect();
        let outline = LwPolyline { vertices, closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
        let c = self.centre();
        let (w, d) = self.shaft_mm();
        let small = self.units.to_units(w.min(d));
        // The symbol is the riser's section: a circle of its diameter, a rectangle of its two sides, else
        // a circle of a quarter of the shaft.
        let r = self.size.map_or(0.25 * small, |s| self.units.to_units(s.width_mm() / 2.0)).max(0.8 * h);
        let rect_half = match self.size {
            Some(RiserSize::Rectangular { width_mm, height_mm }) => {
                Some(Vec2::new(self.units.to_units(width_mm / 2.0).max(0.8 * h), self.units.to_units(height_mm / 2.0).max(0.8 * h)))
            }
            _ => None,
        };
        // The dot or the cross inside is sized to the nearer side; the leader leaves at 45° from the edge.
        let (inner, edge) = match rect_half {
            Some(hf) => (hf.x.min(hf.y), hf.x.min(hf.y) * std::f64::consts::SQRT_2),
            None => (r, r),
        };
        let layer = air_layer(self.air);
        let line = |a: Vec2, b: Vec2| HvacPiece { layer, kind: EntityKind::Line(Line { a: v3(a), b: v3(b) }) };
        let circle = |radius: f64| HvacPiece { layer, kind: EntityKind::Circle(Circle { center: v3(c), radius }) };
        let symbol = match rect_half {
            Some(hf) => {
                let corners = [Vec2::new(-hf.x, -hf.y), Vec2::new(hf.x, -hf.y), Vec2::new(hf.x, hf.y), Vec2::new(-hf.x, hf.y)];
                let poly = LwPolyline {
                    vertices: corners.iter().map(|k| PolyVertex { p: c + *k, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
                    closed: true,
                    const_width: 0.0,
                    elevation: 0.0,
                    plinegen: false,
                };
                HvacPiece { layer, kind: EntityKind::LwPolyline(poly) }
            }
            None => circle(r),
        };
        let mut pieces = vec![HvacPiece { layer: RISER_LAYER, kind: EntityKind::LwPolyline(outline) }, symbol];
        if self.up {
            pieces.push(circle(0.18 * inner));
        } else {
            let k = inner * FRAC_1_SQRT_2;
            pieces.push(line(c + Vec2::new(-k, -k), c + Vec2::new(k, k)));
            pieces.push(line(c + Vec2::new(-k, k), c + Vec2::new(k, -k)));
        }
        // The leader leaves the circle at 45° and ends in an arrow head, where the text begins.
        let dir = Vec2::new(FRAC_1_SQRT_2, FRAC_1_SQRT_2);
        let (start, len) = (c + dir * edge, self.units.to_units(0.6 * (w * w + d * d).sqrt()).max(6.0 * h));
        let end = start + dir * len;
        let (back, side, head) = (-dir, dir.perp(), 1.2 * h);
        pieces.push(line(start, end));
        pieces.push(line(end, end + back * head + side * (0.4 * head)));
        pieces.push(line(end, end + back * head - side * (0.4 * head)));
        let at = end + dir * (0.6 * h);
        let text = Text {
            insert: v3(at),
            align_pt: Some(v3(at)),
            height: h,
            value: self.text(),
            rotation: FRAC_PI_4,
            width_factor: 1.0,
            oblique: 0.0,
            style: "Standard".into(),
            halign: HAlign::Left,
            valign: VAlign::Middle,
        };
        pieces.push(HvacPiece { layer, kind: EntityKind::Text(text) });
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(RISER_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<RiserParams> {
        let p: RiserParams = xdata::from_xdata(RISER_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The riser an object is, when it is a block reference carrying usable riser data.
pub fn riser_of(e: &crate::Entity) -> Option<RiserParams> {
    match &e.kind {
        EntityKind::Insert(_) => RiserParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds riser `p`, its shaft outline given in drawing coordinates: an anonymous block with the
    /// outline, symbol and text, referenced on [`RISER_LAYER`] at the outline's first point.
    pub fn add_riser(&mut self, space: &crate::Space, mut common: crate::Common, mut p: RiserParams) -> Result<crate::Handle> {
        let origin = p.outline.first().copied().ok_or_else(|| invalid("A riser needs a shaft outline."))?;
        for v in &mut p.outline {
            *v -= origin;
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(RISER_APP, &common.xdata, p.to_xdata()?);
        common.layer = RISER_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(RISER_LAYER);
        let insert = crate::Insert { block, insert: v3(origin), ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives riser `h` the parameters `p` (outline in the block's coordinates), redrawing it in place.
    pub fn regenerate_riser(&mut self, h: crate::Handle, p: &RiserParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(RISER_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }
}
