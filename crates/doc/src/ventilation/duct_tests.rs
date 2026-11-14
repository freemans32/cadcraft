use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Fitting, SegmentKind, size_network};

use super::*;
use crate::{Common, Drawing, EntityKind, Handle, Space};

fn ahu() -> AhuParams {
    AhuParams {
        name: "CTA1".into(),
        kind: AhuKind::DoubleFlow,
        ports: vec![],
        width_mm: 1000.0,
        depth_mm: 600.0,
        flow_m3h: None,
        pressure_pa: None,
        extract_flow_m3h: None,
        extract_pressure_pa: None,
        fresh_flow_m3h: None,
        fresh_pressure_pa: None,
        exhaust_flow_m3h: None,
        exhaust_pressure_pa: None,
        port_sizes: Vec::new(),
        height_mm: None,
        model: String::new(),
        weight_kg: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

fn duct(diameter_mm: f64) -> DuctParams {
    DuctParams {
        length: 1.0,
        diameter_mm,
        imposed: false,
        kind: DuctKind::Auto,
        kind_found: None,
        air: AirDirection::Supply,
        air_set: false,
        flow_m3h: None,
        velocity: None,
        shape: DuctShape::Round,
        width_mm: 0.0,
        height_mm: 0.0,
        flexible: false,
        reducer: None,
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_forward: None,
        open_flow_m3h: None,
        gaps_mm: Vec::new(),
        covers: Vec::new(),
        jogs: Vec::new(),
        level_mm: None,
        steps: Vec::new(),
        bottom_mm: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

fn add_duct(d: &mut Drawing, a: (f64, f64), b: (f64, f64)) -> Handle {
    d.add_duct(&Space::Model, Common::default(), duct(200.0), Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).unwrap()
}

fn add_terminal(d: &mut Drawing, at: (f64, f64), flow: f64) -> Handle {
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "SQ600-160").unwrap();
    let t = TerminalParams { model, flow_m3h: flow, air: AirDirection::Supply, label_height_mm: 100.0, units: HvacUnits::default() };
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(at.0, at.1), 0.0).unwrap()
}

/// The unit at the origin, a trunk along x to a tee at (6000, 0) with an elbow up to a terminal
/// at (6000, 3000) and a straight branch to a terminal at (9000, 0).
fn tee_and_elbow() -> (Drawing, [Handle; 3], Handle) {
    let mut d = Drawing::new_metric();
    let unit = d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let trunk = add_duct(&mut d, (500.0, 0.0), (6000.0, 0.0));
    let up = add_duct(&mut d, (6000.0, 0.0), (6000.0, 3000.0));
    let straight = add_duct(&mut d, (6000.0, 0.0), (9000.0, 0.0));
    add_terminal(&mut d, (6000.0, 3000.0), 300.0);
    add_terminal(&mut d, (9000.0, 0.0), 450.0);
    (d, [trunk, up, straight], unit)
}

#[test]
fn a_duct_and_a_unit_survive_their_extended_data() {
    let mut d = Drawing::new_metric();
    let h = add_duct(&mut d, (0.0, 0.0), (3000.0, 4000.0));
    let (p, a, b) = d.duct_ends(h).unwrap();
    assert!((p.length - 5000.0).abs() < 1e-9 && (p.length_m() - 5.0).abs() < 1e-9);
    assert!(a.near(Vec2::ZERO, 1e-9) && b.near(Vec2::new(3000.0, 4000.0), 1e-9));
    assert_eq!(DuctParams::from_xdata(&p.to_xdata().unwrap()), Some(p));
    assert_eq!(AhuParams::from_xdata(&ahu().to_xdata().unwrap()), Some(ahu()));
    // One kind of data is not read as the other.
    assert_eq!(DuctParams::from_xdata(&ahu().to_xdata().unwrap()), None);
}

#[test]
fn bad_ducts_and_units_are_refused() {
    let mut d = Drawing::new_metric();
    assert!(d.add_duct(&Space::Model, Common::default(), duct(200.0), Vec2::ZERO, Vec2::ZERO).is_err(), "no length");
    assert!(d.add_duct(&Space::Model, Common::default(), duct(200.0), Vec2::new(f64::NAN, 0.0), Vec2::ZERO).is_err());
    assert!(d.add_duct(&Space::Model, Common::default(), duct(5.0), Vec2::ZERO, Vec2::new(1000.0, 0.0)).is_err(), "diameter");
    assert!(d.add_duct(&Space::Model, Common::default(), duct(200.0), Vec2::ZERO, Vec2::new(1e12, 0.0)).is_err(), "longer than 10 km");
    assert!(AhuParams { width_mm: 0.0, ..ahu() }.validate().is_err());
    assert!(AhuParams { name: " ".into(), ..ahu() }.validate().is_err());
    assert!(d.model.is_empty(), "nothing was added");
}

fn text_of(pieces: &[HvacPiece]) -> crate::Text {
    pieces.iter().find_map(|p| if let EntityKind::Text(t) = &p.kind { Some(t.clone()) } else { None }).expect("a label")
}

/// The vertices of each polyline of `pieces`.
fn polylines(pieces: &[HvacPiece]) -> Vec<Vec<Vec2>> {
    pieces
        .iter()
        .filter_map(|p| if let EntityKind::LwPolyline(l) = &p.kind { Some(l.vertices.iter().map(|v| v.p).collect()) } else { None })
        .collect()
}

#[test]
fn a_duct_label_reads_upright_whichever_way_it_runs() {
    let p = DuctParams { length: 1000.0, ..duct(200.0) };
    assert_eq!(text_of(&p.generate(false).unwrap()).rotation, 0.0);
    assert!((text_of(&p.generate(true).unwrap()).rotation - std::f64::consts::PI).abs() < 1e-12);
    // The two sides are a diameter apart.
    let sides = polylines(&p.generate(false).unwrap());
    assert_eq!(sides.len(), 2);
    assert_eq!((sides[0][0].y, sides[1][0].y), (100.0, -100.0));
    assert_eq!(sides[0].len(), 2, "a rigid side is a straight line");
}

#[test]
fn a_flexible_duct_is_drawn_as_bellows_above_which_its_label_sits() {
    let p = DuctParams { length: 1000.0, flexible: true, ..duct(125.0) };
    let pieces = p.generate(false).unwrap();
    let sides = polylines(&pieces);
    // Many teeth along each side, bulging outwards, within the run.
    assert!(sides[0].len() > 8 && sides[0].len() == sides[1].len());
    assert!(sides[0].iter().all(|v| v.y >= 62.5 - 1e-9 && v.y <= 62.5 * 1.3 + 1e-9), "bulges outwards only");
    assert!(sides[0].iter().any(|v| v.y > 62.5) && sides[0].iter().any(|v| (v.y - 62.5).abs() < 1e-9));
    assert!((sides[0][0].x).abs() < 1e-9 && (sides[0].last().unwrap().x - 1000.0).abs() < 1e-9);
    assert!(text_of(&pieces).insert.y > 62.5, "the label is above the duct");
    // A very long flexible duct keeps a bounded number of vertices.
    let long = DuctParams { length: 9000.0, flexible: true, units: HvacUnits { mm_per_unit: 1000.0 }, ..duct(125.0) };
    assert!(polylines(&long.generate(false).unwrap())[0].len() <= 2_001);
}

#[test]
fn a_reducer_is_drawn_as_a_cone_where_the_run_joins_one_of_another_diameter() {
    let at_start = DuctParams { length: 1000.0, reducer: Some(DuctReducer { from_mm: 250.0, at_end: false }), ..duct(160.0) };
    let sides = polylines(&at_start.generate(false).unwrap());
    // From the larger radius at the start to the run's own, 15° cone: (125 - 80) / tan 15° = 168 mm long.
    assert_eq!(sides[0][0], Vec2::new(0.0, 125.0));
    assert!((sides[0][1].x - 45.0 / 15f64.to_radians().tan()).abs() < 1e-6 && (sides[0][1].y - 80.0).abs() < 1e-9);
    assert_eq!(sides[0].last().copied(), Some(Vec2::new(1000.0, 80.0)));
    // At the far end it is the other way round.
    let at_end = DuctParams { reducer: Some(DuctReducer { from_mm: 250.0, at_end: true }), ..at_start.clone() };
    let sides = polylines(&at_end.generate(false).unwrap());
    assert_eq!(sides[0].first().copied(), Some(Vec2::new(0.0, 80.0)));
    assert_eq!(sides[0].last().copied(), Some(Vec2::new(1000.0, 125.0)));
    // A short run keeps room for itself: the cone is at most 40 % of it.
    let short = DuctParams { length: 100.0, reducer: Some(DuctReducer { from_mm: 630.0, at_end: false }), ..duct(160.0) };
    assert!(polylines(&short.generate(false).unwrap())[0][1].x <= 40.0 + 1e-9);
}

#[test]
fn sizing_draws_a_reducer_where_the_diameter_changes() {
    let (mut d, [trunk, up, straight], _) = tee_and_elbow();
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d.apply_duct_report(&net, &report).unwrap();
    let diam = |h| d.duct_ends(h).unwrap().0.diameter_mm;
    let red = |h| d.duct_ends(h).unwrap().0.reducer;
    assert_eq!(red(trunk), None, "the run leaving the unit joins nothing");
    for h in [up, straight] {
        match (diam(h) != diam(trunk), red(h)) {
            (true, Some(r)) => assert_eq!((r.from_mm, r.at_end), (diam(trunk), false), "the child starts at the trunk's end"),
            (false, None) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!([up, straight].iter().any(|h| red(*h).is_some()), "the trunk and a child differ in this network");
    // Sizing again after the diameters are equal removes the reducer.
    let (mut p, ..) = d.duct_ends(trunk).unwrap();
    p.imposed = true;
    p.diameter_mm = diam(up);
    d.regenerate_duct(trunk, &p).unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d.apply_duct_report(&net, &report).unwrap();
    assert_eq!(d.duct_ends(up).unwrap().0.reducer, None);
}

#[test]
fn a_flexible_run_is_rougher_in_the_network() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (3000.0, 0.0));
    let flex = DuctParams { flexible: true, ..duct(160.0) };
    d.add_duct(&Space::Model, Common::default(), flex, Vec2::new(3000.0, 0.0), Vec2::new(3000.0, -1000.0)).unwrap();
    add_terminal(&mut d, (3000.0, -1000.0), 150.0);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments.iter().map(|s| s.roughness_mm).collect::<Vec<_>>(), [None, Some(FLEX_ROUGHNESS_MM)]);
}

#[test]
fn the_network_is_read_from_where_the_ends_meet() {
    let (d, [trunk, up, straight], unit) = tee_and_elbow();
    let nets = d.duct_networks().unwrap();
    assert_eq!(nets.len(), 1);
    let n = &nets[0];
    assert_eq!(n.ahu, unit);
    assert_eq!(n.ducts, [trunk, straight, up], "the straightest run first at a tee");
    assert!(n.dangling.is_empty() && n.unconnected.is_empty());
    let s = &n.network.segments;
    assert_eq!(s.iter().map(|x| x.parent).collect::<Vec<_>>(), [None, Some(0), Some(0)]);
    assert!((s[0].length_m - 5.5).abs() < 1e-9);
    assert_eq!(s[1].fittings, [Fitting::TeeStraight]);
    assert_eq!(s[2].fittings, [Fitting::TeeBranch, Fitting::Elbow90]);
    assert_eq!(s[0].kind, SegmentKind::Main, "the trunk carries two terminals");
    assert_eq!((s[1].kind, s[2].kind), (SegmentKind::Connection, SegmentKind::Connection));
    assert_eq!(s[1].terminal.map(|t| t.flow_m3h), Some(450.0));
    assert_eq!(s[2].terminal.map(|t| t.flow_m3h), Some(300.0));
    // The terminal's own pressure drop travels with it.
    assert_eq!(s[1].terminal.map(|t| t.pressure_drop_pa), Some(15.0));
}

#[test]
fn a_network_is_sized_and_the_diameters_written_back() {
    let (mut d, [trunk, ..], unit) = tee_and_elbow();
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    assert!((report.total_flow_m3h - 750.0).abs() < 1e-9);
    d.apply_duct_report(&net, &report).unwrap();
    let (p, ..) = d.duct_ends(trunk).unwrap();
    assert_eq!(p.diameter_mm, report.segments[0].diameter_mm);
    assert_eq!(p.flow_m3h, Some(750.0));
    assert!(p.velocity.is_some_and(|v| v > 0.0));
    let u = ahu_of(d.entity(unit).unwrap()).unwrap();
    assert_eq!(u.flow_m3h, Some(750.0));
    assert!(u.pressure_pa.is_some_and(|p| p > 0.0));
    // Reading again gives the same network: sizing changed no connection.
    assert_eq!(d.duct_networks().unwrap()[0].network.segments.len(), 3);
}

#[test]
fn an_imposed_diameter_is_kept_by_sizing() {
    let (mut d, [trunk, ..], _) = tee_and_elbow();
    let (mut p, ..) = d.duct_ends(trunk).unwrap();
    p.diameter_mm = 630.0;
    p.imposed = true;
    d.regenerate_duct(trunk, &p).unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments[0].diameter_mm, Some(630.0));
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d.apply_duct_report(&net, &report).unwrap();
    assert_eq!(d.duct_ends(trunk).unwrap().0.diameter_mm, 630.0);
}

#[test]
fn a_duct_to_no_terminal_is_left_out_and_a_lost_terminal_is_reported() {
    let (mut d, _, _) = tee_and_elbow();
    let stub = add_duct(&mut d, (9000.0, 0.0), (9000.0, -2000.0));
    let lost = add_terminal(&mut d, (20000.0, 20000.0), 100.0);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.dangling, [stub]);
    assert_eq!(net.unconnected, [lost]);
    assert_eq!(net.ducts.len(), 3, "the stub is not sized");
    assert!(size_network(&net.network, &DesignCriteria::default()).is_ok());
}

#[test]
fn a_duct_starts_a_branch_in_the_middle_of_another() {
    let mut d = Drawing::new_metric();
    let h = add_duct(&mut d, (0.0, 0.0), (4000.0, 0.0));
    assert_eq!(d.split_duct_at(&Space::Model, h, Vec2::new(1000.0, 500.0), 5.0).unwrap(), None, "off the duct");
    assert_eq!(d.split_duct_at(&Space::Model, h, Vec2::new(2.0, 0.0), 5.0).unwrap(), None, "at its end");
    let second = d.split_duct_at(&Space::Model, h, Vec2::new(1500.0, 2.0), 5.0).unwrap().unwrap();
    let (p1, a1, b1) = d.duct_ends(h).unwrap();
    let (p2, a2, b2) = d.duct_ends(second).unwrap();
    assert!((p1.length - 1500.0).abs() < 1e-6 && (p2.length - 2500.0).abs() < 1e-6);
    assert!(a1.near(Vec2::ZERO, 1e-9) && b1.near(a2, 1e-6) && b2.near(Vec2::new(4000.0, 0.0), 1e-6));
    assert_eq!(p2.diameter_mm, p1.diameter_mm);
}

#[test]
fn hostile_drawings_do_not_panic_the_network_reading() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // A closed loop of ducts through the unit, and a duct that starts and ends in it.
    add_duct(&mut d, (0.0, 0.0), (1000.0, 0.0));
    add_duct(&mut d, (1000.0, 0.0), (1000.0, 1000.0));
    add_duct(&mut d, (1000.0, 1000.0), (0.0, 0.0));
    let nets = d.duct_networks().unwrap();
    assert!(nets[0].network.segments.is_empty(), "no terminal: nothing to size");
}

#[test]
fn a_duct_started_near_the_face_of_a_unit_is_connected_to_it() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // The unit is 1000 mm wide: its face is at x = 500. 60 mm off is a click near it, 300 mm is not.
    add_duct(&mut d, (560.0, 0.0), (3000.0, 0.0));
    add_terminal(&mut d, (3000.0, 0.0), 100.0);
    assert_eq!(d.duct_networks().unwrap()[0].network.segments.len(), 1);
    let mut far = Drawing::new_metric();
    far.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut far, (800.0, 0.0), (3000.0, 0.0));
    add_terminal(&mut far, (3000.0, 0.0), 100.0);
    assert!(far.duct_networks().unwrap()[0].network.segments.is_empty());
}

/// The unit turned a quarter turn at the origin, a duct up from its face that ends in the middle of
/// a run laid between two terminals (the run has no end there).
fn stub_into_a_run(offset_mm: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, std::f64::consts::FRAC_PI_2).unwrap();
    add_duct(&mut d, (-2000.0, 2500.0), (2000.0, 2500.0));
    add_duct(&mut d, (0.0, 500.0), (offset_mm, 2500.0));
    add_terminal(&mut d, (-2000.0, 2500.0), 150.0);
    add_terminal(&mut d, (2000.0, 2500.0), 150.0);
    d
}

#[test]
fn a_duct_ending_in_the_middle_of_a_run_is_not_a_tee_until_the_run_is_cut() {
    let mut d = stub_into_a_run(0.0);
    assert!(d.duct_networks().unwrap()[0].network.segments.is_empty(), "read as it is, the stub leads nowhere");
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 1);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments.len(), 3, "the stub and the two halves of the run");
    assert!(net.unconnected.is_empty() && net.dangling.is_empty());
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    assert!((report.total_flow_m3h - 300.0).abs() < 1e-9);
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 0, "cutting twice changes nothing");
}

#[test]
fn a_duct_snapped_to_the_side_of_a_run_still_meets_it() {
    // Snapping to the lines of a duct lands a radius (100 mm) off its axis.
    let mut d = stub_into_a_run(100.0);
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 1);
    assert_eq!(d.duct_networks().unwrap()[0].network.segments.len(), 3);
}

#[test]
fn a_terminal_in_the_middle_of_a_run_cuts_it() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (6000.0, 0.0));
    add_terminal(&mut d, (3000.0, 0.0), 100.0);
    add_terminal(&mut d, (6000.0, 0.0), 100.0);
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 1);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments.len(), 2);
    assert!(net.unconnected.is_empty());
}

fn extract_duct(d: &mut Drawing, a: (f64, f64), b: (f64, f64)) -> Handle {
    let p = DuctParams { air: AirDirection::Extract, air_set: true, ..duct(200.0) };
    d.add_duct(&Space::Model, Common::default(), p, Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).unwrap()
}

fn extract_terminal(d: &mut Drawing, at: (f64, f64), flow: f64) -> Handle {
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "EX125").unwrap();
    let t = TerminalParams { model, flow_m3h: flow, air: AirDirection::Extract, label_height_mm: 100.0, units: HvacUnits::default() };
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(at.0, at.1), 0.0).unwrap()
}

/// A double-flux unit: supply ducts and terminals above it, extract ones below.
fn double_flux() -> Drawing {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 100.0), (4000.0, 100.0));
    add_terminal(&mut d, (4000.0, 100.0), 300.0);
    extract_duct(&mut d, (500.0, -100.0), (4000.0, -100.0));
    extract_terminal(&mut d, (4000.0, -100.0), 250.0);
    d
}

#[test]
fn a_double_flux_unit_has_a_supply_and_an_extract_network_each_with_its_own_terminals() {
    let d = double_flux();
    let nets = d.duct_networks().unwrap();
    assert_eq!(nets.iter().map(|n| n.air).collect::<Vec<_>>(), [AirDirection::Supply, AirDirection::Extract]);
    for n in &nets {
        assert_eq!(n.network.segments.len(), 1, "{:?}", n.air);
        assert!(n.unconnected.is_empty());
    }
    assert_eq!(nets[0].network.segments[0].terminal.map(|t| t.flow_m3h), Some(300.0));
    assert_eq!(nets[1].network.segments[0].terminal.map(|t| t.flow_m3h), Some(250.0));
}

#[test]
fn each_network_gives_its_flow_to_the_unit_and_the_label_shows_both() {
    let mut d = double_flux();
    for net in d.duct_networks().unwrap() {
        let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
        d.apply_duct_report(&net, &report).unwrap();
    }
    let unit = d.model.iter().find_map(|e| ahu_of(e)).unwrap();
    assert_eq!((unit.flow_m3h, unit.extract_flow_m3h), (Some(300.0), Some(250.0)));
    assert!(unit.pressure_pa.is_some() && unit.extract_pressure_pa.is_some());
    // The label with the figures is the second text (the unit's code comes first, the collars' tags last).
    let text =
        unit.generate().unwrap().into_iter().filter_map(|p| if let EntityKind::Text(t) = p.kind { Some(t.value) } else { None }).nth(1).unwrap();
    assert!(text.contains("S 300 / E 250 m³/h"), "{text}");
}

#[test]
fn a_terminal_on_a_duct_of_the_other_air_direction_is_not_connected() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // A supply duct the user fixed by hand: a terminal of the other direction at its end is not reached.
    let fixed = DuctParams { air_set: true, ..duct(200.0) };
    d.add_duct(&Space::Model, Common::default(), fixed, Vec2::new(500.0, 0.0), Vec2::new(4000.0, 0.0)).unwrap();
    let wrong = extract_terminal(&mut d, (4000.0, 0.0), 250.0);
    let net = d.duct_networks().unwrap().remove(0);
    assert!(net.network.segments.is_empty(), "no supply terminal at the end");
    assert_eq!(net.unconnected, [wrong]);
}

/// A unit, a trunk along x and a short duct that drops from it to the top edge of a terminal's
/// face: where a user clicks (a snap to the face), not the terminal's centre.
fn drop_to_the_face(end_y: f64) -> Drawing {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (3000.0, 0.0));
    add_duct(&mut d, (3000.0, 0.0), (3000.0, end_y));
    // The face is 600 mm: its top edge is 300 mm above the centre.
    add_terminal(&mut d, (3000.0, -1000.0), 150.0);
    d
}

#[test]
fn a_duct_ending_on_the_face_of_a_terminal_reaches_it() {
    for end_y in [-700.0, -800.0, -1000.0] {
        let d = drop_to_the_face(end_y);
        let net = d.duct_networks().unwrap().remove(0);
        assert!(net.unconnected.is_empty(), "end at {end_y}");
        assert_eq!(net.network.segments.len(), 2, "end at {end_y}");
    }
    // Past the face by a few hundred millimetres it does not.
    let net = drop_to_the_face(-400.0).duct_networks().unwrap().remove(0);
    assert_eq!(net.unconnected.len(), 1);
}

#[test]
fn a_duct_ending_on_the_face_of_a_turned_terminal_reaches_it() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (3000.0, 0.0));
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "RG300x150").unwrap();
    let t = TerminalParams { model, flow_m3h: 100.0, air: AirDirection::Supply, label_height_mm: 100.0, units: HvacUnits::default() };
    // A 300 x 150 grille turned a quarter turn: 150 mm wide along x, 300 along y. Its near end is 150 mm off.
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(3140.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
    assert!(d.duct_networks().unwrap()[0].unconnected.is_empty());
}

/// The sports hall's trap: extract terminals at the ends of ducts nobody gave a direction to.
#[test]
fn ducts_left_on_auto_take_the_direction_of_the_terminals_they_serve() {
    let mut d = Drawing::new_metric();
    // A collar names its direction; a unit whose collars say nothing useful leaves it to the terminals.
    let unit = AhuParams { ports: vec![AirDirection::Extract; 4], ..ahu() };
    d.add_ahu(&Space::Model, Common::default(), &unit, Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (4000.0, 0.0));
    let t = extract_terminal(&mut d, (4000.0, 0.0), 250.0);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.air, AirDirection::Extract);
    assert!(net.unconnected.is_empty(), "{:?}", net.unconnected);
    assert_eq!(net.network.segments[0].terminal.map(|x| x.flow_m3h), Some(250.0));
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d.apply_duct_report(&net, &report).unwrap();
    let duct = d.model.iter().find_map(|e| duct_of(e).map(|p| (e.handle, p))).unwrap();
    assert_eq!(duct.1.air, AirDirection::Extract, "the duct is now drawn as an extract duct");
    assert!(!duct.1.air_set, "but it stays on auto");
    assert_eq!(d.entity(duct.0).unwrap().common.layer, DUCT_EXTRACT_LAYER);
    assert_eq!(ahu_of(d.model.iter().find(|e| ahu_of(e).is_some()).unwrap()).unwrap().extract_flow_m3h, Some(250.0));
    let _ = t;
    // The collar is tried first, but a terminal that says otherwise still turns the network round.
    let (mut tp, ..) = (air_terminal_of(d.model.iter().find(|e| air_terminal_of(e).is_some()).unwrap()).unwrap(), 0);
    tp.air = AirDirection::Supply;
    let h = d.model.iter().find(|e| air_terminal_of(e).is_some()).unwrap().handle;
    d.regenerate_air_terminal(h, &tp).unwrap();
    assert_eq!(d.duct_networks().unwrap()[0].air, AirDirection::Supply);
}

#[test]
fn a_double_flux_unit_with_both_trees_on_auto_keeps_them_apart() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 100.0), (4000.0, 100.0));
    add_terminal(&mut d, (4000.0, 100.0), 300.0);
    add_duct(&mut d, (500.0, -100.0), (4000.0, -100.0));
    extract_terminal(&mut d, (4000.0, -100.0), 250.0);
    let nets = d.duct_networks().unwrap();
    assert_eq!(nets.iter().map(|n| n.air).collect::<Vec<_>>(), [AirDirection::Supply, AirDirection::Extract]);
    assert!(nets.iter().all(|n| n.network.segments.len() == 1 && n.unconnected.is_empty()));
    assert_eq!(nets[0].network.segments[0].terminal.map(|x| x.flow_m3h), Some(300.0));
    assert_eq!(nets[1].network.segments[0].terminal.map(|x| x.flow_m3h), Some(250.0));
}

fn rect_duct(w: f64, h: f64) -> DuctParams {
    DuctParams { shape: DuctShape::Rectangular, width_mm: w, height_mm: h, diameter_mm: 343.0, ..duct(200.0) }
}

#[test]
fn a_rectangular_duct_is_drawn_as_wide_as_its_width_and_labelled_with_its_sides() {
    let p = DuctParams { length: 1000.0, ..rect_duct(400.0, 250.0) };
    assert_eq!((p.plan_width_mm(), p.size_label().as_str()), (400.0, "400×250"));
    let pieces = p.generate(false).unwrap();
    let sides = polylines(&pieces);
    assert_eq!((sides[0][0].y, sides[1][0].y), (200.0, -200.0), "400 mm wide in plan");
    assert!(text_of(&pieces).value.starts_with("400×250"));
    assert!(DuctParams { width_mm: 5.0, ..p.clone() }.validate().is_err());
    assert!(DuctParams { height_mm: f64::NAN, ..p.clone() }.validate().is_err());
    // A round duct is unchanged.
    assert_eq!(DuctParams { length: 1000.0, ..duct(200.0) }.size_label(), "Ø200");
    // And the shape survives its data.
    assert_eq!(DuctParams::from_xdata(&p.to_xdata().unwrap()), Some(p));
}

fn zone(name: &str, void: f64, bottom: Option<f64>, outline: &[(f64, f64)]) -> ZoneParams {
    ZoneParams {
        name: name.into(),
        void_height_mm: void,
        bottom_mm: bottom,
        outline: outline.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

/// A unit and a trunk reaching four 300 m³/h diffusers by connections, all under a zone of `void`
/// mm available (none when `void` is `None`).
fn hall(void: Option<f64>) -> Drawing {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let mut from = 500.0;
    for x in [2000.0, 4000.0, 6000.0, 8000.0] {
        add_duct(&mut d, (from, 0.0), (x, 0.0));
        add_duct(&mut d, (x, 0.0), (x, -1500.0));
        add_terminal(&mut d, (x, -1500.0), 300.0);
        from = x;
    }
    if let Some(v) = void {
        let all = zone("Plenum", v, None, &[(-1000.0, -2500.0), (9500.0, -2500.0), (9500.0, 1000.0), (-1000.0, 1000.0)]);
        d.add_zone(&Space::Model, Common::default(), all).unwrap();
    }
    d
}

fn size_all(d: &mut Drawing) -> cadcraft_hvac::NetworkReport {
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d.apply_duct_report(&net, &report).unwrap();
    report
}

#[test]
fn the_height_available_less_the_margin_goes_to_every_run_of_the_network() {
    let d = hall(Some(350.0));
    let net = d.duct_networks().unwrap().remove(0);
    assert!(net.network.segments.iter().all(|s| s.max_height_mm == Some(250.0)), "350 less the 100 mm margin");
    assert!(hall(None).duct_networks().unwrap()[0].network.segments.iter().all(|s| s.max_height_mm.is_none()));
    let net = d.duct_networks_with_margin(50.0).unwrap().remove(0);
    assert!(net.network.segments.iter().all(|s| s.max_height_mm == Some(300.0)), "a margin of 50 mm");
}

#[test]
fn a_zone_wins_over_a_space_where_the_duct_runs_in_it() {
    let mut d = hall(None);
    // A room over everything with 350 mm available, and a beam over the trunk's left part with 200.
    let room = SpaceParams {
        name: "Salle".into(),
        room: cadcraft_hvac::RoomUse::Office,
        occupants: 1,
        collective: true,
        flow_m3h: None,
        void_height_mm: Some(350.0),
        outline: vec![Vec2::new(0.0, 0.0), Vec2::new(10500.0, 0.0), Vec2::new(10500.0, 3500.0), Vec2::new(0.0, 3500.0)],
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    };
    d.add_space(&Space::Model, Common::default(), SpaceParams { void_height_mm: Some(350.0), ..room }).unwrap();
    // The room's first point is its origin: it sits at (0, 0); move it so it holds the whole hall.
    let space = d.model.iter().find(|e| space_of(e).is_some()).unwrap().handle;
    d.modify_entity(space, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.insert.x = -1000.0;
            i.insert.y = -2500.0;
        }
    })
    .unwrap();
    d.add_zone(&Space::Model, Common::default(), zone("Poutre", 200.0, None, &[(0.0, -500.0), (3000.0, -500.0), (3000.0, 500.0), (0.0, 500.0)]))
        .unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    let heights: Vec<_> = net.network.segments.iter().map(|s| s.max_height_mm).collect();
    // The first trunk piece (midpoint x = 1250) is under the beam; its neighbours are not.
    assert_eq!(heights[0], Some(100.0), "200 less the margin, under the beam");
    assert!(heights[1..].contains(&Some(250.0)), "the room's 350 elsewhere: {heights:?}");
}

#[test]
fn a_zone_is_drawn_hatched_with_its_height_and_altitude_written_in_it() {
    let z = zone("Poutre P3", 280.0, Some(2450.0), &[(0.0, 0.0), (4000.0, 0.0), (4000.0, 600.0), (0.0, 600.0)]);
    let pieces = z.generate().unwrap();
    assert!(pieces.iter().any(|p| matches!(p.kind, EntityKind::Hatch(_))));
    assert!(pieces.iter().any(|p| matches!(p.kind, EntityKind::LwPolyline(_))));
    assert_eq!(text_of(&pieces).value, "Poutre P3 H=280 Ai:2.45", "readable by another program");
    assert!(pieces.iter().all(|p| p.layer == ZONE_LAYER));
    assert_eq!(ZoneParams::from_xdata(&z.to_xdata().unwrap()), Some(z.clone()));
    for bad in [
        ZoneParams { void_height_mm: 10.0, ..z.clone() },
        ZoneParams { void_height_mm: f64::NAN, ..z.clone() },
        ZoneParams { outline: z.outline[..2].to_vec(), ..z.clone() },
        ZoneParams { name: " ".into(), ..z.clone() },
        ZoneParams { bottom_mm: Some(f64::INFINITY), ..z.clone() },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    // In the drawing: a block on its own layer, its outline read back in drawing coordinates.
    let mut d = Drawing::new_metric();
    let h = d
        .add_zone(&Space::Model, Common::default(), ZoneParams { outline: z.outline.iter().map(|p| *p + Vec2::new(100.0, 200.0)).collect(), ..z })
        .unwrap();
    let (_, outline) = d.zone_outline(h).unwrap();
    assert!(outline[0].near(Vec2::new(100.0, 200.0), 1e-9) && outline[2].near(Vec2::new(4100.0, 800.0), 1e-9));
    assert_eq!(d.entity(h).unwrap().common.layer, ZONE_LAYER);
}

#[test]
fn a_duct_crossing_a_zone_is_cut_at_its_edges_and_the_piece_inside_takes_its_height_and_altitude() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let trunk = add_duct(&mut d, (500.0, 0.0), (9000.0, 0.0));
    add_terminal(&mut d, (9000.0, 0.0), 300.0);
    d.add_zone(
        &Space::Model,
        Common::default(),
        zone("Poutre", 250.0, Some(2450.0), &[(3000.0, -800.0), (5000.0, -800.0), (5000.0, 800.0), (3000.0, 800.0)]),
    )
    .unwrap();
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 2, "in at x = 3000, out at x = 5000");
    assert_eq!(d.split_duct_taps(&Space::Model).unwrap(), 0, "cutting twice changes nothing");
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments.len(), 3);
    let (first, mid, last) = (&net.network.segments[0], &net.network.segments[1], &net.network.segments[2]);
    assert_eq!((first.max_height_mm, mid.max_height_mm, last.max_height_mm), (None, Some(150.0), None));
    assert_eq!(net.bottoms_mm, [None, Some(2450.0), None]);
    // Sized at 400 m³/h (about Ø160): round before and after, flat under the beam (150 mm tall), with
    // the altitude on its label, and a reducer where the section changes.
    let mut d2 = d.clone();
    let big = d2.model.iter().find(|e| air_terminal_of(e).is_some()).unwrap().handle;
    let (mut t, ..) = (air_terminal_of(d2.entity(big).unwrap()).unwrap(), 0);
    t.flow_m3h = 400.0;
    d2.regenerate_air_terminal(big, &t).unwrap();
    let net = d2.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    d2.apply_duct_report(&net, &report).unwrap();
    let pieces: Vec<_> = net.ducts.iter().map(|h| d2.duct_ends(*h).unwrap().0).collect();
    assert_eq!((pieces[0].shape, pieces[2].shape), (DuctShape::Round, DuctShape::Round));
    assert_eq!((pieces[1].shape, pieces[1].height_mm), (DuctShape::Rectangular, 150.0), "{:?}", pieces[1]);
    assert_eq!(pieces[1].bottom_mm, Some(2450.0));
    assert!(pieces[1].size_label().contains('×') && pieces[0].bottom_mm.is_none());
    let label = text_of(&pieces[1].generate(false).unwrap()).value;
    assert!(label.contains("Ai:2.45"), "{label}");
    assert!(pieces[1].reducer.is_some() || pieces[2].reducer.is_some(), "a reducer where the section changes");
    let _ = trunk;
}

#[test]
fn a_network_is_sized_round_where_it_fits_and_rectangular_where_it_does_not() {
    let mut d = hall(Some(350.0));
    let report = size_all(&mut d);
    let sizes: Vec<_> = d
        .duct_networks()
        .unwrap()
        .remove(0)
        .ducts
        .iter()
        .map(|h| {
            let (p, ..) = d.duct_ends(*h).unwrap();
            (p.shape, p.width_mm, p.height_mm, p.size_label())
        })
        .collect();
    // The trunk starts with 1200 m³/h: too big to be round in 250 mm; the connections are round.
    let trunk = &sizes[0];
    assert_eq!(trunk.0, DuctShape::Rectangular, "{sizes:?}");
    assert_eq!(trunk.2, 250.0);
    assert!(trunk.1 % 50.0 == 0.0 && trunk.1 >= 250.0);
    assert!(sizes.iter().filter(|s| s.0 == DuctShape::Round).count() >= 3, "{sizes:?}");
    assert!(sizes.iter().filter(|s| s.0 == DuctShape::Round).all(|s| s.1 == 0.0 && s.3.starts_with('Ø')));
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    // A drawing with no height available is all round, as before.
    let mut d = hall(None);
    size_all(&mut d);
    assert!(d.model.iter().filter_map(|e| duct_of(e)).all(|p| p.shape == DuctShape::Round));
}

#[test]
fn a_reducer_is_drawn_where_a_round_run_meets_a_rectangular_one() {
    let mut d = hall(Some(350.0));
    size_all(&mut d);
    let net = d.duct_networks().unwrap().remove(0);
    let reducers: Vec<_> = net.ducts.iter().filter_map(|h| d.duct_ends(*h)).filter_map(|(p, ..)| p.reducer.map(|r| (p.shape, r.from_mm))).collect();
    // A round connection leaves the rectangular trunk: it starts from the trunk's width.
    assert!(reducers.iter().any(|(shape, from)| *shape == DuctShape::Round && *from >= 250.0), "{reducers:?}");
}

#[test]
fn a_section_the_user_imposed_is_kept_and_a_round_one_too() {
    let mut d = hall(Some(350.0));
    let trunk = d.duct_networks().unwrap()[0].ducts[0];
    let (mut p, ..) = d.duct_ends(trunk).unwrap();
    (p.shape, p.width_mm, p.height_mm, p.imposed) = (DuctShape::Rectangular, 500.0, 200.0, true);
    d.regenerate_duct(trunk, &p).unwrap();
    assert_eq!(d.duct_networks().unwrap()[0].network.segments[0].rect_mm, Some((500.0, 200.0)));
    size_all(&mut d);
    let (after, ..) = d.duct_ends(trunk).unwrap();
    assert_eq!((after.shape, after.width_mm, after.height_mm), (DuctShape::Rectangular, 500.0, 200.0));
    // Imposed round: no rectangle for the segment.
    let (mut q, ..) = d.duct_ends(trunk).unwrap();
    (q.shape, q.width_mm, q.height_mm, q.diameter_mm) = (DuctShape::Round, 0.0, 0.0, 315.0);
    d.regenerate_duct(trunk, &q).unwrap();
    let seg = d.duct_networks().unwrap()[0].network.segments[0].clone();
    assert_eq!((seg.diameter_mm, seg.rect_mm), (Some(315.0), None));
}

#[test]
fn a_height_too_small_for_any_duct_is_an_error_not_a_panic() {
    let d = hall(Some(150.0));
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments[0].max_height_mm, Some(50.0));
    // 50 mm is below the smallest side, but only when a round duct would not fit it: sizing says so.
    assert!(size_network(&net.network, &DesignCriteria::default()).is_err());
}

fn arcs(pieces: &[HvacPiece]) -> Vec<(Vec2, f64, f64, f64)> {
    pieces.iter().filter_map(|p| if let EntityKind::Arc(a) = &p.kind { Some((a.center.xy(), a.radius, a.start, a.end)) } else { None }).collect()
}

fn point_on(arc: &(Vec2, f64, f64, f64), angle: f64) -> Vec2 {
    arc.0 + Vec2::from_angle(angle) * arc.1
}

#[test]
fn an_elbow_is_two_arcs_that_meet_the_sides_of_the_run_without_a_gap() {
    // A 200 mm run of 3000 mm, turning 90° to the left or the right, entered at its start or its end.
    for (angle, at_end) in [(90.0, false), (-90.0, false), (90.0, true), (-90.0, true), (45.0, false), (-60.0, true)] {
        let mut p = DuctParams { length: 3000.0, ..duct(200.0) };
        let e = DuctElbow { angle_deg: angle, radius_mm: 200.0, at_end };
        p.elbow = Some(e);
        let t = e.tangent_mm();
        if at_end {
            p.trim_end_mm = t;
        } else {
            p.trim_start_mm = t;
        }
        let pieces = p.generate(false).unwrap();
        let a = arcs(&pieces);
        assert_eq!(a.len(), 2, "{angle} {at_end}");
        let radii = {
            let mut r: Vec<f64> = a.iter().map(|x| x.1).collect();
            r.sort_by(f64::total_cmp);
            r
        };
        assert_eq!(radii, [100.0, 300.0], "the inner and the outer arc of a 200 mm run bent on a 200 mm radius");
        // The run's own end of each arc is where its side now starts (or ends): no gap, no overlap.
        let sides = polylines(&pieces);
        let side_end = |side: &Vec<Vec2>| if at_end { *side.last().unwrap() } else { side[0] };
        let arc_ends: Vec<Vec2> = a.iter().flat_map(|x| [point_on(x, x.2), point_on(x, x.3)]).collect();
        for side in &sides {
            let e = side_end(side);
            assert!(arc_ends.iter().any(|p| p.near(e, 1e-6)), "{angle} {at_end}: the side ends at {e:?}, the arcs at {arc_ends:?}");
            assert!((e.x - if at_end { 3000.0 - t } else { t }).abs() < 1e-6, "the side is trimmed by the bend's tangent length");
        }
        // The arc's two ends are a quarter turn... the bend's own angle apart.
        let span = (a[0].3 - a[0].2).abs();
        assert!((span - angle.abs().to_radians()).abs() < 1e-9, "{span}");
    }
    // No elbow, no arc; and a bad one is refused.
    assert!(arcs(&DuctParams { length: 3000.0, ..duct(200.0) }.generate(false).unwrap()).is_empty());
    let mut bad = duct(200.0);
    bad.elbow = Some(DuctElbow { angle_deg: 0.0, radius_mm: 200.0, at_end: false });
    assert!(bad.validate().is_err());
    bad.elbow = Some(DuctElbow { angle_deg: 200.0, radius_mm: 200.0, at_end: false });
    assert!(bad.validate().is_err());
}

#[test]
fn sizing_draws_the_elbow_where_a_run_turns_and_trims_both_runs() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let east = add_duct(&mut d, (500.0, 0.0), (4000.0, 0.0));
    let north = add_duct(&mut d, (4000.0, 0.0), (4000.0, 3000.0));
    add_terminal(&mut d, (4000.0, 3000.0), 200.0);
    size_all(&mut d);
    let (up, ..) = d.duct_ends(east).unwrap();
    let (turn, ..) = d.duct_ends(north).unwrap();
    let e = turn.elbow.expect("the second run starts with an elbow");
    assert!((e.angle_deg - 90.0).abs() < 1e-9 && !e.at_end, "a left turn from east to north: {e:?}");
    let t = e.tangent_mm();
    assert!((t - e.radius_mm).abs() < 1e-6, "a 90° bend's tangent is its radius");
    assert_eq!((turn.trim_start_mm, turn.trim_end_mm), (t, 0.0));
    assert_eq!((up.trim_start_mm, up.trim_end_mm), (0.0, t), "the run before it is drawn short of the corner");
    assert_eq!(up.elbow, None);
    // A run of the other orientation: the north run drawn from its far end, turning right.
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (4000.0, 0.0));
    let south = add_duct(&mut d, (4000.0, -3000.0), (4000.0, 0.0));
    add_terminal(&mut d, (4000.0, -3000.0), 200.0);
    size_all(&mut d);
    let (p, ..) = d.duct_ends(south).unwrap();
    let e = p.elbow.expect("an elbow");
    assert!((e.angle_deg + 90.0).abs() < 1e-9 && e.at_end, "a right turn, entered by its end: {e:?}");
    assert_eq!((p.trim_start_mm, p.trim_end_mm), (0.0, e.tangent_mm()));
}

#[test]
fn a_tee_and_a_short_run_get_no_bend_and_a_tight_one_is_drawn_tighter() {
    // A tee has no bare elbow: nothing is drawn at the branch.
    let (mut d, [trunk, up, straight], _) = tee_and_elbow();
    size_all(&mut d);
    for h in [trunk, up, straight] {
        assert_eq!(d.duct_ends(h).unwrap().0.elbow, None);
    }
    // Two runs of 300 mm leave no room for a 250 mm radius bend: it is tightened to fit, and a run
    // too short for even a tight one is left as a corner.
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 0.0), (2500.0, 0.0));
    let short = add_duct(&mut d, (2500.0, 0.0), (2500.0, 300.0));
    add_terminal(&mut d, (2500.0, 300.0), 100.0);
    size_all(&mut d);
    let (p, ..) = d.duct_ends(short).unwrap();
    if let Some(e) = p.elbow {
        assert!(e.tangent_mm() <= 0.4 * 300.0 + 1e-9, "{e:?}");
    }
}

fn unit_of(kind: AhuKind) -> AhuParams {
    let (w, d) = kind.default_size_mm();
    AhuParams { kind, width_mm: w, depth_mm: d, ..ahu() }
}

fn polys(p: &[HvacPiece]) -> usize {
    p.iter().filter(|x| matches!(x.kind, EntityKind::LwPolyline(_))).count()
}

fn circles(p: &[HvacPiece]) -> usize {
    p.iter().filter(|x| matches!(x.kind, EntityKind::Circle(_))).count()
}

#[test]
fn each_kind_of_unit_is_drawn_with_its_own_parts_and_collars() {
    // Casing and inner panel, then the collars: two on each short side of a double-flux unit, one
    // at each end of a single-flux one, two sides and two spigots on a ventilation box.
    let expect =
        [(AhuKind::DoubleFlow, 2 + 4, 2), (AhuKind::SupplyUnit, 2 + 2, 1), (AhuKind::ExtractUnit, 2 + 2, 1), (AhuKind::VmcBox, 2 + 2 + 2, 1)];
    for (kind, polylines, fans) in expect {
        let pieces = unit_of(kind).generate().unwrap();
        assert_eq!((polys(&pieces), circles(&pieces)), (polylines, fans), "{kind:?}");
        assert!(pieces.iter().filter(|p| p.layer == AHU_LAYER).count() > 0);
        assert!(text_of(&pieces).value.starts_with(kind.code()), "the code is written on the unit: {kind:?}");
    }
    // Each kind starts at its own size, and the parse takes the names a command gives and the French ones.
    let sizes: Vec<_> = AhuKind::ALL.iter().map(|k| k.default_size_mm()).collect();
    assert_eq!(sizes[3], (700.0, 550.0));
    assert!(sizes[3].0 < sizes[1].0, "a ventilation box is smaller than a unit");
    for (text, kind) in [
        ("doubleFlow", AhuKind::DoubleFlow),
        ("DF", AhuKind::DoubleFlow),
        ("supply", AhuKind::SupplyUnit),
        ("extraction", AhuKind::ExtractUnit),
        ("VMC", AhuKind::VmcBox),
    ] {
        assert_eq!(AhuKind::parse(text), Some(kind), "{text}");
        assert_eq!(AhuKind::parse(kind.name()), Some(kind));
    }
    assert_eq!(AhuKind::parse("boiler"), None);
}

#[test]
fn a_unit_saved_before_there_were_kinds_is_a_double_flux_one() {
    let old = r#"{"name":"CTA1","widthMm":1500.0,"depthMm":800.0}"#;
    let p: AhuParams = serde_json::from_str(old).unwrap();
    assert_eq!(p.kind, AhuKind::DoubleFlow);
    assert_eq!(AhuParams::from_xdata(&unit_of(AhuKind::VmcBox).to_xdata().unwrap()).map(|p| p.kind), Some(AhuKind::VmcBox));
}

#[test]
fn every_kind_of_unit_feeds_a_network_the_same_way() {
    for kind in AhuKind::ALL {
        let mut d = Drawing::new_metric();
        let (w, _) = kind.default_size_mm();
        // Every collar supplies, so the duct and its supply terminal agree whichever side the kind puts it on.
        let unit = AhuParams { ports: vec![AirDirection::Supply; kind.default_ports().len()], ..unit_of(kind) };
        d.add_ahu(&Space::Model, Common::default(), &unit, Vec2::ZERO, 0.0).unwrap();
        add_duct(&mut d, (w / 2.0 + 150.0, 0.0), (w / 2.0 + 3000.0, 0.0));
        add_terminal(&mut d, (w / 2.0 + 3000.0, 0.0), 150.0);
        let net = d.duct_networks().unwrap().remove(0);
        assert_eq!(net.network.segments.len(), 1, "{kind:?}: a duct starting 150 mm off its face is connected");
    }
}

fn louvre(d: &mut Drawing, at: (f64, f64), flow: f64, air: AirDirection) -> Handle {
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "LV600x400").unwrap();
    let t = TerminalParams { model, flow_m3h: flow, air, label_height_mm: 100.0, units: HvacUnits::default() };
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(at.0, at.1), 0.0).unwrap()
}

/// A double-flux unit with its four connections, each a duct to the terminal of its own direction:
/// supply and extract above and below its right side, fresh air and exhaust on its left.
fn four_connections() -> Drawing {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &unit_of(AhuKind::DoubleFlow), Vec2::ZERO, 0.0).unwrap();
    // The unit is 2000 x 1000: its right face is at x = 1000, its left at x = -1000.
    add_duct(&mut d, (1100.0, 250.0), (5000.0, 250.0));
    add_terminal(&mut d, (5000.0, 250.0), 300.0);
    add_duct(&mut d, (5000.0, -250.0), (1100.0, -250.0));
    extract_terminal(&mut d, (5000.0, -250.0), 280.0);
    add_duct(&mut d, (-5000.0, 250.0), (-1100.0, 250.0));
    louvre(&mut d, (-5000.0, 250.0), 300.0, AirDirection::Fresh);
    add_duct(&mut d, (-1100.0, -250.0), (-5000.0, -250.0));
    louvre(&mut d, (-5000.0, -250.0), 280.0, AirDirection::Exhaust);
    d
}

#[test]
fn a_double_flux_unit_has_four_networks_supply_extract_fresh_air_and_exhaust() {
    let d = four_connections();
    let nets = d.duct_networks().unwrap();
    assert_eq!(
        nets.iter().map(|n| n.air).collect::<Vec<_>>(),
        [AirDirection::Supply, AirDirection::Extract, AirDirection::Fresh, AirDirection::Exhaust]
    );
    assert!(nets.iter().all(|n| n.network.segments.len() == 1 && n.unconnected.is_empty() && n.dangling.is_empty()), "{nets:?}");
    let flows: Vec<_> = nets.iter().map(|n| n.network.segments[0].terminal.map(|t| t.flow_m3h)).collect();
    assert_eq!(flows, [Some(300.0), Some(280.0), Some(300.0), Some(280.0)]);
}

#[test]
fn each_direction_gives_its_flow_to_the_unit_and_the_label_names_the_four() {
    let mut d = four_connections();
    for net in d.duct_networks().unwrap() {
        let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
        d.apply_duct_report(&net, &report).unwrap();
    }
    let unit = d.model.iter().find_map(|e| ahu_of(e)).unwrap();
    assert_eq!(
        (unit.flow_m3h, unit.extract_flow_m3h, unit.fresh_flow_m3h, unit.exhaust_flow_m3h),
        (Some(300.0), Some(280.0), Some(300.0), Some(280.0))
    );
    let text = text_of(&unit.generate().unwrap()).value;
    let all: String = unit
        .generate()
        .unwrap()
        .into_iter()
        .filter_map(|p| if let EntityKind::Text(t) = p.kind { Some(t.value) } else { None })
        .collect::<Vec<_>>()
        .join("|");
    assert!(all.contains("S 300 / E 280 / AN 300 / R 280 m³/h"), "{all} ({text})");
    // The ducts took the layer and colour of their direction.
    let layers: Vec<String> = d.model.iter().filter(|e| duct_of(e).is_some()).map(|e| e.common.layer.clone()).collect();
    for want in [DUCT_LAYER, DUCT_EXTRACT_LAYER, DUCT_FRESH_LAYER, DUCT_EXHAUST_LAYER] {
        assert!(layers.iter().any(|l| l == want), "{want}: {layers:?}");
    }
    use cadcraft_color::{Color, Rgb};
    assert_eq!(d.layer(DUCT_FRESH_LAYER).map(|l| l.color), Some(Color::True(Rgb(0x3c, 0xc8, 0x64))));
    assert_eq!(d.layer(DUCT_EXHAUST_LAYER).map(|l| l.color), Some(Color::True(Rgb(0xff, 0x9a, 0x1e))));
}

#[test]
fn an_arrow_on_each_duct_shows_the_way_the_air_runs() {
    let mut d = four_connections();
    for net in d.duct_networks().unwrap() {
        let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
        d.apply_duct_report(&net, &report).unwrap();
    }
    // The ducts were drawn: supply from the unit out; extract from its terminal in; fresh from its
    // louvre in; exhaust from the unit out. The air runs along each one's block x axis in all four.
    let ducts: Vec<_> = d.model.iter().filter_map(|e| duct_of(e).map(|p| (p.air, p.flow_forward))).collect();
    assert_eq!(ducts.len(), 4);
    for (air, forward) in &ducts {
        assert_eq!(*forward, Some(true), "{air:?}: drawn the way the air runs");
    }
    // A duct drawn the other way round has its arrow the other way: against its x axis.
    let mut e = Drawing::new_metric();
    e.add_ahu(&Space::Model, Common::default(), &unit_of(AhuKind::DoubleFlow), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut e, (5000.0, 250.0), (1100.0, 250.0));
    add_terminal(&mut e, (5000.0, 250.0), 300.0);
    let net = e.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    e.apply_duct_report(&net, &report).unwrap();
    let p = e.model.iter().find_map(|x| duct_of(x)).unwrap();
    assert_eq!(p.flow_forward, Some(false), "supply air leaves the unit, here toward -x");
    // And the arrow is drawn: three lines, the head at the way's end.
    let lines = p.generate(false).unwrap().iter().filter(|x| matches!(x.kind, EntityKind::Line(_))).count();
    assert!(lines >= 3, "the shaft and the two sides of the head");
    assert!(
        DuctParams { length: 3000.0, ..duct(200.0) }.generate(false).unwrap().iter().all(|x| !matches!(x.kind, EntityKind::Line(_))),
        "none before sizing"
    );
}

#[test]
fn a_louvre_is_drawn_with_arrows_of_its_direction_and_comes_in_standard_sizes() {
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "LV600x400").unwrap();
    assert_eq!((model.kind, model.neck_mm, model.max_flow_m3h), (TerminalKind::Louvre, 400.0, 3000.0));
    let t = |air| TerminalParams { model: model.clone(), flow_m3h: 500.0, air, label_height_mm: 100.0, units: HvacUnits::default() };
    for (air, layer) in [(AirDirection::Fresh, AIR_FRESH_LAYER), (AirDirection::Exhaust, AIR_EXHAUST_LAYER)] {
        let pieces = t(air).generate().unwrap();
        assert_eq!(pieces.iter().filter(|p| p.layer == layer).count(), 3, "one arrow: its shaft and two sides of the head");
        assert!(pieces.iter().all(|p| p.layer == AIR_TERMINAL_LAYER || p.layer == layer));
    }
    // Fresh air comes toward the face, exhaust goes away from it: the head is at the face for the first.
    let heads = |air: AirDirection| -> f64 {
        let lines: Vec<_> = t(air)
            .generate()
            .unwrap()
            .into_iter()
            .filter(|p| p.layer == AIR_FRESH_LAYER || p.layer == AIR_EXHAUST_LAYER)
            .filter_map(|p| if let EntityKind::Line(l) = p.kind { Some((l.a.y, l.b.y)) } else { None })
            .collect();
        // The side lines of the head start at the tip.
        lines[1].0
    };
    assert!(heads(AirDirection::Fresh) < heads(AirDirection::Exhaust), "fresh air's tip is at the face, exhaust's farther out");
    for (text, want) in [
        ("fresh", AirDirection::Fresh),
        ("air neuf", AirDirection::Fresh),
        ("rejet", AirDirection::Exhaust),
        ("exhaust", AirDirection::Exhaust),
        ("reprise", AirDirection::Extract),
    ] {
        assert_eq!(AirDirection::parse(text), Some(want), "{text}");
    }
    assert!(AirDirection::Supply.leaves_the_unit() && AirDirection::Exhaust.leaves_the_unit());
    assert!(!AirDirection::Extract.leaves_the_unit() && !AirDirection::Fresh.leaves_the_unit());
}

#[test]
fn a_unit_saved_without_fresh_air_and_exhaust_reads_back_and_a_duct_to_a_louvre_is_its_own_network() {
    let old = r#"{"name":"CTA1","widthMm":1500.0,"depthMm":800.0,"flowM3h":300.0}"#;
    let p: AhuParams = serde_json::from_str(old).unwrap();
    assert_eq!((p.fresh_flow_m3h, p.exhaust_pressure_pa), (None, None));
    // A duct from the unit to a louvre alone: fresh air, though the duct was left on auto.
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (-3000.0, 0.0), (-500.0, 0.0));
    louvre(&mut d, (-3000.0, 0.0), 400.0, AirDirection::Fresh);
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.air, AirDirection::Fresh);
    assert!(net.unconnected.is_empty(), "the louvre is reached");
}

#[test]
fn a_duct_on_a_collar_takes_the_direction_of_that_collar_and_the_roles_can_be_changed() {
    // The 1000 x 600 unit's collars: fresh air top left, exhaust bottom left, supply top right, extract bottom right.
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 300.0), (4000.0, 300.0));
    add_terminal(&mut d, (4000.0, 300.0), 300.0);
    add_duct(&mut d, (500.0, -300.0), (4000.0, -300.0));
    extract_terminal(&mut d, (4000.0, -300.0), 250.0);
    add_duct(&mut d, (-500.0, 300.0), (-4000.0, 300.0));
    louvre(&mut d, (-4000.0, 300.0), 300.0, AirDirection::Fresh);
    let airs = |d: &Drawing| d.duct_networks().unwrap().iter().map(|n| n.air).collect::<Vec<_>>();
    assert_eq!(airs(&d), [AirDirection::Supply, AirDirection::Extract, AirDirection::Fresh]);
    // Each collar is marked with an arrow and a tag of its direction.
    let marks = ahu().generate().unwrap();
    for layer in [AIR_SUPPLY_LAYER, AIR_EXTRACT_LAYER, AIR_FRESH_LAYER, AIR_EXHAUST_LAYER] {
        assert!(marks.iter().any(|p| p.layer == layer && matches!(p.kind, EntityKind::Text(_))), "{layer}");
    }
    // Turning the top right collar into an extract one: its supply terminal still wins, so nothing is left unread.
    let mut p = d.model.iter().find_map(|e| ahu_of(e)).unwrap();
    p.ports = vec![AirDirection::Fresh, AirDirection::Exhaust, AirDirection::Extract, AirDirection::Extract];
    let h = d.model.iter().find(|e| ahu_of(e).is_some()).unwrap().handle;
    d.regenerate_ahu(h, &p).unwrap();
    assert_eq!(airs(&d), [AirDirection::Supply, AirDirection::Extract, AirDirection::Fresh]);
    // A unit takes as many roles as it has collars.
    assert!(AhuParams { ports: vec![AirDirection::Supply], ..ahu() }.validate().is_err());
}

#[test]
fn a_duct_on_a_collar_with_no_terminal_is_an_open_end_of_that_collars_direction() {
    let flow = |n: &DuctNetwork| n.network.segments.first().and_then(|s| s.terminal).map(|t| t.flow_m3h);
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    add_duct(&mut d, (500.0, 300.0), (4000.0, 300.0));
    add_terminal(&mut d, (4000.0, 300.0), 300.0);
    add_duct(&mut d, (500.0, -300.0), (4000.0, -300.0));
    extract_terminal(&mut d, (4000.0, -300.0), 250.0);
    // Fresh air and exhaust end in the open (a wall passage drawn by someone else): no terminal there.
    add_duct(&mut d, (-500.0, 300.0), (-4000.0, 300.0));
    let exhaust = add_duct(&mut d, (-500.0, -300.0), (-4000.0, -300.0));
    let nets = d.duct_networks().unwrap();
    let got: Vec<_> = nets.iter().map(|n| (n.air, flow(n))).collect();
    // Fresh air brings what the supply blows, exhaust takes what the extract draws.
    assert_eq!(
        got,
        [
            (AirDirection::Supply, Some(300.0)),
            (AirDirection::Extract, Some(250.0)),
            (AirDirection::Fresh, Some(300.0)),
            (AirDirection::Exhaust, Some(250.0))
        ]
    );
    assert!(nets.iter().all(|n| n.dangling.is_empty()));
    // A flow set on the open duct wins over the balance.
    let (mut p, ..) = d.duct_ends(exhaust).unwrap();
    p.open_flow_m3h = Some(120.0);
    d.regenerate_duct(exhaust, &p).unwrap();
    assert_eq!(d.duct_networks().unwrap().iter().find(|n| n.air == AirDirection::Exhaust).and_then(flow), Some(120.0));
}

#[test]
fn open_ducts_with_nothing_to_balance_against_need_a_flow_and_are_left_out_until_they_have_one() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let supply = add_duct(&mut d, (500.0, 300.0), (4000.0, 300.0));
    let net = d.duct_networks().unwrap().remove(0);
    assert!(net.network.segments.is_empty(), "no terminal and no flow to give it");
    assert_eq!(net.dangling, [supply]);
    let (mut p, ..) = d.duct_ends(supply).unwrap();
    p.open_flow_m3h = Some(500.0);
    d.regenerate_duct(supply, &p).unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!((net.air, net.network.segments.len(), net.dangling.len()), (AirDirection::Supply, 1, 0));
}

#[test]
fn a_unit_writes_its_make_dimensions_and_weight_beside_it_and_keeps_them_in_its_data() {
    let mut p = ahu();
    assert!(p.spec_lines().is_empty(), "nothing is written for what is not known");
    p.model = "CIAT FLOWAY RHE 5000".into();
    p.width_mm = 1600.0;
    p.depth_mm = 1510.0;
    p.height_mm = Some(1659.0);
    p.weight_kg = Some(600.0);
    assert_eq!(p.spec_lines(), ["CIAT FLOWAY RHE 5000", "Dim. 1600x1510x1659H", "Poids 600kg"]);
    let texts = |p: &AhuParams| p.generate().unwrap().iter().filter(|x| matches!(x.kind, EntityKind::Text(_))).count();
    assert_eq!(texts(&p), texts(&ahu()) + 3);
    let mut d = Drawing::new_metric();
    let h = d.add_ahu(&Space::Model, Common::default(), &p, Vec2::ZERO, 0.0).unwrap();
    let back = ahu_of(d.entity(h).unwrap()).unwrap();
    assert_eq!((back.height_mm, back.weight_kg, back.model.as_str()), (Some(1659.0), Some(600.0), "CIAT FLOWAY RHE 5000"));
    assert!(AhuParams { height_mm: Some(0.0), ..ahu() }.validate().is_err());
    assert!(AhuParams { weight_kg: Some(-1.0), ..ahu() }.validate().is_err());
}

#[test]
fn a_compact_unit_has_its_collars_stacked_at_one_point_and_the_terminals_say_which_duct_is_which() {
    let compact = AhuParams { kind: AhuKind::StackedDoubleFlow, width_mm: 1600.0, depth_mm: 1510.0, ..ahu() };
    assert_eq!(AhuKind::parse("verticale"), Some(AhuKind::StackedDoubleFlow));
    // Exhaust over fresh air on the left, extract over supply on the right.
    assert_eq!(compact.roles(), [AirDirection::Exhaust, AirDirection::Fresh, AirDirection::Extract, AirDirection::Supply]);
    let tips = compact.port_tips();
    assert_eq!(tips[0].0, tips[1].0, "the two collars of the left end are at one point");
    assert_eq!(tips[2].0, tips[3].0, "and those of the right end");
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &compact, Vec2::ZERO, 0.0).unwrap();
    // Two ducts leave the same point, the first to an extract terminal, the second to a supply one.
    add_duct(&mut d, (800.0, 0.0), (4000.0, 300.0));
    extract_terminal(&mut d, (4000.0, 300.0), 250.0);
    add_duct(&mut d, (800.0, 0.0), (4000.0, -300.0));
    add_terminal(&mut d, (4000.0, -300.0), 300.0);
    let nets = d.duct_networks().unwrap();
    let mut airs: Vec<_> = nets.iter().map(|n| n.air).collect();
    airs.sort_by_key(|a| a.name());
    assert_eq!(airs, [AirDirection::Extract, AirDirection::Supply]);
    assert!(nets.iter().all(|n| n.network.segments.len() == 1 && n.dangling.is_empty()));
}

#[test]
fn two_open_ducts_on_stacked_collars_take_the_upper_and_the_lower_direction_in_turn() {
    let compact = AhuParams { kind: AhuKind::StackedDoubleFlow, width_mm: 1600.0, depth_mm: 1510.0, ..ahu() };
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &compact, Vec2::ZERO, 0.0).unwrap();
    // Extract and supply, each with a flow set on its open end: the first duct takes the upper collar's direction.
    let a = add_duct(&mut d, (800.0, 0.0), (4000.0, 300.0));
    let b = add_duct(&mut d, (800.0, 0.0), (4000.0, -300.0));
    for h in [a, b] {
        let (mut p, ..) = d.duct_ends(h).unwrap();
        p.open_flow_m3h = Some(400.0);
        d.regenerate_duct(h, &p).unwrap();
    }
    let nets = d.duct_networks().unwrap();
    let got: Vec<_> = nets.iter().map(|n| (n.air, n.ducts.clone())).collect();
    assert_eq!(got, [(AirDirection::Supply, vec![b]), (AirDirection::Extract, vec![a])]);
}

fn riser_at(d: &mut Drawing, at: (f64, f64), air: AirDirection, incoming: Option<f64>) -> Handle {
    let mut p = RiserParams {
        name: "VMC 3".into(),
        air,
        up: true,
        outline: vec![Vec2::new(0.0, 0.0), Vec2::new(600.0, 0.0), Vec2::new(600.0, 400.0), Vec2::new(0.0, 400.0)],
        incoming_flow_m3h: incoming,
        flow_m3h: None,
        size: None,
        size_set: false,
        height_mm: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    };
    for v in &mut p.outline {
        *v += Vec2::new(at.0, at.1);
    }
    d.add_riser(&Space::Model, Common::default(), p).unwrap()
}

#[test]
fn a_riser_is_where_the_ducts_of_a_floor_start_and_it_carries_the_floor_plus_the_other_floors() {
    let mut d = Drawing::new_metric();
    let r = riser_at(&mut d, (0.0, 0.0), AirDirection::Extract, Some(660.0));
    // Two extract terminals on a duct that starts in the shaft: 250 + 150 m³/h on this floor.
    add_duct(&mut d, (600.0, 200.0), (5000.0, 200.0));
    extract_terminal(&mut d, (5000.0, 200.0), 250.0);
    add_duct(&mut d, (3000.0, 200.0), (3000.0, 2000.0));
    extract_terminal(&mut d, (3000.0, 2000.0), 150.0);
    d.split_duct_taps(&Space::Model).unwrap();
    let nets = d.duct_networks().unwrap();
    assert_eq!(nets.len(), 1, "the riser is the only origin, and it has no empty twin");
    assert_eq!((nets[0].ahu, nets[0].air, nets[0].dangling.len()), (r, AirDirection::Extract, 0));
    // Sizing: the report's flow is the floor's, the riser adds the one typed from the other floors.
    let report = cadcraft_hvac::size_network(&nets[0].network, &DesignCriteria::default()).unwrap();
    assert_eq!(report.total_flow_m3h, 400.0);
    d.apply_duct_report(&nets[0], &report).unwrap();
    let p = riser_of(d.entity(r).unwrap()).unwrap();
    assert_eq!(p.flow_m3h, Some(1060.0));
    // 1060 m³/h in a 600 x 400 shaft less the margin: a round duct of the short side's room or a flat one.
    assert!(p.size.is_some_and(|s| s.width_mm() <= 500.0 || matches!(s, RiserSize::Rectangular { .. })), "{:?}", p.size);
    // A size typed is kept.
    let mut typed = p.clone();
    (typed.size, typed.size_set) = (Some(RiserSize::Round { diameter_mm: 250.0 }), true);
    d.regenerate_riser(r, &typed).unwrap();
    d.apply_duct_report(&nets[0], &report).unwrap();
    assert_eq!(riser_of(d.entity(r).unwrap()).unwrap().size, Some(RiserSize::Round { diameter_mm: 250.0 }));
}

#[test]
fn on_the_roof_the_ducts_of_a_unit_end_on_the_riser_with_the_flow_typed_on_it() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // The unit's extract collar (right, bottom of the default double-flux) to a riser that takes 900 m³/h.
    let r = riser_at(&mut d, (4000.0, -500.0), AirDirection::Extract, Some(900.0));
    add_duct(&mut d, (500.0, -300.0), (4100.0, -300.0));
    let nets = d.duct_networks().unwrap();
    assert_eq!(nets.len(), 1, "the unit reads the duct, the riser has none left: {nets:?}");
    assert_eq!((nets[0].air, nets[0].network.segments.len(), nets[0].dangling.len()), (AirDirection::Extract, 1, 0));
    assert_eq!(nets[0].network.segments[0].terminal.map(|t| t.flow_m3h), Some(900.0));
    assert_ne!(nets[0].ahu, r);
}

#[test]
fn a_stub_past_a_tee_leads_nowhere_but_takes_the_direction_of_the_network_it_hangs_on() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // An extract run from the lower right collar to a terminal, with a dead stub of 100 mm drawn
    // past a tee (left on the default, supply direction).
    add_duct(&mut d, (500.0, -300.0), (4000.0, -300.0));
    extract_terminal(&mut d, (4000.0, -300.0), 250.0);
    let stub = add_duct(&mut d, (2000.0, -300.0), (2000.0, -200.0));
    d.split_duct_taps(&Space::Model).unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!((net.air, net.dangling.as_slice()), (AirDirection::Extract, [stub].as_slice()));
    let report = cadcraft_hvac::size_network(&net.network, &DesignCriteria::default()).unwrap();
    assert_eq!(duct_of(d.entity(stub).unwrap()).unwrap().air, AirDirection::Supply, "it is not read yet");
    d.apply_duct_report(&net, &report).unwrap();
    assert_eq!(duct_of(d.entity(stub).unwrap()).unwrap().air, AirDirection::Extract, "it takes the colour of its network");
}

#[test]
fn a_vmc_box_only_extracts_and_exhausts() {
    let roles = AhuParams { kind: AhuKind::VmcBox, ..ahu() }.roles();
    assert!(roles.iter().all(|r| matches!(r, AirDirection::Extract | AirDirection::Exhaust)), "{roles:?}");
    assert!(roles.contains(&AirDirection::Extract) && roles.contains(&AirDirection::Exhaust));
}

#[test]
fn ducts_that_cross_are_found_a_tee_or_a_corner_is_not_one_and_the_one_that_passes_under_is_drawn_broken() {
    let mut d = Drawing::new_metric();
    let over = add_duct(&mut d, (0.0, 0.0), (4000.0, 0.0));
    let under = add_duct(&mut d, (2000.0, -1500.0), (2000.0, 1500.0));
    // A tee (ends on the first one) and a corner (ends at its end) are junctions, not crossings.
    add_duct(&mut d, (1000.0, 0.0), (1000.0, 1500.0));
    add_duct(&mut d, (4000.0, 0.0), (4000.0, 1500.0));
    let all = d.duct_crossings();
    assert_eq!(all.len(), 1, "{all:?}");
    assert_eq!((all[0].at, all[0].resolved), (Vec2::new(2000.0, 0.0), false));
    assert!([all[0].a, all[0].b].contains(&over) && [all[0].a, all[0].b].contains(&under));
    // Under: the width of the other (200) plus 30 mm on each side, across the crossing at 1500 mm from its start.
    let polylines = |d: &Drawing, h: Handle| {
        let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { return 0 };
        d.block(&i.block).unwrap().entities.iter().filter(|x| matches!(x.kind, EntityKind::LwPolyline(_))).count()
    };
    assert_eq!(polylines(&d, under), 2, "two sides, whole");
    let gap = d.pass_duct_under(under, over, 30.0).unwrap();
    assert_eq!(gap, [1500.0 - 130.0, 1500.0 + 130.0]);
    assert_eq!(polylines(&d, under), 4, "each side is in two pieces");
    assert!(d.duct_crossings()[0].resolved);
    // Ducts that do not cross are refused; a gap is kept when the duct is cut, each part keeping its own.
    let tee = d.model.iter().map(|e| e.handle).nth(2).unwrap();
    assert!(d.pass_duct_under(under, tee, 30.0).is_err());
    let second = d.split_duct_at(&Space::Model, under, Vec2::new(2000.0, 500.0), 10.0).unwrap().unwrap();
    let (first, rest) = (duct_of(d.entity(under).unwrap()).unwrap(), duct_of(d.entity(second).unwrap()).unwrap());
    assert_eq!((first.gaps_mm.clone(), rest.gaps_mm.len()), (vec![[1370.0, 1630.0]], 0), "the cut at 2000 is past the gap");
    assert!(DuctParams { gaps_mm: vec![[5.0, 1.0]], ..duct(200.0) }.validate().is_err());
}

#[test]
fn a_duct_that_goes_over_another_makes_four_45_degree_elbows_that_are_drawn_counted_and_kept_when_it_is_cut() {
    let mut d = Drawing::new_metric();
    let other = add_duct(&mut d, (0.0, 0.0), (6000.0, 0.0));
    let duct = add_duct(&mut d, (3000.0, -3000.0), (3000.0, 3000.0));
    let polylines = |d: &Drawing, h: Handle| {
        let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { return 0 };
        d.block(&i.block).unwrap().entities.iter().filter(|x| matches!(x.kind, EntityKind::LwPolyline(_))).count()
    };
    assert_eq!(polylines(&d, duct), 2);
    // Both 200 mm: it rises 200 + 50 = 250 mm, which takes 250 mm of length on each side of the flat
    // part, as wide as the other duct (200) and 30 mm of clearance each side.
    let jog = d.jog_duct_around(duct, other, false, None, 30.0).unwrap();
    assert_eq!((jog.centre_mm, jog.half_mm, jog.shift_mm, jog.down), (3000.0, 130.0, 250.0, false));
    assert_eq!(jog.elbows_mm(), [2620.0, 2870.0, 3130.0, 3380.0]);
    assert_eq!(polylines(&d, duct), 2 + 4, "the sides and the four elbows");
    // A dip is the same elbows the other way, labelled with the minus sign.
    let dip = d.jog_duct_around(duct, other, true, Some(300.0), 30.0).unwrap();
    assert!(dip.down && dip.shift_mm == 300.0);
    assert_eq!(duct_of(d.entity(duct).unwrap()).unwrap().jogs.len(), 1, "a new jog replaces the one it overlaps");
    // Too near an end for four elbows: refused. Ducts that do not cross: refused.
    let short = add_duct(&mut d, (500.0, -300.0), (500.0, 300.0));
    assert!(d.jog_duct_around(short, other, false, None, 30.0).is_err(), "the crossing is at 300 mm from the end, the four elbows need 380");
    let apart = add_duct(&mut d, (0.0, 2000.0), (5000.0, 2000.0));
    let apart2 = add_duct(&mut d, (0.0, 2500.0), (5000.0, 2500.0));
    assert!(d.jog_duct_around(apart, apart2, false, None, 30.0).is_err());
    // Cut on the side of the flat part: the jog stays on the part that holds it.
    let second = d.split_duct_at(&Space::Model, duct, Vec2::new(3000.0, 1500.0), 10.0).unwrap().unwrap();
    assert_eq!((duct_of(d.entity(duct).unwrap()).unwrap().jogs.len(), duct_of(d.entity(second).unwrap()).unwrap().jogs.len()), (1, 0));
    assert!(
        DuctParams { jogs: vec![DuctJog { centre_mm: 100.0, half_mm: 10.0, shift_mm: 250.0, down: false }], ..duct_with_length(1000.0) }
            .validate()
            .is_err()
    );
}

fn duct_with_length(mm: f64) -> DuctParams {
    DuctParams { length: mm, ..duct(200.0) }
}

#[test]
fn the_four_elbows_of_a_jog_are_in_the_losses_of_the_run() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    let run = add_duct(&mut d, (500.0, -300.0), (6000.0, -300.0));
    extract_terminal(&mut d, (6000.0, -300.0), 250.0);
    let elbows = |d: &Drawing| {
        let net = d.duct_networks().unwrap().remove(0);
        net.network.segments[0].fittings.iter().filter(|f| **f == Fitting::Elbow45).count()
    };
    assert_eq!(elbows(&d), 0);
    let (mut p, ..) = d.duct_ends(run).unwrap();
    p.jogs = vec![DuctJog { centre_mm: 3000.0, half_mm: 130.0, shift_mm: 250.0, down: false }];
    d.regenerate_duct(run, &p).unwrap();
    assert_eq!(elbows(&d), 4);
}

#[test]
fn a_compact_unit_shows_one_collar_at_each_end_as_wide_as_its_orifices() {
    let polys = |p: &AhuParams| -> Vec<Vec<Vec2>> {
        p.generate()
            .unwrap()
            .into_iter()
            .filter_map(|x| match x.kind {
                EntityKind::LwPolyline(l) => Some(l.vertices.iter().map(|v| v.p).collect()),
                _ => None,
            })
            .collect()
    };
    let double = AhuParams { kind: AhuKind::DoubleFlow, width_mm: 1600.0, depth_mm: 1510.0, ..ahu() };
    let compact = AhuParams { kind: AhuKind::StackedDoubleFlow, ..double.clone() };
    // The casing twice, then the collars: four on a double-flux unit, two (one per end) on a compact one.
    assert_eq!((polys(&double).len(), polys(&compact).len()), (6, 4));
    // The orifices (right end: 1000 x 400 over a round 250) make the right collar 1000 mm wide, as plans draw it.
    let sized = AhuParams {
        port_sizes: vec![
            None,
            None,
            Some(RiserSize::Rectangular { width_mm: 1000.0, height_mm: 400.0 }),
            Some(RiserSize::Round { diameter_mm: 250.0 }),
        ],
        ..compact.clone()
    };
    let right = polys(&sized).into_iter().find(|p| p.iter().all(|v| v.x >= 799.9)).unwrap();
    let (lo, hi) = right.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v.y), hi.max(v.y)));
    assert_eq!(hi - lo, 1000.0);
    assert!(AhuParams { port_sizes: vec![None; 5], ..compact }.validate().is_err(), "one size per collar");
}

fn add_wide(d: &mut Drawing, a: (f64, f64), b: (f64, f64), diameter: f64) -> Handle {
    d.add_duct(&Space::Model, Common::default(), duct(diameter), Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).unwrap()
}

#[test]
fn a_duct_running_inside_the_width_of_another_passes_under_it_along_the_stretch_they_share() {
    let polylines = |d: &Drawing, h: Handle| {
        let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { return 0 };
        d.block(&i.block).unwrap().entities.iter().filter(|x| matches!(x.kind, EntityKind::LwPolyline(_))).count()
    };
    // Stacked collars: the supply (200) leaves in the width of the extract (650), 2000 of its 3000 mm, then turns away.
    let mut d = Drawing::new_metric();
    let big = add_wide(&mut d, (0.0, 0.0), (0.0, -3000.0), 650.0);
    let small = add_wide(&mut d, (0.0, 0.0), (0.0, -2000.0), 200.0);
    let turn = add_wide(&mut d, (0.0, -2000.0), (-3000.0, -2000.0), 200.0);
    let found = d.duct_crossings();
    assert_eq!(found.len(), 1, "the horizontal one starts on the axis: a corner, not a crossing; {found:?}");
    assert!(found[0].overlap && !found[0].resolved);
    assert_eq!((found[0].a, found[0].b), (small, big), "the narrower is the one that can lie under");
    assert!(found[0].at.near(Vec2::new(0.0, -1000.0), 1e-6), "the middle of the stretch: {:?}", found[0].at);
    // It lies under along the whole stretch: no side is drawn.
    assert_eq!(polylines(&d, small), 2);
    let gap = d.pass_duct_under(small, big, 30.0).unwrap();
    assert_eq!(gap, [0.0, 2000.0]);
    // Drawn whole (2 sides) with its hidden sides dashed (2): the mask over the other duct hides them while its layer is shown.
    assert_eq!(polylines(&d, small), 4, "wholly under the other");
    assert!(d.duct_crossings()[0].resolved);
    // The one that turns away comes out from under the other's edge: from its axis to its side, with the clearance.
    let gap = d.pass_duct_under(turn, big, 30.0).unwrap();
    assert!(gap[0] == 0.0 && (gap[1] - 355.0).abs() < 1e-6, "{gap:?}");
    // Named the wrong way round (the wide one first), the narrower is the one drawn under.
    let again = add_wide(&mut d, (0.0, -2500.0), (0.0, -2900.0), 200.0);
    let g = d.pass_duct_under(big, again, 30.0).unwrap();
    assert_eq!(g, [2500.0, 2900.0].map(|v| v - 2500.0).map(|v| v), "along the narrower duct");
    assert_eq!(duct_of(d.entity(again).unwrap()).unwrap().gaps_mm.len(), 1);
    assert!(duct_of(d.entity(big).unwrap()).unwrap().gaps_mm.is_empty(), "the wide one is never drawn under the narrow one");
    // A duct whose axis is inside the width lies under it, though a side sticks out a little (snapped to an edge) ...
    let sticking = add_wide(&mut d, (250.0, 0.0), (250.0, -2000.0), 200.0);
    assert!(d.pass_duct_under(sticking, big, 30.0).is_ok());
    // ... one whose axis is outside does not.
    let aside = add_wide(&mut d, (400.0, 0.0), (400.0, -2000.0), 200.0);
    assert!(d.pass_duct_under(aside, big, 30.0).is_err());
    // Two ducts that cross can be swapped: the one drawn broken is whole again.
    let mut x = Drawing::new_metric();
    let h = add_wide(&mut x, (-2000.0, 0.0), (2000.0, 0.0), 200.0);
    let v = add_wide(&mut x, (0.0, -2000.0), (0.0, 2000.0), 200.0);
    x.pass_duct_under(v, h, 30.0).unwrap();
    assert_eq!((duct_of(x.entity(v).unwrap()).unwrap().gaps_mm.len(), duct_of(x.entity(h).unwrap()).unwrap().gaps_mm.len()), (1, 0));
    x.pass_duct_under(h, v, 30.0).unwrap();
    assert_eq!((duct_of(x.entity(v).unwrap()).unwrap().gaps_mm.len(), duct_of(x.entity(h).unwrap()).unwrap().gaps_mm.len()), (0, 1), "swapped");
}

#[test]
fn a_change_of_level_between_runs_counts_two_elbows_in_the_losses_and_a_run_too_short_for_it_is_named() {
    let level = |d: &mut Drawing, h: Handle, mm: f64| {
        let (mut p, ..) = d.duct_ends(h).unwrap();
        p.level_mm = Some(mm);
        d.regenerate_duct(h, &p).unwrap();
    };
    let build = |second_end: f64, second_level: f64| {
        let mut d = Drawing::new_metric();
        d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
        let a = add_duct(&mut d, (500.0, -300.0), (2000.0, -300.0));
        let b = add_duct(&mut d, (2000.0, -300.0), (second_end, -300.0));
        extract_terminal(&mut d, (second_end, -300.0), 250.0);
        level(&mut d, a, 2500.0);
        level(&mut d, b, second_level);
        (d, b)
    };
    let elbows = |d: &Drawing| -> Vec<usize> {
        let net = d.duct_networks().unwrap().remove(0);
        net.network.segments.iter().map(|s| s.fittings.iter().filter(|f| **f == Fitting::Elbow45).count()).collect()
    };
    // The second run leaves the first 300 mm higher: two 45° elbows on it, none on the first.
    let (mut d, b) = build(5000.0, 2800.0);
    assert_eq!(elbows(&d), vec![0, 2]);
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    assert!(d.apply_duct_report(&net, &report).unwrap().is_empty());
    let step = duct_of(d.entity(b).unwrap()).unwrap().steps;
    assert_eq!(step.len(), 1);
    assert!((step[0].shift_mm - 300.0).abs() < 1e-9 && step[0].at_mm == 0.0);
    // The same level, or no level on one of them, makes no step.
    let (mut same, b2) = build(5000.0, 2800.0);
    level(&mut same, b2, 2500.0);
    assert_eq!(elbows(&same), vec![0, 0]);
    // A run of 600 mm cannot take a change of 600 mm and its margin: it is named, and nothing is drawn.
    let (mut short, b3) = build(2600.0, 3100.0);
    let net = short.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    let warnings = short.apply_duct_report(&net, &report).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("too short") && warnings[0].contains("600"), "{warnings:?}");
    assert!(duct_of(short.entity(b3).unwrap()).unwrap().steps.is_empty());
    // A step is drawn: two elbows across the run and the change written.
    let drawn = d.entity(b).and_then(|e| if let EntityKind::Insert(i) = &e.kind { d.block(&i.block) } else { None }).unwrap();
    assert!(drawn.entities.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "+300")));
    assert!(
        DuctParams { steps: vec![DuctStep { at_mm: 0.0, shift_mm: 5.0 }], ..duct_with_length(1000.0) }.validate().is_err(),
        "a step is 20 mm or more"
    );
}

#[test]
fn the_lower_of_two_ducts_passes_under_by_their_altitudes_a_duct_inside_another_only_when_it_is_the_lower() {
    let at = |d: &mut Drawing, h: Handle, mm: Option<f64>| {
        let (mut p, ..) = d.duct_ends(h).unwrap();
        p.level_mm = mm;
        d.regenerate_duct(h, &p).unwrap();
    };
    let mut d = Drawing::new_metric();
    let over = add_duct(&mut d, (-2000.0, 0.0), (2000.0, 0.0));
    let under = add_duct(&mut d, (0.0, -2000.0), (0.0, 2000.0));
    assert_eq!(d.cross_ducts_by_altitude(30.0).unwrap(), 0, "no altitude, no choice");
    at(&mut d, over, Some(2800.0));
    at(&mut d, under, Some(2500.0));
    assert_eq!(d.cross_ducts_by_altitude(30.0).unwrap(), 1);
    assert_eq!((duct_of(d.entity(under).unwrap()).unwrap().gaps_mm.len(), duct_of(d.entity(over).unwrap()).unwrap().gaps_mm.len()), (1, 0));
    assert_eq!(d.cross_ducts_by_altitude(30.0).unwrap(), 0, "done once");
    // Inside the width of a wider duct: hidden only if it is the lower one.
    let mut o = Drawing::new_metric();
    let wide = add_wide(&mut o, (0.0, 0.0), (0.0, -3000.0), 650.0);
    let thin = add_wide(&mut o, (0.0, 0.0), (0.0, -2000.0), 200.0);
    at(&mut o, wide, Some(2500.0));
    at(&mut o, thin, Some(2800.0));
    assert_eq!(o.cross_ducts_by_altitude(30.0).unwrap(), 0, "the narrow one is above: it is not hidden");
    at(&mut o, thin, Some(2000.0));
    assert_eq!(o.cross_ducts_by_altitude(30.0).unwrap(), 1);
}

#[test]
fn the_takeoff_counts_fittings_accessories_risers_and_units_of_the_level() {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &ahu(), Vec2::ZERO, 0.0).unwrap();
    // A trunk with a tee: the run to the right, a branch going up from its middle, a terminal at the end of each.
    add_duct(&mut d, (500.0, -300.0), (4000.0, -300.0));
    add_terminal(&mut d, (4000.0, -300.0), 200.0);
    add_duct(&mut d, (2250.0, -300.0), (2250.0, 2500.0));
    add_terminal(&mut d, (2250.0, 2500.0), 150.0);
    d.split_duct_taps(&Space::Model).unwrap();
    let q = d.takeoff("R+2").unwrap();
    assert_eq!(q.level, "R+2");
    let count = |list: &[CountQuantity], name: &str| list.iter().filter(|c| c.name == name).map(|c| c.count).sum::<usize>();
    assert_eq!((count(&q.fittings, "Tee, straight"), count(&q.fittings, "Tee, branch")), (1, 1), "{:?}", q.fittings);
    assert!(
        q.fittings.iter().any(|c| c.name == "Elbow 90°" || c.name == "Elbow 45°") || q.fittings.iter().all(|c| c.name.starts_with("Tee")),
        "{:?}",
        q.fittings
    );
    assert_eq!((q.totals.duct_count, q.totals.terminals), (3, 2), "the trunk was cut where the branch starts");
    assert_eq!(q.terminals.len(), 1, "one model, two terminals: one line");
    assert_eq!((q.terminals[0].count, q.terminals[0].flow_m3h), (2, 350.0));
    assert_eq!(q.units.len(), 1);
    assert_eq!(q.units[0].name, "CTA1");
    // A reducer is counted with the sizes it joins; an accessory by kind and diameter.
    let (mut p, ..) = d.duct_ends(d.model.iter().filter_map(|e| d.duct_ends(e.handle).map(|_| e.handle)).nth(1).unwrap()).unwrap();
    p.reducer = Some(DuctReducer { from_mm: 315.0, at_end: false });
    let h = d.model.iter().filter_map(|e| d.duct_ends(e.handle).map(|_| e.handle)).nth(1).unwrap();
    d.regenerate_duct(h, &p).unwrap();
    let q = d.takeoff("").unwrap();
    assert!(q.fittings.iter().any(|c| c.name == "Reducer" && c.size.starts_with("315 → ")), "{:?}", q.fittings);
    // CSV: cells separated by `;`, a cell with a `;` or a quote is quoted.
    let csv = q.to_csv(false);
    assert!(csv.starts_with("Level;\n") && csv.contains("Ducts\n"), "{csv}");
    let mut odd = q.clone();
    odd.level = "a;\"b\"".into();
    assert!(odd.to_csv(true).starts_with("Level;\"a;\"\"b\"\"\"\n"));
}

#[test]
fn a_supply_duct_lying_on_the_extract_duct_at_stacked_collars_is_not_read_as_part_of_it() {
    // The roof: a compact unit, an extract run (wide) and a supply run (narrow) both leave its collar,
    // one over the other, as far as M; there the extract goes straight on to its riser, the supply turns to its own.
    let compact = AhuParams { kind: AhuKind::StackedDoubleFlow, width_mm: 1600.0, depth_mm: 1510.0, ..ahu() };
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &compact, Vec2::ZERO, 0.0).unwrap();
    let run = |d: &mut Drawing, air: AirDirection, w: f64, a: (f64, f64), b: (f64, f64)| {
        let p = DuctParams { air, ..duct(w) };
        d.add_duct(&Space::Model, Common::default(), p, Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).unwrap()
    };
    let (e1, s1) = (
        run(&mut d, AirDirection::Extract, 650.0, (800.0, 0.0), (3000.0, 0.0)),
        run(&mut d, AirDirection::Supply, 200.0, (800.0, 0.0), (3000.0, 0.0)),
    );
    let e2 = run(&mut d, AirDirection::Extract, 650.0, (3000.0, 0.0), (7000.0, 0.0));
    let s2 = run(&mut d, AirDirection::Supply, 200.0, (3000.0, 0.0), (3000.0, -4000.0));
    riser_at(&mut d, (6900.0, -300.0), AirDirection::Extract, Some(7000.0));
    riser_at(&mut d, (2900.0, -4100.0), AirDirection::Supply, Some(7000.0));
    let nets = d.duct_networks().unwrap();
    let mut got: Vec<_> = nets
        .iter()
        .map(|n| {
            (
                n.air,
                {
                    let mut v = n.ducts.clone();
                    v.sort();
                    v
                },
                n.dangling.len(),
            )
        })
        .collect();
    got.sort_by_key(|g| g.0 as u8);
    let mut sup = vec![s1, s2];
    let mut ext = vec![e1, e2];
    sup.sort();
    ext.sort();
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got.contains(&(AirDirection::Supply, sup, 0)) && got.contains(&(AirDirection::Extract, ext, 0)), "{got:?}");
}

#[test]
fn a_stretch_under_another_duct_inside_an_elbows_trim_does_not_hide_the_run() {
    // Under the other duct only where the run bends (the last 355 mm, of which the elbow trims 710): the sides stay.
    let e = DuctElbow { angle_deg: -90.0, radius_mm: 710.0, at_end: true };
    let mut p = DuctParams { length: 12000.0, elbow: Some(e), trim_end_mm: 710.0, gaps_mm: vec![[11645.0, 12000.0]], ..duct(710.0) };
    assert_eq!(polylines(&p.generate(false).unwrap()).len(), 2, "both sides drawn");
    // A stretch that does cut the straight part still cuts it.
    p.gaps_mm = vec![[5000.0, 6000.0]];
    assert_eq!(polylines(&p.generate(false).unwrap()).len(), 4, "two stretches, two sides each");
}

#[test]
fn a_bend_mostly_under_another_duct_is_not_drawn() {
    let e = DuctElbow { angle_deg: -90.0, radius_mm: 710.0, at_end: true };
    let run = |gap: [f64; 2]| DuctParams { length: 12000.0, elbow: Some(e), trim_end_mm: 710.0, gaps_mm: vec![gap], ..duct(710.0) };
    assert_eq!(arcs(&run([11000.0, 11100.0]).generate(false).unwrap()).len(), 2, "the bend is clear of it");
    assert!(arcs(&run([11645.0, 12000.0]).generate(false).unwrap()).is_empty(), "half of the bend or more is under it");
}

#[test]
fn the_orifice_of_a_collar_is_found_in_a_drawing_in_metres_with_the_unit_turned() {
    // The unit's collars sit 0.1 m from the duct ends: in metres that is far less than a millimetre-sized threshold.
    let metres = HvacUnits { mm_per_unit: 1000.0 };
    let sized = Some(RiserSize::Rectangular { width_mm: 1580.0, height_mm: 776.0 });
    let compact =
        AhuParams { kind: AhuKind::StackedDoubleFlow, width_mm: 1600.0, depth_mm: 1810.0, port_sizes: vec![sized; 4], units: metres, ..ahu() };
    let mut d = Drawing::new_metric();
    d.header.set_i64("INSUNITS", 6);
    d.add_ahu(&Space::Model, Common::default(), &compact, Vec2::new(0.0, 5.0), 3.0 * std::f64::consts::FRAC_PI_2).unwrap();
    let run = |d: &mut Drawing, air: AirDirection, w: f64, a: (f64, f64), b: (f64, f64)| {
        let p = DuctParams { air, units: metres, ..duct(w) };
        d.add_duct(&Space::Model, Common::default(), p, Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)).unwrap()
    };
    // Both leave the lower face (collars at y = 5 - 0.9 - 0.1), the ducts ending 0.1 m from them.
    let ext = run(&mut d, AirDirection::Extract, 650.0, (0.0, 1.0), (0.0, 4.1));
    let sup = run(&mut d, AirDirection::Supply, 200.0, (0.0, 1.0), (0.0, 4.1));
    riser_at(&mut d, (-0.3, 0.8), AirDirection::Extract, Some(7000.0));
    let nets = d.duct_networks().unwrap();
    let widths: Vec<_> =
        nets.iter().flat_map(|n| n.ducts.iter().zip(&n.collar_widths_mm)).filter(|(h, _)| **h == ext || **h == sup).map(|(_, w)| *w).collect();
    assert!(!widths.is_empty() && widths.iter().all(|w| *w == Some(1580.0)), "{widths:?}");
}

#[test]
fn a_duct_passing_under_is_drawn_whole_with_a_mask_and_dashes_on_the_layer_of_the_duct_over_it() {
    let mut d = Drawing::new_metric();
    let under = add_wide(&mut d, (-2000.0, 0.0), (2000.0, 0.0), 200.0);
    let over = {
        let p = DuctParams { air: AirDirection::Extract, air_set: true, ..duct(400.0) };
        d.add_duct(&Space::Model, Common::default(), p, Vec2::new(0.0, -2000.0), Vec2::new(0.0, 2000.0)).unwrap()
    };
    // Something else drawn after both: the duct over must still end up above the one under.
    add_wide(&mut d, (5000.0, 5000.0), (6000.0, 5000.0), 200.0);
    d.pass_duct_under(under, over, 30.0).unwrap();
    let EntityKind::Insert(i) = &d.entity(under).unwrap().kind else { panic!("not a block reference") };
    let block = d.block(&i.block).unwrap();
    let on = |layer: &str, f: &dyn Fn(&EntityKind) -> bool| block.entities.iter().filter(|e| e.common.layer == layer && f(&e.kind)).count();
    let sides = block.entities.iter().filter(|e| matches!(e.kind, EntityKind::LwPolyline(_)) && e.common.linetype == "ByLayer").count();
    assert_eq!(sides, 2, "the duct is whole");
    assert_eq!(on("M-HVAC-DUCT-EXTR", &|k| matches!(k, EntityKind::Wipeout(_))), 1, "one mask, on the layer of the duct over it");
    let dashed: Vec<_> = block.entities.iter().filter(|e| e.common.linetype == "HVAC_HIDDEN").collect();
    assert_eq!(dashed.len(), 2, "the two hidden sides");
    assert!(dashed.iter().all(|e| e.common.layer == "M-HVAC-DUCT-EXTR"));
    assert!(d.linetypes.iter().any(|l| l.name == "HVAC_HIDDEN"));
    assert_eq!(d.header.i64("WIPEOUTFRAME", 1), 0, "no frame round the masks");
    // The duct over comes after the one under in the draw order.
    let order: Vec<_> = d.model.iter().map(|e| e.handle).collect();
    assert!(order.iter().position(|h| *h == over) > order.iter().position(|h| *h == under));
    // The dashes cover the width of the duct over it (400 mm), the mask that and the clearance.
    let (p, ..) = d.duct_ends(under).unwrap();
    assert_eq!(p.covers.len(), 1);
    assert!((p.covers[0].dash_mm[1] - p.covers[0].dash_mm[0] - 400.0).abs() < 1e-6, "{:?}", p.covers[0]);
    assert!(p.covers[0].gap_mm[1] - p.covers[0].gap_mm[0] > 400.0);
    // When the duct over changes its air direction, the mask follows its layer.
    let (mut po, ..) = d.duct_ends(over).unwrap();
    po.air = AirDirection::Supply;
    d.regenerate_duct(over, &po).unwrap();
    let EntityKind::Insert(i) = &d.entity(under).unwrap().kind else { panic!("not a block reference") };
    let block = d.block(&i.block).unwrap();
    assert!(block.entities.iter().any(|e| matches!(e.kind, EntityKind::Wipeout(_)) && e.common.layer == "M-HVAC-DUCT-SUP"));
    assert!(!block.entities.iter().any(|e| matches!(e.kind, EntityKind::Wipeout(_)) && e.common.layer == "M-HVAC-DUCT-EXTR"));
    // Clearing the crossing draws the duct plain again.
    let (mut p, ..) = d.duct_ends(under).unwrap();
    (p.gaps_mm, p.covers) = (Vec::new(), Vec::new());
    d.regenerate_duct(under, &p).unwrap();
    let EntityKind::Insert(i) = &d.entity(under).unwrap().kind else { panic!("not a block reference") };
    assert!(!d.block(&i.block).unwrap().entities.iter().any(|e| matches!(e.kind, EntityKind::Wipeout(_))));
}

#[test]
fn the_label_of_a_run_under_another_duct_sits_beside_it_and_not_under_the_other_duct_s_label() {
    let text_y = |p: &DuctParams, flip: bool| -> f64 {
        let pieces = p.generate(flip).unwrap();
        let t = pieces.iter().find_map(|x| match &x.kind {
            EntityKind::Text(t) if t.value.contains("Ø") || t.value.contains('m') => Some(t.insert.y),
            _ => None,
        });
        t.expect("a label")
    };
    let mut p = duct(400.0);
    p.units = HvacUnits { mm_per_unit: 1.0 };
    p.length = 4000.0;
    p.label_height_mm = 100.0;
    assert!(text_y(&p, false).abs() < 1e-9, "alone, the label is on the run");
    // Under another duct along its middle: the label is beside the run, past its side (200) by a bit more than a text's height.
    p.covers = vec![CoverMark {
        gap_mm: [1000.0, 3000.0],
        dash_mm: [1200.0, 2800.0],
        over_air: AirDirection::Extract,
        over: 1,
        over_width_mm: 0.0,
        along: false,
    }];
    p.gaps_mm = vec![[1000.0, 3000.0]];
    assert!(text_y(&p, false) > 200.0 + 100.0, "beside the run: {}", text_y(&p, false));
    // A run drawn leftwards has its text turned over: the label stays on the same side as it reads.
    assert!(text_y(&p, true) < -(200.0 + 100.0));
    // Under the other duct elsewhere than at the label: it stays on the run.
    p.covers = vec![CoverMark {
        gap_mm: [100.0, 600.0],
        dash_mm: [200.0, 500.0],
        over_air: AirDirection::Extract,
        over: 1,
        over_width_mm: 0.0,
        along: false,
    }];
    p.gaps_mm = vec![[100.0, 600.0]];
    assert!(text_y(&p, false).abs() < 1e-9, "not under the other duct there: {}", text_y(&p, false));
}

fn dashed_layers(p: &DuctParams) -> Vec<&'static str> {
    p.generate(false).unwrap().iter().filter(|x| x.layer.contains("HVAC_HIDDEN")).map(|x| x.layer).collect()
}

#[test]
fn the_hidden_sides_are_dashed_in_the_colour_of_the_duct_underneath_and_not_where_they_coincide_with_the_duct_over() {
    let mut p = duct(400.0);
    p.units = HvacUnits { mm_per_unit: 1.0 };
    p.length = 4000.0;
    p.air = AirDirection::Supply;
    p.gaps_mm = vec![[0.0, 4000.0]];
    let mark = |over_width_mm: f64, along: bool| CoverMark {
        gap_mm: [0.0, 4000.0],
        dash_mm: [0.0, 4000.0],
        over_air: AirDirection::Extract,
        over: 1,
        over_width_mm,
        along,
    };
    // One over the other on the same route and the same width: its sides are the other's own, nothing is dashed.
    p.covers = vec![mark(400.0, true)];
    assert!(dashed_layers(&p).is_empty(), "{:?}", dashed_layers(&p));
    // Under a wider one on the same route: the two sides are hidden, dashed on the layer of the one over it (so that hiding
    // that layer hides them) in the colour of this one, the supply duct underneath.
    p.covers = vec![mark(600.0, true)];
    assert_eq!(dashed_layers(&p), ["M-HVAC-DUCT-EXTR|HVAC_HIDDEN|M-HVAC-DUCT-SUP"; 2]);
    // Across another duct the sides are hidden under it, whatever its width.
    p.covers = vec![mark(400.0, false)];
    assert_eq!(dashed_layers(&p).len(), 2);
    // The duct underneath is an extract one: the dashes are pink, whatever the duct over it.
    p.air = AirDirection::Extract;
    p.covers = vec![CoverMark { over_air: AirDirection::Supply, ..mark(600.0, true) }];
    assert_eq!(dashed_layers(&p), ["M-HVAC-DUCT-SUP|HVAC_HIDDEN|M-HVAC-DUCT-EXTR"; 2]);
}

#[test]
fn the_dashes_stop_where_a_cone_begins_and_a_run_under_another_keeps_its_bend() {
    let mut p = duct(400.0);
    p.units = HvacUnits { mm_per_unit: 1.0 };
    p.length = 4000.0;
    p.gaps_mm = vec![[0.0, 4000.0]];
    p.covers = vec![CoverMark {
        gap_mm: [0.0, 4000.0],
        dash_mm: [0.0, 4000.0],
        over_air: AirDirection::Extract,
        over: 1,
        over_width_mm: 900.0,
        along: true,
    }];
    // A cone of 1000 mm at the start: the dashes start where the straight sides do.
    p.reducer = Some(DuctReducer { from_mm: 900.0, at_end: false });
    let pieces = p.generate(false).unwrap();
    let xs: Vec<f64> = pieces
        .iter()
        .filter(|x| x.layer.contains("HVAC_HIDDEN"))
        .filter_map(|x| match &x.kind {
            EntityKind::LwPolyline(l) => l.vertices.first().map(|v| v.p.x),
            _ => None,
        })
        .collect();
    assert_eq!(xs.len(), 2);
    assert!(xs.iter().all(|x| *x > 100.0), "the dashes begin after the cone: {xs:?}");
    // The bend at the end of a run under another that goes the same way is drawn (hiding the duct over shows it).
    p.reducer = None;
    p.trim_end_mm = 650.0;
    p.elbow = Some(DuctElbow { angle_deg: -90.0, radius_mm: 650.0, at_end: true });
    let arcs = |p: &DuctParams| {
        p.generate(false)
            .unwrap()
            .iter()
            .filter(|x| {
                matches!(&x.kind, EntityKind::LwPolyline(l) if l.vertices.iter().any(|v| v.bulge != 0.0)) || matches!(x.kind, EntityKind::Arc(_))
            })
            .count()
    };
    let with_elbow = arcs(&p);
    p.covers = vec![CoverMark { along: false, ..p.covers[0] }];
    let crossing = arcs(&p);
    assert!(with_elbow > crossing, "across another duct a covered bend is left out ({with_elbow} against {crossing})");
}

#[test]
fn a_crossing_drawn_before_marks_knew_the_width_and_the_route_is_given_them_at_the_next_sizing() {
    let mut d = Drawing::new_metric();
    let under = add_wide(&mut d, (0.0, 0.0), (4000.0, 0.0), 650.0);
    let over = add_wide(&mut d, (0.0, 0.0), (4000.0, 0.0), 650.0);
    for (h, mm) in [(over, 2800.0), (under, 2500.0)] {
        let (mut p, ..) = d.duct_ends(h).unwrap();
        p.level_mm = Some(mm);
        d.regenerate_duct(h, &p).unwrap();
    }
    assert_eq!(d.cross_ducts_by_altitude(30.0).unwrap(), 1);
    let marks = |d: &Drawing| duct_of(d.entity(under).unwrap()).unwrap().covers;
    assert_eq!((marks(&d)[0].along, marks(&d)[0].over_width_mm), (true, 650.0));
    // The mark as an older version wrote it.
    let (mut p, ..) = d.duct_ends(under).unwrap();
    for c in &mut p.covers {
        (c.along, c.over_width_mm) = (false, 0.0);
    }
    d.regenerate_duct(under, &p).unwrap();
    assert_eq!(d.cross_ducts_by_altitude(30.0).unwrap(), 0, "nothing new to draw");
    assert_eq!((marks(&d)[0].along, marks(&d)[0].over_width_mm), (true, 650.0), "but the mark is brought up to date");
}
