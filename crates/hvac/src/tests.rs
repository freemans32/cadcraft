//! Expected values were computed apart, by solving the Colebrook equation by bisection, so they
//! check the iteration here as well as the arithmetic around it.

use crate::{
    Air, DesignCriteria, Fitting, Governing, HvacError, Network, RoomUse, Segment, SegmentKind, Terminal, fresh_air_rate, friction_factor,
    pressure_gradient, reynolds, size_network, velocity,
};

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-9)
}

#[test]
fn friction_in_a_250_duct_carrying_750_m3h() {
    let air = Air::default();
    assert!(close(velocity(750.0, 250.0), 4.244_131_8, 1e-6));
    assert!(close(pressure_gradient(750.0, 250.0, 0.15, &air).unwrap(), 0.938_768, 1e-4));
}

#[test]
fn laminar_flow_uses_64_over_re() {
    assert!(close(friction_factor(1000.0, 0.15, 100.0).unwrap(), 0.064, 1e-12));
}

#[test]
fn smooth_turbulent_factor_is_close_to_blasius() {
    // Blasius, 0.316 Re^-0.25, holds for smooth pipes around Re = 10⁴ to 10⁵.
    let re = 50_000.0;
    let f = friction_factor(re, 0.0, 250.0).unwrap();
    assert!(close(f, 0.316 * re.powf(-0.25), 0.03), "{f}");
}

#[test]
fn hostile_numbers_are_refused_not_panicked_on() {
    let air = Air::default();
    assert!(pressure_gradient(f64::NAN, 250.0, 0.15, &air).is_err());
    assert!(pressure_gradient(100.0, 0.0, 0.15, &air).is_err());
    assert!(pressure_gradient(100.0, 250.0, f64::INFINITY, &air).is_err());
    assert!(friction_factor(-5.0, 0.15, 250.0).is_err());
    assert_eq!(pressure_gradient(0.0, 250.0, 0.15, &air), Ok(0.0));
    let _ = reynolds(f64::MAX, f64::MAX, &air);
}

#[test]
fn labour_code_rates() {
    assert_eq!(fresh_air_rate(RoomUse::Office, 30, true), Ok(750.0));
    assert_eq!(fresh_air_rate(RoomUse::DiningSalesMeeting, 12, true), Ok(360.0));
    assert_eq!(fresh_air_rate(RoomUse::LightWorkshop, 2, true), Ok(90.0));
    assert_eq!(fresh_air_rate(RoomUse::OtherWorkshop, 1, true), Ok(60.0));
    assert_eq!(fresh_air_rate(RoomUse::Toilet, 0, true), Ok(30.0));
    assert_eq!(fresh_air_rate(RoomUse::Toilet, 0, false), Ok(15.0));
    assert_eq!(fresh_air_rate(RoomUse::Shower, 0, true), Ok(45.0));
    assert_eq!(fresh_air_rate(RoomUse::ShowerWithToilet, 0, true), Ok(60.0));
    assert_eq!(fresh_air_rate(RoomUse::ShowerWithToilet, 0, false), Ok(15.0));
    assert_eq!(fresh_air_rate(RoomUse::GroupedSanitary { fixtures: 3 }, 0, true), Ok(75.0));
    assert_eq!(fresh_air_rate(RoomUse::GroupedWashbasins { fixtures: 4 }, 0, true), Ok(30.0));
    assert!(fresh_air_rate(RoomUse::Office, u32::MAX, true).is_err());
}

fn seg(parent: Option<usize>, length_m: f64, kind: SegmentKind, fittings: &[Fitting], terminal: Option<f64>) -> Segment {
    Segment {
        parent,
        length_m,
        kind,
        fittings: fittings.to_vec(),
        terminal: terminal.map(|flow_m3h| Terminal { flow_m3h, pressure_drop_pa: 15.0 }),
        diameter_mm: None,
    }
}

/// A unit feeding three 250 m³/h offices: a trunk that splits into a run to two diffusers and
/// a branch to the third.
fn three_offices() -> Network {
    use Fitting::{Elbow90, TeeBranch, TeeStraight};
    use SegmentKind::{Branch, Connection, Main};
    Network {
        segments: vec![
            seg(None, 10.0, Main, &[Elbow90], None),
            seg(Some(0), 8.0, Main, &[TeeStraight], None),
            seg(Some(0), 6.0, Branch, &[TeeBranch, Elbow90], None),
            seg(Some(1), 3.0, Connection, &[TeeBranch, Elbow90], Some(250.0)),
            seg(Some(1), 2.0, Connection, &[Elbow90], Some(250.0)),
            seg(Some(2), 1.5, Connection, &[Elbow90], Some(250.0)),
        ],
    }
}

#[test]
fn three_offices_are_sized_and_their_worst_run_found() {
    let r = size_network(&three_offices(), &DesignCriteria::default()).unwrap();
    let flows: Vec<f64> = r.segments.iter().map(|s| s.flow_m3h).collect();
    assert_eq!(flows, [750.0, 500.0, 250.0, 250.0, 250.0, 250.0]);
    let sizes: Vec<f64> = r.segments.iter().map(|s| s.diameter_mm).collect();
    assert_eq!(sizes, [250.0, 250.0, 200.0, 200.0, 200.0, 200.0]);
    // Ø200 would carry 750 m³/h at 6.6 m/s; 500 m³/h fits Ø200 by velocity but not by friction.
    assert_eq!(r.segments[0].governing, Governing::Velocity);
    assert_eq!(r.segments[1].governing, Governing::Friction);
    assert_eq!(r.segments[3].governing, Governing::Velocity);
    let totals = [12.63, 4.5139, 6.0542, 4.9327 + 15.0, 1.6272 + 15.0, 1.4403 + 15.0];
    for (s, t) in r.segments.iter().zip(totals) {
        assert!(close(s.total_pa, t, 1e-3), "{} vs {t}", s.total_pa);
    }
    assert_eq!(r.critical_path, [0, 1, 3]);
    assert!(close(r.required_pressure_pa, 37.0766, 1e-3), "{}", r.required_pressure_pa);
    assert_eq!(r.total_flow_m3h, 750.0);
    assert!(r.warnings.is_empty());
}

#[test]
fn an_imposed_diameter_is_kept_and_a_fast_one_warned_about() {
    let mut net = three_offices();
    net.segments[0].diameter_mm = Some(160.0);
    let r = size_network(&net, &DesignCriteria::default()).unwrap();
    assert_eq!(r.segments[0].diameter_mm, 160.0);
    assert_eq!(r.segments[0].governing, Governing::Imposed);
    assert_eq!(r.warnings.len(), 1);
}

#[test]
fn a_flow_beyond_the_largest_size_uses_it_with_a_warning() {
    let net = Network { segments: vec![seg(None, 1.0, SegmentKind::Main, &[], Some(500_000.0))] };
    let r = size_network(&net, &DesignCriteria::default()).unwrap();
    assert_eq!(r.segments[0].diameter_mm, 1250.0);
    assert_eq!(r.segments[0].governing, Governing::Largest);
    assert_eq!(r.warnings.len(), 1);
}

#[test]
fn bad_networks_are_refused() {
    let c = DesignCriteria::default();
    let bad = |net: Network| matches!(size_network(&net, &c), Err(HvacError::BadNetwork(_)));
    assert!(bad(Network::default()));
    // A loop: 0 → 1 → 0.
    let mut looped = three_offices();
    looped.segments[0].parent = Some(1);
    assert!(bad(looped));
    let mut missing = three_offices();
    missing.segments[3].parent = Some(99);
    assert!(bad(missing));
    // A run that ends without a terminal carries nothing.
    let mut dead_end = three_offices();
    dead_end.segments.push(seg(Some(0), 2.0, SegmentKind::Branch, &[], None));
    assert!(bad(dead_end));
}

#[test]
fn bad_values_are_refused() {
    let c = DesignCriteria::default();
    let mut net = three_offices();
    net.segments[2].length_m = f64::NAN;
    assert!(matches!(size_network(&net, &c), Err(HvacError::BadValue(_))));
    let mut net = three_offices();
    net.segments[2].fittings.push(Fitting::Custom { zeta: -1.0 });
    assert!(size_network(&net, &c).is_err());
    let mut net = three_offices();
    net.segments[5].terminal = Some(Terminal { flow_m3h: f64::INFINITY, pressure_drop_pa: 0.0 });
    assert!(size_network(&net, &c).is_err());
    let unsorted = DesignCriteria { diameters_mm: vec![200.0, 160.0], ..DesignCriteria::default() };
    assert!(size_network(&three_offices(), &unsorted).is_err());
    let no_air = DesignCriteria { air: Air { density: 0.0, kinematic_viscosity: 1.5e-5 }, ..DesignCriteria::default() };
    assert!(size_network(&three_offices(), &no_air).is_err());
}

#[test]
fn a_long_chain_is_handled_without_recursion() {
    let n = crate::network::MAX_SEGMENTS;
    let mut segments: Vec<Segment> = (0..n).map(|i| seg(i.checked_sub(1), 1.0, SegmentKind::Main, &[], None)).collect();
    if let Some(last) = segments.last_mut() {
        last.terminal = Some(Terminal { flow_m3h: 100.0, pressure_drop_pa: 0.0 });
    }
    let r = size_network(&Network { segments }, &DesignCriteria::default()).unwrap();
    assert_eq!(r.critical_path.len(), n);
}

#[test]
fn too_many_segments_are_refused() {
    let n = crate::network::MAX_SEGMENTS + 1;
    let segments = (0..n).map(|_| seg(None, 1.0, SegmentKind::Main, &[], Some(10.0))).collect();
    assert!(size_network(&Network { segments }, &DesignCriteria::default()).is_err());
}

#[test]
fn a_report_reads_back_from_json() {
    let r = size_network(&three_offices(), &DesignCriteria::default()).unwrap();
    let net = three_offices();
    // The network and criteria are what a drawing will store; they must survive serde.
    let json = serde_json::to_string(&(net.clone(), DesignCriteria::default())).unwrap();
    let back: (Network, DesignCriteria) = serde_json::from_str(&json).unwrap();
    assert_eq!(back.0, net);
    assert_eq!(size_network(&back.0, &back.1).unwrap(), r);
}
