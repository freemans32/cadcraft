//! Pipework calculations: the flow a water network carries for a load and a temperature difference, the
//! steel pipe size that carries it within a velocity and a pressure-loss limit, and the pipe that drains a
//! condensate flow by gravity. Plain physics (Darcy-Weisbach through [`crate::pressure_gradient`], Manning for a
//! drain) with indicative default limits that a user may change; no value comes from a manufacturer's catalogue.
//!
//! Units: loads in kW, flows in m³/h (litres per hour for a drain), diameters in mm, velocities in m/s, pressure
//! losses in Pa per metre.

use serde::{Deserialize, Serialize};

use crate::{Air, HvacError, Result, checked, pressure_gradient, velocity};

/// Steel pipe sizes, mm: the nominal diameter (ISO 6708), the outer diameter (ISO 4200) and a typical wall
/// thickness of a medium series, from which the inner diameter follows.
pub const STEEL_PIPES: [(f64, f64, f64); 15] = [
    (10.0, 17.2, 2.3),
    (15.0, 21.3, 2.6),
    (20.0, 26.9, 2.6),
    (25.0, 33.7, 3.2),
    (32.0, 42.4, 3.2),
    (40.0, 48.3, 3.2),
    (50.0, 60.3, 3.6),
    (65.0, 76.1, 3.6),
    (80.0, 88.9, 4.0),
    (100.0, 114.3, 4.5),
    (125.0, 139.7, 4.5),
    (150.0, 168.3, 4.5),
    (200.0, 219.1, 6.3),
    (250.0, 273.0, 6.3),
    (300.0, 323.9, 7.1),
];

/// Plastic drain pipe sizes, mm: the outer diameter and a typical wall thickness.
pub const DRAIN_PIPES: [(f64, f64); 9] =
    [(16.0, 1.5), (20.0, 1.5), (25.0, 1.5), (32.0, 1.8), (40.0, 1.9), (50.0, 2.4), (63.0, 3.0), (75.0, 3.6), (90.0, 4.3)];

/// Heat carried by a cubic metre of water per kelvin, J/(m³·K): ρ·cp for water near 20 °C.
const WATER_VOLUMETRIC_HEAT: f64 = 4.18e6;
/// Roughness of the inside of a steel pipe, mm.
pub const STEEL_ROUGHNESS_MM: f64 = 0.045;
/// Manning roughness of a smooth plastic drain.
const MANNING_N: f64 = 0.011;
/// A drain is sized to carry this much more than the flow it is given.
const DRAIN_SAFETY: f64 = 2.0;

/// The water of a network, for its density and viscosity: cold (chilled water, about 10 °C), hot (about 70 °C) or the
/// mixed temperature the change-over network averages between its two seasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WaterTemperature {
    Cold,
    Hot,
    Mixed,
}

impl WaterTemperature {
    /// The water as a fluid: density in kg/m³ and kinematic viscosity in m²/s.
    pub fn fluid(self) -> Air {
        match self {
            WaterTemperature::Cold => Air { density: 999.7, kinematic_viscosity: 1.31e-6 },
            WaterTemperature::Hot => Air { density: 977.8, kinematic_viscosity: 0.415e-6 },
            WaterTemperature::Mixed => Air { density: 995.6, kinematic_viscosity: 0.80e-6 },
        }
    }
}

/// The limits a pipe is sized within.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipeCriteria {
    /// Highest velocity, m/s; `None`: the one of the size ([`max_velocity_ms`]).
    pub max_velocity_ms: Option<f64>,
    /// Highest pressure loss, Pa/m.
    pub max_gradient_pa_m: f64,
    /// Smallest nominal diameter, mm.
    pub min_dn_mm: f64,
    /// Roughness of the inside of the pipe, mm.
    pub roughness_mm: f64,
}

impl Default for PipeCriteria {
    fn default() -> Self {
        PipeCriteria { max_velocity_ms: None, max_gradient_pa_m: 300.0, min_dn_mm: 15.0, roughness_mm: STEEL_ROUGHNESS_MM }
    }
}

impl PipeCriteria {
    pub fn checked(self) -> Result<PipeCriteria> {
        if let Some(v) = self.max_velocity_ms {
            checked("maximum velocity (m/s)", v, 0.1, 10.0)?;
        }
        checked("maximum pressure loss (Pa/m)", self.max_gradient_pa_m, 1.0, 100_000.0)?;
        checked("smallest diameter (mm)", self.min_dn_mm, 1.0, 1_000.0)?;
        checked("roughness (mm)", self.roughness_mm, 0.0, 5.0)?;
        Ok(self)
    }
}

/// What limits the size chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PipeGoverning {
    /// The smallest size allowed.
    Minimum,
    Velocity,
    Gradient,
    /// The size was given, not chosen.
    Imposed,
}

/// A pipe size and what the flow does in it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipeSizing {
    /// The nominal diameter, mm (the outer diameter of a plastic drain).
    pub dn_mm: f64,
    pub inner_mm: f64,
    pub velocity_ms: f64,
    /// Pressure loss, Pa/m (0 for a drain).
    pub gradient_pa_m: f64,
    pub governing: PipeGoverning,
}

/// The highest velocity worth designing for in a pipe of nominal diameter `dn_mm`, m/s: lower in small pipes, where
/// noise and erosion matter, up to 2 m/s in large ones.
pub fn max_velocity_ms(dn_mm: f64) -> f64 {
    (0.6 + dn_mm / 100.0).clamp(0.7, 2.0)
}

/// The flow of water, m³/h, that carries `load_kw` for a temperature difference of `delta_t_k`.
pub fn water_flow_m3h(load_kw: f64, delta_t_k: f64) -> Result<f64> {
    let load = checked("load (kW)", load_kw, 0.0, 1e7)?;
    let dt = checked("temperature difference (K)", delta_t_k, 0.5, 100.0)?;
    Ok(load * 1000.0 * 3600.0 / (WATER_VOLUMETRIC_HEAT * dt))
}

fn inner_of(dn_mm: f64) -> Option<f64> {
    STEEL_PIPES.iter().find(|(dn, ..)| (*dn - dn_mm).abs() < 1e-6).map(|(_, od, wall)| od - 2.0 * wall)
}

/// The steel pipe of nominal diameter `dn_mm` (or the closest size) with `flow_m3h` in it, within no limit: the inner
/// diameter, the velocity and the pressure loss. For a pipe whose size was given.
pub fn check_water_pipe(flow_m3h: f64, dn_mm: f64, water: WaterTemperature, criteria: &PipeCriteria) -> Result<PipeSizing> {
    let flow = checked("flow (m³/h)", flow_m3h, 0.0, 1e7)?;
    let (dn, inner) = STEEL_PIPES
        .iter()
        .min_by(|a, b| (a.0 - dn_mm).abs().total_cmp(&(b.0 - dn_mm).abs()))
        .map(|(dn, od, wall)| (*dn, od - 2.0 * wall))
        .ok_or_else(|| HvacError::BadValue("no pipe size".into()))?;
    let v = velocity(flow, inner);
    let gradient = pressure_gradient(flow, inner, criteria.roughness_mm, &water.fluid())?;
    Ok(PipeSizing { dn_mm: dn, inner_mm: inner, velocity_ms: v, gradient_pa_m: gradient, governing: PipeGoverning::Imposed })
}

/// The smallest steel pipe that carries `flow_m3h` of water within the criteria's velocity (that of its size unless
/// one is given) and pressure loss. The largest size is returned, over the limit, when none is enough.
pub fn size_water_pipe(flow_m3h: f64, water: WaterTemperature, criteria: &PipeCriteria) -> Result<PipeSizing> {
    let flow = checked("flow (m³/h)", flow_m3h, 0.0, 1e7)?;
    let criteria = criteria.checked()?;
    let fluid = water.fluid();
    let mut last: Option<PipeSizing> = None;
    // What the size before the one chosen broke: what governs the choice.
    let mut broke = None;
    for (dn, od, wall) in STEEL_PIPES {
        if dn < criteria.min_dn_mm - 1e-9 {
            continue;
        }
        let inner = od - 2.0 * wall;
        let v = velocity(flow, inner);
        let gradient = pressure_gradient(flow, inner, criteria.roughness_mm, &fluid)?;
        let v_limit = criteria.max_velocity_ms.unwrap_or_else(|| max_velocity_ms(dn));
        if v <= v_limit && gradient <= criteria.max_gradient_pa_m {
            let governing = broke.unwrap_or(PipeGoverning::Minimum);
            return Ok(PipeSizing { dn_mm: dn, inner_mm: inner, velocity_ms: v, gradient_pa_m: gradient, governing });
        }
        broke = Some(if v > v_limit { PipeGoverning::Velocity } else { PipeGoverning::Gradient });
        last = Some(PipeSizing {
            dn_mm: dn,
            inner_mm: inner,
            velocity_ms: v,
            gradient_pa_m: gradient,
            governing: broke.unwrap_or(PipeGoverning::Velocity),
        });
    }
    last.ok_or_else(|| HvacError::BadValue("no pipe size is at least the smallest allowed".into()))
}

/// The plastic pipe that drains `flow_lph` litres of condensate an hour by gravity at `slope_percent`: the smallest
/// whose capacity, half full by Manning's formula, is twice the flow (and at least 20 mm across).
pub fn size_condensate_drain(flow_lph: f64, slope_percent: f64) -> Result<PipeSizing> {
    let flow = checked("condensate flow (l/h)", flow_lph, 0.0, 1e7)?;
    let slope = checked("slope (%)", slope_percent, 0.1, 50.0)? / 100.0;
    let mut last = None;
    for (od, wall) in DRAIN_PIPES {
        let inner = od - 2.0 * wall;
        let d = inner / 1000.0;
        // Full-bore flow in m³/s; half full carries half of it.
        let full = (1.0 / MANNING_N) * (std::f64::consts::PI * d * d / 4.0) * (d / 4.0).powf(2.0 / 3.0) * slope.sqrt();
        let capacity_lph = 0.5 * full * 1000.0 * 3600.0;
        // The velocity at half depth is the full-bore one.
        let v = full / (std::f64::consts::PI * d * d / 4.0);
        let sizing = PipeSizing { dn_mm: od, inner_mm: inner, velocity_ms: v, gradient_pa_m: 0.0, governing: PipeGoverning::Gradient };
        if od >= 20.0 && capacity_lph >= DRAIN_SAFETY * flow {
            return Ok(PipeSizing { governing: if od <= 20.0 { PipeGoverning::Minimum } else { PipeGoverning::Gradient }, ..sizing });
        }
        last = Some(sizing);
    }
    last.ok_or_else(|| HvacError::BadValue("no drain size".into()))
}

/// The inner diameter, mm, of the steel pipe of nominal diameter `dn_mm`, when it is a standard size.
pub fn steel_inner_diameter_mm(dn_mm: f64) -> Option<f64> {
    inner_of(dn_mm)
}

#[cfg(test)]
mod pipe_tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-9)
    }

    #[test]
    fn the_flow_of_water_follows_the_load_and_the_temperature_difference() {
        // 100 kW at 5 K: 100 000 W / (4.18e6 J/m³K × 5 K) = 4.785e-3 m³/s = 17.2 m³/h.
        assert!(close(water_flow_m3h(100.0, 5.0).unwrap(), 17.225, 1e-3));
        assert!(close(water_flow_m3h(100.0, 20.0).unwrap(), 4.306, 1e-3), "four times less at four times the difference");
        assert_eq!(water_flow_m3h(0.0, 5.0).unwrap(), 0.0);
        assert!(water_flow_m3h(-1.0, 5.0).is_err() && water_flow_m3h(10.0, 0.0).is_err() && water_flow_m3h(f64::NAN, 5.0).is_err());
    }

    #[test]
    fn a_water_pipe_is_the_smallest_whose_velocity_and_loss_are_within_the_limits() {
        let c = PipeCriteria::default();
        let big = size_water_pipe(17.2, WaterTemperature::Cold, &c).unwrap();
        // DN65 (inner 68.9) would run at 1.28 m/s, over its 1.25 m/s: DN80.
        assert_eq!(big.dn_mm, 80.0);
        assert!(close(big.velocity_ms, 0.929, 2e-2), "{}", big.velocity_ms);
        assert!(big.gradient_pa_m > 50.0 && big.gradient_pa_m < 300.0, "{}", big.gradient_pa_m);
        assert_eq!(big.governing, PipeGoverning::Velocity);
        // A trickle takes the smallest size allowed.
        let small = size_water_pipe(0.05, WaterTemperature::Hot, &c).unwrap();
        assert_eq!((small.dn_mm, small.governing), (15.0, PipeGoverning::Minimum));
        // A lower pressure loss limit makes the pipe larger; the velocity of a size can be given.
        let tight = size_water_pipe(17.2, WaterTemperature::Cold, &PipeCriteria { max_gradient_pa_m: 60.0, ..c }).unwrap();
        assert!(tight.dn_mm > big.dn_mm && tight.gradient_pa_m <= 60.0, "{tight:?}");
        let slow = size_water_pipe(17.2, WaterTemperature::Cold, &PipeCriteria { max_velocity_ms: Some(0.5), ..c }).unwrap();
        assert!(slow.dn_mm > big.dn_mm && slow.velocity_ms <= 0.5);
        // More flow, a bigger pipe.
        let more = size_water_pipe(120.0, WaterTemperature::Cold, &c).unwrap();
        assert!(more.dn_mm > big.dn_mm);
        assert!(size_water_pipe(f64::NAN, WaterTemperature::Cold, &c).is_err());
        assert!(size_water_pipe(5.0, WaterTemperature::Cold, &PipeCriteria { max_gradient_pa_m: 0.0, ..c }).is_err());
    }

    #[test]
    fn a_pipe_whose_size_is_given_is_checked_not_changed() {
        let c = PipeCriteria::default();
        let r = check_water_pipe(17.2, 50.0, WaterTemperature::Cold, &c).unwrap();
        assert_eq!((r.dn_mm, r.governing), (50.0, PipeGoverning::Imposed));
        assert!(r.velocity_ms > max_velocity_ms(50.0), "too small for the flow: {}", r.velocity_ms);
        assert!(close(r.inner_mm, 53.1, 1e-9));
    }

    #[test]
    fn a_condensate_drain_is_sized_by_gravity_at_its_slope() {
        // A few litres an hour: the smallest drain, 20 mm.
        let tiny = size_condensate_drain(8.0, 1.0).unwrap();
        assert_eq!((tiny.dn_mm, tiny.governing), (20.0, PipeGoverning::Minimum));
        // A big plant's drain is bigger, and a steeper one is smaller.
        let flat = size_condensate_drain(5_000.0, 0.5).unwrap();
        let steep = size_condensate_drain(5_000.0, 5.0).unwrap();
        assert!(flat.dn_mm > tiny.dn_mm && steep.dn_mm <= flat.dn_mm, "{flat:?} {steep:?}");
        assert!(size_condensate_drain(-1.0, 1.0).is_err() && size_condensate_drain(10.0, 0.0).is_err());
    }
}
