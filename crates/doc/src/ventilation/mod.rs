//! Ventilation objects drawn in plan: spaces (rooms with the fresh air they need) and air
//! terminals (diffusers and grilles). Each is a block reference to an anonymous block holding
//! its generated outline and labels, its parameters travelling as extended data of their own
//! registered application, as walls do: a DWG save keeps entity xdata, copying the reference
//! copies the object, and other programs see an ordinary block.
//!
//! The flow rates and sizes come from [`cadcraft_hvac`]; this module only draws and stores.

mod distribute;
mod library;
mod space;
mod terminal;
mod xdata;

pub use distribute::{Distribution, MAX_DISTRIBUTED, distribute_in_polygon};
pub use library::standard_air_terminal_models;
pub use space::*;
pub use terminal::*;

use cadcraft_geom::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::DocError;

/// Longest space or model name, in characters.
pub const MAX_HVAC_NAME_CHARS: usize = 64;
/// Smallest and largest label height, millimetres.
const LABEL_HEIGHT_MM: (f64, f64) = (1.0, 10_000.0);

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
    /// A new anonymous block holding `pieces`, at the block's origin; returns its name.
    fn hvac_block(&mut self, pieces: Vec<HvacPiece>) -> String {
        let name = self.anonymous_block_name("*U");
        let mut b = crate::Block::new(&name);
        b.anonymous = true;
        for piece in pieces {
            let common = crate::Common { layer: piece.layer.into(), ..Default::default() };
            b.entities.push(crate::Entity { handle: self.new_handle(), common, kind: piece.kind });
        }
        self.blocks.insert(name.clone(), std::sync::Arc::new(b));
        name
    }
}

#[cfg(test)]
mod tests;
