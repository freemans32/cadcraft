//! AHUADD places an air handling unit; DUCTADD draws round duct runs, one per segment, and cuts a
//! run another starts on; DUCTEDIT changes them; DUCTSIZE sizes each network from its terminals.

use cadcraft_doc::{AccessoryKind, Drawing, DuctKind, Handle, accessory_of, ahu_of, duct_of};
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
    let r = s.execute("ductsize", &json!({})).unwrap();
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
    assert_eq!(s.doc().unwrap().entity(h).unwrap().common.layer, "M-HVAC-DUCT");
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
