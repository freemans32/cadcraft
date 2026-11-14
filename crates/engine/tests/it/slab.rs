//! SLABADD draws a slab over a closed outline; SLABEDIT changes it.

use cadcraft_doc::{Drawing, Handle, SlabKind, SlabParams, slab_of};
use cadcraft_engine::Session;
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::new();
    *s.doc_mut().unwrap() = Drawing::new_metric();
    s
}

fn slab(s: &Session, v: &Value) -> SlabParams {
    slab_of(s.doc().unwrap().entity(Handle::parse_hex(v.as_str().unwrap()).unwrap()).unwrap()).expect("a slab")
}

#[test]
fn slabadd_draws_a_slab_and_slabedit_changes_it() {
    let mut s = metric();
    let r = s.execute("slabadd", &json!({"points": [[0, 0], [5000, 0], [5000, 4000], [0, 4000]], "kind": "roof", "elevation": 6.0})).unwrap();
    assert_eq!((r["area"].as_f64(), r["volume"].as_f64()), (Some(20.0), Some(5.0)));
    let p = slab(&s, &r["handle"]);
    assert_eq!((p.kind, p.thickness_mm, p.elevation_mm), (SlabKind::Roof, 250.0, 6000.0));
    s.execute("slabedit", &json!({"handle": r["handle"], "kind": "floor", "thickness": 180})).unwrap();
    let p = slab(&s, &r["handle"]);
    assert_eq!((p.kind, p.thickness_mm), (SlabKind::Floor, 180.0));
    assert!(s.execute("slabadd", &json!({"points": [[0, 0], [1, 0], [2, 0]]})).is_err(), "a flat outline");
    assert!(s.execute("slabadd", &json!({"points": [[0, 0], [1, 0]]})).is_err());
    assert!(s.execute("slabedit", &json!({"handle": "0"})).is_err());
}

#[test]
fn slabadd_at_the_command_line_takes_the_options_then_the_points_and_remembers_them() {
    let mut s = metric();
    for t in ["slabadd", "k", "foundation", "t", "500", "0,0", "3000,0", "3000,3000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let slabs: Vec<SlabParams> = s.doc().unwrap().model.iter().filter_map(|e| slab_of(e)).collect();
    assert_eq!(slabs.len(), 1);
    assert_eq!((slabs[0].kind, slabs[0].thickness_mm), (SlabKind::Foundation, 500.0));
    for t in ["slabadd", "0,5000", "1000,5000", "1000,6000", ""] {
        s.cmdline(t).unwrap();
    }
    let last = s.doc().unwrap().model.iter().filter_map(|e| slab_of(e)).last().unwrap();
    assert_eq!((last.kind, last.thickness_mm), (SlabKind::Foundation, 500.0));
}
