//! Ventilation objects drawn in plan: spaces (rooms with the fresh air they need) and air
//! terminals (diffusers and grilles). Each is a block reference to an anonymous block holding
//! its generated outline and labels, its parameters travelling as extended data of their own
//! registered application, as walls do: a DWG save keeps entity xdata, copying the reference
//! copies the object, and other programs see an ordinary block.
//!
//! The flow rates and sizes come from [`cadcraft_hvac`]; this module only draws and stores.

mod accessory;
mod distribute;
mod duct;
mod library;
mod network;
mod space;
mod terminal;
mod xdata;
mod zone;

pub use accessory::*;
pub use distribute::{Distribution, MAX_DISTRIBUTED, distribute_in_polygon};
pub use duct::*;
pub use library::standard_air_terminal_models;
pub use network::{DEFAULT_MARGIN_MM, DuctNetwork, FLEX_ROUGHNESS_MM, MAX_DUCTS};
pub use space::*;
pub use terminal::*;
pub use zone::*;

use cadcraft_geom::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::DocError;

/// Longest space or model name, in characters.
pub const MAX_HVAC_NAME_CHARS: usize = 64;
/// Smallest and largest label height, millimetres.
const LABEL_HEIGHT_MM: (f64, f64) = (1.0, 10_000.0);

/// The layers of supply and extract arrows: blue and magenta, as a plan shows the two air flows.
pub const AIR_SUPPLY_LAYER: &str = "M-HVAC-SUPPLY";
pub const AIR_EXTRACT_LAYER: &str = "M-HVAC-EXTRACT";

/// The colour a ventilation layer is created with, when it has one of its own: a bright blue and
/// pink, readable on a dark background as on white paper.
fn hvac_layer_color(name: &str) -> Option<cadcraft_color::Color> {
    use cadcraft_color::{Color, Rgb};
    match name {
        AIR_SUPPLY_LAYER | DUCT_LAYER => Some(Color::True(Rgb(0x1e, 0x90, 0xff))),
        AIR_EXTRACT_LAYER | DUCT_EXTRACT_LAYER => Some(Color::True(Rgb(0xff, 0x4d, 0xb8))),
        ZONE_LAYER => Some(Color::True(Rgb(0x2e, 0xb8, 0xa6))),
        _ => None,
    }
}

/// The pieces of an object as preview entities, turned `rotation` radians and moved to `at`: what a
/// command draws under the cursor before the object exists. Each carries its layer's colour, so it
/// looks as it will even when the layer is not in the drawing yet.
pub fn preview_entities(pieces: Vec<HvacPiece>, at: Vec2, rotation: f64) -> Vec<crate::Entity> {
    let m = cadcraft_geom::Mat3::rotate(rotation).then(cadcraft_geom::Mat3::translate(at));
    pieces
        .into_iter()
        .map(|p| {
            let color = hvac_layer_color(p.layer).unwrap_or(cadcraft_color::Color::ByLayer);
            let mut e = crate::Entity {
                handle: crate::Handle(0),
                common: crate::Common { layer: p.layer.into(), color, ..Default::default() },
                kind: p.kind,
            };
            e.kind.transform(&m);
            e
        })
        .collect()
}

/// How a ventilation object's millimetres become drawing units.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HvacUnits {
    /// Millimetres per drawing unit (1 for millimetres, 1000 for metres).
    pub mm_per_unit: f64,
}

impl Default for HvacUnits {
    fn default() -> Self {
        HvacUnits { mm_per_unit: 1.0 }
    }
}

impl HvacUnits {
    /// The units of drawing `d`: its `INSUNITS`, else millimetres in a metric drawing and inches
    /// in an imperial one.
    pub fn of(d: &crate::Drawing) -> Self {
        let mm_per_unit = match d.header.i64("INSUNITS", 0) {
            1 => 25.4,
            2 => 304.8,
            4 => 1.0,
            5 => 10.0,
            6 => 1000.0,
            14 => 100.0,
            _ if d.header.i64("MEASUREMENT", 0) == 1 => 1.0,
            _ => 25.4,
        };
        HvacUnits { mm_per_unit }
    }
    fn validate(self) -> crate::Result<()> {
        if self.mm_per_unit.is_finite() && self.mm_per_unit > 0.0 && self.mm_per_unit <= 1e6 {
            Ok(())
        } else {
            Err(invalid("The drawing units are not usable."))
        }
    }
    /// Millimetres as drawing units.
    pub fn to_units(self, mm: f64) -> f64 {
        mm / self.mm_per_unit
    }
    /// Square drawing units as square metres.
    pub fn area_m2(self, area_units: f64) -> f64 {
        let m = self.mm_per_unit / 1000.0;
        area_units * m * m
    }
}

fn invalid(m: impl Into<String>) -> DocError {
    DocError::Invalid(m.into())
}

fn v3(p: Vec2) -> Vec3 {
    Vec3::new(p.x, p.y, 0.0)
}

fn check_name(what: &str, name: &str) -> crate::Result<()> {
    if name.trim().is_empty() || name.chars().count() > MAX_HVAC_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(invalid(format!("A {what} name has 1 to {MAX_HVAC_NAME_CHARS} characters, without control characters.")));
    }
    Ok(())
}

fn check_label_height(mm: f64) -> crate::Result<()> {
    if mm.is_finite() && (LABEL_HEIGHT_MM.0..=LABEL_HEIGHT_MM.1).contains(&mm) {
        Ok(())
    } else {
        Err(invalid(format!("The label height is {} to {} mm.", LABEL_HEIGHT_MM.0, LABEL_HEIGHT_MM.1)))
    }
}

/// A centred single-line text at `at`.
fn label(at: Vec2, height: f64, value: String) -> crate::EntityKind {
    crate::EntityKind::Text(crate::Text {
        insert: v3(at),
        align_pt: Some(v3(at)),
        height,
        value,
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: crate::HAlign::Center,
        valign: crate::VAlign::Middle,
    })
}

/// One object of a ventilation object's generated geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct HvacPiece {
    pub layer: &'static str,
    pub kind: crate::EntityKind,
}

impl crate::Drawing {
    /// Gives ventilation object `h` (a block reference) the generated `pieces` and the extended
    /// data `xdata`: a new block, the old one removed when nothing else refers to it. A scaled
    /// or arrayed reference is refused: its drawn size would no longer match its data.
    fn regenerate_hvac(&mut self, h: crate::Handle, pieces: Vec<HvacPiece>, xdata: Vec<crate::RawTag>) -> crate::Result<()> {
        let e = self.entity(h).ok_or(DocError::NoEntity(h))?;
        let crate::EntityKind::Insert(i) = &e.kind else { return Err(invalid("The object is not a block reference.")) };
        let unit = |s: f64| (s - 1.0).abs() <= 1e-9;
        if !(unit(i.scale.x) && unit(i.scale.y) && i.cols <= 1 && i.rows <= 1) {
            return Err(invalid("The object was scaled or arrayed: it can't be regenerated."));
        }
        let old = i.block.clone();
        let block = self.hvac_block(pieces);
        self.modify_entity(h, |e| {
            if let crate::EntityKind::Insert(i) = &mut e.kind {
                i.block = block;
            }
            e.common.xdata = xdata;
        })?;
        self.drop_unreferenced_hvac_block(&old);
        Ok(())
    }

    /// Removes anonymous block `name` when no object refers to it.
    fn drop_unreferenced_hvac_block(&mut self, name: &str) {
        let refers = |e: &std::sync::Arc<crate::Entity>| matches!(&e.kind, crate::EntityKind::Insert(i) if i.block.eq_ignore_ascii_case(name));
        let used = self.model.iter().any(refers)
            || self.layouts.iter().any(|l| l.entities.iter().any(refers))
            || self.blocks.values().any(|b| b.entities.iter().any(refers) || b.array.as_ref().is_some_and(|a| a.source.eq_ignore_ascii_case(name)));
        if !used && self.block(name).is_some_and(|b| b.anonymous) {
            self.blocks.retain(|k, _| !k.eq_ignore_ascii_case(name));
        }
    }

    /// A new anonymous block holding `pieces`, at the block's origin; returns its name.
    fn hvac_block(&mut self, pieces: Vec<HvacPiece>) -> String {
        let name = self.anonymous_block_name("*U");
        let mut b = crate::Block::new(&name);
        b.anonymous = true;
        for piece in pieces {
            if self.ensure_layer(piece.layer)
                && let Some(c) = hvac_layer_color(piece.layer)
                && let Some(l) = self.layers.iter_mut().find(|l| l.name == piece.layer)
            {
                l.color = c;
            }
            let common = crate::Common { layer: piece.layer.into(), ..Default::default() };
            b.entities.push(crate::Entity { handle: self.new_handle(), common, kind: piece.kind });
        }
        self.blocks.insert(name.clone(), std::sync::Arc::new(b));
        name
    }
}

#[cfg(test)]
mod accessory_tests;
#[cfg(test)]
mod duct_tests;
#[cfg(test)]
mod tests;
