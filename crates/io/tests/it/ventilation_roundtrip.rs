//! Spaces and air terminals (block references carrying `CADCRAFT_SPACE` and
//! `CADCRAFT_AIRTERMINAL` xdata) and the drawing's air terminal models survive DXF and DWG saves.

use cadcraft_doc::{
    ACCESSORY_APP, AHU_APP, AIR_TERMINAL_APP, AccessoryKind, AccessoryParams, AhuKind, AhuParams, AirDirection, Common, DUCT_APP, Drawing, DuctKind,
    DuctParams, DuctShape, EQUIPMENT_APP, EquipmentParams, HvacUnits, RISER_APP, RiserParams, RiserSize, SPACE_APP, Space, SpaceParams,
    TerminalParams, air_terminal_of, equipment_model_named, equipment_of, riser_of, space_of, standard_air_terminal_models,
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
        void_height_mm: Some(350.0),
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

/// A unit, a trunk with a branch up from a tee, and a terminal at the end of each run.
fn ducted() -> Drawing {
    let mut d = Drawing::new_metric();
    let units = HvacUnits::of(&d);
    let ahu = AhuParams {
        name: "CTA1".into(),
        kind: AhuKind::DoubleFlow,
        ports: vec![],
        width_mm: 1500.0,
        depth_mm: 800.0,
        flow_m3h: Some(500.0),
        pressure_pa: Some(120.0),
        extract_flow_m3h: Some(450.0),
        extract_pressure_pa: Some(90.0),
        fresh_flow_m3h: None,
        fresh_pressure_pa: None,
        exhaust_flow_m3h: None,
        exhaust_pressure_pa: None,
        port_sizes: Vec::new(),
        height_mm: None,
        model: String::new(),
        weight_kg: None,
        label_height_mm: 100.0,
        units,
    };
    d.add_ahu(&Space::Model, Common::default(), &ahu, Vec2::ZERO, 0.0).expect("add unit");
    let duct = DuctParams {
        length: 1.0,
        diameter_mm: 250.0,
        imposed: false,
        kind: DuctKind::Auto,
        kind_found: None,
        air: AirDirection::Supply,
        air_set: true,
        flow_m3h: Some(500.0),
        velocity: Some(3.5),
        shape: DuctShape::Round,
        width_mm: 0.0,
        height_mm: 0.0,
        flexible: false,
        reducer: None,
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_forward: None,
        open_flow_m3h: None,
        gaps_mm: Vec::new(),
        jogs: Vec::new(),
        bottom_mm: None,
        label_height_mm: 100.0,
        units,
    };
    for (a, b) in [((750.0, 0.0), (6000.0, 0.0)), ((6000.0, 0.0), (9000.0, 0.0)), ((6000.0, 0.0), (6000.0, 3000.0))] {
        d.add_duct(&Space::Model, Common::default(), duct.clone(), Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).expect("add duct");
    }
    let silencer = AccessoryParams {
        kind: AccessoryKind::Silencer,
        length_mm: 1200.0,
        diameter_mm: 250.0,
        zeta: None,
        pressure_drop_pa: Some(35.0),
        label_height_mm: 100.0,
        units,
    };
    d.add_accessory(&Space::Model, Common::default(), &silencer, Vec2::new(3000.0, 0.0), 0.0).expect("add accessory");
    let model = standard_air_terminal_models().into_iter().next().expect("a model");
    let t = TerminalParams { model, flow_m3h: 150.0, air: AirDirection::Supply, label_height_mm: 100.0, units };
    for at in [(9000.0, 0.0), (6000.0, 3000.0)] {
        d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(at.0, at.1), 0.0).expect("add terminal");
    }
    d
}

#[test]
fn ducts_and_units_survive_dxf_and_dwg_and_the_network_is_read_back() {
    let d = ducted();
    let want = d.duct_networks().expect("network");
    assert_eq!(want[0].network.segments.len(), 3);
    for name in ["ducts.dxf", "ducts.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let got = back.duct_networks().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(got.len(), 1, "{name}: one unit");
        assert_eq!(got[0].network, want[0].network, "{name}: the same network");
        assert_eq!(got[0].network.segments[0].fixed_pa, 35.0, "{name}: the silencer's loss is in it");
        let unit = back.model.iter().find_map(|e| cadcraft_doc::ahu_of(e)).expect("unit");
        assert_eq!((unit.name.as_str(), unit.flow_m3h, unit.pressure_pa), ("CTA1", Some(500.0), Some(120.0)), "{name}");
        assert_eq!((unit.extract_flow_m3h, unit.extract_pressure_pa), (Some(450.0), Some(90.0)), "{name}");
    }
}

#[test]
fn other_programs_see_the_duct_applications_registered() {
    let text = String::from_utf8(cadcraft_io::write(&ducted(), "ducts.dxf").expect("dxf")).expect("utf-8");
    for app in [AHU_APP, DUCT_APP, ACCESSORY_APP] {
        assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{app}")), "APPID record for {app}");
    }
}

#[test]
fn a_riser_survives_dxf_and_dwg_and_its_application_is_registered() {
    let mut d = Drawing::new_metric();
    let p = RiserParams {
        name: "VMC 3".into(),
        air: AirDirection::Extract,
        up: false,
        outline: vec![Vec2::new(0.0, 0.0), Vec2::new(600.0, 0.0), Vec2::new(600.0, 400.0), Vec2::new(0.0, 400.0)],
        incoming_flow_m3h: Some(660.0),
        flow_m3h: Some(900.0),
        size: Some(RiserSize::Rectangular { width_mm: 400.0, height_mm: 250.0 }),
        size_set: true,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    };
    d.add_riser(&Space::Model, Common::default(), p.clone()).unwrap();
    for name in ["riser.dxf", "riser.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let risers: Vec<RiserParams> = back.model.iter().filter_map(|e| riser_of(e)).collect();
        assert_eq!(risers, std::slice::from_ref(&p), "{name}: the riser");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "riser.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{RISER_APP}")), "APPID record");
}

#[test]
fn an_equipment_survives_dxf_and_dwg_and_its_application_is_registered() {
    let model = equipment_model_named(&cadcraft_doc::standard_equipment_models(), "Heat-pump chiller 87 kW").unwrap();
    let mut p = EquipmentParams::from_model(&model, "PAC1", HvacUnits::default());
    p.weight_kg = Some(800.0);
    p.connections = "Victaulic 2\"".into();
    let mut d = Drawing::new_metric();
    d.add_equipment(&Space::Model, Common::default(), &p, Vec2::new(1000.0, 2000.0), 0.0).unwrap();
    for name in ["equip.dxf", "equip.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let found: Vec<EquipmentParams> = back.model.iter().filter_map(|e| equipment_of(e)).collect();
        assert_eq!(found, std::slice::from_ref(&p), "{name}: the equipment");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "equip.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{EQUIPMENT_APP}")), "APPID record");
}
