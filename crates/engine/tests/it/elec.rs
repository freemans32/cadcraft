use cadcraft_doc::{Drawing, Handle, cable_of, device_of};
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::new();
    *s.doc_mut().unwrap() = Drawing::new_metric();
    s
}

fn hd(v: &Value) -> Handle {
    Handle::parse_hex(v.as_str().unwrap()).unwrap()
}

fn circuit<'a>(r: &'a Value, name: &str) -> &'a Value {
    r["circuits"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("no circuit {name} in {r}"))
}

#[test]
fn a_circuit_is_drawn_checked_sized_and_renamed_through_the_commands() {
    let mut s = metric();
    let at: Vec<Value> = (0..6).map(|i| json!([i * 2000, 500])).collect();
    s.execute("elecdeviceadd", &json!({"points": at, "kind": "socket", "circuit": "TD1-C1"})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [-1500, 0], "kind": "distributionBoard", "name": "TD1", "supply": 40})).unwrap();
    let r = s.execute("cableadd", &json!({"points": [[0, 0], [12000, 0], [12000, 20000]], "circuit": "TD1-C1", "usage": "power"})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 2);
    // Six sockets of 500 W, 32 m: the circuit is read, the section is to be sized.
    let check = s.execute("circuitcheck", &json!({})).unwrap();
    let c = circuit(&check, "TD1-C1");
    assert_eq!((c["devices"].as_u64(), c["powerW"].as_f64(), c["panelDrawn"].as_bool()), (Some(6), Some(3000.0), Some(true)));
    assert_eq!(c["cableLengthM"].as_f64(), Some(32.0));
    assert!(c["requiredSectionMm2"].as_f64().is_some(), "{c}");
    // A section that is too small is told; sizing puts the right one and the circuit is then right.
    s.execute("circuitedit", &json!({"name": "td1-c1", "section": 1.5, "protection": 25})).unwrap();
    let bad = s.execute("circuitcheck", &json!({})).unwrap();
    assert_eq!(bad["problems"], 1, "{bad}");
    let sized = s.execute("circuitsize", &json!({"overwrite": true})).unwrap();
    assert_eq!(sized["problems"], 0, "{sized}");
    let first = hd(&r["handles"][0]);
    let cable = cable_of(s.doc().unwrap().entity(first).unwrap()).unwrap();
    assert_eq!(cable.section_mm2, circuit(&sized, "TD1-C1")["requiredSectionMm2"].as_f64());
    // Renaming moves the cables, the devices and the colour.
    s.execute("circuitedit", &json!({"name": "TD1-C1", "rename": "TD1-C7"})).unwrap();
    let after = s.execute("circuitcheck", &json!({})).unwrap();
    assert_eq!(after["circuits"].as_array().unwrap().len(), 1);
    assert_eq!(circuit(&after, "TD1-C7")["devices"], 6);
    // Hostile values are refused, not turned into numbers.
    assert!(s.execute("elecdeviceadd", &json!({"at": [0, 0], "power": "lots"})).is_err());
    assert!(s.execute("cableadd", &json!({"points": [[0, 0], [10, 0]], "section": 0})).is_err());
    assert!(s.execute("circuitcheck", &json!({"diversity": 2})).is_err());
    assert!(s.execute("elecedit", &json!({"handle": first.hex(), "cosPhi": 7})).is_err());
}

#[test]
fn equipment_and_units_are_fed_by_a_circuit_and_the_quantities_are_priced_and_written() {
    let mut s = metric();
    s.execute("equipmentadd", &json!({"at": [0, 0], "kind": "vrfOutdoor", "name": "GE1"})).unwrap();
    let eq = s.doc().unwrap().model.iter().find(|e| cadcraft_doc::equipment_of(e).is_some()).unwrap().handle;
    s.execute("equipmentedit", &json!({"handle": eq.hex(), "electricKw": 14, "circuit": "TD1-V1"})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 3000], [20000, 3000]], "circuit": "TD1-V1", "phases": "three", "section": 6})).unwrap();
    let r = s.execute("circuitcheck", &json!({})).unwrap();
    let c = circuit(&r, "TD1-V1");
    assert_eq!((c["hvac"].as_u64(), c["powerW"].as_f64()), (Some(1), Some(14_000.0)));
    // The same through the selection.
    s.execute("elecdeviceadd", &json!({"at": [100, 100], "kind": "light"})).unwrap();
    let l = s.doc().unwrap().model.iter().find(|e| device_of(e).is_some()).unwrap().handle;
    s.execute("circuitassign", &json!({"handles": [l.hex()], "circuit": "TD1-E1"})).unwrap();
    assert_eq!(device_of(s.doc().unwrap().entity(l).unwrap()).unwrap().circuit, "TD1-E1");
    let path = std::env::temp_dir().join(format!("cadcraft-elec-{}.xlsx", std::process::id()));
    let t = s
        .execute(
            "electakeoff",
            &json!({"level": "RDC", "prices": {"Cable|3G6": 0.0, "Device|Ceiling light": 45.0}, "path": path.display().to_string()}),
        )
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(bytes.starts_with(b"PK"), "an xlsx");
    assert_eq!(t["costing"]["total"].as_f64(), Some(45.0), "{t}");
    assert_eq!(t["hvac"].as_array().unwrap().len(), 1);
    assert!(s.execute("electakeoff", &json!({"prices": {"Cable|x": -1}})).is_err());
}

#[test]
fn the_lighting_of_a_room_is_checked_against_the_code_by_its_class() {
    let mut s = metric();
    s.execute("spaceadd", &json!({"points": [[0, 0], [6000, 0], [6000, 5000], [0, 5000]], "name": "Bureau", "flow": 100})).unwrap();
    s.execute("spaceadd", &json!({"points": [[10000, 0], [20000, 0], [20000, 1500], [10000, 1500]], "name": "Couloir", "flow": 100})).unwrap();
    let r = s.execute("lightingcheck", &json!({})).unwrap();
    assert_eq!(r["problems"], 2, "no luminaire: {r}");
    // Four panels of 3400 lm in 30 m²: 4 x 3400 x 0.5 x 0.8 / 30 = 181 lux, above 120. One of 700 lm in the corridor (15 m²): 12 lux, below 40.
    for at in [[1000, 1000], [5000, 1000], [1000, 4000], [5000, 4000]] {
        s.execute("elecdeviceadd", &json!({"at": at, "kind": "ceilingLight", "flux": 3400})).unwrap();
    }
    s.execute("elecdeviceadd", &json!({"at": [15000, 700], "kind": "downlight", "flux": 700})).unwrap();
    let r = s.execute("lightingcheck", &json!({})).unwrap();
    let find = |n: &str| r["spaces"].as_array().unwrap().iter().find(|x| x["space"] == n).unwrap().clone();
    assert_eq!((find("Bureau")["lux"].as_f64(), find("Bureau")["ok"].as_bool()), (Some(181.0), Some(true)));
    assert_eq!(
        (find("Couloir")["class"].as_str(), find("Couloir")["minimumLux"].as_f64(), find("Couloir")["ok"].as_bool()),
        (Some("circulation"), Some(40.0), Some(false))
    );
    assert!(s.execute("lightingcheck", &json!({"utilisation": 3})).is_err());
}

#[test]
fn the_cable_and_the_device_are_drawn_from_the_command_line_and_their_grips_work() {
    let mut s = metric();
    for t in ["elecdeviceadd", "ty", "downlight", "c", "TD3-E2", "1000,1000", "2000,1000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none(), "{}", s.prompt_text());
    let lights: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| device_of(e)).collect();
    assert_eq!((lights.len(), lights[0].circuit.as_str(), lights[0].kind.name()), (2, "TD3-E2", "Downlight"));
    for t in ["cableadd", "c", "TD3-E2", "u", "lighting", "0,0", "4000,0", ""] {
        s.cmdline(t).unwrap();
    }
    let cable = s.doc().unwrap().model.iter().find(|e| cable_of(e).is_some()).unwrap().handle;
    s.set_selection(vec![cable]);
    assert_eq!(s.grips_of(cable).unwrap().len(), 4, "its ends, its middle and its label");
    s.start_grip_edit(&[(cable, 2)]).unwrap();
    s.cmdline("5000,0").unwrap();
    let (_, a, b) = s.doc().unwrap().cable_ends(cable).unwrap();
    assert_eq!((a.x.round(), b.x.round()), (0.0, 5000.0));
}

#[test]
fn the_circuit_legend_follows_the_circuits_and_the_emergency_check_tells_where_nothing_is_drawn() {
    let mut s = metric();
    s.execute("elecdeviceadd", &json!({"at": [500, 500], "kind": "socket", "circuit": "TD1-C1"})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 0], [10000, 0]], "circuit": "TD1-C1", "section": 2.5})).unwrap();
    let r = s.execute("circuitlegend", &json!({"at": [0, -5000], "title": "Circuits RDC"})).unwrap();
    let h = hd(&r["handle"]);
    let texts = |s: &Session| -> Vec<String> {
        let d = s.doc().unwrap();
        let cadcraft_doc::EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { panic!("a block reference") };
        d.block(&i.block)
            .unwrap()
            .entities
            .iter()
            .filter_map(|e| if let cadcraft_doc::EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
            .collect()
    };
    assert!(texts(&s).iter().any(|t| t.starts_with("TD1-C1") && t.contains("3G2.5") && t.contains("10 m")), "{:?}", texts(&s));
    // Renaming the circuit redraws the legend.
    s.execute("circuitedit", &json!({"name": "TD1-C1", "rename": "TD1-P4"})).unwrap();
    assert!(texts(&s).iter().any(|t| t.starts_with("TD1-P4")), "{:?}", texts(&s));
    // A corridor with no emergency light is told, and one with a BAES is not.
    s.execute("spaceadd", &json!({"points": [[0, 3000], [10000, 3000], [10000, 4500], [0, 4500]], "name": "Couloir", "flow": 100})).unwrap();
    s.execute("spaceadd", &json!({"points": [[0, 8000], [10000, 8000], [10000, 9500], [0, 9500]], "name": "Couloir 2", "flow": 100})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [5000, 8700], "kind": "emergencyLight"})).unwrap();
    let e = s.execute("emergencycheck", &json!({})).unwrap();
    assert_eq!(e["problems"], 1, "{e}");
    assert!(e["warnings"][0].as_str().unwrap().starts_with("Couloir:"), "{e}");
    assert!(e["note"].as_str().unwrap().contains("2011"));
}

#[test]
fn a_recessed_light_and_a_board_of_a_given_size_and_underside_are_drawn_and_edited_by_command() {
    let mut s = metric();
    let r = s.execute("elecdeviceadd", &json!({"at": [0, 0], "kind": "recessedLight600x600"})).unwrap();
    let light = device_of(s.doc().unwrap().entity(hd(&r["handles"][0])).unwrap()).unwrap();
    assert_eq!((light.kind.name(), light.power_w, light.flux_lm, light.height_mm), ("Recessed light 600x600", 40.0, Some(4000.0), 2600.0));
    let r = s.execute("elecdeviceadd", &json!({"at": [3000, 0], "kind": "distributionBoard", "name": "TD1", "boardHeight": 1800, "boardWidth": 800, "boardDepth": 300, "height": 200})).unwrap();
    let board = hd(&r["handles"][0]);
    let p = device_of(s.doc().unwrap().entity(board).unwrap()).unwrap();
    assert_eq!((p.board_mm(), p.height_mm), ((1800.0, 800.0, 300.0), 200.0));
    // Editing keeps what is not changed; the plan follows the new width.
    s.execute("elecedit", &json!({"handle": board.hex(), "boardWidth": 1200, "height": 0})).unwrap();
    let p = device_of(s.doc().unwrap().entity(board).unwrap()).unwrap();
    assert_eq!((p.board_mm(), p.height_mm), ((1800.0, 1200.0, 300.0), 0.0));
    // A size that a board cannot have, or a size given to another object, changes nothing and says so.
    for (cmd, params) in [
        ("elecedit", json!({"handle": board.hex(), "boardDepth": 5})),
        ("elecedit", json!({"handle": board.hex(), "boardHeight": "tall"})),
        ("elecedit", json!({"handle": hd(&r["handles"][0]).hex(), "boardWidth": 100000})),
        ("elecdeviceadd", json!({"at": [0, 5000], "kind": "socket", "boardWidth": 800})),
    ] {
        assert!(s.execute(cmd, &params).is_err(), "{params}");
    }
    assert_eq!(device_of(s.doc().unwrap().entity(board).unwrap()).unwrap().board_mm(), (1800.0, 1200.0, 300.0));
    let e = s.execute("elecedit", &json!({"handle": hd(&json!(light_handle(&s))).hex(), "boardWidth": 800})).unwrap_err().to_string();
    assert!(e.contains("distribution board"), "{e}");
}

/// The handle of the recessed light of the drawing.
fn light_handle(s: &Session) -> String {
    s.doc().unwrap().model.iter().find(|e| device_of(e).is_some_and(|d| d.kind == cadcraft_doc::DeviceKind::Recessed)).unwrap().handle.hex()
}

#[test]
fn the_short_circuit_checks_tell_a_cable_too_long_for_its_breaker_a_weak_breaker_and_a_board_too_small_for_selectivity() {
    let mut s = metric();
    // A board of 25 A and a circuit of 16 A (2.5 mm² over 100 m of cable): the breaker's magnetic trip (10 x 16 = 160 A, curve C) is not seen at the end.
    s.execute("elecdeviceadd", &json!({"at": [-1000, 0], "kind": "distributionBoard", "name": "TD1", "supply": 25})).unwrap();
    s.execute("elecdeviceadd", &json!({"points": [[0, 500], [2000, 500]], "kind": "socket", "circuit": "TD1-C1", "power": 3000})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 0], [100000, 0]], "circuit": "TD1-C1", "usage": "power"})).unwrap();
    s.execute("circuitedit", &json!({"name": "TD1-C1", "section": 2.5, "protection": 16})).unwrap();
    let check = s.execute("circuitcheck", &json!({})).unwrap();
    let c = circuit(&check, "TD1-C1");
    let text = |c: &Value| c["warnings"].as_array().unwrap().iter().filter_map(|w| w.as_str()).collect::<Vec<_>>().join(" | ");
    assert!(c["ikMinA"].as_f64().unwrap() < 160.0 && c["maxLengthM"].as_f64().unwrap() < 100.0, "{c}");
    assert!(text(c).contains("magnetic trip"), "{}", text(c));
    // The main breaker is 25 A, the circuit's 16 A: 1.56 times, below the 1.6 rule of thumb.
    assert!((c["selectivityRatio"].as_f64().unwrap() - 1.56).abs() < 1e-9);
    assert!(text(c).contains("no selectivity"), "{}", text(c));
    // A shorter cable and a bigger board: both are right.
    let mut t = metric();
    t.execute("elecdeviceadd", &json!({"at": [-1000, 0], "kind": "distributionBoard", "name": "TD1", "supply": 63})).unwrap();
    t.execute("elecdeviceadd", &json!({"at": [0, 500], "kind": "socket", "circuit": "TD1-C1", "power": 3000})).unwrap();
    t.execute("cableadd", &json!({"points": [[0, 0], [20000, 0]], "circuit": "TD1-C1", "usage": "power"})).unwrap();
    t.execute("circuitedit", &json!({"name": "TD1-C1", "section": 2.5, "protection": 16})).unwrap();
    let ok = t.execute("circuitcheck", &json!({})).unwrap();
    let c = circuit(&ok, "TD1-C1");
    assert!(c["warnings"].as_array().unwrap().is_empty(), "{c}");
    assert!((c["ikMinA"].as_f64().unwrap() - 409.0).abs() < 1.0 && (c["ikMaxKa"].as_f64().unwrap() - 0.79).abs() < 0.02, "{c}");
    // A weak breaker for the origin is told once for each circuit, and the options are checked.
    let weak = t.execute("circuitcheck", &json!({"sourceKa": 25, "breakingKa": 10})).unwrap();
    assert!(text(circuit(&weak, "TD1-C1")).contains("breaking capacity"), "{weak}");
    let d = t.execute("circuitcheck", &json!({"curve": "D"})).unwrap();
    // A D16 trips for sure at 320 A, seen up to 26 m of cable: at 20 m it is right still, and the longest cable is shorter than with a C.
    assert!(
        circuit(&d, "TD1-C1")["warnings"].as_array().unwrap().is_empty()
            && (circuit(&d, "TD1-C1")["maxLengthM"].as_f64().unwrap() - 26.0).abs() < 1.0,
        "{d}"
    );
    for bad in [json!({"curve": "E"}), json!({"curve": "CD"}), json!({"sourceKa": 0}), json!({"breakingKa": "x"}), json!({"sourceKa": 5000})] {
        assert!(t.execute("circuitcheck", &bad).is_err(), "{bad}");
    }
}

#[test]
fn the_single_line_diagram_is_drawn_from_the_circuits_and_follows_them() {
    let mut s = metric();
    s.execute("elecdeviceadd", &json!({"at": [-1000, 0], "kind": "distributionBoard", "name": "TD1", "supply": 63})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [0, 500], "kind": "socket", "circuit": "TD1-C1", "power": 3000})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 0], [20000, 0]], "circuit": "TD1-C1", "usage": "power"})).unwrap();
    s.execute("circuitedit", &json!({"name": "TD1-C1", "section": 2.5, "protection": 16})).unwrap();
    let r = s.execute("singleline", &json!({"at": [0, -30000], "sourceKa": 25, "breakingKa": 25, "curve": "B", "title": "TGBT"})).unwrap();
    assert_eq!((r["boards"].as_u64(), r["circuits"].as_u64()), (Some(1), Some(1)), "{r}");
    let h = hd(&r["handle"]);
    let texts = |s: &Session| -> Vec<String> {
        match &s.doc().unwrap().entity(h).unwrap().kind {
            cadcraft_doc::EntityKind::Insert(i) => s
                .doc()
                .unwrap()
                .block(&i.block)
                .unwrap()
                .entities
                .iter()
                .filter_map(|e| if let cadcraft_doc::EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
                .collect(),
            k => panic!("{k:?}"),
        }
    };
    let t = texts(&s);
    assert!(
        t.iter().any(|x| x.starts_with("TGBT") && x.contains("25.0 kA")) && t.contains(&"16 A B".to_string()) && t.contains(&"63 A".to_string()),
        "{t:?}"
    );
    // Sizing and editing the circuit redraws the diagram by itself.
    s.execute("circuitedit", &json!({"name": "TD1-C1", "protection": 20})).unwrap();
    assert!(texts(&s).contains(&"20 A B".to_string()), "{:?}", texts(&s));
    // A bad option is refused with no diagram drawn; the command line puts one at a click.
    let n = s.doc().unwrap().model.len();
    assert!(s.execute("singleline", &json!({"at": [0, -50000], "curve": "Z"})).is_err());
    assert!(s.execute("singleline", &json!({"curve": "B"})).is_err(), "a place is needed");
    assert_eq!(s.doc().unwrap().model.len(), n);
    s.cmdline("singleline").unwrap();
    s.cmdline("0,-60000").unwrap();
    assert!(s.running.is_none());
    assert_eq!(s.doc().unwrap().model.len(), n + 1);
}

#[test]
fn a_board_fed_by_a_circuit_of_another_gives_it_its_load_its_short_circuit_current_and_its_voltage_drop() {
    let mut s = metric();
    // TGBT feeds TD1 by the circuit TGBT-D1 (35 mm², 40 m, 63 A); TD1 has one circuit of 3 kW of sockets.
    s.execute("elecdeviceadd", &json!({"at": [-3000, 0], "kind": "distributionBoard", "name": "TGBT", "supply": 160})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [-1000, 0], "kind": "distributionBoard", "name": "TD1", "supply": 32, "circuit": "TGBT-D1"})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [0, 500], "kind": "socket", "circuit": "TD1-C1", "power": 3000})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 0], [20000, 0]], "circuit": "TD1-C1", "usage": "power"})).unwrap();
    s.execute("circuitedit", &json!({"name": "TD1-C1", "section": 2.5, "protection": 16})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 3000], [40000, 3000]], "circuit": "TGBT-D1", "usage": "power"})).unwrap();
    s.execute("circuitedit", &json!({"name": "TGBT-D1", "section": 16, "protection": 63})).unwrap();
    let check = s.execute("circuitcheck", &json!({})).unwrap();
    let feeder = circuit(&check, "TGBT-D1");
    let branch = circuit(&check, "TD1-C1");
    // The feeder carries the board's load, and says which board it feeds.
    assert_eq!(feeder["powerW"].as_f64(), Some(3000.0), "{feeder}");
    assert_eq!(feeder["feeds"], json!(["TD1"]));
    // Its end is where TD1's circuits start: the short-circuit current there is lower than the origin's, and so the circuit's, below.
    let at_board = feeder["ikMaxKa"].as_f64().unwrap();
    assert!(at_board < 10.0 && at_board > 1.0, "{feeder}");
    assert!(branch["ikMaxKa"].as_f64().unwrap() < at_board, "{branch}");
    // The voltage drop of the feeder is counted in the drop of the circuits of the board.
    assert!(
        branch["upstreamDropPercent"].as_f64().unwrap() > 0.5
            && (branch["upstreamDropPercent"].as_f64().unwrap() - feeder["dropPercent"].as_f64().unwrap()).abs() < 0.011,
        "{branch} {feeder}"
    );
    assert!(feeder.get("upstreamDropPercent").is_none_or(Value::is_null), "the first level has nothing upstream: {feeder}");
    // The feeder's breaker (63 A) is twice the board's main one (32 A): enough; a 50 A one (1.56 times) is not.
    assert!(feeder["warnings"].as_array().unwrap().is_empty(), "{feeder}");
    s.execute("circuitedit", &json!({"name": "TGBT-D1", "protection": 50})).unwrap();
    let again = s.execute("circuitcheck", &json!({})).unwrap();
    assert!(
        circuit(&again, "TGBT-D1")["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("main breaker of the board TD1")),
        "{again}"
    );
    // A board fed by its own circuit (a loop) is cut after a few levels, with no hang and no panic.
    s.execute("elecdeviceadd", &json!({"at": [-5000, 0], "kind": "distributionBoard", "name": "TGBT2", "circuit": "TGBT2-X"})).unwrap();
    s.execute("elecdeviceadd", &json!({"at": [-6000, 0], "kind": "socket", "circuit": "TGBT2-X", "power": 1000})).unwrap();
    assert!(s.execute("circuitcheck", &json!({})).is_ok());
}

#[test]
fn the_lighting_classes_of_the_table_of_article_r4223_4_are_told_by_the_name_of_the_space() {
    let mut s = metric();
    let rooms = [
        ("Bureau", "workRoom", 120.0),
        ("Salle de tri aveugle", "blindWorkRoom", 200.0),
        ("Couloir RDC", "circulation", 40.0),
        ("Escalier A", "stairsWarehouse", 60.0),
        ("Parvis extérieur", "outdoorCirculation", 10.0),
        ("Quai de travail extérieur", "outdoorWork", 40.0),
    ];
    for (k, (name, ..)) in rooms.iter().enumerate() {
        let y = k as f64 * 4000.0;
        s.execute("spaceadd", &json!({"points": [[0, y], [3000, y], [3000, y + 3000.0], [0, y + 3000.0]], "name": name, "flow": 10})).unwrap();
    }
    let r = s.execute("lightingcheck", &json!({})).unwrap();
    for (name, class, minimum) in rooms {
        let row = r["spaces"].as_array().unwrap().iter().find(|x| x["space"] == name).unwrap();
        assert_eq!((row["class"].as_str(), row["minimumLux"].as_f64()), (Some(class), Some(minimum)), "{name}");
    }
}

#[test]
fn a_power_or_lighting_cable_under_1_5_mm2_of_copper_is_told_and_a_signal_cable_is_not() {
    let mut s = metric();
    s.execute("cableadd", &json!({"points": [[0, 0], [5000, 0]], "circuit": "TD1-C1", "usage": "lighting", "section": 1.0})).unwrap();
    s.execute("cableadd", &json!({"points": [[0, 500], [5000, 500]], "circuit": "BUS1", "usage": "bus", "section": "2x0.75 blindé"})).unwrap();
    let r = s.execute("circuitcheck", &json!({})).unwrap();
    let warns = |n: &str| circuit(&r, n)["warnings"].as_array().unwrap().iter().filter_map(|w| w.as_str()).collect::<Vec<_>>().join(" | ");
    assert!(warns("TD1-C1").contains("1.5 mm² of copper"), "{}", warns("TD1-C1"));
    assert!(!warns("BUS1").contains("1.5 mm²"), "{}", warns("BUS1"));
}
