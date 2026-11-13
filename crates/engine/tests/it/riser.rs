//! RISERADD draws the shaft of a riser with its symbol and label, by points, by a closed
//! polyline or by a centre and a size; RISEREDIT changes the way, the air, the flows, the size and the shaft.

use cadcraft_doc::{AirDirection, Drawing, Handle, RiserSize, riser_of};
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

fn riser(s: &Session, h: Handle) -> cadcraft_doc::RiserParams {
    riser_of(s.doc().unwrap().entity(h).unwrap()).unwrap()
}

#[test]
fn riseradd_takes_a_shaft_by_points_by_a_centre_and_size_or_by_a_polyline() {
    let mut s = metric();
    let r = s
        .execute(
            "riseradd",
            &json!({"points": [[0, 0], [600, 0], [600, 400], [0, 400]], "name": "VMC 3", "air": "extract", "size": 200, "flow": 240}),
        )
        .unwrap();
    let p = riser(&s, hd(&r["handle"]));
    assert_eq!((p.name.as_str(), p.air, p.up, p.shaft_mm()), ("VMC 3", AirDirection::Extract, true, (600.0, 400.0)));
    assert_eq!((p.size, p.flow_m3h, p.text().as_str()), (Some(RiserSize::Round { diameter_mm: 200.0 }), Some(240.0), "VMC 3 Ø200-240 m³/h"));
    // A centre and a size make a rectangle around it; the way is given.
    let r = s.execute("riseradd", &json!({"at": [5000, 5000], "width": 800, "depth": 500, "direction": "down", "air": "fresh"})).unwrap();
    let q = riser(&s, hd(&r["handle"]));
    assert_eq!((q.shaft_mm(), q.up, q.air, q.name.as_str()), ((800.0, 500.0), false, AirDirection::Fresh, "Riser 2"));
    // A closed polyline already drawn.
    let pl = s.execute("pline", &json!({"points": [[0, 2000], [300, 2000], [300, 2300], [0, 2300]], "closed": true})).unwrap();
    let r = s.execute("riseradd", &json!({"handle": pl["handle"]})).unwrap();
    assert_eq!(riser(&s, hd(&r["handle"])).shaft_mm(), (300.0, 300.0));
    assert!(s.execute("riseradd", &json!({"points": [[0, 0], [1, 0], [1, 1]], "air": "sideways"})).is_err());
    assert!(s.execute("riseradd", &json!({"name": "x"})).is_err());
}

#[test]
fn riseredit_changes_the_way_the_flows_the_size_and_the_shaft_and_clears_what_is_null() {
    let mut s = metric();
    let r = s.execute("riseradd", &json!({"at": [0, 0], "width": 600, "depth": 400})).unwrap();
    let h = r["handle"].clone();
    s.execute("riseredit", &json!({"handle": h, "direction": "down", "air": "supply", "incomingFlow": 660, "size": "400x250", "width": 800}))
        .unwrap();
    let p = riser(&s, hd(&h));
    assert_eq!((p.up, p.air, p.incoming_flow_m3h, p.shaft_mm()), (false, AirDirection::Supply, Some(660.0), (800.0, 400.0)));
    assert_eq!(p.size, Some(RiserSize::Rectangular { width_mm: 400.0, height_mm: 250.0 }));
    assert_eq!(p.text(), "Riser 1 400×250-660 m³/h");
    s.execute("riseredit", &json!({"handle": h, "incomingFlow": null, "size": "auto"})).unwrap();
    let p = riser(&s, hd(&h));
    assert_eq!((p.incoming_flow_m3h, p.size), (None, None));
    assert!(s.execute("riseredit", &json!({"handle": h, "flow": -5})).is_err());
    assert!(s.execute("riseredit", &json!({"handle": h, "size": "wide"})).is_err());
    // Not a riser.
    let line = s.execute("line", &json!({"points": [[0, 0], [10, 0]]})).unwrap();
    assert!(s.execute("riseredit", &json!({"handle": line["handles"][0], "direction": "up"})).is_err());
}

#[test]
fn riseradd_at_the_command_line_takes_the_options_then_the_shaft_points() {
    let mut s = metric();
    for t in ["riseradd", "n", "A.R", "a", "fresh", "d", "0,0", "600,0", "600,400", "c"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let d = s.doc().unwrap();
    let p = d.model.iter().find_map(|e| riser_of(e)).unwrap();
    assert_eq!((p.name.as_str(), p.air, p.up, p.shaft_mm()), ("A.R", AirDirection::Fresh, false, (600.0, 400.0)));
    // What was used last is the next default.
    let r = s.execute("riseradd", &json!({"at": [3000, 0], "width": 300, "depth": 300, "name": "B"})).unwrap();
    let q = riser(&s, hd(&r["handle"]));
    assert_eq!((q.air, q.up), (AirDirection::Fresh, false));
}

#[test]
fn ductsize_reads_a_floor_from_its_riser_and_writes_back_the_flow_it_carries_and_its_size() {
    let mut s = metric();
    // The shaft, 600 x 400, extract, 660 m³/h coming from the floors below; two extract terminals of this floor.
    let r =
        s.execute("riseradd", &json!({"at": [300, 200], "width": 600, "depth": 400, "name": "A.R", "air": "extract", "incomingFlow": 660})).unwrap();
    s.execute("ductadd", &json!({"points": [[600, 200], [5000, 200]], "air": "extract"})).unwrap();
    s.execute("airterminaladd", &json!({"at": [5000, 200], "flow": 250, "air": "extract", "type": "squareCeiling"})).unwrap();
    let out = s.execute("ductsize", &json!({})).unwrap();
    let nets = out["networks"].as_array().unwrap();
    assert_eq!((nets.len(), nets[0]["air"].as_str(), nets[0]["totalFlow"].as_f64()), (1, Some("extract"), Some(250.0)), "{out}");
    let p = riser(&s, hd(&r["handle"]));
    assert_eq!(p.flow_m3h, Some(910.0), "this floor's 250 and the 660 from below");
    assert!(p.size.is_some(), "the riser is sized for that flow in its shaft");
    assert!(p.text().contains("910 m³/h"), "{}", p.text());
    // A size typed on the riser is kept by the next sizing.
    s.execute("riseredit", &json!({"handle": r["handle"], "size": 250})).unwrap();
    s.execute("ductsize", &json!({})).unwrap();
    assert_eq!(riser(&s, hd(&r["handle"])).size, Some(RiserSize::Round { diameter_mm: 250.0 }));
}

#[test]
fn a_run_started_on_the_edge_of_another_near_its_corner_leaves_no_short_piece_out_of_the_sizing() {
    // From the riser up, then right to the far end; then, from the corner facing the riser's outlet, to
    // the left: the click lands on the edge of the run, a few centimetres from where the first one turns.
    for start in [[300.0, 3000.0], [300.0, 3090.0], [330.0, 3090.0], [270.0, 3085.0], [380.0, 3090.0]] {
        let mut s = metric();
        s.execute("riseradd", &json!({"at": [300, 200], "width": 600, "depth": 400, "air": "extract", "name": "R"})).unwrap();
        s.execute("ductadd", &json!({"points": [[300, 200], [300, 3000], [6000, 3000]], "diameter": 200})).unwrap();
        s.execute("ductadd", &json!({"points": [start, [-4000, 3090]], "diameter": 200})).unwrap();
        // Another run's end a hand's breadth before the end of the left one: it must not leave a stub either.
        for at in [[6000, 3000], [-4000, 3090]] {
            s.execute("airterminaladd", &json!({"at": at, "flow": 200, "air": "extract", "type": "squareCeiling"})).unwrap();
        }
        let r = s.execute("ductsize", &json!({})).unwrap();
        let net = &r["networks"][0];
        assert_eq!(net["totalFlow"].as_f64(), Some(400.0), "start {start:?}: {r}");
        assert!(net["warnings"].as_array().unwrap().is_empty(), "start {start:?}: {net}");
        assert_eq!((r["reached"].as_u64(), r["ducts"].as_u64()), (r["ducts"].as_u64(), r["ducts"].as_u64()), "{r}");
        let d = s.doc().unwrap();
        for e in d.model.iter() {
            if let Some((p, a, b)) = d.duct_ends(e.handle) {
                assert!(a.dist(b) > 100.0, "start {start:?}: a piece of {:.0} mm", a.dist(b));
                assert_eq!(p.air, cadcraft_doc::AirDirection::Extract, "start {start:?}");
            }
        }
    }
}
