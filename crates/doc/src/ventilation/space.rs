//! Spaces: a room's outline with what it is used for and how many people work there, from which
//! the fresh air it needs follows (or a flow the user sets).

use cadcraft_geom::{PolyVertex, Vec2, point_in_polygon, polygon_centroid, shoelace};
use cadcraft_hvac::{RoomUse, fresh_air_rate};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, v3, xdata};
use crate::{EntityKind, LwPolyline, RawTag, Result};

/// The registered application name of a space's extended data.
pub const SPACE_APP: &str = "CADCRAFT_SPACE";
/// The layer of a space's block reference, outline and labels.
pub const SPACE_LAYER: &str = "M-SPACE";
/// Most vertices of a space's outline.
pub const MAX_SPACE_VERTICES: usize = 256;
/// Most occupants of one space.
pub const MAX_OCCUPANTS: u32 = 100_000;
/// Largest flow a user may set for one space, m³/h.
pub const MAX_SPACE_FLOW_M3H: f64 = 1_000_000.0;
const XDATA_FORMAT: &str = "SPACE1";

fn yes() -> bool {
    true
}

fn default_label_mm() -> f64 {
    200.0
}

/// A space: what its block reference keeps to regenerate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceParams {
    pub name: String,
    pub room: RoomUse,
    #[serde(default)]
    pub occupants: u32,
    /// Whether toilets and showers are for collective use (the labour code asks less of a
    /// private one).
    #[serde(default = "yes")]
    pub collective: bool,
    /// A flow the user sets instead of the labour code's, m³/h.
    #[serde(default)]
    pub flow_m3h: Option<f64>,
    /// The closed outline in the block's coordinates (drawing units), without repeating the
    /// first point.
    pub outline: Vec<Vec2>,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

impl SpaceParams {
    /// Checks parameters from a file, a command or a script.
    pub fn validate(&self) -> Result<()> {
        check_name("space", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if self.occupants > MAX_OCCUPANTS {
            return Err(invalid(format!("A space has at most {MAX_OCCUPANTS} occupants.")));
        }
        if let Some(f) = self.flow_m3h
            && !(f.is_finite() && f > 0.0 && f <= MAX_SPACE_FLOW_M3H)
        {
            return Err(invalid(format!("A space's flow is more than 0 and at most {MAX_SPACE_FLOW_M3H} m³/h.")));
        }
        if self.outline.len() < 3 || self.outline.len() > MAX_SPACE_VERTICES {
            return Err(invalid(format!("A space's outline has 3 to {MAX_SPACE_VERTICES} points.")));
        }
        if self.outline.iter().any(|p| !p.is_finite() || p.x.abs() > 1e12 || p.y.abs() > 1e12) {
            return Err(invalid("A space's outline has a point that is not a usable number."));
        }
        if shoelace(&self.outline).abs() <= 1e-12 {
            return Err(invalid("A space's outline encloses no area."));
        }
        Ok(())
    }

    /// The floor area, m².
    pub fn area_m2(&self) -> f64 {
        self.units.area_m2(shoelace(&self.outline).abs())
    }

    /// The fresh air the space needs, m³/h: the flow the user set, else the labour code's.
    pub fn flow(&self) -> Result<f64> {
        match self.flow_m3h {
            Some(f) => Ok(f),
            None => fresh_air_rate(self.room, self.occupants, self.collective).map_err(|e| invalid(e.to_string())),
        }
    }

    /// The point labels and terminals are centred on: the outline's centroid, or, for a shape
    /// whose centroid falls outside it (an L or a U), the middle of its first diagonal inside.
    pub fn label_point(&self) -> Vec2 {
        if let Some(c) = polygon_centroid(&self.outline)
            && point_in_polygon(&self.outline, c)
        {
            return c;
        }
        let n = self.outline.len();
        for i in 0..n {
            for j in i + 2..n {
                if let (Some(a), Some(b)) = (self.outline.get(i), self.outline.get(j)) {
                    let m = (*a + *b) * 0.5;
                    if point_in_polygon(&self.outline, m) {
                        return m;
                    }
                }
            }
        }
        self.outline.first().copied().unwrap_or(Vec2::ZERO)
    }

    /// The outline, then the name, the area and the flow centred one above the other.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let flow = self.flow()?;
        let outline = LwPolyline {
            vertices: self.outline.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect(),
            closed: true,
            const_width: 0.0,
            elevation: 0.0,
            plinegen: false,
        };
        let h = self.units.to_units(self.label_height_mm);
        let at = self.label_point();
        let step = Vec2::new(0.0, 1.6 * h);
        let lines = [self.name.clone(), format!("{:.1} m²", self.area_m2()), format!("{flow:.0} m³/h")];
        let mut pieces = vec![HvacPiece { layer: SPACE_LAYER, kind: EntityKind::LwPolyline(outline) }];
        for (k, text) in lines.into_iter().enumerate() {
            let at = at + step * (1.0 - k as f64);
            pieces.push(HvacPiece { layer: SPACE_LAYER, kind: label(at, h, text) });
        }
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(SPACE_APP, XDATA_FORMAT, self)
    }

    /// The space parameters in an object's extended data, when they are there and usable.
    pub fn from_xdata(tags: &[RawTag]) -> Option<SpaceParams> {
        let p: SpaceParams = xdata::from_xdata(SPACE_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The space an object is, when it is a block reference carrying usable space data.
pub fn space_of(e: &crate::Entity) -> Option<SpaceParams> {
    match &e.kind {
        EntityKind::Insert(_) => SpaceParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds space `p` to `space`, its outline given in drawing coordinates: an anonymous block
    /// with its outline and labels, referenced on [`SPACE_LAYER`] at the outline's first point.
    pub fn add_space(&mut self, space: &crate::Space, mut common: crate::Common, mut p: SpaceParams) -> Result<crate::Handle> {
        let origin = p.outline.first().copied().ok_or_else(|| invalid("A space needs an outline."))?;
        for v in &mut p.outline {
            *v -= origin;
        }
        // Everything that can fail comes before the drawing changes.
        let pieces = p.generate()?;
        common.xdata = xdata::replace(SPACE_APP, &common.xdata, p.to_xdata()?);
        common.layer = SPACE_LAYER.into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(SPACE_LAYER);
        let insert = crate::Insert { block, insert: v3(origin), ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// The outline of space `h` in drawing coordinates, with its data: the reference's position,
    /// rotation and scale applied to the outline it keeps.
    pub fn space_outline(&self, h: crate::Handle) -> Option<(SpaceParams, Vec<Vec2>)> {
        let e = self.entity(h)?;
        let EntityKind::Insert(i) = &e.kind else { return None };
        let p = space_of(e)?;
        let (s, c) = i.rotation.sin_cos();
        let world = p
            .outline
            .iter()
            .map(|v| {
                let x = v.x * i.scale.x;
                let y = v.y * i.scale.y;
                Vec2::new(i.insert.x + x * c - y * s, i.insert.y + x * s + y * c)
            })
            .collect();
        Some((p, world))
    }
}
