//! PIPEADD draws pipe runs of a service one segment at a time; PIPEEDIT changes them.

use cadcraft_doc::{Drawing, Handle, PipeLeg, PipeParams, PipeService, pipe_of};
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

fn pipe(s: &Session, h: Handle) -> PipeParams {
    pipe_of(s.doc().unwrap().entity(h).unwrap()).expect("a pipe")
}

#[test]
fn pipeadd_draws_one_pipe_per_segment_with_the_size_of_its_service() {
    let mut s = metric();
    let r = s.execute("pipeadd", &json!({"points": [[0, 0], [3000, 0], [3000, 2000]], "service": "hotWater", "leg": "return"})).unwrap();
    let hs = r["handles"].as_array().unwrap();
    assert_eq!(hs.len(), 2);
    let p = pipe(&s, hd(&hs[0]));
    assert_eq!((p.service, p.leg, p.dn_mm, p.imposed), (PipeService::HotWater, PipeLeg::Return, 40.0, false));
    assert_eq!(p.length, 3000.0);
    // Each service starts at a size of its own; a refrigerant tube is sized by its outer diameter.
    for (service, dn) in [("chilledWater", 50.0), ("changeOver", 50.0), ("condensate", 25.0), ("refrigerantLiquid", 9.52), ("refrigerantGas", 15.88)]
    {
        let r = s.execute("pipeadd", &json!({"points": [[0, 5000], [2000, 5000]], "service": service})).unwrap();
        assert_eq!(pipe(&s, hd(&r["handles"][0])).dn_mm, dn, "{service}");
    }
    // A size typed is kept: sizing leaves it. A condensate drain has no supply and return.
    let r = s.execute("pipeadd", &json!({"points": [[0, 8000], [2000, 8000]], "service": "condensate", "leg": "return", "dn": 32, "insulation": 13, "altitude": 2.4, "load": 18, "source": true})).unwrap();
    let c = pipe(&s, hd(&r["handles"][0]));
    assert_eq!((c.dn_mm, c.imposed, c.insulation_mm, c.level_mm, c.load_kw, c.source), (32.0, true, 13.0, Some(2400.0), Some(18.0), true));
    assert_eq!(c.leg, PipeLeg::Supply, "no return for a drain");
    let layer = s.doc().unwrap().entity(hd(&r["handles"][0])).unwrap().common.layer.clone();
    assert_eq!(layer, cadcraft_doc::PIPE_CONDENSATE_LAYER);
}

#[test]
fn pipeadd_refuses_what_it_cannot_draw() {
    let mut s = metric();
    let n = s.doc().unwrap().model.len();
    for bad in [
        json!({}),
        json!({"points": [[0, 0]]}),
        json!({"points": [[0, 0], [1000, 0]], "service": "steam"}),
        json!({"points": [[0, 0], [1000, 0]], "leg": "up"}),
        json!({"points": [[0, 0], [1000, 0]], "dn": 0}),
        json!({"points": [[0, 0], [1000, 0]], "dn": 9000}),
        json!({"points": [[0, 0], [1000, 0]], "insulation": -5}),
        json!({"points": [[0, 0], [1000, 0]], "load": "x"}),
        json!({"points": [[0, 0], [1000, 0]], "source": 1}),
    ] {
        assert!(s.execute("pipeadd", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.doc().unwrap().model.len(), n, "nothing is drawn");
}

#[test]
fn pipeedit_changes_the_service_the_size_and_what_is_typed_on_the_pipes() {
    let mut s = metric();
    let r = s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": "chilledWater"})).unwrap();
    let h = r["handles"][0].clone();
    // A new service takes its own size when none was typed.
    s.execute("pipeedit", &json!({"handle": h, "service": "hotWater", "leg": "return"})).unwrap();
    let p = pipe(&s, hd(&h));
    assert_eq!((p.service, p.leg, p.dn_mm), (PipeService::HotWater, PipeLeg::Return, 40.0));
    assert_eq!(s.doc().unwrap().entity(hd(&h)).unwrap().common.layer, cadcraft_doc::PIPE_HW_RETURN_LAYER);
    // A size typed is imposed, and a new service keeps it; auto gives it back to sizing.
    s.execute("pipeedit", &json!({"handle": h, "dn": 65})).unwrap();
    s.execute("pipeedit", &json!({"handle": h, "service": "changeOver"})).unwrap();
    assert_eq!((pipe(&s, hd(&h)).dn_mm, pipe(&s, hd(&h)).imposed), (65.0, true));
    s.execute("pipeedit", &json!({"handle": h, "dn": "auto"})).unwrap();
    assert!(!pipe(&s, hd(&h)).imposed);
    // The load, the altitude and the source; null clears.
    s.execute("pipeedit", &json!({"handle": h, "load": 30, "altitude": 2.5, "source": true, "insulation": 20})).unwrap();
    let p = pipe(&s, hd(&h));
    assert_eq!((p.load_kw, p.level_mm, p.source, p.insulation_mm), (Some(30.0), Some(2500.0), true, 20.0));
    s.execute("pipeedit", &json!({"handle": h, "load": null, "altitude": null})).unwrap();
    let p = pipe(&s, hd(&h));
    assert_eq!((p.load_kw, p.level_mm), (None, None));
    // A drain has no return: the leg goes back to the supply.
    s.execute("pipeedit", &json!({"handle": h, "service": "condensate"})).unwrap();
    assert_eq!(pipe(&s, hd(&h)).leg, PipeLeg::Supply);
    // Refused, and nothing changes: a duct is not a pipe, a bad size, no handle.
    let duct = s.execute("ductadd", &json!({"points": [[0, 3000], [2000, 3000]]})).unwrap();
    for bad in [json!({"handle": duct["handles"][0], "dn": 50}), json!({"handle": h, "dn": -3}), json!({"handle": h, "service": "air"}), json!({})] {
        assert!(s.execute("pipeedit", &bad).is_err(), "{bad}");
    }
}

#[test]
fn pipeadd_at_the_command_line_takes_the_options_then_the_points() {
    let mut s = metric();
    for t in ["pipeadd", "se", "condensate", "d", "20", "0,0", "2500,0", "2500,1500", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let pipes: Vec<PipeParams> = s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).collect();
    assert_eq!(pipes.len(), 2);
    assert!(pipes.iter().all(|p| p.service == PipeService::Condensate && p.dn_mm == 20.0 && p.imposed));
    // The next PIPEADD remembers the service and the size.
    for t in ["pipeadd", "0,5000", "1000,5000", ""] {
        s.cmdline(t).unwrap();
    }
    let last = s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).last().unwrap();
    assert_eq!((last.service, last.dn_mm), (PipeService::Condensate, 20.0));
    // Undo takes the last run back.
    for t in ["pipeadd", "0,7000", "1000,7000", "2000,7000", "u", ""] {
        s.cmdline(t).unwrap();
    }
    assert_eq!(s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).count(), 4);
}

#[test]
fn pipesize_sizes_a_network_from_the_loads_at_its_ends_and_refuses_what_is_not_a_limit() {
    let mut s = metric();
    let root = s.execute("pipeadd", &json!({"points": [[0, 0], [5000, 0]], "service": "chilledWater", "source": true})).unwrap();
    s.execute("pipeadd", &json!({"points": [[5000, 0], [5000, 3000]], "service": "chilledWater", "load": 30})).unwrap();
    s.execute("pipeadd", &json!({"points": [[5000, 0], [5000, -3000]], "service": "chilledWater", "load": 70})).unwrap();
    let r = s.execute("pipesize", &json!({})).unwrap();
    let net = &r["networks"][0];
    assert_eq!((net["service"].as_str(), net["leg"].as_str(), net["totalLoad"].as_f64()), (Some("chilledWater"), Some("supply"), Some(100.0)), "{r}");
    assert_eq!(net["runs"].as_array().unwrap().len(), 3);
    let run = net["runs"].as_array().unwrap().iter().find(|x| x["handle"] == root["handles"][0]).unwrap();
    assert_eq!(run["dn"], json!(80.0));
    assert!((run["flowM3h"].as_f64().unwrap() - 17.225).abs() < 0.01);
    assert_eq!(pipe(&s, hd(&root["handles"][0])).dn_mm, 80.0);
    assert!(r["warnings"].as_array().unwrap().is_empty());
    // A larger temperature difference carries the same load in less water: smaller pipes.
    let r2 = s.execute("pipesize", &json!({"deltaT": {"chilledWater": 10}})).unwrap();
    assert!(r2["networks"][0]["runs"][0]["dn"].as_f64().unwrap() < 80.0, "{r2}");
    for bad in [json!({"maxGradient": 0}), json!({"deltaT": {"hotWater": 0}}), json!({"minDn": "x"}), json!({"slope": 0})] {
        assert!(s.execute("pipesize", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_refrigerant_link_is_drawn_as_one_run_with_its_tubes_given_in_inches() {
    let mut s = metric();
    let r = s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": "refrigerantPair", "dn": "3/8", "gas": "5/8"})).unwrap();
    let h = hd(&r["handles"][0]);
    let p = pipe(&s, h);
    assert_eq!((p.service, p.dn_mm, p.gas_dn_mm, p.imposed), (PipeService::RefrigerantPair, 9.52, 15.88, true));
    // By default: 3/8 and 5/8.
    let d = s.execute("pipeadd", &json!({"points": [[0, 1000], [2000, 1000]], "service": "refrigerantPair"})).unwrap();
    let p = pipe(&s, hd(&d["handles"][0]));
    assert_eq!((p.dn_mm, p.gas_dn_mm), (9.52, 15.88));
    // Changed in inches, or in millimetres.
    s.execute("pipeedit", &json!({"handle": h.hex(), "dn": "1/2", "gas": "7/8"})).unwrap();
    let p = pipe(&s, h);
    assert_eq!((p.dn_mm, p.gas_dn_mm), (12.7, 22.2));
    s.execute("pipeedit", &json!({"handle": h.hex(), "gas": 19.05})).unwrap();
    assert_eq!(pipe(&s, h).gas_dn_mm, 19.05);
    // A single liquid line takes inches too; a size that is not one is refused.
    let l = s.execute("pipeadd", &json!({"points": [[0, 2000], [2000, 2000]], "service": "refrigerantLiquid", "dn": "1/4"})).unwrap();
    assert_eq!(pipe(&s, hd(&l["handles"][0])).dn_mm, 6.35);
    assert!(s.execute("pipeadd", &json!({"points": [[0, 3000], [2000, 3000]], "service": "refrigerantPair", "dn": "abc"})).is_err());
    assert!(s.execute("pipeedit", &json!({"handle": h.hex(), "gas": "x/y"})).is_err());
    // A water pipe is in millimetres: an inch fraction is no size there.
    assert!(s.execute("pipeadd", &json!({"points": [[0, 4000], [2000, 4000]], "service": "chilledWater", "dn": "3/8"})).is_err());
    // Sizing leaves refrigerant alone.
    let n = s.execute("pipesize", &json!({})).unwrap();
    assert!(n["networks"].as_array().unwrap().is_empty());
}

#[test]
fn pipeadd_at_the_command_line_takes_the_refrigerant_sizes_in_inches() {
    let mut s = metric();
    for t in ["pipeadd", "se", "refrigerantpair", "d", "1/4", "g", "1/2", "0,0", "3000,0", ""] {
        s.cmdline(t).unwrap();
    }
    let p = s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).next().unwrap();
    assert_eq!((p.service, p.dn_mm, p.gas_dn_mm), (PipeService::RefrigerantPair, 6.35, 12.7));
}
