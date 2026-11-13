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
