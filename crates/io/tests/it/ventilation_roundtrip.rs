//! Spaces and air terminals (block references carrying `CADCRAFT_SPACE` and
//! `CADCRAFT_AIRTERMINAL` xdata) and the drawing's air terminal models survive DXF and DWG saves.

use cadcraft_doc::{
    AIR_TERMINAL_APP, AirDirection, Common, Drawing, HvacUnits, SPACE_APP, Space, SpaceParams, TerminalParams, air_terminal_of, space_of,
    standard_air_terminal_models,
};
use cadcraft_geom::Vec2;
use cadcraft_hvac::RoomUse;

fn drawing() -> (Drawing, SpaceParams, TerminalParams) {
    let mut d = Drawing::new_metric();
    let mut model = standard_air_terminal_models().into_iter().next().expect("a model");
    model.name = "Diffuseur 4 voies 600".into();
    model.max_flow_m3h = 320.0;
    d.air_terminal_models.push(model.clone());
    let space = SpaceParams {
        name: "Salle de réunion".into(),
        room: RoomUse::DiningSalesMeeting,
        occupants: 12,
        collective: true,
        flow_m3h: None,
        outline: vec![Vec2::new(0.0, 0.0), Vec2::new(8000.0, 0.0), Vec2::new(8000.0, 5000.0), Vec2::new(0.0, 5000.0)],
        label_height_mm: 200.0,
        units: HvacUnits::of(&d),
    };
    d.add_space(&Space::Model, Common::default(), space.clone()).expect("add space");
    let t = TerminalParams { model, flow_m3h: 180.0, air: AirDirection::Supply, label_height_mm: 100.0, units: HvacUnits::of(&d) };
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(2000.0, 2500.0), 0.0).expect("add terminal");
    (d, space, t)
}

#[test]
fn spaces_terminals_and_models_survive_dxf_and_dwg() {
    let (d, space, terminal) = drawing();
    for name in ["hvac.dxf", "hvac.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let spaces: Vec<SpaceParams> = back.model.iter().filter_map(|e| space_of(e)).collect();
        assert_eq!(spaces.len(), 1, "{name}: one space");
        assert_eq!(spaces[0].name, space.name, "{name}");
        assert_eq!(spaces[0].occupants, 12, "{name}");
        let terminals: Vec<TerminalParams> = back.model.iter().filter_map(|e| air_terminal_of(e)).collect();
        assert_eq!(terminals, std::slice::from_ref(&terminal), "{name}: the terminal");
        assert_eq!(back.air_terminal_models, d.air_terminal_models, "{name}: the drawing's models");
    }
}

#[test]
fn other_programs_see_the_applications_registered() {
    let (d, _, _) = drawing();
    let text = String::from_utf8(cadcraft_io::write(&d, "hvac.dxf").expect("dxf")).expect("utf-8");
    for app in [SPACE_APP, AIR_TERMINAL_APP] {
        assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{app}")), "APPID record for {app}");
    }
}
