//! The standard air terminal models: generic sizes to start from, not any manufacturer's range.
//! A user changes them, or adds the models they install, in the drawing's library.

use super::{TerminalKind, TerminalModel};

fn model(name: &str, kind: TerminalKind, width_mm: f64, depth_mm: f64, neck_mm: f64, max_flow_m3h: f64) -> TerminalModel {
    TerminalModel {
        name: name.into(),
        kind,
        width_mm,
        depth_mm,
        slots: if kind == TerminalKind::Linear { 2 } else { 0 },
        neck_mm,
        max_flow_m3h,
        pressure_drop_pa: 15.0,
    }
}

/// The standard models, smallest first within each kind.
pub fn standard_air_terminal_models() -> Vec<TerminalModel> {
    use TerminalKind::{Extract, Linear, Rectangular, Round, SquareCeiling};
    vec![
        model("SQ600-160", SquareCeiling, 600.0, 600.0, 160.0, 250.0),
        model("SQ600-200", SquareCeiling, 600.0, 600.0, 200.0, 400.0),
        model("RD160", Round, 300.0, 300.0, 160.0, 200.0),
        model("RD200", Round, 400.0, 400.0, 200.0, 300.0),
        model("RD250", Round, 500.0, 500.0, 250.0, 450.0),
        model("RG300x150", Rectangular, 300.0, 150.0, 160.0, 250.0),
        model("RG400x150", Rectangular, 400.0, 150.0, 200.0, 350.0),
        model("RG500x200", Rectangular, 500.0, 200.0, 250.0, 500.0),
        model("LN1000-2", Linear, 1000.0, 100.0, 160.0, 200.0),
        model("EX100", Extract, 150.0, 150.0, 100.0, 60.0),
        model("EX125", Extract, 180.0, 180.0, 125.0, 90.0),
        model("EX160", Extract, 220.0, 220.0, 160.0, 150.0),
    ]
}
