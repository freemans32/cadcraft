//! Minimum flow rates of rooms in workplaces, from the French labour code: fresh air per
//! occupant in rooms where people are the only pollution (article R4222-6, mechanical
//! ventilation), and air per room in sanitary rooms (article R4212-6).

use serde::{Deserialize, Serialize};

use crate::{HvacError, Result};

/// Most occupants or fixtures one room is given.
const MAX_COUNT: u32 = 100_000;

/// What a room is used for, as the labour code groups rooms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "use")]
pub enum RoomUse {
    /// Offices and rooms without physical work: 25 m³/h per occupant.
    Office,
    /// Restaurants, shops and meeting rooms: 30 m³/h per occupant.
    DiningSalesMeeting,
    /// Workshops and rooms with light physical work: 45 m³/h per occupant.
    LightWorkshop,
    /// Other workshops and rooms: 60 m³/h per occupant.
    OtherWorkshop,
    /// A separate toilet: 30 m³/h, or 15 m³/h when it is not for collective use.
    Toilet,
    /// A separate bathroom or shower room: 45 m³/h, or 15 m³/h when not for collective use.
    Shower,
    /// A bathroom or shower room with a toilet: 60 m³/h, or 15 m³/h when not for collective use.
    ShowerWithToilet,
    /// Grouped baths, showers and toilets: 30 + 15 m³/h per fixture.
    GroupedSanitary { fixtures: u32 },
    /// Grouped washbasins: 10 + 5 m³/h per washbasin.
    GroupedWashbasins { fixtures: u32 },
}

impl RoomUse {
    /// Fresh air per occupant, m³/h, for a room whose rate depends on its occupants.
    pub fn per_occupant(self) -> Option<f64> {
        match self {
            RoomUse::Office => Some(25.0),
            RoomUse::DiningSalesMeeting => Some(30.0),
            RoomUse::LightWorkshop => Some(45.0),
            RoomUse::OtherWorkshop => Some(60.0),
            _ => None,
        }
    }
}

/// The minimum flow rate of a room, m³/h. `occupants` counts for the rooms whose rate is per
/// occupant; `collective` says whether a toilet or a bathroom is for collective use.
pub fn fresh_air_rate(room: RoomUse, occupants: u32, collective: bool) -> Result<f64> {
    let count = |n: u32, what: &str| {
        if n <= MAX_COUNT { Ok(f64::from(n)) } else { Err(HvacError::BadValue(format!("{what} = {n} (at most {MAX_COUNT})"))) }
    };
    if let Some(per) = room.per_occupant() {
        return Ok(per * count(occupants, "occupants")?);
    }
    let individual = if collective { None } else { Some(15.0) };
    Ok(match room {
        RoomUse::Toilet => individual.unwrap_or(30.0),
        RoomUse::Shower => individual.unwrap_or(45.0),
        RoomUse::ShowerWithToilet => individual.unwrap_or(60.0),
        RoomUse::GroupedSanitary { fixtures } => 30.0 + 15.0 * count(fixtures, "fixtures")?,
        RoomUse::GroupedWashbasins { fixtures } => 10.0 + 5.0 * count(fixtures, "washbasins")?,
        _ => 0.0,
    })
}
