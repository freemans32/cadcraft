//! EQUIPMENTADD places chillers, outdoor units and indoor units from a library of models, by name or by
//! power, with any value given instead; EQUIPMENTEDIT changes them.

use cadcraft_doc::{Drawing, EquipmentKind, Handle, equipment_of};
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

fn equip(s: &Session, h: &Value) -> cadcraft_doc::EquipmentParams {
    equipment_of(s.doc().unwrap().entity(hd(h)).unwrap()).unwrap()
}

#[test]
fn equipmentadd_takes_a_model_by_name_or_by_power_and_any_value_given() {
    let mut s = metric();
    // By power: the smallest chiller that gives 100 kW.
    let r = s.execute("equipmentadd", &json!({"at": [0, 0], "kind": "chiller", "capacity": 100})).unwrap();
    let e = equip(&s, &r["handle"]);
    assert_eq!(
        (e.model.as_str(), e.capacity_kw, e.length_mm, e.width_mm, e.fans, e.clearance_mm),
        ("Chiller 107 kW", Some(107.0), 2275.0, 2125.0, 2, 1000.0),
        "{r}"
    );
    assert_eq!(e.name, "GE1");
    // By name, with the name, the sizes and the power given instead.
    let r = s
        .execute("equipmentadd", &json!({"at": [5000, 0], "model": "Chiller 523 kW", "name": "CH-TOIT", "length": 5000, "weight": 3000, "capacityKw": 500, "connections": "Victaulic 5\""}))
        .unwrap();
    let e = equip(&s, &r["handle"]);
    assert_eq!(
        (e.name.as_str(), e.length_mm, e.width_mm, e.weight_kg, e.capacity_kw, e.fans),
        ("CH-TOIT", 5000.0, 2253.0, Some(3000.0), Some(500.0), 8)
    );
    // An indoor unit by kind and power.
    let r = s.execute("equipmentadd", &json!({"at": [0, 5000], "kind": "cassette", "capacity": 3})).unwrap();
    let e = equip(&s, &r["handle"]);
    assert_eq!((e.kind, e.model.as_str(), e.name.as_str()), (EquipmentKind::Cassette, "Cassette 600 3.5 kW", "UI3"));
    // Refused.
    assert!(s.execute("equipmentadd", &json!({"at": [0, 0], "model": "nothing"})).is_err());
    assert!(s.execute("equipmentadd", &json!({"at": [0, 0], "kind": "boiler"})).is_err());
    assert!(s.execute("equipmentadd", &json!({"at": [0, 0], "length": -5})).is_err());
    assert!(s.execute("equipmentadd", &json!({"kind": "chiller"})).is_err(), "a point is required");
    // The library is listed.
    let all = s.execute("equipmentadd", &json!({"list": true})).unwrap();
    let chillers = s.execute("equipmentadd", &json!({"list": true, "kind": "chiller"})).unwrap();
    assert!(all["models"].as_array().unwrap().len() > chillers["models"].as_array().unwrap().len());
    assert!(chillers["models"].as_array().unwrap().iter().all(|m| m["kind"] == "chiller"));
}

#[test]
fn equipmentedit_changes_each_value_or_takes_another_model_and_keeps_the_name() {
    let mut s = metric();
    let r = s.execute("equipmentadd", &json!({"at": [0, 0], "model": "Chiller 41.7 kW", "name": "CH1"})).unwrap();
    let h = r["handle"].clone();
    s.execute(
        "equipmentedit",
        &json!({"handle": h, "capacityKw": 45, "height": 1400, "fans": 2, "clearance": 0, "weight": null, "reference": "mon modèle"}),
    )
    .unwrap();
    let e = equip(&s, &h);
    assert_eq!(
        (e.capacity_kw, e.height_mm, e.fans, e.clearance_mm, e.weight_kg, e.reference.as_str()),
        (Some(45.0), 1400.0, 2, 0.0, None, "mon modèle")
    );
    // Another model: its values, the name kept, then a value over it.
    s.execute("equipmentedit", &json!({"handle": h, "model": "Chiller 124 kW", "width": 2000})).unwrap();
    let e = equip(&s, &h);
    assert_eq!(
        (e.name.as_str(), e.model.as_str(), e.capacity_kw, e.length_mm, e.width_mm, e.weight_kg),
        ("CH1", "Chiller 124 kW", Some(124.0), 2275.0, 2000.0, None)
    );
    assert!(s.execute("equipmentedit", &json!({"handle": h, "length": 0})).is_err());
    assert!(s.execute("equipmentedit", &json!({"handle": h, "model": "nothing"})).is_err());
    let line = s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    assert!(s.execute("equipmentedit", &json!({"handle": line["handles"][0], "name": "x"})).is_err(), "not an equipment");
}

#[test]
fn equipmentadd_at_the_command_line_keeps_placing_under_the_next_name() {
    let mut s = metric();
    for t in ["equipmentadd", "t", "vrv", "s", "10", "0,0", "3000,0", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let d = s.doc().unwrap();
    let all: Vec<_> = d.model.iter().filter_map(|e| equipment_of(e)).collect();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|e| e.kind == EquipmentKind::VrfOutdoor && e.model == "VRV 10 CV"), "{all:?}");
    assert_eq!(all.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["UE1", "UE2"]);
}

#[test]
fn hvactakeoff_counts_the_level_by_section_fitting_terminal_space_and_writes_a_spreadsheet() {
    let mut s = metric();
    // A space of 6 x 5 m, a unit, a run of 4 m then a run of 2 m at another section, two terminals in the space, an equipment.
    s.execute("spaceadd", &json!({"points": [[0, 0], [6000, 0], [6000, 5000], [0, 5000]], "name": "Salle", "flow": 600})).unwrap();
    s.execute("ahuadd", &json!({"at": [-3000, 1000]})).unwrap();
    s.execute("ductadd", &json!({"points": [[-1500, 1250], [1000, 1250], [4000, 1250]], "diameter": 200, "air": "supply", "altitude": 2.5})).unwrap();
    s.execute("airterminaladd", &json!({"at": [4000, 1250], "flow": 300, "air": "supply"})).unwrap();
    s.execute("airterminaladd", &json!({"at": [1000, 3000], "flow": 100, "air": "extract", "type": "extract"})).unwrap();
    s.execute("equipmentadd", &json!({"at": [9000, 0], "model": "VRV 14 CV", "name": "UE1"})).unwrap();
    s.execute("equipmentadd", &json!({"at": [9000, 4000], "model": "VRV 14 CV", "name": "UE2"})).unwrap();
    let t = s.execute("hvactakeoff", &json!({"level": "R+1"})).unwrap();
    assert_eq!(t["level"], "R+1");
    // The two runs are 2500 and 3000 mm long, Ø200, supply, at 2.5 m: one line.
    let ducts = t["ducts"].as_array().unwrap();
    assert_eq!(ducts.len(), 1, "{t}");
    assert_eq!((ducts[0]["count"].as_u64(), ducts[0]["section"].as_str(), ducts[0]["altitudeM"].as_f64()), (Some(2), Some("Ø200"), Some(2.5)));
    assert!((ducts[0]["lengthM"].as_f64().unwrap() - 5.5).abs() < 1e-9);
    assert!((ducts[0]["areaM2"].as_f64().unwrap() - 5.5 * std::f64::consts::PI * 0.2).abs() < 0.01);
    assert_eq!(t["totals"]["terminals"], 2);
    assert_eq!(t["equipment"][0]["count"], 2);
    assert_eq!(t["equipment"][0]["power"], 28.0, "two 14 CV units");
    // The space holds the two terminals and the 3.5 m of duct that run through it (from x = 0 to 4000, and 1000 on).
    let sp = &t["spaces"][0];
    assert_eq!((sp["supplyTerminals"].as_u64(), sp["extractTerminals"].as_u64(), sp["neededM3h"].as_f64()), (Some(1), Some(1), Some(600.0)), "{sp}");
    assert!((sp["ductM"].as_f64().unwrap() - 4.0).abs() < 0.1, "{sp}");
    // The spreadsheet: `;` between cells, a comma for the decimal mark, an accent that survives.
    let path = std::env::temp_dir().join(format!("cadcraft-takeoff-{}.csv", std::process::id()));
    let r = s.execute("hvactakeoff", &json!({"level": "R+1", "path": path.to_str().unwrap()})).unwrap();
    assert_eq!(r["path"], path.to_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF], "a byte order mark");
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    assert!(text.starts_with("Level;R+1\n"), "{text}");
    assert!(text.contains("supply;rigid;Ø200;2,5;2;5,5;"), "{text}");
    assert!(s.execute("hvactakeoff", &json!({"decimal": "dot"})).is_err());
    let point = s.execute("hvactakeoff", &json!({"path": path.to_str().unwrap(), "decimal": "point"})).unwrap();
    let text = String::from_utf8(std::fs::read(point["path"].as_str().unwrap()).unwrap()[3..].to_vec()).unwrap();
    std::fs::remove_file(&path).ok();
    assert!(text.contains("supply;rigid;Ø200;2.5;2;5.5;"), "{text}");
}

#[test]
fn a_riser_height_is_counted_in_the_takeoff() {
    let mut s = metric();
    s.execute("riseradd", &json!({"at": [0, 0], "width": 600, "depth": 400, "height": 2800})).unwrap();
    let h = s.execute("riseradd", &json!({"at": [2000, 0], "width": 600, "depth": 400})).unwrap();
    let t = s.execute("hvactakeoff", &json!({"level": "R+1"})).unwrap();
    assert_eq!(t["totals"]["riserLengthM"], 2.8);
    s.execute("riseredit", &json!({"handle": h["handle"], "height": 3000})).unwrap();
    let t = s.execute("hvactakeoff", &json!({"level": "R+1"})).unwrap();
    assert_eq!(t["totals"]["riserLengthM"], 5.8);
    assert!(s.execute("riseredit", &json!({"handle": h["handle"], "height": -1})).is_err());
}
