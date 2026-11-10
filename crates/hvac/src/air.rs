use serde::{Deserialize, Serialize};

use crate::{Result, checked};

/// The properties of the air carried by the ducts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Air {
    /// Density, kg/m³.
    pub density: f64,
    /// Kinematic viscosity, m²/s.
    pub kinematic_viscosity: f64,
}

impl Default for Air {
    /// Dry air at 20 °C and sea-level pressure.
    fn default() -> Self {
        Air { density: 1.2, kinematic_viscosity: 1.51e-5 }
    }
}

impl Air {
    /// The same air, refused if a property is not a plausible value for air.
    pub fn checked(self) -> Result<Air> {
        checked("air density (kg/m³)", self.density, 0.5, 2.0)?;
        checked("air kinematic viscosity (m²/s)", self.kinematic_viscosity, 1e-6, 1e-4)?;
        Ok(self)
    }

    /// Dynamic pressure ρv²/2 at `velocity` m/s, in Pa.
    pub fn dynamic_pressure(&self, velocity: f64) -> f64 {
        self.density * velocity * velocity / 2.0
    }
}
