use cadcraft_color::{Color, Rgb};

use super::*;
use crate::{Common, Drawing, EntityKind, Space};

fn pipe(service: PipeService, leg: PipeLeg, dn: f64) -> PipeParams {
    PipeParams { leg, length: 3000.0, ..PipeParams::new(service, dn, HvacUnits { mm_per_unit: 1.0 }) }
}

#[test]
fn a_service_is_found_by_its_name_in_english_or_french() {
    for s in PipeService::ALL {
        assert_eq!(PipeService::parse(s.name()), Some(s));
    }
    assert_eq!(PipeService::parse("Eau glacée"), Some(PipeService::ChilledWater));
    assert_eq!(PipeService::parse("eau chaude"), Some(PipeService::HotWater));
    assert_eq!(PipeService::parse("Change-over"), Some(PipeService::ChangeOver));
    assert_eq!(PipeService::parse("condensats"), Some(PipeService::Condensate));
    assert_eq!(PipeService::parse("gaz"), Some(PipeService::RefrigerantGas));
    assert_eq!(PipeService::parse("steam"), None);
    assert_eq!((PipeLeg::parse("retour"), PipeLeg::parse("départ"), PipeLeg::parse("x")), (Some(PipeLeg::Return), Some(PipeLeg::Supply), None));
    assert!(PipeService::ChangeOver.has_legs() && !PipeService::Condensate.has_legs() && PipeService::RefrigerantGas.is_refrigerant());
}

#[test]
fn a_pipe_is_drawn_to_its_outer_diameter_and_its_insulation_with_the_service_written_on_it() {
    let mut p = pipe(PipeService::HotWater, PipeLeg::Supply, 40.0);
    assert_eq!(p.outer_diameter_mm(), 48.3, "DN40 steel");
    p.insulation_mm = 20.0;
    assert!((p.plan_width_mm() - 88.3).abs() < 1e-9);
    let pieces = p.generate(false).unwrap();
    let sides: Vec<f64> = pieces
        .iter()
        .filter_map(|x| match &x.kind {
            EntityKind::Line(l) if x.layer == PIPE_HW_SUPPLY_LAYER => Some(l.a.y),
            _ => None,
        })
        .collect();
    assert_eq!(sides, [-44.15, 44.15], "two sides, half the width either side");
    let text = pieces.iter().find_map(|x| if let EntityKind::Text(t) = &x.kind { Some(t.value.clone()) } else { None }).unwrap();
    assert_eq!(text, "DN40 HW-S");
    // A refrigerant line is the copper tube's outer diameter.
    let r = pipe(PipeService::RefrigerantLiquid, PipeLeg::Supply, 9.52);
    assert_eq!(r.outer_diameter_mm(), 9.52);
    assert!(r.label_text().starts_with("Ø9.5 REF-L"));
    // Sized: the flow and the altitude are written, and an arrow shows the way.
    let mut s = pipe(PipeService::ChilledWater, PipeLeg::Return, 50.0);
    (s.flow_m3h, s.level_mm, s.flow_forward) = (Some(8.6), Some(2800.0), Some(true));
    assert_eq!(s.label_text(), "DN50 CHW-R 8.60 m³/h Ai:2.80");
    assert!(s.generate(false).unwrap().len() > pipe(PipeService::ChilledWater, PipeLeg::Return, 50.0).generate(false).unwrap().len());
    // A condensate flow is in litres per hour.
    let mut c = pipe(PipeService::Condensate, PipeLeg::Supply, 25.0);
    c.flow_m3h = Some(0.012);
    assert_eq!(c.label_text(), "DN25 COND 12.0 l/h");
}

#[test]
fn the_condensate_and_the_chilled_water_are_blue_with_a_yellow_and_a_white_band_as_iso_14726_says() {
    let mut d = Drawing::new_metric();
    let add = |d: &mut Drawing, p: PipeParams| {
        d.add_pipe(&Space::Model, Common::default(), p, cadcraft_geom::Vec2::ZERO, cadcraft_geom::Vec2::new(3000.0, 0.0)).unwrap()
    };
    let cond = add(&mut d, pipe(PipeService::Condensate, PipeLeg::Supply, 25.0));
    let chw = add(&mut d, pipe(PipeService::ChilledWater, PipeLeg::Supply, 50.0));
    let hw = add(&mut d, pipe(PipeService::HotWater, PipeLeg::Return, 32.0));
    let blue = Color::True(Rgb(0x1e, 0x6f, 0xd9));
    let layer_color = |d: &Drawing, name: &str| d.layer(name).map(|l| l.color);
    assert_eq!(layer_color(&d, PIPE_CONDENSATE_LAYER), Some(blue));
    assert_eq!(layer_color(&d, PIPE_CHW_SUPPLY_LAYER), Some(blue));
    assert_eq!(layer_color(&d, PIPE_HW_RETURN_LAYER), Some(Color::True(Rgb(0xff, 0x8a, 0x80))));
    // The line along the axis: yellow for the condensate, white (index 7) for the chilled water, none for hot water.
    let axis = |d: &Drawing, h| {
        let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { panic!("a block reference") };
        d.block(&i.block)
            .unwrap()
            .entities
            .iter()
            .filter(|e| matches!(&e.kind, EntityKind::Line(l) if l.a.y == 0.0 && l.b.y == 0.0 && l.b.x > 0.0 && e.common.color != Color::ByLayer))
            .map(|e| e.common.color)
            .collect::<Vec<_>>()
    };
    assert_eq!(axis(&d, cond), [Color::True(Rgb(0xf5, 0xc4, 0x00))]);
    assert_eq!(axis(&d, chw), [Color::Index(7)]);
    assert!(axis(&d, hw).is_empty());
}

#[test]
fn a_pipe_keeps_its_parameters_through_its_extended_data_and_refuses_what_cannot_be_drawn() {
    let mut p = pipe(PipeService::ChangeOver, PipeLeg::Return, 65.0);
    (p.load_kw, p.source, p.level_mm, p.insulation_mm) = (Some(42.5), true, Some(2600.0), 25.0);
    assert_eq!(PipeParams::from_xdata(&p.to_xdata().unwrap()), Some(p.clone()));
    for bad in [
        PipeParams { dn_mm: 0.0, ..p.clone() },
        PipeParams { dn_mm: f64::NAN, ..p.clone() },
        PipeParams { length: 0.0, ..p.clone() },
        PipeParams { insulation_mm: 500.0, ..p.clone() },
        PipeParams { load_kw: Some(-1.0), ..p.clone() },
        PipeParams { level_mm: Some(f64::INFINITY), ..p.clone() },
        PipeParams { label_height_mm: 0.0, ..p.clone() },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    // Added to a drawing and regenerated: the layer follows the service and the leg.
    let mut d = Drawing::new_metric();
    let h = d.add_pipe(&Space::Model, Common::default(), p.clone(), cadcraft_geom::Vec2::ZERO, cadcraft_geom::Vec2::new(0.0, 4000.0)).unwrap();
    assert_eq!(d.entity(h).unwrap().common.layer, PIPE_CO_RETURN_LAYER);
    let (back, a, b) = d.pipe_ends(h).unwrap();
    assert_eq!((back.length, a.y, b.y), (4000.0, 0.0, 4000.0));
    d.regenerate_pipe(h, &PipeParams { service: PipeService::HotWater, leg: PipeLeg::Supply, ..back }).unwrap();
    assert_eq!(d.entity(h).unwrap().common.layer, PIPE_HW_SUPPLY_LAYER);
    assert!(d.add_pipe(&Space::Model, Common::default(), p, cadcraft_geom::Vec2::ZERO, cadcraft_geom::Vec2::new(f64::NAN, 0.0)).is_err());
}

fn add(d: &mut Drawing, service: PipeService, leg: PipeLeg, a: (f64, f64), b: (f64, f64), load: Option<f64>, source: bool) -> crate::Handle {
    let p = PipeParams { leg, load_kw: load, source, ..PipeParams::new(service, 25.0, HvacUnits { mm_per_unit: 1.0 }) };
    d.add_pipe(&Space::Model, Common::default(), p, cadcraft_geom::Vec2::new(a.0, a.1), cadcraft_geom::Vec2::new(b.0, b.1)).unwrap()
}

#[test]
fn a_water_network_is_sized_from_the_loads_at_its_ends_back_to_its_source() {
    let mut d = Drawing::new_metric();
    let (s, c) = (PipeLeg::Supply, PipeService::ChilledWater);
    let root = add(&mut d, c, s, (0.0, 0.0), (5000.0, 0.0), None, true);
    let a = add(&mut d, c, s, (5000.0, 0.0), (5000.0, 3000.0), Some(30.0), false);
    let b = add(&mut d, c, s, (5000.0, 0.0), (5000.0, -3000.0), Some(70.0), false);
    let (reports, warnings) = d.size_pipes(&PipeSizingParams::default()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(reports.len(), 1);
    let r = &reports[0];
    assert_eq!((r.service, r.leg, r.source, r.total_load), (c, s, root, 100.0));
    let run = |h| r.runs.iter().find(|x| x.handle == h).unwrap().clone();
    // 100 kW at 5 K is 17.2 m³/h in DN80; the branches carry their own loads.
    assert_eq!((run(root).dn_mm, run(root).load), (80.0, 100.0));
    assert!((run(root).flow_m3h - 17.225).abs() < 0.01);
    assert_eq!((run(a).dn_mm, run(b).dn_mm), (50.0, 65.0));
    // What was found is written on the runs: the size, the flow, the velocity and the way the water runs.
    let pa = pipe_of(d.entity(a).unwrap()).unwrap();
    assert_eq!(pa.dn_mm, 50.0);
    assert!(pa.flow_m3h.is_some_and(|f| (f - 5.17).abs() < 0.01) && pa.velocity.is_some() && pa.loss_pa_m.is_some());
    assert_eq!(pa.flow_forward, Some(true), "away from the source along the run");
    assert!(d.entity(a).unwrap().common.layer == PIPE_CHW_SUPPLY_LAYER);
    // The return of the same network runs the other way: towards the source.
    let rroot = add(&mut d, c, PipeLeg::Return, (0.0, 200.0), (5000.0, 200.0), None, true);
    let rleaf = add(&mut d, c, PipeLeg::Return, (5000.0, 200.0), (5000.0, 3200.0), Some(30.0), false);
    let (reports, _) = d.size_pipes(&PipeSizingParams::default()).unwrap();
    assert_eq!(reports.len(), 2, "one network per leg");
    assert_eq!(pipe_of(d.entity(rleaf).unwrap()).unwrap().flow_forward, Some(false));
    assert_eq!(pipe_of(d.entity(rroot).unwrap()).unwrap().flow_forward, Some(false));
}

#[test]
fn a_run_whose_size_was_given_is_kept_and_checked_and_runs_without_a_source_or_a_load_are_reported() {
    let mut d = Drawing::new_metric();
    let (s, h) = (PipeLeg::Supply, PipeService::HotWater);
    let root = add(&mut d, h, s, (0.0, 0.0), (3000.0, 0.0), None, true);
    add(&mut d, h, s, (3000.0, 0.0), (3000.0, 2000.0), Some(200.0), false);
    add(&mut d, h, s, (3000.0, 0.0), (3000.0, -2000.0), None, false);
    // Far from the network, with no source marked.
    add(&mut d, h, s, (50_000.0, 0.0), (52_000.0, 0.0), Some(10.0), false);
    let (mut p, ..) = d.pipe_ends(root).unwrap();
    (p.dn_mm, p.imposed) = (25.0, true);
    d.regenerate_pipe(root, &p).unwrap();
    let (reports, warnings) = d.size_pipes(&PipeSizingParams::default()).unwrap();
    assert_eq!(pipe_of(d.entity(root).unwrap()).unwrap().dn_mm, 25.0, "kept");
    let r = &reports[0];
    assert!(r.warnings.iter().any(|w| w.contains("too small")), "{:?}", r.warnings);
    assert!(r.warnings.iter().any(|w| w.contains("serves no load")), "{:?}", r.warnings);
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("belongs to no network"), "{warnings:?}");
}

#[test]
fn a_condensate_drain_is_sized_for_the_flow_typed_at_its_ends_and_runs_towards_its_outfall() {
    let mut d = Drawing::new_metric();
    let (s, c) = (PipeLeg::Supply, PipeService::Condensate);
    let outfall = add(&mut d, c, s, (0.0, 0.0), (4000.0, 0.0), None, true);
    let leaf = add(&mut d, c, s, (4000.0, 0.0), (4000.0, 2000.0), Some(6000.0), false);
    let (reports, warnings) = d.size_pipes(&PipeSizingParams::default()).unwrap();
    assert!(warnings.is_empty());
    let r = &reports[0];
    assert_eq!((r.service, r.total_load), (c, 6000.0));
    let p = pipe_of(d.entity(outfall).unwrap()).unwrap();
    assert!(p.dn_mm >= 25.0, "{}", p.dn_mm);
    assert!(p.flow_m3h.is_some_and(|f| (f - 6.0).abs() < 1e-9), "stored in m³/h: 6000 l/h");
    assert_eq!(p.loss_pa_m, None, "a drain has no pressure loss");
    assert!(p.label_text().contains("6000.0 l/h"), "{}", p.label_text());
    // Towards the outfall: against the way the run is entered from the source.
    assert_eq!(pipe_of(d.entity(leaf).unwrap()).unwrap().flow_forward, Some(false));
    // A refrigerant line is not sized: its size is the user's.
    let l = add(&mut d, PipeService::RefrigerantLiquid, s, (0.0, 9000.0), (3000.0, 9000.0), None, true);
    d.size_pipes(&PipeSizingParams::default()).unwrap();
    assert_eq!(pipe_of(d.entity(l).unwrap()).unwrap().flow_m3h, None);
}

#[test]
fn the_takeoff_counts_the_pipes_by_service_leg_size_and_insulation() {
    let mut d = Drawing::new_metric();
    let (s, r) = (PipeLeg::Supply, PipeLeg::Return);
    let c = PipeService::ChilledWater;
    add(&mut d, c, s, (0.0, 0.0), (5000.0, 0.0), None, true);
    add(&mut d, c, s, (5000.0, 0.0), (5000.0, 3000.0), Some(30.0), false);
    add(&mut d, c, r, (0.0, 200.0), (5000.0, 200.0), None, true);
    add(&mut d, PipeService::Condensate, s, (0.0, 900.0), (2000.0, 900.0), None, true);
    add(&mut d, PipeService::RefrigerantGas, s, (0.0, 1500.0), (3000.0, 1500.0), None, false);
    let t = d.takeoff("RDC").unwrap();
    let find = |service, leg| t.pipes.iter().find(|q| q.service == service && q.leg == leg).cloned();
    // The two supply runs of DN25 (the default of `add`): 5 m and 3 m.
    let supply = find(c, s).unwrap();
    assert_eq!((supply.size.as_str(), supply.count, supply.length_m), ("DN25", 2, 8.0));
    assert_eq!(find(c, r).unwrap().count, 1);
    // A drain and a refrigerant line have no leg; the tube is sized by its outer diameter.
    assert_eq!(find(PipeService::Condensate, s).unwrap().length_m, 2.0);
    assert_eq!(find(PipeService::RefrigerantGas, s).unwrap().size, "Ø25.0");
    assert_eq!((t.totals.pipe_count, t.totals.pipe_length_m), (5, 8.0 + 5.0 + 2.0 + 3.0));
    // In the sheets: the xlsx has a Pipes sheet (in French too), the csv a Pipes block.
    let bytes = |lang: &str| String::from_utf8_lossy(&t.to_xlsx(None, lang).unwrap()).into_owned();
    assert!(bytes("en").contains("Pipes") && bytes("fr").contains("Tuyauteries") && bytes("fr").contains("Eau glacée"));
    let csv = t.to_csv(false);
    assert!(csv.contains("Pipes\n") && csv.contains("chilledWater;supply;DN25;0;2;8"), "{csv}");
    assert_eq!(Drawing::new_metric().takeoff("").unwrap().totals.pipe_count, 0);
}
