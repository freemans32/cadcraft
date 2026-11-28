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
    d.save_cable_type(cadcraft_doc::CableType {
        name: "Bus KNX".into(),
        usage: Some(cadcraft_doc::CableUsage::Bus),
        spec: cadcraft_doc::parse_cable_spec("2x0.8 blindé").expect("a cable"),
    })
    .expect("a cable type");
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
    let t = TerminalParams { model, flow_m3h: 180.0, air: AirDirection::Supply, label_height_mm: 100.0, bottom_mm: None, units: HvacUnits::of(&d) };
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
        assert_eq!(back.cable_types, d.cable_types, "{name}: the drawing's cable types");
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
        typed_flows: Vec::new(),
        port_sizes: Vec::new(),
        port_levels_mm: Vec::new(),
        height_mm: None,
        model: String::new(),
        weight_kg: None,
        coils: Default::default(),
        electric_kw: None,
        circuit: String::new(),
        address: String::new(),
        show_address: true,
        label_height_mm: 100.0,
        bottom_mm: None,
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
        reducer_len_mm: None,
        reducer_shape: None,
        label_shift_mm: None,
        kept_edge: None,
        hide_label: false,
        cut_deg: [0.0, 0.0],
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_set: false,
        insulation: None,
        cap_start: false,
        cap_end: false,
        flow_forward: None,
        open_flow_m3h: None,
        gaps_mm: Vec::new(),
        covers: Vec::new(),
        jogs: Vec::new(),
        level_mm: None,
        end_level_mm: None,
        steps: Vec::new(),
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
        bottom_mm: None,
        units,
        air: None,
        height_mm: None,
        fixed: false,
        overhang_mm: None,
    };
    d.add_accessory(&Space::Model, Common::default(), &silencer, Vec2::new(3000.0, 0.0), 0.0).expect("add accessory");
    let model = standard_air_terminal_models().into_iter().next().expect("a model");
    let t = TerminalParams { model, flow_m3h: 150.0, air: AirDirection::Supply, label_height_mm: 100.0, bottom_mm: None, units };
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
        flow_set: false,
        size: Some(RiserSize::Rectangular { width_mm: 400.0, height_mm: 250.0 }),
        incoming_size: None,
        size_set: true,
        height_mm: Some(2800.0),
        label_height_mm: 100.0,
        units: HvacUnits::default(),
        cable: Default::default(),
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

#[test]
fn a_pipework_part_and_the_models_of_the_drawing_survive_dxf_and_dwg_and_its_application_is_registered() {
    let mut d = Drawing::new_metric();
    let lib = d.part_entry_named("Balancing valve (TA)").unwrap();
    let mut p = lib.params("V1", HvacUnits::default());
    p.kvs = Some(6.3);
    p.height_mm = Some(210.0);
    d.add_part(&Space::Model, Common::default(), &p, Vec2::new(1000.0, 2000.0), 0.3).unwrap();
    // A model of the drawing's own, made from that part.
    d.save_part_model("Vanne TA maison", cadcraft_doc::PartCategory::Valves, &p).unwrap();
    for name in ["part.dxf", "part.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let found: Vec<cadcraft_doc::PartParams> = back.model.iter().filter_map(|e| cadcraft_doc::part_of(e)).collect();
        assert_eq!(found, std::slice::from_ref(&p), "{name}: the part");
        assert_eq!(back.part_models, d.part_models, "{name}: the drawing's models");
        assert!(back.part_entry_named("vanne ta maison").is_some_and(|e| e.user && e.template.kvs == Some(6.3)), "{name}");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "part.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{}", cadcraft_doc::PART_APP)), "APPID record");
}

#[test]
fn named_networks_with_their_styles_and_a_legend_survive_dxf_and_dwg() {
    use cadcraft_doc::{PipeLeg, PipeParams, PipeService, network_legend_of, pipe_of};
    let mut d = Drawing::new_metric();
    d.set_network_style("Réseau toiture", Some([1, 2, 3]), Some("NET_DASH")).unwrap();
    d.set_network_style("CTA 2", None, None).unwrap();
    let p = PipeParams {
        leg: PipeLeg::Supply,
        network: "Réseau toiture".into(),
        ..PipeParams::new(PipeService::ChilledWater, 65.0, HvacUnits::default())
    };
    d.add_pipe(&Space::Model, Common::default(), p, Vec2::new(0.0, 0.0), Vec2::new(5000.0, 0.0)).unwrap();
    d.add_network_legend(&Space::Model, Common::default(), &cadcraft_doc::NetworkLegendParams::new(HvacUnits::default()), Vec2::new(0.0, 3000.0))
        .unwrap();
    for name in ["net.dxf", "net.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.network_styles, d.network_styles, "{name}: the styles");
        let pipes: Vec<PipeParams> = back.model.iter().filter_map(|e| pipe_of(e)).collect();
        assert_eq!(pipes.len(), 1, "{name}");
        assert_eq!(pipes[0].network, "Réseau toiture", "{name}");
        assert_eq!(back.model.iter().filter(|e| network_legend_of(e).is_some()).count(), 1, "{name}: the legend");
        // The layers kept their colour and line type as they were saved.
        let l = back.layer("M-PIPE-CHWS-RESEAU_TOITURE").unwrap_or_else(|| panic!("{name}: the network's layer"));
        assert_eq!(l.color, cadcraft_color::Color::True(cadcraft_color::Rgb(1, 2, 3)), "{name}");
        assert_eq!(l.linetype, "NET_DASH", "{name}");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "net.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{}", cadcraft_doc::NETWORK_LEGEND_APP)), "APPID record");
}

#[test]
fn cables_devices_and_equipment_circuits_survive_dxf_and_dwg_and_their_applications_are_registered() {
    use cadcraft_doc::{CABLE_APP, CableParams, CableUsage, DEVICE_APP, DeviceKind, DeviceParams, cable_of, device_of};
    let mut d = Drawing::new_metric();
    let mut c = CableParams::new("TD1-C3", CableUsage::Lighting, HvacUnits::default());
    c.section_mm2 = Some(2.5);
    c.protection_a = Some(16.0);
    d.add_cable(&Space::Model, Common::default(), c, Vec2::new(0.0, 0.0), Vec2::new(8000.0, 0.0)).unwrap();
    let mut p = DeviceParams::new(DeviceKind::DownLight, "L1", HvacUnits::default());
    p.circuit = "TD1-C3".into();
    p.flux_lm = Some(2300.0);
    d.add_device(&Space::Model, Common::default(), &p, Vec2::new(500.0, 1000.0), 0.0).unwrap();
    for name in ["elec.dxf", "elec.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let cables: Vec<CableParams> = back.model.iter().filter_map(|e| cable_of(e)).collect();
        let devices: Vec<DeviceParams> = back.model.iter().filter_map(|e| device_of(e)).collect();
        assert_eq!((cables.len(), cables[0].section_mm2, cables[0].protection_a), (1, Some(2.5), Some(16.0)), "{name}");
        assert_eq!(devices, std::slice::from_ref(&p), "{name}: the device");
        assert_eq!(back.network_styles, d.network_styles, "{name}: the style of the circuit");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "elec.dxf").expect("dxf")).expect("utf-8");
    for app in [CABLE_APP, DEVICE_APP] {
        assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{app}")), "APPID record {app}");
    }
}

#[test]
fn control_devices_survive_dxf_and_dwg_and_their_application_is_registered() {
    use cadcraft_doc::{CONTROL_APP, ControlKind, ControlParams, control_of};
    let mut d = Drawing::new_metric();
    let mut p = ControlParams::new(ControlKind::Controller, "AU1", HvacUnits::default());
    p.circuit = "TD1-R1".into();
    p.bus = "BUS1".into();
    p.capacity.ai = 12;
    d.add_control(&Space::Model, Common::default(), &p, Vec2::new(500.0, 800.0), 0.0).unwrap();
    let mut q = ControlParams::new(ControlKind::Thermostat, "TH1", HvacUnits::default());
    q.controller = "AU1".into();
    q.setpoint = Some(21.0);
    d.add_control(&Space::Model, Common::default(), &q, Vec2::new(900.0, 800.0), 0.0).unwrap();
    for name in ["ctrl.dxf", "ctrl.dwg"] {
        let back = cadcraft_io::read(&cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}")), name)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut found: Vec<ControlParams> = back.model.iter().filter_map(|e| control_of(e)).collect();
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found, vec![p.clone(), q.clone()], "{name}");
    }
    let text = String::from_utf8(cadcraft_io::write(&d, "ctrl.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{CONTROL_APP}")), "APPID record");
}

#[test]
fn the_pages_of_pdfs_attached_on_the_web_are_kept_in_dxf_and_dwg_and_the_pixels_of_files_are_not() {
    let (w, h) = (6u32, 4u32);
    let pixels = |k: u8| -> Vec<u8> { (0..w * h).flat_map(|i| [(i as u8).wrapping_mul(k), k, 255 - k, 255]).collect() };
    let page = cadcraft_doc::Raster::new(w, h, pixels(9)).expect("a raster");
    let file = cadcraft_doc::Raster::new(w, h, pixels(3)).expect("a raster");
    let mut d = Drawing::new_metric();
    d.rasters.insert("pdf-underlay:plan.pdf:1:1", std::sync::Arc::new(page.clone()));
    d.rasters.insert("C:/images/photo.png", std::sync::Arc::new(file));
    for name in ["pages.dxf", "pages.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let kept = back.rasters.get("pdf-underlay:plan.pdf:1:1").unwrap_or_else(|| panic!("{name}: the page is kept"));
        assert_eq!((kept.width(), kept.height(), kept.rgba()), (w, h, page.rgba()), "{name}");
        assert!(back.rasters.get("C:/images/photo.png").is_none(), "{name}: a file's pixels come from the file");
    }
}

#[test]
fn the_colours_of_pipes_and_ducts_survive_dxf_and_dwg() {
    let mut d = Drawing::new_metric();
    d.set_colour_convention(cadcraft_doc::ColourConvention::NfX08100).expect("a convention");
    for name in ["conv.dxf", "conv.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.colour_convention, cadcraft_doc::ColourConvention::NfX08100, "{name}");
    }
    // The plan's own is not written, and is what a drawing without it has.
    let plain = cadcraft_io::read(&cadcraft_io::write(&Drawing::new_metric(), "p.dxf").unwrap(), "p.dxf").unwrap();
    assert_eq!(plain.colour_convention, cadcraft_doc::ColourConvention::Plan);
}
