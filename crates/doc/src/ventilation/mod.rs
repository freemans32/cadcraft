//! Ventilation objects drawn in plan: spaces (rooms with the fresh air they need) and air
//! terminals (diffusers and grilles). Each is a block reference to an anonymous block holding
//! its generated outline and labels, its parameters travelling as extended data of their own
//! registered application, as walls do: a DWG save keeps entity xdata, copying the reference
//! copies the object, and other programs see an ordinary block.
//!
//! The flow rates and sizes come from [`cadcraft_hvac`]; this module only draws and stores.

mod accessory;
mod addresses;
mod cable_types;
mod circuit_legend;
mod control;
mod control_report;
mod control_schema;
mod convention;
mod crossing;
mod curtain;
mod design;
mod distribute;
mod duct;
mod elbows;
mod elec;
mod elec_circuits;
mod elec_report;
mod equipment;
mod furniture;
mod furniture_catalog;
mod labels;
mod library;
mod network;
mod network_legend;
mod part;
mod part_library;
mod pipe;
mod pipe_fittings;
mod pipe_network;
mod pipe_network_names;
mod pipe_networks;
mod pipe_riser;
mod refrigerant;
mod refrigerant_charge;
mod riser;
mod roof;
mod sanitary;
mod single_line;
mod slab;
mod space;
mod structure;
mod takeoff;
mod takeoff_cost;
mod terminal;
mod titleblock;
mod upright;
pub(crate) mod xdata;
mod xlsx;
mod zone;

pub use accessory::*;
pub use addresses::*;
pub use cable_types::*;
pub use circuit_legend::*;
pub use control::*;
pub use control_report::*;
pub use control_schema::*;
pub use convention::*;
pub use crossing::DuctCrossing;
pub use curtain::*;
pub use design::*;
pub use distribute::{Distribution, MAX_DISTRIBUTED, distribute_in_polygon};
pub use duct::*;
pub use elec::*;
pub use elec_circuits::*;
pub use elec_report::*;
pub use equipment::*;
pub use furniture::*;
pub use library::standard_air_terminal_models;
pub use network::{DEFAULT_MARGIN_MM, DuctNetwork, FLEX_ROUGHNESS_MM, MAX_DUCTS};
pub use network_legend::*;
pub use part::*;
pub use part_library::{
    MAX_USER_PART_MODELS, PartEntry, PartModel, PartTemplate, branch_box_outlet_counts, branch_box_with_outlets, short_part_name,
    standard_part_model_named, standard_part_models, tag_prefix as part_tag_prefix,
};
pub use pipe::*;
pub use pipe_fittings::{MAX_FITTING_PIPES, PipeFitting, PipeFittingKind};
pub use pipe_network::{MAX_PIPES as MAX_PIPE_RUNS, PipeNetworkReport, PipeResult, PipeSizingParams};
pub use pipe_network_names::NetworkSummary;
pub use pipe_networks::*;
pub use pipe_riser::*;
pub use refrigerant::{OutletReport, PartReport, RefrigerantNetwork, RefrigerantReport};
pub use refrigerant_charge::{ChargeReport, ChargeRoom, practical_limit_kg_m3};
pub use riser::*;
pub use roof::*;
pub use sanitary::*;
pub use single_line::*;
pub use slab::*;
pub use space::*;
pub use structure::*;
pub use takeoff::*;
pub use takeoff_cost::*;
pub use terminal::*;
pub use titleblock::*;
pub use xlsx::{SheetCell, Workbook};
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
/// The layers of fresh air (air neuf) and exhaust air (rejet) arrows: green and orange.
pub const AIR_FRESH_LAYER: &str = "M-HVAC-FRESH";
pub const AIR_EXHAUST_LAYER: &str = "M-HVAC-EXHAUST";

/// The colour a ventilation layer is created with, when it has one of its own: a bright blue and
/// pink, readable on a dark background as on white paper.
fn hvac_layer_color(name: &str) -> Option<cadcraft_color::Color> {
    use cadcraft_color::{Color, Rgb};
    match name {
        AIR_SUPPLY_LAYER | DUCT_LAYER => Some(Color::True(Rgb(0x1e, 0x90, 0xff))),
        AIR_EXTRACT_LAYER | DUCT_EXTRACT_LAYER => Some(Color::True(Rgb(0xff, 0x4d, 0xb8))),
        AIR_FRESH_LAYER | DUCT_FRESH_LAYER => Some(Color::True(Rgb(0x3c, 0xc8, 0x64))),
        AIR_EXHAUST_LAYER | DUCT_EXHAUST_LAYER => Some(Color::True(Rgb(0xff, 0x9a, 0x1e))),
        DUCT_INSULATION_LAYER => Some(Color::True(Rgb(0xe0, 0xc0, 0x60))),
        ZONE_LAYER => Some(Color::True(Rgb(0x2e, 0xb8, 0xa6))),
        // The safety devices are red.
        ACCESSORY_FIRE_LAYER => Some(Color::True(Rgb(0xe0, 0x24, 0x24))),
        EQUIPMENT_ZONE_LAYER => Some(Color::True(Rgb(0x9a, 0x9a, 0x9a))),
        _ => pipe::pipe_layer_color(name)
            .or_else(|| slab::slab_layer_color(name))
            .or_else(|| structure::structure_layer_color(name))
            .or_else(|| design::design_layer_color(name))
            .or_else(|| titleblock::titleblock_layer_color(name))
            .or_else(|| curtain::curtain_layer_color(name))
            .or_else(|| roof::roof_layer_color(name))
            .or_else(|| part::part_layer_color(name))
            .or_else(|| elec::elec_layer_color(name))
            .or_else(|| sanitary::sanitary_layer_color(name))
            .or_else(|| furniture::furniture_layer_color(name))
            .or_else(|| control::control_layer_color(name))
            .or_else(|| pipe_networks::network_base(name).and_then(hvac_layer_color)),
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
            // The layer's name is before a bar (`layer|linetype|colour`).
            let layer = p.layer.split('|').next().unwrap_or(p.layer);
            let color = hvac_layer_color(layer).unwrap_or(cadcraft_color::Color::ByLayer);
            let mut e =
                crate::Entity { handle: crate::Handle(0), common: crate::Common { layer: layer.into(), color, ..Default::default() }, kind: p.kind };
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
    text_at(at, height, value, crate::HAlign::Center)
}

/// A single-line text starting at `at`, its middle on the line.
fn label_left(at: Vec2, height: f64, value: String) -> crate::EntityKind {
    text_at(at, height, value, crate::HAlign::Left)
}

/// The name of a riser, at the lower right outside its shaft (the leaders go up to the right and down to the left, so nothing is there).
fn riser_name_label(outline: &[Vec2], height: f64, value: &str) -> crate::EntityKind {
    let right = outline.iter().map(|p| p.x).fold(f64::MIN, f64::max);
    let bottom = outline.iter().map(|p| p.y).fold(f64::MAX, f64::min);
    label_left(Vec2::new(right + 0.5 * height, bottom), height, value.to_string())
}

/// The altitude of the underside of an element as it is written beside it ("Ai:2.50", metres), when it is given.
pub(crate) fn altitude_tag(bottom_mm: Option<f64>) -> Option<String> {
    bottom_mm.map(|b| format!("Ai:{:.2}", b / 1000.0))
}

/// Whether the altitude of an underside, mm, can be kept.
pub(crate) fn check_altitude(bottom_mm: Option<f64>) -> crate::Result<()> {
    match bottom_mm {
        Some(b) if !(b.is_finite() && (-100_000.0..=1_000_000.0).contains(&b)) => {
            Err(invalid("The altitude of the underside is a number of millimetres."))
        }
        _ => Ok(()),
    }
}

fn text_at(at: Vec2, height: f64, value: String, halign: crate::HAlign) -> crate::EntityKind {
    crate::EntityKind::Text(crate::Text {
        insert: v3(at),
        align_pt: Some(v3(at)),
        height,
        value,
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign,
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
    pub(crate) fn regenerate_hvac(&mut self, h: crate::Handle, pieces: Vec<HvacPiece>, xdata: Vec<crate::RawTag>) -> crate::Result<()> {
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

    /// Makes `layer` when it is missing, with its own colour, or for a network the colour and the line type of its style.
    pub(crate) fn ensure_hvac_layer(&mut self, layer: &str) {
        if !self.ensure_layer(layer) {
            return;
        }
        let own = self.hvac_colour(layer);
        let look = self.network_look(layer);
        if let Some(ref lt) = look.as_ref().map(|(_, lt)| lt.clone()) {
            self.ensure_network_linetype(lt);
        }
        if let Some(l) = self.layers.iter_mut().find(|l| l.name == layer) {
            match look {
                Some((c, lt)) => (l.color, l.linetype) = (c, lt),
                None => {
                    if let Some(c) = own {
                        l.color = c;
                    }
                }
            }
        }
    }

    /// A new anonymous block holding `pieces`, at the block's origin; returns its name.
    pub(crate) fn hvac_block(&mut self, pieces: Vec<HvacPiece>) -> String {
        let name = self.anonymous_block_name("*U");
        let mut b = crate::Block::new(&name);
        b.anonymous = true;
        for piece in pieces {
            // A piece's layer may carry a linetype after a bar, and then the layer whose colour it takes:
            // `layer|linetype|colour layer`.
            let (layer, rest) = piece.layer.split_once('|').unwrap_or((piece.layer, "ByLayer"));
            let (linetype, colour_of) = rest.split_once('|').map_or((rest, None), |(l, c)| (l, Some(c)));
            self.ensure_hvac_layer(layer);
            let mut common = crate::Common { layer: layer.into(), linetype: linetype.into(), ..Default::default() };
            // A colour of its own is written "#rrggbb", ByBlock takes the colour given to the object; else it is the colour of the layer named.
            if let Some(c) = colour_of.and_then(|n| match riser::parse_hex_colour(n) {
                Some((r, g, b)) => Some(cadcraft_color::Color::True(cadcraft_color::Rgb(r, g, b))),
                None if n == "ByBlock" => Some(cadcraft_color::Color::ByBlock),
                None => self.hvac_colour(n),
            }) {
                common.color = c;
            }
            b.entities.push(crate::Entity { handle: self.new_handle(), common, kind: piece.kind });
        }
        self.blocks.insert(name.clone(), std::sync::Arc::new(b));
        name
    }
}

/// The piece layer of an object drawn on `layer` whose pieces take the colour given to the object (ByBlock): the layer's while it is ByLayer.
pub(crate) fn by_block(layer: &'static str) -> &'static str {
    match layer {
        AHU_LAYER => "M-HVAC-AHU|ByLayer|ByBlock",
        AIR_TERMINAL_LAYER => "M-HVAC-DIFF|ByLayer|ByBlock",
        ACCESSORY_LAYER => "M-HVAC-ACC|ByLayer|ByBlock",
        ACCESSORY_FIRE_LAYER => "M-HVAC-FIRE|ByLayer|ByBlock",
        DUCT_LAYER => "M-HVAC-DUCT-SUP|ByLayer|ByBlock",
        DUCT_EXTRACT_LAYER => "M-HVAC-DUCT-EXTR|ByLayer|ByBlock",
        DUCT_FRESH_LAYER => "M-HVAC-DUCT-FRESH|ByLayer|ByBlock",
        DUCT_EXHAUST_LAYER => "M-HVAC-DUCT-EXH|ByLayer|ByBlock",
        DEVICE_LAYER => "E-DEVICE|ByLayer|ByBlock",
        LIGHT_LAYER => "E-LIGHT|ByLayer|ByBlock",
        PANEL_LAYER => "E-PANEL|ByLayer|ByBlock",
        SAFETY_LAYER => "E-SAFETY|ByLayer|ByBlock",
        other => other,
    }
}

/// Checks the name of a sub-network (at most 80 characters) and its colour ("#rrggbb").
pub(crate) fn check_subnetwork(name: &str, colour: Option<&str>) -> crate::Result<()> {
    if name.chars().count() > 80 {
        return Err(invalid("The name of a sub-network is at most 80 characters."));
    }
    if colour.is_some_and(|c| parse_hex_colour(c).is_none()) {
        return Err(invalid("The colour of a sub-network is written #rrggbb."));
    }
    Ok(())
}

/// The piece layer of `layer` drawn in a sub-network's own colour, when it has one.
pub(crate) fn subnetwork_layer(layer: &'static str, colour: Option<&str>) -> &'static str {
    match colour {
        Some(c) => pipe_networks::intern(format!("{layer}|ByLayer|{c}")).unwrap_or(layer),
        None => layer,
    }
}

#[cfg(test)]
mod accessory_tests;
#[cfg(test)]
mod control_schema_tests;
#[cfg(test)]
mod design_tests;
#[cfg(test)]
mod duct_tests;
#[cfg(test)]
mod elec_tests;
#[cfg(test)]
mod equipment_tests;
#[cfg(test)]
mod part_tests;
#[cfg(test)]
mod pipe_networks_tests;
#[cfg(test)]
mod pipe_riser_tests;
#[cfg(test)]
mod pipe_tests;
#[cfg(test)]
mod refrigerant_tests;
#[cfg(test)]
mod riser_tests;
#[cfg(test)]
mod single_line_tests;
#[cfg(test)]
mod slab_tests;
#[cfg(test)]
mod structure_tests;
#[cfg(test)]
mod tests;
