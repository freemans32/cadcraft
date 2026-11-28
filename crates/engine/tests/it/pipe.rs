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
    for t in ["pipeadd", "se", "refrigerantpair", "liq", "1/4", "g", "1/2", "0,0", "3000,0", ""] {
        s.cmdline(t).unwrap();
    }
    let p = s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).next().unwrap();
    assert_eq!((p.service, p.dn_mm, p.gas_dn_mm), (PipeService::RefrigerantPair, 6.35, 12.7));
}

#[test]
fn a_heat_recovery_link_takes_three_sizes_in_inches() {
    let mut s = metric();
    let r = s
        .execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": "refrigerantTriple", "dn": "3/8", "gas": "7/8", "hpGas": "3/4"}))
        .unwrap();
    let h = hd(&r["handles"][0]);
    let p = pipe(&s, h);
    assert_eq!((p.service, p.dn_mm, p.gas_dn_mm, p.gas2_dn_mm), (PipeService::RefrigerantTriple, 9.52, 22.2, 19.05));
    s.execute("pipeedit", &json!({"handle": h.hex(), "hpGas": "1 1/8"})).unwrap();
    assert_eq!(pipe(&s, h).gas2_dn_mm, 28.58);
    // A pair changed into a heat-recovery link gets a third tube of its own.
    let q = s.execute("pipeadd", &json!({"points": [[0, 1000], [2000, 1000]], "service": "refrigerantPair"})).unwrap();
    s.execute("pipeedit", &json!({"handle": q["handles"][0], "service": "refrigerantTriple"})).unwrap();
    assert_eq!(pipe(&s, hd(&q["handles"][0])).gas2_dn_mm, 19.05);
    for t in ["pipeadd", "se", "refrigeranttriple", "liq", "1/4", "g", "1/2", "hp", "5/8", "0,3000", "3000,3000", ""] {
        s.cmdline(t).unwrap();
    }
    let last = s.doc().unwrap().model.iter().filter_map(|e| pipe_of(e)).last().unwrap();
    assert_eq!((last.dn_mm, last.gas_dn_mm, last.gas2_dn_mm), (6.35, 12.7, 15.88));
}

#[test]
fn hvactakeoff_prices_the_lines_and_adds_the_levels_of_a_building() {
    let mut s = metric();
    s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": "chilledWater", "dn": 25, "insulation": 20})).unwrap();
    let ground = s.execute("hvactakeoff", &json!({"level": "RDC"})).unwrap();
    assert!(ground["totals"]["pipeInsulationM2"].as_f64().unwrap() > 0.0);
    // A second floor of the same pipe: the building has twice the length and each floor's totals.
    let both = s
        .execute(
            "hvactakeoff",
            &json!({"level": "R+1", "levels": [ground], "building": "Total", "prices": {"Pipe|chilledWater|DN25": 12.5, "Pipe insulation": 4.0}}),
        )
        .unwrap();
    assert_eq!((both["level"].as_str(), both["totals"]["pipeLengthM"].as_f64()), (Some("Total"), Some(8.0)));
    assert_eq!(both["levels"].as_array().map(Vec::len), Some(2));
    let c = &both["costing"];
    assert_eq!(c["unpriced"], 0);
    assert!(c["total"].as_f64().unwrap() > 100.0, "{c}");
    // A price that is not a number is refused.
    assert!(s.execute("hvactakeoff", &json!({"prices": {"Pipe|x": "cheap"}})).is_err());
    assert!(s.execute("hvactakeoff", &json!({"prices": {"Pipe|x": -3}})).is_err());
}

#[test]
fn pipefittings_lists_the_elbows_and_tees_of_the_drawing() {
    let mut s = metric();
    s.execute("pipeadd", &json!({"points": [[0, 0], [3000, 0], [3000, 3000]], "service": "chilledWater", "dn": 50})).unwrap();
    s.execute("pipeadd", &json!({"points": [[1500, 0], [1500, -2000]], "service": "chilledWater", "dn": 25})).unwrap();
    let r = s.execute("pipefittings", &json!({})).unwrap();
    assert_eq!(r["counts"]["Pipe elbow 90°"], 1, "{r}");
    assert_eq!(r["counts"]["Pipe tee"], 1, "{r}");
    let tee = r["fittings"].as_array().unwrap().iter().find(|f| f["kind"] == "Tee").unwrap();
    assert_eq!(tee["size"], "DN50", "a tee has the size of the larger pipe");
    assert_eq!(s.execute("pipefittings", &json!({})).unwrap()["fittings"].as_array().unwrap().len(), 2);
}

/// The two ends of a pipe or duct run, in drawing units.
fn run_ends(s: &Session, h: Handle) -> ((f64, f64), (f64, f64)) {
    let d = s.doc().unwrap();
    let (a, b) = d.pipe_ends(h).map(|(_, a, b)| (a, b)).or_else(|| d.duct_ends(h).map(|(_, a, b)| (a, b))).unwrap();
    ((a.x.round(), a.y.round()), (b.x.round(), b.y.round()))
}

#[test]
fn the_three_grips_of_a_pipe_and_a_duct_work_one_by_one_and_hot() {
    let mut s = metric();
    let pipe = hd(&s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": "chilledWater", "dn": 25})).unwrap()["handles"][0]);
    let next = hd(&s.execute("pipeadd", &json!({"points": [[4000, 0], [4000, 3000]], "service": "chilledWater", "dn": 25})).unwrap()["handles"][0]);
    let duct = hd(&s.execute("ductadd", &json!({"points": [[0, 2000], [3000, 2000]], "diameter": 200})).unwrap()["handles"][0]);
    for h in [pipe, duct] {
        assert_eq!(s.grips_of(h).unwrap().len(), 3, "the ends and the middle");
    }
    // Hot grip on the end of the first pipe, the second pipe selected with it: it follows its end.
    s.set_selection(vec![pipe, next]);
    s.start_grip_edit(&[(pipe, 2)]).unwrap();
    s.cmdline("4000,1000").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(run_ends(&s, pipe), ((0.0, 0.0), (4000.0, 1000.0)));
    assert_eq!(run_ends(&s, next).0, (4000.0, 1000.0), "the pipe that met it follows");
    // The middle grip moves the run, the other end grip of a duct stretches it.
    s.set_selection(vec![pipe]);
    s.start_grip_edit(&[(pipe, 1)]).unwrap();
    s.cmdline("2000,-500").unwrap();
    assert_eq!(run_ends(&s, pipe), ((0.0, -1000.0), (4000.0, 0.0)));
    s.set_selection(vec![duct]);
    s.start_grip_edit(&[(duct, 0)]).unwrap();
    s.cmdline("1000,2000").unwrap();
    assert_eq!(run_ends(&s, duct), ((1000.0, 2000.0), (3000.0, 2000.0)));
    // The single-grip command takes the same indices, and refuses one that does not exist.
    s.execute("grip.stretch", &json!({"handle": duct.hex(), "index": 2, "to": [3500, 2000]})).unwrap();
    assert_eq!(run_ends(&s, duct).1, (3500.0, 2000.0));
    assert!(s.execute("grip.stretch", &json!({"handle": duct.hex(), "index": 3, "to": [0, 0]})).is_err());
}

#[test]
fn the_links_of_one_refrigerant_system_are_one_network_through_the_branch_parts() {
    let mut s = metric();
    s.execute("pipepartadd", &json!({"at": [5000, 0], "model": "Refrigerant header, index below 290", "rotation": 0})).unwrap();
    // Three links ending in the header from three sides, and a fourth that is nowhere near.
    for (a, b) in [([0, 0], [5000, 0]), ([5000, 4000], [5000, 0]), ([10000, 0], [5000, 0]), ([0, 20000], [3000, 20000])] {
        s.execute("pipeadd", &json!({"points": [a, b], "service": "refrigerantPair", "dn": "1/4", "gas": "1/2"})).unwrap();
    }
    let r = s.execute("pipenetwork", &json!({})).unwrap();
    let nets = r["networks"].as_array().unwrap();
    let mut pipes: Vec<u64> = nets.iter().map(|n| n["pipes"].as_u64().unwrap()).collect();
    pipes.sort_unstable();
    assert_eq!(pipes, vec![1, 3], "the three links on the header are one network, the lone one another: {nets:?}");
}

#[test]
fn links_named_one_by_one_become_one_network_when_a_branch_joins_them() {
    let mut s = metric();
    // Their ends are a little apart, inside the outline of the header that is drawn later.
    for (a, b) in [([0, 0], [4880, 0]), ([5000, 4000], [5000, 120]), ([10000, 0], [5120, 0])] {
        s.execute("pipeadd", &json!({"points": [a, b], "service": "refrigerantPair", "dn": "1/4", "gas": "1/2"})).unwrap();
    }
    // Named before the header is drawn: three networks, REF1 to REF3, each with its colour.
    let first = s.execute("pipenetwork", &json!({})).unwrap();
    assert_eq!(first["networks"].as_array().unwrap().len(), 3);
    s.execute("pipepartadd", &json!({"at": [5000, 0], "model": "Refrigerant header, index below 290", "rotation": 0})).unwrap();
    // Naming again joins them under the lowest of the names, and the others lose their layers' use.
    let again = s.execute("pipenetwork", &json!({})).unwrap();
    let nets = again["networks"].as_array().unwrap();
    assert_eq!(nets.len(), 1, "{nets:?}");
    assert_eq!(nets[0]["name"], "REF1");
    assert_eq!(nets[0]["pipes"], 3);
}

#[test]
fn pipes_laid_by_the_command_get_their_elbow_and_follow_a_change_of_diameter_without_any_sizing() {
    let mut s = metric();
    let r = s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0], [4000, 3000]], "service": "chilledWater", "dn": 50})).unwrap();
    let hs: Vec<Handle> = r["handles"].as_array().unwrap().iter().map(hd).collect();
    let p = |s: &Session, h: Handle| pipe(s, h);
    let elbows = |s: &Session| hs.iter().filter(|h| p(s, **h).elbow.is_some()).count();
    assert_eq!(elbows(&s), 1, "one elbow where the two pipes turn");
    // A new diameter on the later one: the elbow follows its width, and the other pipe gets the cone from it.
    s.execute("pipeedit", &json!({"handle": hs[1].hex(), "dn": 100})).unwrap();
    let (a, b) = (p(&s, hs[0]), p(&s, hs[1]));
    assert!((b.elbow.unwrap().radius_mm - 1.5 * b.plan_width_mm()).abs() < 1e-6);
    assert!(a.reducer.is_some_and(|r| r.at_end && (r.from_mm - b.plan_width_mm()).abs() < 1e-6), "{:?}", a.reducer);
    // Equal again: the cone goes, the elbow stays.
    s.execute("pipeedit", &json!({"handle": hs[1].hex(), "dn": 50})).unwrap();
    assert!(p(&s, hs[0]).reducer.is_none() && elbows(&s) == 1);
}

#[test]
fn a_pipe_or_a_refrigerant_link_counts_the_extra_length_it_is_given() {
    let mut s = metric();
    for (service, dn) in [("chilledWater", json!(50)), ("refrigerantPair", json!("3/8"))] {
        let r = s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0]], "service": service, "dn": dn})).unwrap();
        let hp = hd(&r["handles"][0]);
        let drawn = pipe(&s, hp).length_m();
        s.execute("pipeedit", &json!({"handles": [hp.hex()], "extra": 2.5})).unwrap();
        assert!((pipe(&s, hp).length_m() - drawn - 2.5).abs() < 1e-9, "{service}: the rise is counted");
        assert!(s.execute("pipeedit", &json!({"handles": [hp.hex()], "extra": -1})).is_err(), "{service}");
        assert!(s.execute("pipeedit", &json!({"handles": [hp.hex()], "extra": 5000})).is_err(), "{service}");
        assert!((pipe(&s, hp).length_m() - drawn - 2.5).abs() < 1e-9, "{service}: a refused value leaves it");
    }
}

#[test]
fn a_named_sub_network_is_listed_with_its_length_and_can_take_a_colour_of_its_own() {
    let mut s = metric();
    let r =
        s.execute("pipeadd", &json!({"points": [[0, 0], [4000, 0], [4000, 3000]], "service": "refrigerantPair", "dn": "1/4", "gas": "3/8"})).unwrap();
    let runs: Vec<Handle> = r["handles"].as_array().unwrap().iter().map(hd).collect();
    let other = hd(&s
        .execute("pipeadd", &json!({"points": [[0, 5000], [2000, 5000]], "service": "refrigerantPair", "dn": "1/4", "gas": "3/8"}))
        .unwrap()["handles"][0]);
    s.execute("pipeedit", &json!({"handles": [runs[0].hex(), other.hex()], "network": "VRV"})).unwrap();
    // The first run named, then given a colour; the second named after takes the colour; the third is not part of it.
    s.execute("pipeedit", &json!({"handles": [runs[0].hex()], "subnetwork": "UI salon", "extra": 1.5})).unwrap();
    s.execute("pipeedit", &json!({"handles": [runs[0].hex()], "subnetworkColour": "#FF8000"})).unwrap();
    s.execute("pipeedit", &json!({"handles": [runs[1].hex()], "subnetwork": "UI salon"})).unwrap();
    assert_eq!(pipe(&s, runs[1]).subnetwork_colour.as_deref(), Some("#ff8000"), "a run named into it takes its colour");
    assert_eq!(pipe(&s, other).subnetwork_colour, None);
    // Drawn in it: its pieces on their layers are of that colour.
    let d = s.doc().unwrap();
    let cadcraft_doc::EntityKind::Insert(i) = &d.entity(runs[1]).unwrap().kind else { panic!() };
    let orange = cadcraft_color::Color::True(cadcraft_color::Rgb(0xff, 0x80, 0x00));
    assert!(d.block(&i.block).unwrap().entities.iter().any(|e| e.common.color == orange), "drawn in the sub-network's colour");
    // The take-off lists it with its two runs, its length (the extra counted) and its size.
    let t = s.execute("hvactakeoff", &json!({})).unwrap();
    let subs = t["subnetworks"].as_array().unwrap();
    assert_eq!(subs.len(), 1, "{subs:?}");
    assert_eq!((subs[0]["name"].as_str(), subs[0]["count"].as_u64()), (Some("UI salon"), Some(2)));
    assert!((subs[0]["lengthM"].as_f64().unwrap() - 8.5).abs() < 0.011, "{subs:?}");
    // Back to the network's colour, and a run taken out of it.
    s.execute("pipeedit", &json!({"handles": [runs[0].hex()], "subnetworkColour": null})).unwrap();
    assert_eq!(pipe(&s, runs[1]).subnetwork_colour, None);
    s.execute("pipeedit", &json!({"handles": [runs[1].hex()], "subnetwork": ""})).unwrap();
    assert!(pipe(&s, runs[1]).subnetwork.is_empty());
    assert!(s.execute("pipeedit", &json!({"handles": [other.hex()], "subnetworkColour": "#123456"})).is_err(), "no sub-network to colour");
    assert!(s.execute("pipeedit", &json!({"handles": [other.hex()], "subnetworkColour": "orange"})).is_err());
}
