//! Ventilation calculations, with no drawing: the fresh-air flow rate a room needs, the round
//! duct sizes of a ventilation network fed by an air handling unit, and the pressure the unit
//! must supply to its least favoured terminal.
//!
//! Everything here is plain physics and published French rules (the labour code's flow rates),
//! written from those sources: Darcy-Weisbach for friction, the Colebrook equation for the
//! friction factor, and loss coefficients for fittings. The default coefficients and limits are
//! indicative design values that a user may change; none comes from a manufacturer's catalogue.
//!
//! Units: flow rates in m³/h, diameters in mm, lengths in m, velocities in m/s, pressures in Pa.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod air;
mod fittings;
mod friction;
mod network;
mod pipe;
mod rates;
mod sizes;

pub use air::Air;
pub use fittings::{Fitting, FittingCoefficients};
pub use friction::{friction_factor, pressure_gradient, reynolds, velocity};
pub use network::{DesignCriteria, Governing, Network, NetworkReport, Section, Segment, SegmentKind, SegmentResult, Terminal, size_network};
pub use pipe::{
    DRAIN_PIPES, PipeCriteria, PipeGoverning, PipeSizing, STEEL_PIPES, STEEL_ROUGHNESS_MM, WaterTemperature, check_water_pipe, max_velocity_ms,
    size_condensate_drain, size_water_pipe, steel_inner_diameter_mm, water_flow_m3h,
};
pub use rates::{RoomUse, fresh_air_rate};
pub use sizes::{ROUND_DIAMETERS_MM, equivalent_diameter_mm, rect_area_m2, round_area_m2};

/// What a calculation refuses, with the reason in words a user can act on.
#[derive(Clone, Debug, PartialEq)]
pub enum HvacError {
    /// A number given by the user (or read from a drawing) is missing, not finite or out of range.
    BadValue(String),
    /// The network's shape is wrong: a loop, a missing parent, an empty run.
    BadNetwork(String),
}

impl std::fmt::Display for HvacError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HvacError::BadValue(m) => write!(f, "invalid value: {m}"),
            HvacError::BadNetwork(m) => write!(f, "invalid network: {m}"),
        }
    }
}

impl std::error::Error for HvacError {}

pub type Result<T> = std::result::Result<T, HvacError>;

/// `v` if it is finite and within `min..=max`, else an error naming `what`.
fn checked(what: &str, v: f64, min: f64, max: f64) -> Result<f64> {
    if v.is_finite() && v >= min && v <= max { Ok(v) } else { Err(HvacError::BadValue(format!("{what} = {v} (expected {min} to {max})"))) }
}

#[cfg(test)]
mod tests;
