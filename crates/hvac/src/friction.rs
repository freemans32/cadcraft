//! Friction in a round duct: Darcy-Weisbach, with the friction factor from the Colebrook
//! equation (laminar flow uses 64/Re).

use crate::{Air, Result, checked, round_area_m2};

/// Below this Reynolds number the flow is laminar.
const LAMINAR_REYNOLDS: f64 = 2300.0;
/// Most iterations of the Colebrook equation; it settles in under ten from a good start.
const MAX_ITERATIONS: usize = 50;

/// Mean air velocity, m/s, of `flow_m3h` in a round duct of `diameter_mm`.
pub fn velocity(flow_m3h: f64, diameter_mm: f64) -> f64 {
    flow_m3h / 3600.0 / round_area_m2(diameter_mm)
}

/// Reynolds number of air at `velocity` m/s in a round duct of `diameter_mm`.
pub fn reynolds(velocity: f64, diameter_mm: f64, air: &Air) -> f64 {
    velocity * diameter_mm / 1000.0 / air.kinematic_viscosity
}

/// Darcy friction factor for `reynolds` and a relative roughness `roughness_mm / diameter_mm`.
pub fn friction_factor(reynolds: f64, roughness_mm: f64, diameter_mm: f64) -> Result<f64> {
    let re = checked("Reynolds number", reynolds, 1e-9, 1e9)?;
    let rel = checked("roughness (mm)", roughness_mm, 0.0, 10.0)? / checked("diameter (mm)", diameter_mm, 1.0, 10_000.0)?;
    if re < LAMINAR_REYNOLDS {
        return Ok(64.0 / re);
    }
    // Start from the explicit Swamee-Jain approximation, then iterate Colebrook on x = 1/√f.
    let start = 0.25 / (rel / 3.7 + 5.74 / re.powf(0.9)).log10().powi(2);
    let mut x = 1.0 / start.sqrt();
    for _ in 0..MAX_ITERATIONS {
        let next = -2.0 * (rel / 3.7 + 2.51 * x / re).log10();
        if !next.is_finite() || next <= 0.0 {
            break;
        }
        let done = (next - x).abs() < 1e-12;
        x = next;
        if done {
            break;
        }
    }
    let f = 1.0 / (x * x);
    if f.is_finite() && f > 0.0 { Ok(f) } else { Ok(start) }
}

/// Pressure loss per metre, Pa/m, of `flow_m3h` in a round duct of `diameter_mm` whose wall
/// roughness is `roughness_mm`.
pub fn pressure_gradient(flow_m3h: f64, diameter_mm: f64, roughness_mm: f64, air: &Air) -> Result<f64> {
    checked("flow rate (m³/h)", flow_m3h, 0.0, 1e7)?;
    let d = checked("diameter (mm)", diameter_mm, 1.0, 10_000.0)?;
    if flow_m3h == 0.0 {
        return Ok(0.0);
    }
    let v = velocity(flow_m3h, d);
    let f = friction_factor(reynolds(v, d, air), roughness_mm, d)?;
    Ok(f / (d / 1000.0) * air.dynamic_pressure(v))
}
