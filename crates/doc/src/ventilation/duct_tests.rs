use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Fitting, SegmentKind, size_network};

use super::*;
use crate::{Common, Drawing, EntityKind, Handle, Space};

fn ahu() -> AhuParams {
    AhuParams {
        name: "CTA1".into(),
        width_mm: 1000.0,
        depth_mm: 600.0,
        air: AirDirection::Supply,
        flow_m3h: None,
        pressure_pa: None,
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
        air: AirDirection::Supply,
        flow_m3h: None,
        velocity: None,
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

#[test]
fn a_duct_label_reads_upright_whichever_way_it_runs() {
    let rotation_of = |p: &HvacPiece| match &p.kind {
        EntityKind::Text(t) => t.rotation,
        _ => f64::NAN,
    };
    let p = DuctParams { length: 1000.0, ..duct(200.0) };
    assert_eq!(rotation_of(&p.generate(false).unwrap()[2]), 0.0);
    assert!((rotation_of(&p.generate(true).unwrap()[2]) - std::f64::consts::PI).abs() < 1e-12);
    // The two sides are a diameter apart.
    let pieces = p.generate(false).unwrap();
    let ys: Vec<f64> = pieces.iter().filter_map(|x| if let EntityKind::Line(l) = &x.kind { Some(l.a.y) } else { None }).collect();
    assert_eq!(ys, [100.0, -100.0]);
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
