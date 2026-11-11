//! SPACEADD draws spaces with the fresh air they need; AIRTERMINALADD places diffusers and
//! grilles, one by one or spread over a space; AIRTERMINALMODELEDIT manages the models.

use cadcraft_doc::{
    AIR_TERMINAL_LAYER, AirDirection, Drawing, EntityKind, Handle, SPACE_LAYER, TerminalKind, TerminalParams, air_terminal_of, space_of,
};
use cadcraft_engine::{Input, Session};
use cadcraft_hvac::RoomUse;
use serde_json::{Value, json};

fn metric() -> Session {
    let mut s = Session::new();
    *s.doc_mut().unwrap() = Drawing::new_metric();
    s
}

fn h(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}

fn terminals(s: &Session) -> Vec<(Handle, TerminalParams)> {
    s.doc().unwrap().model.iter().filter_map(|e| air_terminal_of(e).map(|t| (e.handle, t))).collect()
}

fn insert_at(s: &Session, hd: Handle) -> (f64, f64, f64) {
    match &s.doc().unwrap().entity(hd).unwrap().kind {
        EntityKind::Insert(i) => (i.insert.x, i.insert.y, i.rotation),
        _ => panic!("not a block reference"),
    }
}

const ROOM: [[f64; 2]; 4] = [[0.0, 0.0], [6000.0, 0.0], [6000.0, 5000.0], [0.0, 5000.0]];

#[test]
fn spaceadd_draws_an_office_with_the_labour_code_flow() {
    let mut s = metric();
    let r = s.execute("spaceadd", &json!({"points": ROOM, "name": "Bureau", "occupants": 6})).unwrap();
    assert_eq!(r["flow"], 150.0);
    assert!((r["area"].as_f64().unwrap() - 30.0).abs() < 1e-9);
    let d = s.doc().unwrap();
    let e = d.entity(h(&r)).unwrap();
    assert_eq!(e.common.layer, SPACE_LAYER);
    let sp = space_of(e).unwrap();
    assert_eq!((sp.name.as_str(), sp.room, sp.occupants), ("Bureau", RoomUse::Office, 6));
}

#[test]
fn spaceadd_takes_sanitary_rooms_a_set_flow_and_a_closed_polyline() {
    let mut s = metric();
    let r = s.execute("spaceadd", &json!({"points": ROOM, "use": "sanitary", "fixtures": 4})).unwrap();
    assert_eq!(r["flow"], 90.0);
    let r = s.execute("spaceadd", &json!({"points": ROOM, "use": "toilet", "collective": false})).unwrap();
    assert_eq!(r["flow"], 15.0);
    let r = s.execute("spaceadd", &json!({"points": ROOM, "use": "meeting", "occupants": 10, "flow": 400})).unwrap();
    assert_eq!(r["flow"], 400.0);
    let pl = s.execute("pline", &json!({"points": ROOM, "closed": true})).unwrap();
    let r = s.execute("spaceadd", &json!({"handle": pl["handle"]})).unwrap();
    assert!((r["area"].as_f64().unwrap() - 30.0).abs() < 1e-9);
}

#[test]
fn spaceadd_refuses_what_it_cannot_draw() {
    let mut s = metric();
    let bad = |s: &mut Session, p: Value| s.execute("spaceadd", &p).is_err();
    assert!(bad(&mut s, json!({})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1, 0]]})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1, 0], [2, 0]]})), "no area");
    assert!(bad(&mut s, json!({"points": ROOM, "use": "kitchen"})));
    assert!(bad(&mut s, json!({"points": ROOM, "occupants": -1})));
    assert!(bad(&mut s, json!({"points": ROOM, "flow": 0})));
    let open = s.execute("pline", &json!({"points": ROOM})).unwrap();
    assert!(bad(&mut s, json!({"handle": open["handle"]})), "an open polyline");
    assert!(s.doc().unwrap().model.iter().all(|e| space_of(e).is_none()));
}

#[test]
fn spaceadd_at_the_command_line() {
    let mut s = metric();
    for t in ["spaceadd", "n", "Réunion", "u", "m", "oc", "8", "0,0", "4000,0", "4000,3000", "0,3000", "c"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let sp: Vec<_> = s.doc().unwrap().model.iter().filter_map(|e| space_of(e)).collect();
    assert_eq!(sp.len(), 1);
    assert_eq!((sp[0].name.as_str(), sp[0].room, sp[0].occupants), ("Réunion", RoomUse::DiningSalesMeeting, 8));
    assert_eq!(sp[0].flow().ok(), Some(240.0));
}

#[test]
fn airterminaladd_picks_the_smallest_model_for_the_flow() {
    let mut s = metric();
    let r = s.execute("airterminaladd", &json!({"at": [1000, 1000], "flow": 300})).unwrap();
    assert_eq!(r["model"], "SQ600-200");
    let r = s.execute("airterminaladd", &json!({"at": [2000, 1000], "flow": 80, "air": "extract"})).unwrap();
    assert_eq!(r["model"], "EX125");
    let r = s.execute("airterminaladd", &json!({"at": [3000, 1000], "flow": 200, "type": "rectangular", "rotation": 90})).unwrap();
    assert_eq!(r["model"], "RG300x150");
    let hd = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    let (x, y, rot) = insert_at(&s, hd);
    assert_eq!((x, y), (3000.0, 1000.0));
    assert!((rot - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    let all = terminals(&s);
    assert_eq!(all.len(), 3);
    assert_eq!(all[1].1.air, AirDirection::Extract);
    let d = s.doc().unwrap();
    assert_eq!(d.entity(hd).unwrap().common.layer, AIR_TERMINAL_LAYER);
    assert!(d.air_terminal_model("SQ600-200").is_some(), "a standard model used is copied into the drawing");
    // Too much for any square diffuser, or a model that does not exist.
    assert!(s.execute("airterminaladd", &json!({"at": [0, 0], "flow": 5000})).is_err());
    assert!(s.execute("airterminaladd", &json!({"at": [0, 0], "model": "nope"})).is_err());
    assert!(s.execute("airterminaladd", &json!({})).is_err());
}

#[test]
fn airterminaladd_spreads_a_space_flow_over_its_terminals() {
    let mut s = metric();
    let sp = s.execute("spaceadd", &json!({"points": ROOM, "occupants": 24})).unwrap();
    assert_eq!(sp["flow"], 600.0);
    let r = s.execute("airterminaladd", &json!({"space": sp["handle"], "count": 4})).unwrap();
    assert_eq!(r["flow"], 150.0);
    assert_eq!(r["model"], "SQ600-160");
    assert_eq!(r["handles"].as_array().unwrap().len(), 4);
    let r = s.execute("airterminaladd", &json!({"space": sp["handle"], "rows": 1, "cols": 3, "type": "linear"})).unwrap();
    assert_eq!(r["flow"], 200.0);
    let hd = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    assert_eq!(insert_at(&s, hd), (1000.0, 2500.0, 0.0));
    assert_eq!(terminals(&s).len(), 7);
    assert!(s.execute("airterminaladd", &json!({"space": sp["handle"], "count": 100000})).is_err());
}

#[test]
fn airterminaladd_at_the_command_line_places_and_distributes() {
    let mut s = metric();
    for t in ["airterminaladd", "t", "r", "f", "120", "500,500", "45", "1500,500", "", ""] {
        s.cmdline(t).unwrap();
    }
    let all = terminals(&s);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|(_, t)| t.model.kind == TerminalKind::Round && t.flow_m3h == 120.0));
    assert!((insert_at(&s, all[0].0).2 - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    // The settings are remembered, and DISTRIBUTE shares a space's flow.
    let sp = s.execute("spaceadd", &json!({"points": ROOM, "occupants": 12})).unwrap();
    s.cmdline("airterminaladd").unwrap();
    s.cmdline("d").unwrap();
    s.input(Input::Pick(vec![h(&sp)])).unwrap();
    s.cmdline("2x2").unwrap();
    assert!(s.running.is_none());
    let all = terminals(&s);
    assert_eq!(all.len(), 6);
    assert!(all[2..].iter().all(|(_, t)| t.model.kind == TerminalKind::Round && t.flow_m3h == 75.0));
}

#[test]
fn airterminalmodeledit_lists_creates_changes_and_deletes() {
    let mut s = metric();
    let all = s.execute("airterminalmodeledit", &json!({})).unwrap();
    assert!(all["models"].as_array().unwrap().iter().all(|m| m["standard"] == true));
    let r =
        s.execute("airterminalmodeledit", &json!({"name": "DF 4V 625", "type": "squareCeiling", "width": 625, "neck": 250, "maxFlow": 550})).unwrap();
    assert_eq!(r["model"]["depthMm"], 625.0);
    let r = s.execute("airterminaladd", &json!({"at": [0, 0], "flow": 500})).unwrap();
    assert_eq!(r["model"], "DF 4V 625");
    // A standard model changed in the drawing replaces the standard one.
    s.execute("airterminalmodeledit", &json!({"name": "SQ600-160", "maxFlow": 280})).unwrap();
    assert_eq!(s.execute("airterminaladd", &json!({"at": [0, 0], "flow": 270})).unwrap()["model"], "SQ600-160");
    assert!(s.execute("airterminalmodeledit", &json!({"name": "x", "neck": -5})).is_err());
    assert!(s.execute("airterminalmodeledit", &json!({"name": "x", "type": "linear", "slots": 0})).is_err());
    s.execute("airterminalmodeledit", &json!({"delete": "DF 4V 625"})).unwrap();
    assert!(s.execute("airterminalmodeledit", &json!({"delete": "DF 4V 625"})).is_err());
}

#[test]
fn spaceedit_turns_an_office_into_a_1200_m3h_gym() {
    // Guillaume's first test: a gym drawn as the default office, corrected afterwards.
    let mut s = metric();
    let sp = s.execute("spaceadd", &json!({"points": ROOM})).unwrap();
    assert_eq!(sp["flow"], 25.0);
    let r = s.execute("spaceedit", &json!({"handle": sp["handle"], "name": "Salle de sport", "flow": 1200})).unwrap();
    assert_eq!(r["flow"], 1200.0);
    let e = space_of(s.doc().unwrap().entity(h(&sp)).unwrap()).unwrap();
    assert_eq!((e.name.as_str(), e.flow_m3h), ("Salle de sport", Some(1200.0)));
    // By use instead: other rooms, 20 people; null goes back to the labour code's flow.
    let r = s.execute("spaceedit", &json!({"handle": sp["handle"], "use": "workshop", "occupants": 20, "flow": null})).unwrap();
    assert_eq!(r["flow"], 1200.0);
    assert_eq!(space_of(s.doc().unwrap().entity(h(&sp)).unwrap()).unwrap().flow_m3h, None);
    // Fixtures alone change a grouped sanitary room's count.
    let wc = s.execute("spaceadd", &json!({"points": ROOM, "use": "sanitary", "fixtures": 2})).unwrap();
    assert_eq!(s.execute("spaceedit", &json!({"handle": wc["handle"], "fixtures": 5})).unwrap()["flow"], 105.0);
    assert!(s.execute("spaceedit", &json!({"handle": sp["handle"], "use": "kitchen"})).is_err());
    assert!(s.execute("spaceedit", &json!({"handle": sp["handle"], "flow": -3})).is_err());
    s.execute("undo", &json!({})).unwrap();
}

#[test]
fn spaceedit_at_the_command_line() {
    let mut s = metric();
    let sp = s.execute("spaceadd", &json!({"points": ROOM})).unwrap();
    s.set_selection(vec![h(&sp)]);
    for t in ["spaceedit", "f", "1200", "n", "Gymnase", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let e = space_of(s.doc().unwrap().entity(h(&sp)).unwrap()).unwrap();
    assert_eq!((e.name.as_str(), e.flow().ok()), ("Gymnase", Some(1200.0)));
}

#[test]
fn airterminaledit_changes_several_terminals_at_once() {
    let mut s = metric();
    let sp = s.execute("spaceadd", &json!({"points": ROOM, "flow": 1200})).unwrap();
    let r = s.execute("airterminaladd", &json!({"space": sp["handle"], "count": 4, "flow": 250})).unwrap();
    assert_eq!(r["model"], "SQ600-160");
    let hs = r["handles"].clone();
    s.execute("airterminaledit", &json!({"handles": hs, "flow": 300, "model": "auto"})).unwrap();
    let all = terminals(&s);
    assert!(all.iter().all(|(_, t)| t.model.name == "SQ600-200" && t.flow_m3h == 300.0), "{all:?}");
    s.execute("airterminaledit", &json!({"handles": hs, "type": "round", "rotation": 45})).unwrap();
    let all = terminals(&s);
    assert!(all.iter().all(|(_, t)| t.model.name == "RD200"));
    assert!((insert_at(&s, all[0].0).2 - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    s.execute("airterminaledit", &json!({"handles": hs, "model": "RD250", "air": "extract"})).unwrap();
    assert!(terminals(&s).iter().all(|(_, t)| t.model.name == "RD250" && t.air == AirDirection::Extract));
    // A model too small for the flow is refused for "auto"; a missing model or a space is refused.
    assert!(s.execute("airterminaledit", &json!({"handles": hs, "flow": 5000, "model": "auto"})).is_err());
    assert!(s.execute("airterminaledit", &json!({"handles": hs, "model": "nope"})).is_err());
    assert!(s.execute("airterminaledit", &json!({"handle": sp["handle"], "flow": 100})).is_err());
    assert!(terminals(&s).iter().all(|(_, t)| t.flow_m3h == 300.0), "nothing changed by a refused edit");
}

#[test]
fn airterminaledit_at_the_command_line() {
    let mut s = metric();
    let r = s.execute("airterminaladd", &json!({"at": [0, 0], "flow": 100})).unwrap();
    let hd = Handle::parse_hex(r["handles"][0].as_str().unwrap()).unwrap();
    s.set_selection(vec![hd]);
    for t in ["airterminaledit", "f", "350", "s", "auto", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let t = &terminals(&s)[0].1;
    assert_eq!((t.model.name.as_str(), t.flow_m3h), ("SQ600-200", 350.0));
}

#[test]
fn a_count_written_with_a_decimal_point_is_a_whole_number() {
    let mut s = metric();
    let r = s.execute("spaceadd", &json!({"points": ROOM, "occupants": 6})).unwrap();
    // A palette field sends 20.0 for the 20 typed in it.
    s.execute("spaceedit", &json!({"handle": r["handle"], "occupants": 20.0})).unwrap();
    assert_eq!(space_of(s.doc().unwrap().entity(h(&r)).unwrap()).unwrap().occupants, 20);
    for bad in [json!(20.5), json!(-1.0), json!(1e12), json!("20")] {
        assert!(s.execute("spaceedit", &json!({"handle": r["handle"], "occupants": bad})).is_err(), "{bad}");
    }
}
