use serde::{Deserialize, Serialize};

use crate::{Result, checked};

/// A fitting on a duct run: its loss is ζ times the run's dynamic pressure.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Fitting {
    Elbow90,
    Elbow45,
    /// A tee, for the air that goes straight through it.
    TeeStraight,
    /// A tee, for the air that turns into the branch.
    TeeBranch,
    Reducer,
    /// Any other fitting, with its own loss coefficient.
    Custom {
        zeta: f64,
    },
}

/// Loss coefficients ζ of the standard fittings. The defaults are indicative values for
/// smooth spiral-duct fittings (a pressed 90° elbow, a tee with a conical branch); a user sets
/// the coefficients of the fittings they actually install.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FittingCoefficients {
    pub elbow90: f64,
    pub elbow45: f64,
    pub tee_straight: f64,
    pub tee_branch: f64,
    pub reducer: f64,
}

impl Default for FittingCoefficients {
    fn default() -> Self {
        FittingCoefficients { elbow90: 0.3, elbow45: 0.2, tee_straight: 0.2, tee_branch: 1.0, reducer: 0.1 }
    }
}

/// Largest loss coefficient accepted for one fitting.
const MAX_ZETA: f64 = 100.0;

impl FittingCoefficients {
    /// The same coefficients, refused if one is negative, too large or not a number.
    pub fn checked(self) -> Result<FittingCoefficients> {
        for (name, z) in [
            ("elbow 90° ζ", self.elbow90),
            ("elbow 45° ζ", self.elbow45),
            ("tee straight ζ", self.tee_straight),
            ("tee branch ζ", self.tee_branch),
            ("reducer ζ", self.reducer),
        ] {
            checked(name, z, 0.0, MAX_ZETA)?;
        }
        Ok(self)
    }

    /// ζ of `fitting`.
    pub fn zeta(&self, fitting: Fitting) -> Result<f64> {
        Ok(match fitting {
            Fitting::Elbow90 => self.elbow90,
            Fitting::Elbow45 => self.elbow45,
            Fitting::TeeStraight => self.tee_straight,
            Fitting::TeeBranch => self.tee_branch,
            Fitting::Reducer => self.reducer,
            Fitting::Custom { zeta } => checked("fitting ζ", zeta, 0.0, MAX_ZETA)?,
        })
    }
}
