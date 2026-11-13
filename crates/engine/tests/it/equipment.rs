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
