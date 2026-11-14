//! AHUADD places an air handling unit; DUCTADD draws round duct runs, one per segment, and cuts a
//! run another starts on; DUCTEDIT changes them; DUCTSIZE sizes each network from its terminals.

use cadcraft_doc::{AccessoryKind, Drawing, DuctKind, EntityKind, Handle, accessory_of, ahu_of, duct_of};
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

fn ducts(s: &Session) -> usize {
    s.doc().unwrap().model.iter().filter(|e| duct_of(e).is_some()).count()
}

/// A unit, a trunk to a tee with a branch up and one straight on, a terminal at each end.
fn network(s: &mut Session) -> (Value, Value) {
    let unit = s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    let trunk = s.execute("ductadd", &json!({"points": [[750, 0], [6000, 0], [9000, 0]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[6000, 0], [6000, 3000]]})).unwrap();
    s.execute("airterminaladd", &json!({"at": [9000, 0], "flow": 300})).unwrap();
    s.execute("airterminaladd", &json!({"at": [6000, 3000], "flow": 200})).unwrap();
    (unit, trunk)
}

#[test]
fn ahuadd_places_a_numbered_unit() {
    let mut s = metric();
    let a = s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    let b = s.execute("ahuadd", &json!({"at": [5000, 0], "name": "CTA bureaux", "width": 2000, "depth": 900, "rotation": 90})).unwrap();
    assert_eq!(a["name"], "CTA1");
    assert_eq!(b["name"], "CTA bureaux");
    let u = ahu_of(s.doc().unwrap().entity(hd(&b["handle"])).unwrap()).unwrap();
    assert_eq!((u.width_mm, u.depth_mm), (2000.0, 900.0));
    for bad in [json!({}), json!({"at": [0, 0], "width": -1}), json!({"at": [0, 0], "name": " "})] {
        assert!(s.execute("ahuadd", &bad).is_err(), "{bad}");
    }
}

#[test]
fn ductadd_draws_one_duct_per_segment_and_a_run_started_on_another_cuts_it() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [4000, 0], [4000, 3000]], "diameter": 315})).unwrap();
    assert_eq!(r["handles"].as_array().unwrap().len(), 2);
    assert_eq!(ducts(&s), 2);
    let first = duct_of(s.doc().unwrap().entity(hd(&r["handles"][0])).unwrap()).unwrap();
    assert!(first.imposed && first.diameter_mm == 315.0 && (first.length - 4000.0).abs() < 1e-9);
    // A branch started in the middle of the first run cuts it in two.
    s.execute("ductadd", &json!({"points": [[1000, 0], [1000, -2000]]})).unwrap();
    assert_eq!(ducts(&s), 4);
    let first = duct_of(s.doc().unwrap().entity(hd(&r["handles"][0])).unwrap()).unwrap();
    assert!((first.length - 1000.0).abs() < 1e-6, "{}", first.length);
    for bad in [
        json!({}),
        json!({"points": [[0, 0]]}),
        json!({"points": [[0, 0], [0, 0]]}),
        json!({"points": [[0, 0], [1, 1]], "diameter": 1}),
        json!({"points": [[0, 0], [1, 1]], "kind": "huge"}),
    ] {
        assert!(s.execute("ductadd", &bad).is_err(), "{bad}");
    }
}

#[test]
fn ductadd_at_the_command_line_draws_runs_and_undoes_the_last() {
    let mut s = metric();
    for t in ["ductadd", "d", "250", "0,0", "3000,0", "3000,2000", "u", "6000,0", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    // 0,0 → 3000,0 → 3000,2000 is undone, so the next click draws from 3000,0 to 6000,0.
    assert_eq!(ducts(&s), 2);
    let d = s.doc().unwrap();
    let ends: Vec<_> = d.model.iter().filter_map(|e| d.duct_ends(e.handle)).map(|(p, a, b)| (p.diameter_mm, a.x, b.x)).collect();
    assert_eq!(ends, [(250.0, 0.0, 3000.0), (250.0, 3000.0, 6000.0)]);
}

#[test]
fn ductedit_changes_the_diameter_kind_and_gives_the_choice_back_to_sizing() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [2000, 0]]})).unwrap();
    let h = r["handles"][0].clone();
    assert_eq!(s.execute("ductedit", &json!({"handle": h, "diameter": 400, "kind": "main"})).unwrap()["changed"], 1);
    let p = duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap();
    assert!(p.imposed && p.diameter_mm == 400.0 && p.kind == DuctKind::Main);
    s.execute("ductedit", &json!({"handle": h, "diameter": "auto"})).unwrap();
    assert!(!duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap().imposed);
    for bad in [json!({"handle": h, "diameter": 3}), json!({"handle": h, "diameter": "wide"}), json!({"handle": h, "kind": "x"}), json!({})] {
        assert!(s.execute("ductedit", &bad).is_err(), "{bad}");
    }
    // Something that is not a duct is refused, and nothing changes.
    let unit = s.execute("ahuadd", &json!({"at": [0, 5000]})).unwrap();
    assert!(s.execute("ductedit", &json!({"handles": [h, unit["handle"]], "diameter": 500})).is_err());
    assert_eq!(duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap().diameter_mm, 400.0);
}

#[test]
fn ductsize_sizes_the_network_and_writes_it_back() {
    let mut s = metric();
    let (unit, trunk) = network(&mut s);
    let r = s.execute("ductsize", &json!({})).unwrap();
    let net = &r["networks"][0];
    assert_eq!(net["ahu"], unit["handle"]);
    assert_eq!(net["totalFlow"], 500.0);
    assert!(net["requiredPressure"].as_f64().unwrap() > 0.0);
    assert_eq!(net["segments"].as_array().unwrap().len(), 3);
    assert!(net["warnings"].as_array().unwrap().is_empty(), "{net}");
    let d = s.doc().unwrap();
    let t = duct_of(d.entity(hd(&trunk["handles"][0])).unwrap()).unwrap();
    assert_eq!(t.flow_m3h, Some(500.0));
    assert_eq!(t.diameter_mm, net["segments"][0]["diameter"].as_f64().unwrap());
    assert!(ahu_of(d.entity(hd(&unit["handle"])).unwrap()).unwrap().flow_m3h == Some(500.0));
    // Tighter limits give larger ducts.
    let before = t.diameter_mm;
    s.execute("ductsize", &json!({"maxVelocityMain": 2.0})).unwrap();
    let t = duct_of(s.doc().unwrap().entity(hd(&trunk["handles"][0])).unwrap()).unwrap();
    assert!(t.diameter_mm > before);
    // Undo puts the sizes back.
    s.cmdline("u").unwrap();
    let t = duct_of(s.doc().unwrap().entity(hd(&trunk["handles"][0])).unwrap()).unwrap();
    assert_eq!(t.diameter_mm, before);
}

#[test]
fn ductsize_says_what_it_cannot_size() {
    let mut s = metric();
    assert!(s.execute("ductsize", &json!({})).is_err(), "no unit");
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert!(r["networks"][0]["warnings"][0].as_str().unwrap().contains("no duct"));
    assert!(s.execute("ductsize", &json!({"ahu": "ZZZ"})).is_err());
    assert!(s.execute("ductsize", &json!({"maxGradient": -1})).is_err());
}

/// The sports hall: a unit turned a quarter turn, a duct up from it drawn first, then a run
/// along the terminals that crosses the duct's end.
#[test]
fn ductsize_cuts_a_run_the_stub_ends_on_and_compares_the_terminals_with_the_space() {
    let mut s = metric();
    s.execute("spaceadd", &json!({"points": [[-4000, 0], [4000, 0], [4000, 6000], [-4000, 6000]], "name": "Salle", "flow": 1200})).unwrap();
    let unit = s.execute("ahuadd", &json!({"at": [0, 1000], "rotation": 90})).unwrap();
    // Up from the unit's face to the middle of the run laid afterwards, its end a radius off the axis.
    s.execute("ductadd", &json!({"points": [[0, 1800], [100, 4000]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[-3000, 4000], [-1000, 4000], [1000, 4000], [3000, 4000]]})).unwrap();
    for x in [-3000, -1000, 1000, 3000] {
        s.execute("airterminaladd", &json!({"at": [x, 4000], "flow": 300})).unwrap();
    }
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(r["cuts"], 1, "{r}");
    let net = &r["networks"][0];
    assert_eq!(net["ahu"], unit["handle"]);
    assert_eq!(net["totalFlow"], 1200.0, "{net}");
    assert_eq!(net["segments"].as_array().unwrap().len(), 5, "the stub and the run's four pieces");
    assert!(net["warnings"].as_array().unwrap().is_empty(), "{net}");
    assert!(r["spaces"].as_array().unwrap().is_empty(), "1200 m³/h supplied, 1200 needed");
    // Twice the terminals: the space is said to get more than it needs.
    for x in [-3000, 3000] {
        s.execute("airterminaladd", &json!({"at": [x, 500], "flow": 300})).unwrap();
    }
    let r = s.execute("ductsize", &json!({"balanceTerminals": false})).unwrap();
    assert_eq!(r["spaces"][0]["supplied"], 1800.0);
    assert_eq!(r["spaces"][0]["needed"], 1200.0);
    assert!(r["networks"][0]["warnings"][0].as_str().unwrap().contains("reached by no duct"));
}

/// The sports hall again, with a double-flux unit: four supply diffusers and four extract valves,
/// each side carrying the 1200 m³/h the hall needs.
#[test]
fn a_double_flux_unit_carries_the_flow_of_the_space_in_both_directions() {
    let mut s = metric();
    s.execute("spaceadd", &json!({"points": [[-4000, -4000], [4000, -4000], [4000, 4000], [-4000, 4000]], "name": "Salle", "flow": 1200})).unwrap();
    let unit = s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    // From each face, up (or down) to the middle, then an arm to each side with its terminals on it.
    for (air, y) in [("supply", 1), ("extract", -1)] {
        s.execute("ductadd", &json!({"points": [[0, 400 * y], [0, 2500 * y]], "air": air})).unwrap();
        for end in [-3000, 3000] {
            s.execute("ductadd", &json!({"points": [[0, 2500 * y], [end, 2500 * y]], "air": air})).unwrap();
        }
    }
    for x in [-3000, -1000, 1000, 3000] {
        s.execute("airterminaladd", &json!({"at": [x, 2500], "flow": 300})).unwrap();
        s.execute("airterminaladd", &json!({"at": [x, -2500], "flow": 300, "air": "extract", "type": "squareCeiling"})).unwrap();
    }
    let r = s.execute("ductsize", &json!({})).unwrap();
    let nets = r["networks"].as_array().unwrap();
    assert_eq!(nets.iter().map(|n| n["air"].as_str().unwrap()).collect::<Vec<_>>(), ["supply", "extract"], "{r}");
    assert!(nets.iter().all(|n| n["totalFlow"] == 1200.0 && n["warnings"].as_array().unwrap().is_empty()), "{r}");
    assert!(r["spaces"].as_array().unwrap().is_empty(), "{r}");
    let u = cadcraft_doc::ahu_of(s.doc().unwrap().entity(hd(&unit["handle"])).unwrap()).unwrap();
    assert_eq!((u.flow_m3h, u.extract_flow_m3h), (Some(1200.0), Some(1200.0)));
}

fn accessories(s: &Session) -> Vec<(Handle, cadcraft_doc::AccessoryParams)> {
    s.doc().unwrap().model.iter().filter_map(|e| accessory_of(e).map(|a| (e.handle, a))).collect()
}

#[test]
fn ductaccessoryadd_sits_on_the_duct_with_its_diameter_and_direction() {
    let mut s = metric();
    s.execute("ductadd", &json!({"points": [[0, 0], [0, 4000]], "diameter": 315})).unwrap();
    let r = s.execute("ductaccessoryadd", &json!({"at": [60, 2000], "type": "fireDamper"})).unwrap();
    let a = &accessories(&s)[0].1;
    assert_eq!((a.kind, a.diameter_mm, a.length_mm), (AccessoryKind::FireDamper, 315.0, 300.0));
    let (x, y, rot) = match &s.doc().unwrap().entity(hd(&r["handle"])).unwrap().kind {
        cadcraft_doc::EntityKind::Insert(i) => (i.insert.x, i.insert.y, i.rotation),
        _ => panic!("not a block reference"),
    };
    assert!((x.abs() < 1e-9) && (y - 2000.0).abs() < 1e-9, "on the axis, not where it was clicked");
    assert!((rot - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "along the duct");
    for bad in [
        json!({"at": [3000, 2000]}),
        json!({"at": [0, 5000]}),
        json!({"at": [0, 100], "type": "valve"}),
        json!({"at": [0, 100], "length": -1}),
        json!({"at": [0, 100], "zeta": "a"}),
        json!({}),
    ] {
        assert!(s.execute("ductaccessoryadd", &bad).is_err(), "{bad}");
    }
    assert_eq!(accessories(&s).len(), 1);
}

#[test]
fn ductaccessoryadd_at_the_command_line_places_several_and_remembers_the_type() {
    let mut s = metric();
    s.execute("ductadd", &json!({"points": [[0, 0], [6000, 0]]})).unwrap();
    for t in ["ductaccessoryadd", "s", "1500,0", "f", "4000,0", "3000,500", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let kinds: Vec<_> = accessories(&s).iter().map(|(_, a)| a.kind).collect();
    assert_eq!(kinds, [AccessoryKind::Silencer, AccessoryKind::FireDamper], "the click off the duct placed nothing");
    // The next use starts with the last type.
    s.cmdline("ductaccessoryadd").unwrap();
    s.cmdline("2500,0").unwrap();
    s.cmdline("").unwrap();
    assert_eq!(accessories(&s)[2].1.kind, AccessoryKind::FireDamper);
}

#[test]
fn ductaccessoryedit_changes_the_loss_and_clears_it_back_to_the_default() {
    let mut s = metric();
    s.execute("ductadd", &json!({"points": [[0, 0], [4000, 0]]})).unwrap();
    let r = s.execute("ductaccessoryadd", &json!({"at": [2000, 0]})).unwrap();
    let h = r["handle"].clone();
    s.execute("ductaccessoryedit", &json!({"handle": h, "zeta": 1.2, "length": 400})).unwrap();
    let a = &accessories(&s)[0].1;
    assert_eq!((a.zeta, a.length_mm), (Some(1.2), 400.0));
    s.execute("ductaccessoryedit", &json!({"handle": h, "type": "silencer", "pressureDrop": 45})).unwrap();
    assert_eq!(accessories(&s)[0].1.loss(), cadcraft_doc::Loss::Pascals(45.0));
    s.execute("ductaccessoryedit", &json!({"handle": h, "pressureDrop": null, "zeta": null})).unwrap();
    assert_eq!(accessories(&s)[0].1.loss(), AccessoryKind::Silencer.default_loss());
    let duct = s.doc().unwrap().model.iter().find_map(|e| duct_of(e).map(|_| e.handle)).unwrap();
    assert!(s.execute("ductaccessoryedit", &json!({"handles": [h, duct.hex()], "length": 100})).is_err(), "a duct is not an accessory");
    assert!(s.execute("ductaccessoryedit", &json!({"handle": h, "length": 0})).is_err());
    assert_eq!(accessories(&s)[0].1.length_mm, 400.0);
}

#[test]
fn ductsize_counts_the_accessories_and_the_reducers_in_the_pressure() {
    let mut s = metric();
    network(&mut s);
    let base = s.execute("ductsize", &json!({})).unwrap()["networks"][0]["requiredPressure"].as_f64().unwrap();
    // A reducer on every change of diameter: the network's own sizing, without them, asks less.
    let plain = {
        let net = s.doc().unwrap().duct_networks().unwrap().remove(0);
        let sized = cadcraft_hvac::size_network(&net.network, &cadcraft_hvac::DesignCriteria::default()).unwrap();
        sized.required_pressure_pa
    };
    assert!(base > plain, "{base} vs {plain}: the reducers add their loss");
    // A silencer of 30 Pa on the trunk adds 30 Pa.
    s.execute("ductaccessoryadd", &json!({"at": [3000, 0], "type": "silencer"})).unwrap();
    let with = s.execute("ductsize", &json!({})).unwrap();
    let with = with["networks"][0]["requiredPressure"].as_f64().unwrap();
    assert!((with - base - 30.0).abs() < 1e-6, "{with} vs {base}");
    // An accessory on no duct is named.
    let h = s.execute("ductaccessoryadd", &json!({"at": [3000, 0]})).unwrap()["handle"].clone();
    s.execute("move", &json!({"handles": [h], "from": [0, 0], "to": [0, 5000]})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert!(r["networks"][0]["warnings"].as_array().unwrap().iter().any(|w| w.as_str().unwrap().contains("accessory at")), "{r}");
}

#[test]
fn an_unreached_terminal_says_how_far_the_nearest_duct_end_is() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[500, 0], [3000, 0]]})).unwrap();
    // 2 m past the duct's end, and of the other air direction.
    s.execute("airterminaladd", &json!({"at": [5000, 0], "flow": 150})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    let ws: Vec<&str> = r["networks"][0]["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    let w = ws.iter().find(|w| w.starts_with("the terminal")).expect("the terminal is named");
    assert!(w.contains("2000 mm away") && w.contains("supply duct"), "{w}");
}

/// A short duct drops from the trunk to the edge of the terminal's face, as a snap to the face lands.
#[test]
fn ductsize_reaches_a_terminal_by_the_edge_of_its_face() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[500, 0], [3000, 0], [6000, 0]]})).unwrap();
    for x in [3000, 6000] {
        s.execute("ductadd", &json!({"points": [[x, 0], [x, -700]]})).unwrap();
        s.execute("airterminaladd", &json!({"at": [x, -1000], "flow": 150})).unwrap();
    }
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(r["networks"][0]["totalFlow"], 300.0, "{r}");
    assert!(r["networks"][0]["warnings"].as_array().unwrap().is_empty(), "{r}");
}

fn bounds_x(es: &[cadcraft_doc::Entity]) -> (f64, f64) {
    use cadcraft_doc::EntityKind;
    let mut xs = Vec::new();
    for e in es {
        match &e.kind {
            EntityKind::Line(l) => xs.extend([l.a.x, l.b.x]),
            EntityKind::Circle(c) => xs.extend([c.center.x - c.radius, c.center.x + c.radius]),
            EntityKind::LwPolyline(p) => xs.extend(p.vertices.iter().map(|v| v.p.x)),
            _ => {}
        }
    }
    (xs.iter().copied().fold(f64::MAX, f64::min), xs.iter().copied().fold(f64::MIN, f64::max))
}

#[test]
fn a_terminal_is_seen_under_the_cursor_as_soon_as_its_type_is_chosen() {
    let mut s = metric();
    s.cmdline("airterminaladd").unwrap();
    let at = cadcraft_geom::Vec2::new(5000.0, 3000.0);
    // Before any click, with the default type: the 600 mm face is centred on the cursor.
    let p = s.preview(at);
    assert!(!p.is_empty(), "the terminal is drawn before the click");
    let (lo, hi) = bounds_x(&p);
    assert!(lo < 5000.0 - 300.0 && hi > 5000.0 + 300.0, "{lo} {hi}");
    // Choosing another type changes it at once; the arrows are on the supply layer.
    s.cmdline("t").unwrap();
    s.cmdline("g").unwrap();
    assert!(s.preview(at).iter().any(|e| e.common.layer == "M-HVAC-SUPPLY"));
    // After the click it follows the cursor's direction: a grille, turned toward it, changes its width along x.
    s.cmdline("5000,3000").unwrap();
    let turned = s.preview(cadcraft_geom::Vec2::new(5000.0, 6000.0));
    let straight = s.preview(cadcraft_geom::Vec2::new(8000.0, 3000.0));
    assert_ne!(bounds_x(&turned), bounds_x(&straight));
    s.cmdline("").unwrap();
    s.cmdline("").unwrap();
    assert!(s.preview(at).is_empty(), "nothing once the command ends");
}

#[test]
fn the_unit_the_duct_and_the_accessory_are_seen_before_they_are_placed() {
    let mut s = metric();
    s.cmdline("ahuadd").unwrap();
    assert!(!s.preview(cadcraft_geom::Vec2::new(0.0, 0.0)).is_empty(), "the unit");
    s.cmdline("").unwrap();
    s.cmdline("ductadd").unwrap();
    assert!(s.preview(cadcraft_geom::Vec2::new(1000.0, 0.0)).is_empty(), "no start point yet");
    s.cmdline("0,0").unwrap();
    let p = s.preview(cadcraft_geom::Vec2::new(4000.0, 0.0));
    assert!(p.iter().filter(|e| matches!(e.kind, cadcraft_doc::EntityKind::LwPolyline(_))).count() >= 2, "the two sides of the duct");
    s.cmdline("4000,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("ductaccessoryadd").unwrap();
    assert!(!s.preview(cadcraft_geom::Vec2::new(2000.0, 90.0)).is_empty(), "on the duct");
    assert!(s.preview(cadcraft_geom::Vec2::new(2000.0, 3000.0)).is_empty(), "off the duct");
}

#[test]
fn ductadd_draws_a_flexible_duct_and_ductedit_changes_the_material() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [1000, 0]], "diameter": 125, "flexible": true})).unwrap();
    let h = r["handles"][0].clone();
    assert!(duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap().flexible);
    s.execute("ductedit", &json!({"handle": h, "flexible": false})).unwrap();
    assert!(!duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap().flexible);
    assert!(s.execute("ductadd", &json!({"points": [[0, 0], [1000, 0]], "flexible": "yes"})).is_err());
    // At the command line: the Material keyword.
    for t in ["ductadd", "m", "f", "0,500", "1000,500", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.doc().unwrap().model.iter().filter_map(|e| duct_of(e)).filter(|p| p.flexible).count() == 1);
}

#[test]
fn ductedit_moves_a_duct_to_the_layer_of_its_air_direction() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    let h = hd(&r["handles"][0]);
    assert_eq!(s.doc().unwrap().entity(h).unwrap().common.layer, "M-HVAC-DUCT-SUP");
    s.execute("ductedit", &json!({"handle": r["handles"][0], "air": "extract"})).unwrap();
    assert_eq!(s.doc().unwrap().entity(h).unwrap().common.layer, "M-HVAC-DUCT-EXTR");
    assert!(s.doc().unwrap().layer("M-HVAC-DUCT-EXTR").is_some());
}

/// Three diffusers beside a trunk, each a few hundred millimetres from it; the middle one is on the
/// other side and a fourth, of the other air direction, has no duct of its own.
#[test]
fn ductconnect_draws_flexible_ducts_from_the_trunk_to_the_terminals() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[500, 0], [8000, 0]], "diameter": 250})).unwrap();
    for (x, y) in [(2000, 1500), (4000, -1200), (6000, 1500)] {
        s.execute("airterminaladd", &json!({"at": [x, y], "flow": 150})).unwrap();
    }
    s.execute("airterminaladd", &json!({"at": [3000, 9000], "flow": 100, "air": "extract", "type": "squareCeiling"})).unwrap();
    let r = s.execute("ductconnect", &json!({})).unwrap();
    assert_eq!(r["created"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(r["skipped"].as_array().unwrap().len(), 1, "the extract terminal has no duct near");
    let flex: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| duct_of(e).filter(|p| p.flexible)).collect();
    assert_eq!(flex.len(), 3);
    // Each is as wide as a 600 mm diffuser's neck (160 mm), imposed, and about as long as the offset.
    assert!(flex.iter().all(|p| p.diameter_mm == 160.0 && p.imposed && p.kind == DuctKind::Connection));
    assert!(flex.iter().all(|p| p.length > 1000.0 && p.length < 1600.0), "{:?}", flex.iter().map(|p| p.length).collect::<Vec<_>>());
    // The trunk was cut where each starts, and the whole network now sizes: three diffusers of 150.
    let sized = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(sized["networks"][0]["totalFlow"], 450.0, "{sized}");
    // Nothing is left to connect.
    let again = s.execute("ductconnect", &json!({})).unwrap();
    assert_eq!(again["created"].as_array().unwrap().len(), 0);
    // A distance limit and a bad handle.
    assert!(s.execute("ductconnect", &json!({"terminals": ["ZZZ"]})).is_err());
    assert!(s.execute("ductconnect", &json!({"maxDistance": -5})).is_err());
}

/// The sports hall as drawn (measured off a screenshot): a unit turned a quarter turn above a main
/// of four runs, its duct dropping to the main's upper line, and four ducts leaving the main's
/// LOWER line to the centres of four 600 mm diffusers.
#[test]
fn ductsize_sizes_the_sports_hall_as_drawn() {
    let mut s = metric();
    s.execute("spaceadd", &json!({"points": [[-500, -2500], [9000, -2500], [9000, 3500], [-500, 3500]], "name": "Salle de sport", "flow": 1200}))
        .unwrap();
    let unit = s.execute("ahuadd", &json!({"at": [4260, 1200], "rotation": 90})).unwrap();
    s.execute("ductadd", &json!({"points": [[4260, 450], [4260, 100]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[0, 0], [2820, 0], [4260, 0], [5640, 0], [8490, 0]]})).unwrap();
    for x in [0, 2820, 5640, 8490] {
        s.execute("ductadd", &json!({"points": [[x, -100], [x, -570]]})).unwrap();
        s.execute("airterminaladd", &json!({"at": [x, -570], "flow": 300, "model": "SQ600-200"})).unwrap();
    }
    let r = s.execute("ductsize", &json!({})).unwrap();
    let net = &r["networks"][0];
    assert_eq!(net["ahu"], unit["handle"], "{r}");
    assert_eq!(net["totalFlow"], 1200.0, "{r}");
    assert!(net["warnings"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!(net["segments"].as_array().unwrap().len(), 9, "{r}");
    assert!(r["spaces"].as_array().unwrap().is_empty(), "{r}");
}

#[test]
fn ductsize_names_the_ducts_no_unit_reaches_and_how_far_the_nearest_end_is() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[500, 0], [3000, 0]]})).unwrap();
    // A second run 400 mm past the first one's end: not touching it.
    s.execute("ductadd", &json!({"points": [[3400, 0], [6000, 0]]})).unwrap();
    s.execute("airterminaladd", &json!({"at": [3000, 0], "flow": 100})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!((r["ducts"].as_u64(), r["reached"].as_u64()), (Some(2), Some(1)), "{r}");
    let w: Vec<&str> = r["networks"][0]["warnings"].as_array().unwrap().iter().map(|w| w.as_str().unwrap()).collect();
    assert!(w.iter().any(|w| w.contains("is not connected to a unit") && w.contains("400 mm away")), "{w:?}");
}

/// The same hall, the terminals set to extract and the ducts left as they were drawn.
#[test]
fn ductsize_works_on_extract_terminals_with_ducts_nobody_set() {
    let mut s = metric();
    let unit = s.execute("ahuadd", &json!({"at": [4260, 1200], "rotation": 90})).unwrap();
    s.execute("ductadd", &json!({"points": [[4260, 450], [4260, 100]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[0, 0], [2820, 0], [4260, 0], [5640, 0], [8490, 0]]})).unwrap();
    for x in [0, 2820, 5640, 8490] {
        s.execute("ductadd", &json!({"points": [[x, -100], [x, -570]]})).unwrap();
        s.execute("airterminaladd", &json!({"at": [x, -570], "flow": 300, "air": "extract", "type": "squareCeiling"})).unwrap();
    }
    let r = s.execute("ductsize", &json!({})).unwrap();
    let net = &r["networks"][0];
    assert_eq!((net["air"].as_str(), net["totalFlow"].as_f64()), (Some("extract"), Some(1200.0)), "{r}");
    assert!(net["warnings"].as_array().unwrap().is_empty(), "{r}");
    assert_eq!((r["ducts"].as_u64(), r["reached"].as_u64()), (Some(9), Some(9)));
    let u = cadcraft_doc::ahu_of(s.doc().unwrap().entity(hd(&unit["handle"])).unwrap()).unwrap();
    assert_eq!((u.flow_m3h, u.extract_flow_m3h), (None, Some(1200.0)));
    // A duct whose direction was set by hand to supply is left alone, and the message says what to do.
    let first = s.doc().unwrap().model.iter().find_map(|e| duct_of(e).map(|_| e.handle)).unwrap();
    s.execute("ductedit", &json!({"handle": first.hex(), "air": "supply"})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert!(r["networks"][0]["totalFlow"].as_f64().unwrap_or(0.0) < 1200.0, "{r}");
    s.execute("ductedit", &json!({"handle": first.hex(), "air": "auto"})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(r["networks"][0]["totalFlow"], 1200.0, "back on auto, it works again: {r}");
}

fn sections(s: &Session) -> Vec<(cadcraft_doc::DuctShape, f64, f64)> {
    s.doc().unwrap().model.iter().filter_map(|e| duct_of(e)).map(|p| (p.shape, p.width_mm, p.height_mm)).collect()
}

/// A unit and a trunk to four diffusers of 300 m³/h each by connections, all under a zone with `void`
/// mm available (none when `None`). Returns the zone's handle, or null.
fn hall(s: &mut Session, void: Option<f64>) -> Value {
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    let mut from = 500;
    for x in [2000, 4000, 6000, 8000] {
        s.execute("ductadd", &json!({"points": [[from, 0], [x, 0]]})).unwrap();
        s.execute("ductadd", &json!({"points": [[x, 0], [x, -1500]]})).unwrap();
        s.execute("airterminaladd", &json!({"at": [x, -1500], "flow": 300})).unwrap();
        from = x;
    }
    match void {
        Some(v) => {
            s.execute("zoneadd", &json!({"points": [[-1000, -2500], [9500, -2500], [9500, 1000], [-1000, 1000]], "voidHeight": v})).unwrap()["handle"]
                .clone()
        }
        None => Value::Null,
    }
}

#[test]
fn ductsize_makes_the_trunk_rectangular_where_the_height_available_is_too_small_for_a_round_one() {
    let mut s = metric();
    let zone = hall(&mut s, Some(350.0));
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert!(r["networks"][0]["warnings"].as_array().unwrap().is_empty(), "{r}");
    let segs = r["networks"][0]["segments"].as_array().unwrap();
    let rect: Vec<_> = segs.iter().filter(|g| g["section"] == "rectangular").collect();
    assert!(!rect.is_empty() && rect.len() < segs.len(), "some rectangular, some round: {r}");
    assert!(rect.iter().all(|g| g["height"] == 250.0 && g["width"].as_f64().unwrap() % 50.0 == 0.0), "{r}");
    assert!(segs.iter().filter(|g| g["section"] == "round").all(|g| g["diameter"].as_f64().unwrap() <= 250.0));
    assert!(sections(&s).iter().any(|x| x.0 == cadcraft_doc::DuctShape::Rectangular));
    // A smaller margin gives taller ducts.
    let r = s.execute("ductsize", &json!({"margin": 50})).unwrap();
    assert!(r["networks"][0]["segments"].as_array().unwrap().iter().filter(|g| g["section"] == "rectangular").all(|g| g["height"] == 300.0), "{r}");
    assert!(s.execute("ductsize", &json!({"margin": -1})).is_err());
    // Without the zone, all round, as before.
    s.execute("erase", &json!({"handles": [zone]})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert!(r["networks"][0]["segments"].as_array().unwrap().iter().all(|g| g["section"] == "round"), "{r}");
    assert!(sections(&s).iter().all(|x| x.0 == cadcraft_doc::DuctShape::Round), "the rectangle goes back to round");
    // A height too small for any duct says so.
    let tiny = s.execute("zoneadd", &json!({"points": [[-1000, -2500], [9500, -2500], [9500, 1000], [-1000, 1000]], "voidHeight": 180})).unwrap();
    let e = s.execute("ductsize", &json!({})).unwrap_err().to_string();
    assert!(e.contains("below the smallest duct side"), "{e}");
    assert!(s.execute("zoneedit", &json!({"handle": tiny["handle"], "voidHeight": 10})).is_err());
}

#[test]
fn a_space_gives_the_height_where_no_zone_does_and_a_zone_wins_over_it() {
    let mut s = metric();
    hall(&mut s, None);
    // A room over the whole trunk with 250 mm available: 150 after the margin.
    let sp = s.execute("spaceadd", &json!({"points": [[-500, -2000], [9000, -2000], [9000, 500], [-500, 500]], "voidHeight": 250})).unwrap();
    let r = s.execute("ductsize", &json!({"balanceTerminals": false})).unwrap();
    let heights = |r: &Value| -> Vec<f64> {
        r["networks"][0]["segments"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|g| g["section"] == "rectangular")
            .map(|g| g["height"].as_f64().unwrap())
            .collect()
    };
    assert!(!heights(&r).is_empty() && heights(&r).iter().all(|h| *h == 150.0), "{r}");
    // A zone over the same area with 350 wins: the ducts are taller (250).
    let z = s.execute("zoneadd", &json!({"points": [[-500, -2000], [9000, -2000], [9000, 500], [-500, 500]], "voidHeight": 350})).unwrap();
    let r = s.execute("ductsize", &json!({"balanceTerminals": false})).unwrap();
    assert!(heights(&r).iter().all(|h| *h == 250.0), "{r}");
    s.execute("erase", &json!({"handles": [z["handle"]]})).unwrap();
    // Clearing the space's height leaves nothing to size to: all round.
    s.execute("spaceedit", &json!({"handle": sp["handle"], "voidHeight": null})).unwrap();
    let r = s.execute("ductsize", &json!({"balanceTerminals": false})).unwrap();
    assert!(heights(&r).is_empty(), "{r}");
    assert!(s.execute("spaceedit", &json!({"handle": sp["handle"], "voidHeight": -3})).is_err());
}

#[test]
fn zoneadd_and_zoneedit_draw_and_change_a_zone_and_ductsize_cuts_a_duct_that_crosses_it() {
    let mut s = metric();
    assert!(s.execute("zoneadd", &json!({"points": [[0, 0], [1000, 0], [1000, 500]]})).is_err(), "a height is required");
    assert!(s.execute("zoneadd", &json!({"points": [[0, 0], [1000, 0]], "voidHeight": 250})).is_err(), "three points at least");
    assert!(s.execute("zoneadd", &json!({"points": [[0, 0], [1000, 0], [1000, 500]], "voidHeight": 5})).is_err());
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[500, 0], [9000, 0]]})).unwrap();
    s.execute("airterminaladd", &json!({"at": [9000, 0], "flow": 400})).unwrap();
    let z = s
        .execute(
            "zoneadd",
            &json!({"points": [[3000, -800], [5000, -800], [5000, 800], [3000, 800]], "voidHeight": 250, "altitude": 2.45, "name": "Poutre"}),
        )
        .unwrap();
    assert_eq!(z["name"], "Poutre");
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(r["cuts"], 2, "in and out of the beam: {r}");
    let segs = r["networks"][0]["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 3, "{r}");
    assert_eq!(segs[1]["section"], "rectangular", "flat under the beam: {r}");
    assert!(segs[0]["section"] == "round" && segs[2]["section"] == "round");
    let under = s.doc().unwrap().model.iter().filter_map(|e| duct_of(e)).find(|p| p.shape == cadcraft_doc::DuctShape::Rectangular).unwrap();
    assert_eq!((under.height_mm, under.bottom_mm), (150.0, Some(2450.0)));
    // Editing: the height and the altitude, then no altitude.
    s.execute("zoneedit", &json!({"handle": z["handle"], "voidHeight": 300, "altitude": 2.7})).unwrap();
    let zp = cadcraft_doc::zone_of(s.doc().unwrap().entity(hd(&z["handle"])).unwrap()).unwrap();
    assert_eq!((zp.void_height_mm, zp.bottom_mm), (300.0, Some(2700.0)));
    s.execute("zoneedit", &json!({"handle": z["handle"], "altitude": null})).unwrap();
    assert_eq!(cadcraft_doc::zone_of(s.doc().unwrap().entity(hd(&z["handle"])).unwrap()).unwrap().bottom_mm, None);
    let h = s.doc().unwrap().model.iter().find_map(|e| duct_of(e).map(|_| e.handle)).unwrap();
    assert!(s.execute("zoneedit", &json!({"handles": [z["handle"], h.hex()], "voidHeight": 200})).is_err(), "a duct is not a zone");
    // At the command line: Height and Altitude options, then the points.
    for t in ["zoneadd", "h", "400", "a", "2.1", "0,3000", "2000,3000", "2000,4000", ""] {
        s.cmdline(t).unwrap();
    }
    let n = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| cadcraft_doc::zone_of(e))
        .filter(|z| z.void_height_mm == 400.0 && z.bottom_mm == Some(2100.0))
        .count();
    assert_eq!(n, 1);
}

#[test]
fn a_rectangular_section_can_be_imposed_when_drawing_editing_or_typing_it() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [3000, 0]], "width": 400, "height": 250})).unwrap();
    let h = r["handles"][0].clone();
    let p = duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap();
    assert_eq!((p.shape, p.width_mm, p.height_mm, p.imposed), (cadcraft_doc::DuctShape::Rectangular, 400.0, 250.0, true));
    assert!((p.diameter_mm - 343.3).abs() < 0.1, "the equivalent diameter: {}", p.diameter_mm);
    assert!(s.execute("ductadd", &json!({"points": [[0, 0], [3000, 0]], "width": 400})).is_err());
    // Editing: a round diameter replaces it, width and height bring it back, auto gives it to sizing.
    s.execute("ductedit", &json!({"handle": h, "diameter": 315})).unwrap();
    let p = duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap();
    assert_eq!((p.shape, p.diameter_mm, p.width_mm), (cadcraft_doc::DuctShape::Round, 315.0, 0.0));
    s.execute("ductedit", &json!({"handle": h, "width": 600, "height": 200})).unwrap();
    assert_eq!(sections(&s), [(cadcraft_doc::DuctShape::Rectangular, 600.0, 200.0)]);
    assert!(s.execute("ductedit", &json!({"handle": h, "height": 200})).is_err());
    assert!(s.execute("ductedit", &json!({"handle": h, "width": 3, "height": 200})).is_err());
    s.execute("ductedit", &json!({"handle": h, "diameter": "auto"})).unwrap();
    assert!(!duct_of(s.doc().unwrap().entity(hd(&h)).unwrap()).unwrap().imposed);
    // At the command line: 400x250 in the Diameter option.
    for t in ["ductadd", "d", "500x300", "0,1000", "2000,1000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(sections(&s).contains(&(cadcraft_doc::DuctShape::Rectangular, 500.0, 300.0)));
    s.cmdline("ductadd").unwrap();
    s.cmdline("d").unwrap();
    assert!(s.cmdline("big").is_err() || s.running.is_some(), "a bad size is refused, the command goes on");
}

#[test]
fn ductsize_keeps_an_imposed_rectangle_and_takes_its_aspect_limit_from_its_parameters() {
    let mut s = metric();
    hall(&mut s, Some(350.0));
    let first = s.doc().unwrap().model.iter().find_map(|e| duct_of(e).map(|_| e.handle)).unwrap();
    s.execute("ductedit", &json!({"handle": first.hex(), "width": 800, "height": 200})).unwrap();
    s.execute("ductsize", &json!({})).unwrap();
    let p = duct_of(s.doc().unwrap().entity(first).unwrap()).unwrap();
    assert_eq!((p.width_mm, p.height_mm), (800.0, 200.0));
    // A tighter aspect ratio makes the others taller... narrower: none wider than 3 times its height.
    s.execute("ductedit", &json!({"handle": first.hex(), "diameter": "auto"})).unwrap();
    let r = s.execute("ductsize", &json!({"maxAspect": 2.0, "sideStep": 25})).unwrap();
    let ok = r["networks"][0]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| g["section"] == "rectangular")
        .all(|g| g["width"].as_f64().unwrap() <= 2.0 * g["height"].as_f64().unwrap() && g["width"].as_f64().unwrap() % 25.0 == 0.0);
    assert!(ok, "{r}");
    assert!(s.execute("ductsize", &json!({"maxAspect": 0})).is_err());
    assert!(s.execute("ductsize", &json!({"sideStep": -5})).is_err());
}

#[test]
fn ahuadd_draws_the_kind_of_unit_asked_for_at_its_own_size() {
    let mut s = metric();
    let ahu = |s: &Session, h: &Value| cadcraft_doc::ahu_of(s.doc().unwrap().entity(hd(h)).unwrap()).unwrap();
    let df = s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    assert_eq!((ahu(&s, &df["handle"]).kind, ahu(&s, &df["handle"]).width_mm), (cadcraft_doc::AhuKind::DoubleFlow, 2000.0));
    let vmc = s.execute("ahuadd", &json!({"at": [5000, 0], "type": "vmc"})).unwrap();
    let v = ahu(&s, &vmc["handle"]);
    assert_eq!((v.kind, v.width_mm, v.depth_mm), (cadcraft_doc::AhuKind::VmcBox, 700.0, 550.0));
    let big = s.execute("ahuadd", &json!({"at": [9000, 0], "type": "supplyUnit", "width": 4000})).unwrap();
    assert_eq!((ahu(&s, &big["handle"]).width_mm, ahu(&s, &big["handle"]).depth_mm), (4000.0, 900.0), "the width given, the depth of the kind");
    assert!(s.execute("ahuadd", &json!({"at": [0, 0], "type": "boiler"})).is_err());
    // Editing the type keeps the size; at the command line the Type keyword draws the next one that way.
    s.execute("ahuedit", &json!({"handle": vmc["handle"], "type": "extractUnit"})).unwrap();
    assert_eq!((ahu(&s, &vmc["handle"]).kind, ahu(&s, &vmc["handle"]).width_mm), (cadcraft_doc::AhuKind::ExtractUnit, 700.0));
    for t in ["ahuadd", "t", "df", "0,5000", "0", "", ""] {
        s.cmdline(t).unwrap();
    }
    let n = s.doc().unwrap().model.iter().filter_map(|e| cadcraft_doc::ahu_of(e)).filter(|a| a.kind == cadcraft_doc::AhuKind::DoubleFlow).count();
    assert_eq!(n, 2);
}

/// A double-flux unit with its four connections: supply and extract on its right, fresh air and
/// exhaust on its left, each a duct to a terminal of its own direction (ducts left on auto).
fn four_connections(s: &mut Session, fresh: f64) -> Vec<Value> {
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    s.execute("ductadd", &json!({"points": [[1100, 250], [5000, 250]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[5000, -250], [1100, -250]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[-5000, 250], [-1100, 250]]})).unwrap();
    s.execute("ductadd", &json!({"points": [[-1100, -250], [-5000, -250]]})).unwrap();
    vec![
        s.execute("airterminaladd", &json!({"at": [5000, 250], "flow": 300})).unwrap(),
        s.execute("airterminaladd", &json!({"at": [5000, -250], "flow": 280, "air": "extract", "type": "squareCeiling"})).unwrap(),
        s.execute("airterminaladd", &json!({"at": [-5000, 250], "flow": fresh, "air": "fresh"})).unwrap(),
        s.execute("airterminaladd", &json!({"at": [-5000, -250], "flow": 280, "air": "exhaust"})).unwrap(),
    ]
}

#[test]
fn a_louvre_is_the_terminal_of_fresh_air_and_exhaust_and_ductsize_sizes_the_four_directions() {
    let mut s = metric();
    let ts = four_connections(&mut s, 300.0);
    let kinds: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| cadcraft_doc::air_terminal_of(e)).map(|t| (t.model.kind, t.air)).collect();
    assert!(kinds.contains(&(cadcraft_doc::TerminalKind::Louvre, cadcraft_doc::AirDirection::Fresh)), "{kinds:?}");
    assert!(kinds.contains(&(cadcraft_doc::TerminalKind::Louvre, cadcraft_doc::AirDirection::Exhaust)));
    let r = s.execute("ductsize", &json!({})).unwrap();
    let nets = r["networks"].as_array().unwrap();
    assert_eq!(nets.iter().map(|n| n["air"].as_str().unwrap()).collect::<Vec<_>>(), ["supply", "extract", "fresh", "exhaust"], "{r}");
    assert_eq!(nets.iter().map(|n| n["totalFlow"].as_f64().unwrap()).collect::<Vec<_>>(), [300.0, 280.0, 300.0, 280.0]);
    assert!(nets.iter().all(|n| n["warnings"].as_array().unwrap().is_empty()), "{r}");
    assert!(r["balance"].as_array().unwrap().is_empty(), "fresh air matches the supply and the exhaust the extract: {r}");
    assert_eq!((r["ducts"].as_u64(), r["reached"].as_u64()), (Some(4), Some(4)));
    let unit = cadcraft_doc::ahu_of(s.doc().unwrap().model.iter().find(|e| cadcraft_doc::ahu_of(e).is_some()).unwrap()).unwrap();
    assert_eq!((unit.fresh_flow_m3h, unit.exhaust_flow_m3h), (Some(300.0), Some(280.0)));
    // The fresh air a unit takes in no longer matches what it blows out: the sizing says so.
    s.execute("airterminaledit", &json!({"handle": ts[2]["handles"][0], "flow": 200})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    let b = r["balance"].as_array().unwrap();
    assert_eq!(b.len(), 1, "{r}");
    assert_eq!(
        (b[0]["left"].as_str(), b[0]["right"].as_str(), b[0]["leftFlow"].as_f64(), b[0]["rightFlow"].as_f64()),
        (Some("supply"), Some("fresh"), Some(300.0), Some(200.0))
    );
    assert!(s.execute("airterminaladd", &json!({"at": [0, 5000], "air": "sideways"})).is_err());
}

#[test]
fn ductedit_can_fix_a_duct_to_fresh_air_or_exhaust_and_ductadd_takes_them_as_options() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [2000, 0]], "air": "fresh"})).unwrap();
    let h = hd(&r["handles"][0]);
    let p = duct_of(s.doc().unwrap().entity(h).unwrap()).unwrap();
    assert_eq!((p.air, p.air_set), (cadcraft_doc::AirDirection::Fresh, true));
    assert_eq!(s.doc().unwrap().entity(h).unwrap().common.layer, "M-HVAC-DUCT-FRESH");
    s.execute("ductedit", &json!({"handle": r["handles"][0], "air": "exhaust"})).unwrap();
    assert_eq!(s.doc().unwrap().entity(h).unwrap().common.layer, "M-HVAC-DUCT-EXH");
    s.execute("ductedit", &json!({"handle": r["handles"][0], "air": "auto"})).unwrap();
    assert!(!duct_of(s.doc().unwrap().entity(h).unwrap()).unwrap().air_set);
    // At the command line: the Air keyword takes Fresh and Exhaust.
    for t in ["ductadd", "a", "fresh", "0,500", "1000,500", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(
        s.doc()
            .unwrap()
            .model
            .iter()
            .filter_map(|e| duct_of(e))
            .any(|p| p.air == cadcraft_doc::AirDirection::Fresh && p.air_set && p.length > 900.0 && p.length < 1100.0)
    );
}

#[test]
fn ahuedit_sets_the_direction_of_each_collar() {
    let mut s = Session::new();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    let h = s.doc().unwrap().model.iter().find(|e| cadcraft_doc::ahu_of(e).is_some()).unwrap().handle;
    let ports = |s: &Session| cadcraft_doc::ahu_of(s.doc().unwrap().entity(h).unwrap()).unwrap().roles();
    assert_eq!(ports(&s).len(), 4);
    s.execute("ahuedit", &json!({"handle": h, "ports": ["extract", "exhaust", "supply", "fresh"]})).unwrap();
    assert_eq!(ports(&s)[3], cadcraft_doc::AirDirection::Fresh);
    assert!(s.execute("ahuedit", &json!({"handle": h, "ports": ["supply"]})).is_err(), "one role per collar");
    assert!(s.execute("ahuedit", &json!({"handle": h, "ports": ["up", "up", "up", "up"]})).is_err());
    s.execute("ahuedit", &json!({"handle": h, "type": "supplyUnit"})).unwrap();
    assert_eq!(ports(&s).len(), 2, "a change of kind restores that kind's roles");
}

#[test]
fn ductsize_sizes_fresh_air_and_exhaust_ducts_that_end_in_the_open_and_sets_their_direction() {
    let mut s = metric();
    four_connections(&mut s, 300.0);
    // Take the two louvres away: the fresh air and exhaust ducts now end in a wall passage nobody drew.
    let louvres: Vec<Handle> = s
        .doc()
        .unwrap()
        .model
        .iter()
        .filter(|e| cadcraft_doc::air_terminal_of(e).is_some_and(|t| t.model.kind == cadcraft_doc::TerminalKind::Louvre))
        .map(|e| e.handle)
        .collect();
    assert_eq!(louvres.len(), 2);
    s.execute("erase", &json!({"handles": louvres.iter().map(|h| h.hex()).collect::<Vec<_>>()})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    let nets = r["networks"].as_array().unwrap();
    assert_eq!(nets.iter().map(|n| n["air"].as_str().unwrap()).collect::<Vec<_>>(), ["supply", "extract", "fresh", "exhaust"], "{r}");
    // Fresh air takes the supply's flow, exhaust the extract's.
    assert_eq!(nets.iter().map(|n| n["totalFlow"].as_f64().unwrap()).collect::<Vec<_>>(), [300.0, 280.0, 300.0, 280.0], "{r}");
    let airs: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| duct_of(e).map(|p| p.air)).collect();
    assert_eq!(airs.iter().filter(|a| **a == cadcraft_doc::AirDirection::Fresh).count(), 1, "{airs:?}");
    assert_eq!(airs.iter().filter(|a| **a == cadcraft_doc::AirDirection::Exhaust).count(), 1, "{airs:?}");
}

#[test]
fn ahuadd_and_ahuedit_take_the_height_the_make_and_the_weight_of_the_unit() {
    let mut s = metric();
    let r = s
        .execute("ahuadd", &json!({"at": [0, 0], "width": 1600, "depth": 1510, "height": 1659, "model": "CIAT FLOWAY RHE 5000", "weight": 600}))
        .unwrap();
    let h = hd(&r["handle"]);
    let a = ahu_of(s.doc().unwrap().entity(h).unwrap()).unwrap();
    assert_eq!((a.height_mm, a.weight_kg, a.model.as_str()), (Some(1659.0), Some(600.0), "CIAT FLOWAY RHE 5000"));
    s.execute("ahuedit", &json!({"handle": r["handle"], "height": 1800, "weight": null, "depth": 1400})).unwrap();
    let a = ahu_of(s.doc().unwrap().entity(h).unwrap()).unwrap();
    assert_eq!((a.height_mm, a.weight_kg, a.depth_mm), (Some(1800.0), None, 1400.0));
    assert!(s.execute("ahuedit", &json!({"handle": r["handle"], "height": -3})).is_err());
}

#[test]
fn ductsize_shares_the_flow_a_space_asks_between_its_terminals_and_leaves_unequal_ones() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [-3000, 2500]})).unwrap();
    let room = [[0, 0], [6000, 0], [6000, 5000], [0, 5000]];
    let sp = s.execute("spaceadd", &json!({"points": room, "name": "RDC", "flow": 790})).unwrap();
    let placed = s.execute("airterminaladd", &json!({"space": sp["handle"], "count": 9})).unwrap();
    let handles: Vec<Handle> = placed["handles"].as_array().unwrap().iter().map(hd).collect();
    let flows = |s: &Session| -> Vec<f64> {
        handles.iter().map(|h| cadcraft_doc::air_terminal_of(s.doc().unwrap().entity(*h).unwrap()).unwrap().flow_m3h).collect()
    };
    assert!(flows(&s).iter().all(|f| (f - 790.0 / 9.0).abs() < 1e-6));
    // The space is asked for more afterwards: sizing shares 3500 m³/h between the 9, rounded up to 10.
    s.execute("spaceedit", &json!({"handle": sp["handle"], "flow": 3500})).unwrap();
    let r = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(flows(&s), vec![390.0; 9], "{r}");
    let done = r["rebalanced"].as_array().unwrap();
    assert_eq!((done.len(), done[0]["terminals"].as_u64(), done[0]["to"].as_f64()), (1, Some(9), Some(390.0)), "{r}");
    assert!(done[0].get("error").is_none(), "{r}");
    // Done once: a second sizing has nothing to change.
    assert!(s.execute("ductsize", &json!({})).unwrap()["rebalanced"].as_array().unwrap().is_empty());
    // One terminal set apart is a choice: the others stay too, and `balanceTerminals: false` leaves all.
    s.execute("airterminaledit", &json!({"handle": placed["handles"][0], "flow": 100})).unwrap();
    s.execute("spaceedit", &json!({"handle": sp["handle"], "flow": 2000})).unwrap();
    assert!(s.execute("ductsize", &json!({})).unwrap()["rebalanced"].as_array().unwrap().is_empty());
    assert_eq!(flows(&s)[0], 100.0);
    s.execute("airterminaledit", &json!({"handles": placed["handles"], "flow": 150})).unwrap();
    assert!(s.execute("ductsize", &json!({"balanceTerminals": false})).unwrap()["rebalanced"].as_array().unwrap().is_empty());
    assert_eq!(flows(&s), vec![150.0; 9]);
}

#[test]
fn ductcross_lists_the_crossings_ductsize_reports_the_ones_left_and_the_one_under_is_drawn_broken() {
    let mut s = metric();
    let a = s.execute("ductadd", &json!({"points": [[0, 0], [4000, 0]], "diameter": 200})).unwrap();
    let b = s.execute("ductadd", &json!({"points": [[2000, -1500], [2000, 1500]], "diameter": 160})).unwrap();
    let (over, under) = (a["handles"][0].clone(), b["handles"][0].clone());
    let list = s.execute("ductcross", &json!({})).unwrap();
    assert_eq!(list["crossings"].as_array().unwrap().len(), 1, "{list}");
    assert_eq!(list["crossings"][0]["resolved"], false);
    let r = s.execute("ductcross", &json!({"under": under, "over": over, "clearance": 50})).unwrap();
    assert_eq!(r["gap"], json!([1500.0 - 150.0, 1500.0 + 150.0]), "{r}");
    assert_eq!(s.execute("ductcross", &json!({})).unwrap()["crossings"][0]["resolved"], true);
    // DUCTSIZE names the crossings still to draw (none now); a new crossing is named.
    s.execute("ahuadd", &json!({"at": [-3000, 0]})).unwrap();
    assert!(s.execute("ductsize", &json!({})).unwrap()["crossings"].as_array().unwrap().is_empty());
    s.execute("ductadd", &json!({"points": [[3000, -1000], [3000, 1000]], "diameter": 160})).unwrap();
    let out = s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(out["crossings"].as_array().unwrap().len(), 1, "{out}");
    assert_eq!(out["crossings"][0]["at"], json!([3000.0, 0.0]));
    // Drawn whole again.
    assert_eq!(s.execute("ductcross", &json!({"clear": [under]})).unwrap()["cleared"], 1);
    assert_eq!(s.execute("ductcross", &json!({})).unwrap()["crossings"].as_array().unwrap().iter().filter(|c| c["resolved"] == true).count(), 0);
    assert!(s.execute("ductcross", &json!({"under": under, "over": under})).is_err());
}

#[test]
fn ductcross_with_jog_makes_the_named_duct_rise_or_dip_by_four_elbows_and_ductsize_counts_them() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [-3000, 0]})).unwrap();
    let a = s.execute("ductadd", &json!({"points": [[0, 0], [6000, 0]], "diameter": 200, "air": "supply"})).unwrap();
    let b = s.execute("ductadd", &json!({"points": [[3000, -3000], [3000, 3000]], "diameter": 200, "air": "extract"})).unwrap();
    let (over, under) = (a["handles"][0].clone(), b["handles"][0].clone());
    let r = s.execute("ductcross", &json!({"under": under, "over": over, "jog": "under", "shift": 300})).unwrap();
    assert_eq!(r["jog"]["down"], true, "{r}");
    assert_eq!(r["jog"]["shift"], 300.0);
    let jogs = |s: &Session, h: &Value| cadcraft_doc::duct_of(s.doc().unwrap().entity(hd(h)).unwrap()).unwrap().jogs.len();
    assert_eq!((jogs(&s, &under), jogs(&s, &over)), (1, 0), "the duct that dips makes the elbows, the other stays straight");
    // The elbows over: the other way round.
    let r = s.execute("ductcross", &json!({"under": under, "over": over, "jog": "over"})).unwrap();
    assert_eq!(r["jog"]["down"], false, "{r}");
    assert_eq!(jogs(&s, &over), 1);
    assert!(s.execute("ductcross", &json!({"under": under, "over": over, "jog": "sideways"})).is_err());
    // Drawn whole again: no gap, no elbows.
    s.execute("ductcross", &json!({"clear": [under, over]})).unwrap();
    assert_eq!((jogs(&s, &under), jogs(&s, &over)), (0, 0));
}

#[test]
fn a_long_run_to_a_single_terminal_is_a_branch_not_a_connection_so_it_is_not_sized_at_the_connection_velocity() {
    // 350 m³/h: a round Ø200 runs at 3.1 m/s, under a branch's 4 m/s but over a connection's 2.5 m/s.
    let diameter = |len: f64, kind: Option<&str>| {
        let mut s = metric();
        s.execute("ahuadd", &json!({"at": [-3000, 0], "type": "supplyUnit"})).unwrap();
        let d = s.execute("ductadd", &json!({"points": [[-1700, 0], [-1700, len]]})).unwrap();
        if let Some(k) = kind {
            s.execute("ductedit", &json!({"handle": d["handles"][0], "kind": k})).unwrap();
        }
        s.execute("airterminaladd", &json!({"at": [-1700, len], "flow": 350, "air": "supply"})).unwrap();
        let r = s.execute("ductsize", &json!({})).unwrap();
        r["networks"][0]["segments"][0]["diameter"].as_f64().unwrap()
    };
    assert_eq!(diameter(1500.0, None), 250.0, "a drop of 1.5 m is a connection: 2.5 m/s");
    assert_eq!(diameter(8000.0, None), 200.0, "8 m is a branch: 4 m/s");
    // What the user sets stays: a long run given as a connection is sized as one.
    assert_eq!(diameter(8000.0, Some("connection")), 250.0);
    assert_eq!(diameter(1500.0, Some("branch")), 200.0);
}

#[test]
fn ductsize_records_the_type_it_gave_a_run_left_on_auto_and_the_kind_edit_forgets_it() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [-3000, 0], "type": "supplyUnit"})).unwrap();
    let long = s.execute("ductadd", &json!({"points": [[-1700, 0], [-1700, 8000]]})).unwrap();
    let short = s.execute("ductadd", &json!({"points": [[-2500, 0], [-2500, 1500]]})).unwrap();
    s.execute("airterminaladd", &json!({"at": [-1700, 8000], "flow": 350, "air": "supply"})).unwrap();
    s.execute("airterminaladd", &json!({"at": [-2500, 1500], "flow": 350, "air": "supply"})).unwrap();
    let found = |s: &Session, h: &Value| cadcraft_doc::duct_of(s.doc().unwrap().entity(hd(h)).unwrap()).unwrap().kind_found;
    assert_eq!(found(&s, &long["handles"][0]), None, "not sized yet");
    s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(found(&s, &long["handles"][0]), Some(DuctKind::Main), "a long run on the trunk");
    assert_eq!(found(&s, &short["handles"][0]), Some(DuctKind::Connection));
    s.execute("ductedit", &json!({"handle": long["handles"][0], "kind": "main"})).unwrap();
    assert_eq!(found(&s, &long["handles"][0]), None, "the type was set by hand");
}

#[test]
fn the_orifice_of_a_collar_is_joined_to_the_duct_that_starts_on_it_by_a_reducer_sized_for_the_flow() {
    let mut s = metric();
    let u = s.execute("ahuadd", &json!({"at": [0, 0], "type": "supplyUnit"})).unwrap();
    // The supply collar (right) is 800 x 400, the fresh air one (left) 315.
    s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": [315, "800x400"]})).unwrap();
    let unit = ahu_of(s.doc().unwrap().entity(hd(&u["handle"])).unwrap()).unwrap();
    assert_eq!(unit.port_sizes.len(), 2);
    let d = s.execute("ductadd", &json!({"points": [[1300, 0], [6000, 0]], "air": "supply"})).unwrap();
    s.execute("airterminaladd", &json!({"at": [6000, 0], "flow": 300, "air": "supply"})).unwrap();
    s.execute("ductsize", &json!({})).unwrap();
    let p = duct_of(s.doc().unwrap().entity(hd(&d["handles"][0])).unwrap()).unwrap();
    // The duct is the size its 300 m³/h ask, not the orifice's; the reducer takes the orifice's width down to it.
    assert!(p.width_mm < 800.0 && p.diameter_mm < 400.0, "{:?} {} {}", p.shape, p.width_mm, p.diameter_mm);
    let red = p.reducer.expect("a reducer between the unit and the duct");
    assert_eq!(red.from_mm, 800.0);
    assert!(!red.at_end, "at the unit's end, the duct's start");
    // A size set by hand is kept, and the reducer goes to it.
    s.execute("ductedit", &json!({"handle": d["handles"][0], "diameter": 250})).unwrap();
    s.execute("ductsize", &json!({})).unwrap();
    let p = duct_of(s.doc().unwrap().entity(hd(&d["handles"][0])).unwrap()).unwrap();
    assert_eq!((p.shape, p.diameter_mm, p.reducer.map(|r| r.from_mm)), (cadcraft_doc::DuctShape::Round, 250.0, Some(800.0)));
    // An orifice of the duct's own width needs none.
    s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": [315, 250]})).unwrap();
    s.execute("ductsize", &json!({})).unwrap();
    assert!(duct_of(s.doc().unwrap().entity(hd(&d["handles"][0])).unwrap()).unwrap().reducer.is_none());
    assert!(s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": ["wide"]})).is_err());
    assert!(s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": [1, 2, 3]})).is_err(), "a supply unit has two collars");
}

#[test]
fn a_double_flux_unit_is_made_horizontal_or_vertical_by_its_type_and_keeps_the_sizes_of_its_orifices() {
    let mut s = metric();
    let u = s.execute("ahuadd", &json!({"at": [0, 0], "type": "horizontal"})).unwrap();
    let kind = |s: &Session| ahu_of(s.doc().unwrap().entity(hd(&u["handle"])).unwrap()).unwrap();
    assert_eq!(kind(&s).kind, cadcraft_doc::AhuKind::DoubleFlow);
    s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": [null, null, "1000x400", 250]})).unwrap();
    s.execute("ahuedit", &json!({"handle": u["handle"], "type": "vertical"})).unwrap();
    let v = kind(&s);
    assert_eq!(v.kind, cadcraft_doc::AhuKind::StackedDoubleFlow);
    assert_eq!(v.port_sizes.len(), 4, "the sizes stay");
    // Its collars are the stacked ones: the two of an end at one point.
    let tips = v.port_tips();
    assert_eq!((tips[0].0, tips[2].0), (tips[1].0, tips[3].0));
    s.execute("ahuedit", &json!({"handle": u["handle"], "type": "horizontal"})).unwrap();
    assert_eq!(kind(&s).kind, cadcraft_doc::AhuKind::DoubleFlow);
}

#[test]
fn ductcross_draws_a_duct_under_another_it_runs_inside_of_and_lists_it_as_an_overlap() {
    let mut s = metric();
    let big = s.execute("ductadd", &json!({"points": [[0, 0], [0, -3000]], "diameter": 650})).unwrap();
    let small = s.execute("ductadd", &json!({"points": [[0, 0], [0, -2000]], "diameter": 200})).unwrap();
    let list = s.execute("ductcross", &json!({})).unwrap();
    let c = &list["crossings"][0];
    assert_eq!((c["overlap"].as_bool(), c["resolved"].as_bool(), c["a"].clone()), (Some(true), Some(false), small["handles"][0].clone()), "{list}");
    let r = s.execute("ductcross", &json!({"under": small["handles"][0], "over": big["handles"][0]})).unwrap();
    assert_eq!(r["gap"], json!([0.0, 2000.0]), "{r}");
    assert_eq!(s.execute("ductcross", &json!({})).unwrap()["crossings"][0]["resolved"], true);
}

#[test]
fn a_run_started_inside_the_width_of_another_cuts_it_and_the_narrower_still_passes_under_the_whole_run() {
    let mut s = metric();
    // The extract (650) down, then the supply (200) from the same point, down inside it and turning away: the
    // turn starts on the axis of the wide one, which is cut there into two pieces.
    s.execute("ductadd", &json!({"points": [[0, 0], [0, -3000]], "diameter": 650})).unwrap();
    s.execute("ductadd", &json!({"points": [[0, 0], [0, -2000], [-3000, -2000]], "diameter": 200})).unwrap();
    let ducts = |s: &Session| -> Vec<(Handle, cadcraft_doc::DuctParams, cadcraft_geom::Vec2, cadcraft_geom::Vec2)> {
        let d = s.doc().unwrap();
        d.model.iter().filter_map(|e| d.duct_ends(e.handle).map(|(p, a, b)| (e.handle, p, a, b))).collect()
    };
    let all = ducts(&s);
    let wide: Vec<_> = all.iter().filter(|x| x.1.diameter_mm == 650.0).collect();
    assert_eq!(wide.len(), 2, "the wide duct was cut in two where the turn starts");
    let small = all.iter().find(|x| x.1.diameter_mm == 200.0 && (x.2.x - x.3.x).abs() < 1e-6).unwrap().0;
    // Naming the lower piece, the one the click may reach, is enough: the narrower passes under the whole run.
    let lower = wide.iter().find(|x| x.2.y.min(x.3.y) < -2500.0).unwrap().0;
    let r = s.execute("ductcross", &json!({"under": small.hex(), "over": lower.hex()})).unwrap();
    assert_eq!(r["gap"], json!([0.0, 2000.0]), "{r}");
    // Named the wrong way round, the same.
    s.execute("ductcross", &json!({"clear": [small.hex()]})).unwrap();
    let r = s.execute("ductcross", &json!({"under": lower.hex(), "over": small.hex()})).unwrap();
    assert_eq!(r["gap"], json!([0.0, 2000.0]), "{r}");
    // The one that turns away comes out from under the edge.
    let turn = all.iter().find(|x| x.1.diameter_mm == 200.0 && (x.2.y - x.3.y).abs() < 1e-6).unwrap().0;
    let r = s.execute("ductcross", &json!({"under": turn.hex(), "over": lower.hex(), "clearance": 30})).unwrap();
    let hi = r["gap"][1].as_f64().unwrap();
    assert!((hi - 355.0).abs() < 1e-6, "{r}");
}

#[test]
fn a_height_given_to_a_run_makes_the_step_between_runs_at_other_levels_and_the_lower_of_two_crossing_ducts_goes_under() {
    let mut s = metric();
    s.execute("ahuadd", &json!({"at": [0, 0]})).unwrap();
    // Two joined runs of the supply, the second 250 mm higher than the first; a terminal at its end.
    let a = s.execute("ductadd", &json!({"points": [[800, -300], [3000, -300]], "diameter": 200, "air": "supply", "altitude": 2.5})).unwrap();
    let b = s.execute("ductadd", &json!({"points": [[3000, -300], [6000, -300]], "diameter": 200, "air": "supply", "altitude": 2.75})).unwrap();
    s.execute("airterminaladd", &json!({"at": [6000, -300], "flow": 200, "air": "supply"})).unwrap();
    // A duct that crosses the second one, lower than it, and one at the same level as it.
    let low = s.execute("ductadd", &json!({"points": [[4500, -2000], [4500, 1500]], "diameter": 200, "altitude": 2.0})).unwrap();
    let same = s.execute("ductadd", &json!({"points": [[5300, -2000], [5300, 1500]], "diameter": 200, "altitude": 2.75})).unwrap();
    let p = |s: &Session, h: &Value| cadcraft_doc::duct_of(s.doc().unwrap().entity(hd(&h["handles"][0])).unwrap()).unwrap();
    assert_eq!((p(&s, &a).level_mm, p(&s, &b).level_mm), (Some(2500.0), Some(2750.0)));
    let out = s.execute("ductsize", &json!({})).unwrap();
    assert!(out["stepWarnings"].as_array().unwrap().is_empty(), "{out}");
    let (pa, pb) = (p(&s, &a), p(&s, &b));
    assert!(pa.steps.is_empty(), "the first run is where the unit leaves it");
    assert_eq!(pb.steps.len(), 1, "{:?}", pb.steps);
    assert_eq!((pb.steps[0].at_mm, pb.steps[0].shift_mm), (0.0, 250.0), "up 250 mm at the start of the second run");
    // The altitude is written on the label.
    let labels = |s: &Session, h: &Value| -> Vec<String> {
        let d = s.doc().unwrap();
        let EntityKind::Insert(i) = &d.entity(hd(&h["handles"][0])).unwrap().kind else { return Vec::new() };
        d.block(&i.block)
            .unwrap()
            .entities
            .iter()
            .filter_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.value.clone()) } else { None })
            .collect()
    };
    assert!(labels(&s, &b).iter().any(|t| t.contains("Ai:2.75")), "{:?}", labels(&s, &b));
    assert!(labels(&s, &b).iter().any(|t| t == "+250"), "the step is written: {:?}", labels(&s, &b));
    // The lower duct passes under where it crosses; the one at the same level is left.
    assert_eq!(out["autoCrossed"], 1, "{out}");
    assert_eq!(p(&s, &low).gaps_mm.len(), 1);
    assert!(p(&s, &same).gaps_mm.is_empty() && p(&s, &b).gaps_mm.is_empty());
    // A run too short for its step is named. The altitude can be given back to its zone.
    s.execute("ductedit", &json!({"handle": b["handles"][0], "altitude": null})).unwrap();
    assert_eq!(p(&s, &b).level_mm, None);
    assert!(s.execute("ductedit", &json!({"handle": a["handles"][0], "altitude": "high"})).is_err());
}

#[test]
fn duct_grips_stretch_an_end_move_the_run_and_drag_the_runs_that_meet_there() {
    let mut s = metric();
    let r = s.execute("ductadd", &json!({"points": [[0, 0], [4000, 0], [4000, 3000]], "diameter": 315})).unwrap();
    let hs: Vec<Handle> = r["handles"].as_array().unwrap().iter().map(hd).collect();
    let ends = |s: &Session, h: Handle| s.doc().unwrap().duct_ends(h).map(|(p, a, b)| (p.length, a, b)).unwrap();
    let grips = s.grips_of(hs[0]).unwrap_or_default();
    assert_eq!(grips.len(), 1, "grips_of still reads the shape; the duct grips come from the drawing");
    let d = s.doc().unwrap();
    assert_eq!(d.entity_grips(d.entity(hs[0]).unwrap()).len(), 3);
    // Select both runs, drag the shared corner: both follow.
    s.set_selection(hs.clone());
    s.grip_edit(hs[0], 2, cadcraft_geom::Vec2::new(5000.0, 500.0), cadcraft_engine::grips::GripMode::Stretch).unwrap();
    let (l0, _, b0) = ends(&s, hs[0]);
    let (_, a1, _) = ends(&s, hs[1]);
    assert!(b0.dist(cadcraft_geom::Vec2::new(5000.0, 500.0)) < 1e-6);
    assert!(a1.dist(cadcraft_geom::Vec2::new(5000.0, 500.0)) < 1e-6);
    assert!((l0 - 5000.0f64.hypot(500.0)).abs() < 1e-6);
    // The middle grip moves the whole run.
    s.set_selection(hs[..1].to_vec());
    let (_, a, b) = ends(&s, hs[0]);
    s.grip_edit(hs[0], 1, a.mid(b) + cadcraft_geom::Vec2::new(0.0, 100.0), cadcraft_engine::grips::GripMode::Stretch).unwrap();
    let (_, a2, b2) = ends(&s, hs[0]);
    assert!((a2.y - a.y - 100.0).abs() < 1e-6 && (b2.y - b.y - 100.0).abs() < 1e-6);
    // Dragging an end onto the other end is refused.
    assert!(s.grip_edit(hs[0], 0, b2, cadcraft_engine::grips::GripMode::Stretch).is_err());
}

#[test]
fn the_sizing_counts_the_loss_of_the_reducer_from_the_units_orifice_to_the_duct() {
    let pressure = |sizes: Value| {
        let mut s = metric();
        let u = s.execute("ahuadd", &json!({"at": [0, 0], "type": "supplyUnit"})).unwrap();
        s.execute("ahuedit", &json!({"handle": u["handle"], "portSizes": sizes})).unwrap();
        s.execute("ductadd", &json!({"points": [[1300, 0], [6000, 0]], "air": "supply"})).unwrap();
        s.execute("airterminaladd", &json!({"at": [6000, 0], "flow": 300, "air": "supply"})).unwrap();
        let r = s.execute("ductsize", &json!({})).unwrap();
        r["networks"][0]["requiredPressure"].as_f64().unwrap()
    };
    let (none, wide) = (pressure(json!([null, null])), pressure(json!([null, "800x400"])));
    assert!(wide > none, "the reducer adds its loss: {wide} against {none}");
}

/// The supply drawn first, 80 mm up, then the extract over it on the same route, 900 mm up, both from the stacked
/// collars of a vertical unit to a riser each (as on a roof plan): each keeps its own air direction, whichever was drawn first.
fn superposed_roof(supply_first: bool) -> (Session, Vec<Handle>, Vec<Handle>) {
    let mut s = metric();
    let u = s.execute("ahuadd", &json!({"at": [0, 0], "type": "vertical"})).unwrap();
    let tip = ahu_of(s.doc().unwrap().entity(hd(&u["handle"])).unwrap()).unwrap().port_tips()[2].0;
    let p = tip.x;
    s.execute("riseradd", &json!({"at": [5000, 3400], "width": 800, "depth": 600, "air": "supply", "incomingFlow": 7000})).unwrap();
    s.execute("riseradd", &json!({"at": [7500, 3400], "width": 800, "depth": 600, "air": "extract", "incomingFlow": 7000})).unwrap();
    let mut supply = Vec::new();
    let mut extract = Vec::new();
    let mut add_supply = |s: &mut Session| {
        for pts in [json!([[p, 0], [5000, 0]]), json!([[5000, 0], [5000, 3100]])] {
            supply.push(hd(&s.execute("ductadd", &json!({"points": pts, "altitude": 0.08})).unwrap()["handles"][0]));
        }
    };
    let mut add_extract = |s: &mut Session| {
        for pts in [json!([[p, 0], [5000, 0]]), json!([[5000, 0], [7500, 0]]), json!([[7500, 0], [7500, 3100]])] {
            extract.push(hd(&s.execute("ductadd", &json!({"points": pts, "altitude": 0.9})).unwrap()["handles"][0]));
        }
    };
    if supply_first {
        add_supply(&mut s);
        s.execute("ductsize", &json!({})).unwrap();
        add_extract(&mut s);
    } else {
        add_extract(&mut s);
        s.execute("ductsize", &json!({})).unwrap();
        add_supply(&mut s);
    }
    s.execute("ductsize", &json!({})).unwrap();
    (s, supply, extract)
}

#[test]
fn two_ducts_one_over_the_other_from_stacked_collars_keep_their_air_whichever_was_drawn_first() {
    for supply_first in [true, false] {
        let (s, supply, extract) = superposed_roof(supply_first);
        let air = |h: &Handle| duct_of(s.doc().unwrap().entity(*h).unwrap()).unwrap().air;
        assert!(
            supply.iter().all(|h| air(h) == cadcraft_doc::AirDirection::Supply),
            "supply first {supply_first}: {:?}",
            supply.iter().map(air).collect::<Vec<_>>()
        );
        assert!(
            extract.iter().all(|h| air(h) == cadcraft_doc::AirDirection::Extract),
            "supply first {supply_first}: {:?}",
            extract.iter().map(air).collect::<Vec<_>>()
        );
    }
}
