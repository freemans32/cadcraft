//! Slabs: a floor, roof or foundation slab drawn as a closed outline, hatched, with its thickness and
//! the elevation of its top written on it. Like the other plan objects it is a block reference to an
//! anonymous block, its data travelling as extended data of its own registered application.

use cadcraft_geom::{PolyVertex, Vec2, shoelace};
use serde::{Deserialize, Serialize};

use super::{HvacPiece, HvacUnits, check_label_height, check_name, invalid, label, v3, xdata};
use crate::{EntityKind, Hatch, HatchLoop, LwPolyline, RawTag, Result};

/// The registered application name of a slab's extended data.
pub const SLAB_APP: &str = "CADCRAFT_SLAB";
/// The layers of floor, roof and foundation slabs.
pub const SLAB_FLOOR_LAYER: &str = "A-FLOR-SLAB";
pub const SLAB_ROOF_LAYER: &str = "A-ROOF-SLAB";
pub const SLAB_FOUNDATION_LAYER: &str = "A-FNDN-SLAB";
/// The layer of a slab's hatch.
pub const SLAB_PATTERN_LAYER: &str = "A-SLAB-PATT";
/// Most vertices of a slab's outline.
pub const MAX_SLAB_VERTICES: usize = 256;
/// Largest thickness and elevation, mm.
pub const MAX_SLAB_MM: f64 = 10_000.0;
const XDATA_FORMAT: &str = "SLAB1";

fn default_label_mm() -> f64 {
    150.0
}

fn default_hatch_scale() -> f64 {
    1.0
}

/// What a slab is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SlabKind {
    #[default]
    Floor,
    Roof,
    Foundation,
}

impl SlabKind {
    pub const ALL: [SlabKind; 3] = [SlabKind::Floor, SlabKind::Roof, SlabKind::Foundation];

    pub fn name(self) -> &'static str {
        match self {
            SlabKind::Floor => "floor",
            SlabKind::Roof => "roof",
            SlabKind::Foundation => "foundation",
        }
    }

    pub fn parse(text: &str) -> Option<SlabKind> {
        let t: String = text.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        match t.as_str() {
            "floor" | "slab" | "plancher" | "dalle" => Some(SlabKind::Floor),
            "roof" | "toit" | "toiture" | "toitureterrasse" => Some(SlabKind::Roof),
            "foundation" | "fondation" | "radier" => Some(SlabKind::Foundation),
            _ => None,
        }
    }

    pub fn layer(self) -> &'static str {
        match self {
            SlabKind::Floor => SLAB_FLOOR_LAYER,
            SlabKind::Roof => SLAB_ROOF_LAYER,
            SlabKind::Foundation => SLAB_FOUNDATION_LAYER,
        }
    }

    /// The thickness a new slab of the kind starts with, mm.
    pub fn default_thickness_mm(self) -> f64 {
        match self {
            SlabKind::Floor => 200.0,
            SlabKind::Roof => 250.0,
            SlabKind::Foundation => 400.0,
        }
    }
}

/// A slab: what its block reference keeps to regenerate it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlabParams {
    pub name: String,
    #[serde(default)]
    pub kind: SlabKind,
    pub thickness_mm: f64,
    /// The elevation of the slab's top, mm, from the drawing's zero.
    #[serde(default)]
    pub elevation_mm: f64,
    /// The closed outline in the block's coordinates (drawing units), without repeating the first point.
    pub outline: Vec<Vec2>,
    /// The hatch pattern; empty for none.
    #[serde(default = "default_pattern")]
    pub pattern: String,
    /// The pattern's scale in drawing units.
    #[serde(default = "default_hatch_scale")]
    pub hatch_scale: f64,
    #[serde(default = "default_label_mm")]
    pub label_height_mm: f64,
    #[serde(default)]
    pub units: HvacUnits,
}

fn default_pattern() -> String {
    "AR-CONC".into()
}

impl SlabParams {
    /// A slab of `kind` over `outline`, with the kind's thickness and the concrete hatch.
    pub fn new(name: &str, kind: SlabKind, outline: Vec<Vec2>, units: HvacUnits) -> SlabParams {
        SlabParams {
            name: name.to_string(),
            kind,
            thickness_mm: kind.default_thickness_mm(),
            elevation_mm: 0.0,
            outline,
            pattern: default_pattern(),
            hatch_scale: default_hatch_scale(),
            label_height_mm: default_label_mm(),
            units,
        }
    }

    pub fn validate(&self) -> Result<()> {
        check_name("slab", &self.name)?;
        check_label_height(self.label_height_mm)?;
        self.units.validate()?;
        if !(self.thickness_mm.is_finite() && (10.0..=MAX_SLAB_MM).contains(&self.thickness_mm)) {
            return Err(invalid(format!("A slab's thickness is 10 to {MAX_SLAB_MM} mm.")));
        }
        if !(self.elevation_mm.is_finite() && self.elevation_mm.abs() <= 1.0e6) {
            return Err(invalid("A slab's elevation is not a usable number."));
        }
        if !(self.hatch_scale.is_finite() && self.hatch_scale > 0.0 && self.hatch_scale <= 1.0e6)
            || self.pattern.chars().count() > 32
            || self.pattern.chars().any(char::is_control)
        {
            return Err(invalid("A slab's pattern is not usable."));
        }
        if self.outline.len() < 3 || self.outline.len() > MAX_SLAB_VERTICES {
            return Err(invalid(format!("A slab's outline has 3 to {MAX_SLAB_VERTICES} points.")));
        }
        if self.outline.iter().any(|p| !p.is_finite() || p.x.abs() > 1e12 || p.y.abs() > 1e12) {
            return Err(invalid("A slab's outline has a point that is not a usable number."));
        }
        if shoelace(&self.outline).abs() <= 1e-12 {
            return Err(invalid("A slab's outline encloses no area."));
        }
        Ok(())
    }

    /// The area, m².
    pub fn area_m2(&self) -> f64 {
        self.units.area_m2(shoelace(&self.outline).abs())
    }

    /// The volume, m³.
    pub fn volume_m3(&self) -> f64 {
        self.area_m2() * self.thickness_mm / 1000.0
    }

    /// The text written on the slab: its name, thickness and the elevation of its top.
    pub fn label_text(&self) -> String {
        format!("{} {:.0} mm {:+.2} m", self.name, self.thickness_mm, self.elevation_mm / 1000.0)
    }

    /// The outline, its hatch and the label at the outline's first point's centroid.
    pub fn generate(&self) -> Result<Vec<HvacPiece>> {
        self.validate()?;
        let vertices: Vec<PolyVertex> = self.outline.iter().map(|p| PolyVertex { p: *p, bulge: 0.0, start_width: 0.0, end_width: 0.0 }).collect();
        let outline = LwPolyline { vertices, closed: true, const_width: 0.0, elevation: 0.0, plinegen: false };
        let mut pieces = vec![HvacPiece { layer: self.kind.layer(), kind: EntityKind::LwPolyline(outline) }];
        let pattern = self.pattern.trim();
        if !pattern.is_empty() {
            let solid = pattern.eq_ignore_ascii_case("SOLID");
            let hatch = Hatch {
                pattern: pattern.to_ascii_uppercase(),
                solid,
                loops: vec![HatchLoop { vertices: self.outline.iter().copied().map(PolyVertex::new).collect(), outer: true, sources: Vec::new() }],
                scale: if solid { 1.0 } else { self.hatch_scale },
                angle: 0.0,
                associative: false,
                style: 0,
                elevation: 0.0,
                gradient: None,
                origin: Vec2::ZERO,
                background: None,
                pattern_lines: Vec::new(),
            };
            pieces.push(HvacPiece { layer: SLAB_PATTERN_LAYER, kind: EntityKind::Hatch(hatch) });
        }
        let c = cadcraft_geom::polygon_centroid(&self.outline).unwrap_or_else(|| self.outline.first().copied().unwrap_or(Vec2::ZERO));
        pieces.push(HvacPiece { layer: self.kind.layer(), kind: label(c, self.units.to_units(self.label_height_mm), self.label_text()) });
        Ok(pieces)
    }

    pub fn to_xdata(&self) -> Result<Vec<RawTag>> {
        xdata::to_xdata(SLAB_APP, XDATA_FORMAT, self)
    }

    pub fn from_xdata(tags: &[RawTag]) -> Option<SlabParams> {
        let p: SlabParams = xdata::from_xdata(SLAB_APP, XDATA_FORMAT, tags)?;
        p.validate().is_ok().then_some(p)
    }
}

/// The slab an object is, when it is a block reference carrying usable slab data.
pub fn slab_of(e: &crate::Entity) -> Option<SlabParams> {
    match &e.kind {
        EntityKind::Insert(_) => SlabParams::from_xdata(&e.common.xdata),
        _ => None,
    }
}

/// The colour of a slab layer: a mid grey for a floor, brown for a roof, dark for a foundation.
pub(super) fn slab_layer_color(name: &str) -> Option<cadcraft_color::Color> {
    use cadcraft_color::{Color, Rgb};
    match name {
        SLAB_FLOOR_LAYER => Some(Color::True(Rgb(0xa0, 0xa8, 0xb4))),
        SLAB_ROOF_LAYER => Some(Color::True(Rgb(0xc0, 0x7a, 0x4a))),
        SLAB_FOUNDATION_LAYER => Some(Color::True(Rgb(0x7a, 0x70, 0x68))),
        SLAB_PATTERN_LAYER => Some(Color::True(Rgb(0x6e, 0x76, 0x80))),
        _ => None,
    }
}

impl crate::Drawing {
    /// Adds slab `p` to `space`, its outline given in drawing coordinates.
    pub fn add_slab(&mut self, space: &crate::Space, mut common: crate::Common, mut p: SlabParams) -> Result<crate::Handle> {
        let origin = p.outline.first().copied().ok_or_else(|| invalid("A slab needs an outline."))?;
        for v in &mut p.outline {
            *v -= origin;
        }
        let pieces = p.generate()?;
        common.xdata = xdata::replace(SLAB_APP, &common.xdata, p.to_xdata()?);
        common.layer = p.kind.layer().into();
        let block = self.hvac_block(pieces);
        self.ensure_layer(p.kind.layer());
        let insert = crate::Insert { block, insert: v3(origin), ..Default::default() };
        self.add(space, common, EntityKind::Insert(insert))
    }

    /// Gives slab `h` the parameters `p` (outline in the block's coordinates), redrawing it in place.
    pub fn regenerate_slab(&mut self, h: crate::Handle, p: &SlabParams) -> Result<()> {
        let pieces = p.generate()?;
        let e = self.entity(h).ok_or(crate::DocError::NoEntity(h))?;
        let tags = xdata::replace(SLAB_APP, &e.common.xdata, p.to_xdata()?);
        self.regenerate_hvac(h, pieces, tags)?;
        let layer = p.kind.layer();
        self.ensure_layer(layer);
        self.modify_entity(h, |e| e.common.layer = layer.into())?;
        Ok(())
    }
}
