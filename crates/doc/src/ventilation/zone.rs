//! Height zones: an outline, hatched, where the height available for ducts is less than elsewhere
//! (under a beam, a drop, a low false ceiling) and, if the user gives it, the altitude of the
//! underside of the ducts there. A duct that crosses a zone is cut at its edges and sized flat
//! inside it. A zone is a block reference like the other ventilation objects, its parameters in
//! extended data, and also written out as text in the drawing, so another program shows them.

use cadcraft_geom::{PolyVertex, Vec2, polygon_centroid};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, v3, xdata};
use crate::{EntityKind, Hatch, HatchLoop, LwPolyline, RawTag, Result};

/// The registered application name of a zone's extended data.
pub const ZONE_APP: &str = "CADCRAFT_VOIDZONE";
/// The layer of a zone: its outline, hatching and text.
pub const ZONE_LAYER: &str = "M-HVAC-ZONE";
/// Most vertices of a zone's outline.
pub const MAX_ZONE_VERTICES: usize = 256;
const XDATA_FORMAT: &str = "ZONE1";

fn default_label_mm() -> f64 {
    100.0
}

/// A zone: what its block reference keeps to regenerate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoneParams {
    pub name: String,
    /// The height available for ducts inside the zone, mm.
    pub void_height_mm: f64,
    /// The altitude of the underside of the ducts inside the zone, mm above the drawing's zero, when
    /// the user gives it: written on the ducts' labels (the plan stays two-dimensional).
    #[serde(default)]
    pub bottom_mm: Option<f64>,
    /// The closed outline in the block's coordinates (drawing units), without repeating the first point.
    pub outline: Vec<Vec2>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl ZoneParams {
    pub fn validate(&self) -> Result<()> {
        check_name("zone", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if !(self.void_height_mm.is_finite() && (20.0..=super::MAX_VOID_MM).contains(&self.void_height_mm)) {
            return Err(invalid(format!("The height available is 20 to {} mm.", super::MAX_VOID_MM)));
        }
        if self.bottom_mm.is_some_and(|b| !(b.is_finite() && (-100_000.0..=1_000_000.0).contains(&b))) {
            return Err(invalid("The altitude of the underside is a number of millimetres."));
        }
        if !(3..=MAX_ZONE_VERTICES).contains(&self.outline.len()) || self.outline.iter().any(|p| !p.is_finite()) {
            return Err(invalid(format!("A zone's outline has 3 to {MAX_ZONE_VERTICES} points.")));
        }
        Ok(())
    }

    /// The text written in the zone, readable by any program.
    pub fn text(&self) -> String {
        let mut t = format!("{} H={:.0}", self.name, self.void_height_mm);
        if let Some(b) = self.bottom_mm {
            t.push_str(&format!(" Ai:{:.2}", b / 1000.0));
        }
        t
    }

    /// The outline, its hatching and its text.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let vertices: Vec<PolyVertex> = self.outline.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect();
        let outline = LwPolyline { vertices: vertices.clone(), closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
        // Diagonal hatching about 150 mm apart (the pattern's lines are 0.125 apart at scale 1).
        let hatch = Hatch {
            pattern: "ANSI31".into(),
            solid: false,
            loops: vec![HatchLoop { vertices, outer: true, sources: Vec::new() }],
            scale: self.units.to_units(150.0) / 0.125,
            angle: 0.0,
            associative: false,
            style: 0,
            elevation: 0.0,
            gradient: None,
            origin: Vec2::ZERO,
            background: None,
            pattern_lines: Vec::new(),
        };
        let h = self.units.to_units(self.label_height_mm);
        // The text near the top edge, so it does not sit on the ducts that cross the middle.
        let centre = polygon_centroid(&self.outline).or_else(|| self.outline.first().copied()).unwrap_or(Vec2::ZERO);
        let top = self.outline.iter().map(|p| p.y).fold(f64::MIN, f64::max);
        let at = Vec2::new(centre.x, top - 1.5 * h);
        Ok(vec![
            HvacPiece { layer: ZONE_LAYER, kind: EntityKind::Hatch(hatch) },
            HvacPiece { layer: ZONE_LAYER, kind: EntityKind::LwPolyline(outline) },
            HvacPiece { layer: ZONE_LAYER, kind: label(at, h, self.text()) },
        ])
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(ZONE_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<ZoneParams> {
        let p: ZoneParams = xdata::from_xdata(ZONE_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The zone an object is, when it is a block reference carrying usable zone data.
pub fn zone_of(e: &crate::Entity) -> Option<ZoneParams> {
    match &e.kind {
        EntityKind::Insert(_) => ZoneParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds zone `p`, its outline given in drawing coordinates: an anonymous block with the outline,
    /// hatching and text, referenced on [`ZONE_LAYER`] at the outline's first point.
    pub fn add_zone(&mut self, space: &crate::Space, mut common: crate::Common, mut p: ZoneParams) -> Result<crate::Handle> {
        let origin = p.outline.first().copied().ok_or_else(|| invalid("A zone needs an outline."))?;
        for v in &mut p.outline {
            *v -= origin;
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(ZONE_APP, &common.xdata, p.to_xdata()?);
        common.layer = ZONE_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(ZONE_LAYER);
        let insert = crate::Insert { block, insert: v3(origin), ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives zone `h` the parameters `p` (outline in the block's coordinates), redrawing it in place.
    pub fn regenerate_zone(&mut self, h: crate::Handle, p: &ZoneParams) -> Result<()> {
        let pieces = p.generate()?;
        let old = self.entity(h).ok_or(crate::DocError::NoEntity(h))?.common.xdata.clone();
        let tags = xdata::replace(ZONE_APP, &old, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)
    }

    /// The outline of zone `h` in drawing coordinates, with its data.
    pub fn zone_outline(&self, h: crate::Handle) -> Option<(ZoneParams, Vec<Vec2>)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = zone_of(e)?;
        let (s, c) = i.rotation.sin_cos();
        let world = p
            .outline
            .iter()
            .map(|v| {
                let (x, y) = (v.x * i.scale.x, v.y * i.scale.y);
                Vec2::new(i.insert.x + x * c - y * s, i.insert.y + x * s + y * c)
            })
            .collect();
        Some((p, world))
    }
}
