//! Expected values were computed apart, by solving the Colebrook equation by bisection, so they
//! check the iteration here as well as the arithmetic around it.

use crate::{
    Air, DesignCriteria, Fitting, Governing, HvacError, Network, RoomUse, Section, Segment, SegmentKind, SegmentResult, Terminal,
    equivalent_diameter_mm, fresh_air_rate, friction_factor, pressure_gradient, reynolds, size_network, velocity,
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
        fixed_pa: 0.0,
        roughness_mm: None,
        max_height_mm: None,
        rect_mm: None,
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

#[test]
fn an_accessory_given_in_pascals_adds_to_its_segments_losses() {
    let mut net = three_offices();
    let plain = size_network(&net, &DesignCriteria::default()).unwrap();
    // A silencer of 30 Pa on the trunk.
    net.segments[0].fixed_pa = 30.0;
    let with = size_network(&net, &DesignCriteria::default()).unwrap();
    assert!((with.segments[0].fittings_pa - plain.segments[0].fittings_pa - 30.0).abs() < 1e-9);
    assert!((with.required_pressure_pa - plain.required_pressure_pa - 30.0).abs() < 1e-9);
    assert_eq!(with.segments[0].diameter_mm, plain.segments[0].diameter_mm, "the size does not depend on the accessories");
    for bad in [-1.0, f64::NAN, 1e9] {
        net.segments[0].fixed_pa = bad;
        assert!(size_network(&net, &DesignCriteria::default()).is_err(), "{bad}");
    }
}

#[test]
fn a_rougher_run_loses_more_and_may_need_a_larger_size() {
    let mut net = three_offices();
    let plain = size_network(&net, &DesignCriteria::default()).unwrap();
    // The last connection in flexible duct (about 3 mm of roughness).
    let last = net.segments.len() - 1;
    net.segments[last].roughness_mm = Some(3.0);
    let rough = size_network(&net, &DesignCriteria::default()).unwrap();
    assert!(rough.segments[last].gradient_pa_m > plain.segments[last].gradient_pa_m);
    assert!(rough.segments[last].diameter_mm >= plain.segments[last].diameter_mm);
    assert_eq!(rough.segments[0], plain.segments[0], "the other runs are not affected");
    net.segments[last].roughness_mm = Some(-1.0);
    assert!(size_network(&net, &DesignCriteria::default()).is_err());
}

fn one(flow: f64, tweak: impl Fn(&mut Segment)) -> (SegmentResult, Vec<String>) {
    let mut s = seg(None, 10.0, SegmentKind::Main, &[], Some(flow));
    tweak(&mut s);
    let r = size_network(&Network { segments: vec![s] }, &DesignCriteria::default()).unwrap();
    (r.segments[0].clone(), r.warnings)
}

#[test]
fn the_equivalent_diameter_of_a_rectangular_duct_matches_the_published_formula() {
    // Values computed apart from the published formula 1.3 · (a·b)^0.625 / (a+b)^0.25.
    assert!((equivalent_diameter_mm(400.0, 250.0) - 343.33).abs() < 0.01);
    assert!((equivalent_diameter_mm(300.0, 300.0) - 327.95).abs() < 0.01);
    assert!((equivalent_diameter_mm(1000.0, 100.0) - 301.02).abs() < 0.01, "a flat duct 1 m wide loses like a Ø301 round one");
}

#[test]
fn a_run_stays_round_when_it_fits_the_height_and_turns_rectangular_when_it_does_not() {
    // 150 m³/h in a connection: Ø100 or so, well under 250 mm.
    let (small, _) = one(150.0, |s| {
        s.kind = SegmentKind::Connection;
        s.max_height_mm = Some(250.0)
    });
    assert_eq!(small.section, Section::Round);
    assert!(small.diameter_mm <= 250.0 && small.width_mm == small.height_mm);
    // 1800 m³/h needs about Ø355 round: it does not fit 250 mm.
    let free = one(1800.0, |_| {}).0;
    assert!(free.diameter_mm > 250.0 && free.section == Section::Round);
    let (rect, warnings) = one(1800.0, |s| s.max_height_mm = Some(250.0));
    assert_eq!(rect.section, Section::Rectangular);
    assert_eq!(rect.height_mm, 250.0, "as tall as the space allows");
    assert!(rect.width_mm >= rect.height_mm && rect.width_mm % 50.0 == 0.0 && rect.width_mm / rect.height_mm <= 4.0);
    assert!(rect.velocity <= 6.0 + 1e-9 && rect.gradient_pa_m <= 1.0 + 1e-9, "{rect:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    // It is the narrowest that does: 50 mm less breaks a limit.
    let narrower = (rect.width_mm - 50.0).max(250.0);
    let v = 1800.0 / 3600.0 / (narrower / 1000.0 * 0.25);
    let g = pressure_gradient(1800.0, equivalent_diameter_mm(narrower, 250.0), 0.15, &Air::default()).unwrap();
    assert!(narrower == rect.width_mm || v > 6.0 || g > 1.0, "{narrower}: {v} m/s, {g} Pa/m");
    // The velocity is the real one (flow over the section), the friction the equivalent diameter's.
    assert!((rect.velocity - 1800.0 / 3600.0 / (rect.width_mm * rect.height_mm / 1e6)).abs() < 1e-9);
    assert!((rect.diameter_mm - equivalent_diameter_mm(rect.width_mm, rect.height_mm)).abs() < 1e-9);
}

#[test]
fn a_section_the_user_imposes_is_kept_and_a_height_too_small_is_refused() {
    let (r, _) = one(600.0, |s| s.rect_mm = Some((500.0, 200.0)));
    assert_eq!((r.section, r.width_mm, r.height_mm, r.governing), (Section::Rectangular, 500.0, 200.0, Governing::Imposed));
    assert!((r.velocity - 600.0 / 3600.0 / 0.1).abs() < 1e-9);
    // A rectangular section wins over a diameter.
    let (r, _) = one(600.0, |s| {
        s.rect_mm = Some((500.0, 200.0));
        s.diameter_mm = Some(200.0)
    });
    assert_eq!(r.section, Section::Rectangular);
    let mut s = seg(None, 10.0, SegmentKind::Main, &[], Some(1800.0));
    s.max_height_mm = Some(60.0);
    assert!(size_network(&Network { segments: vec![s] }, &DesignCriteria::default()).is_err(), "below the smallest side");
}

#[test]
fn the_aspect_ratio_limits_the_width_and_a_warning_says_when_it_is_not_enough() {
    // 100 mm tall allows 400 mm wide: far too small for 5000 m³/h.
    let (r, warnings) = one(5000.0, |s| s.max_height_mm = Some(100.0));
    assert_eq!((r.width_mm, r.height_mm, r.governing), (400.0, 100.0, Governing::Largest));
    assert!(warnings.iter().any(|w| w.contains("exceeds the limits even at 400×100")), "{warnings:?}");
    // With a looser limit it can be wider.
    let c = DesignCriteria { max_aspect: 20.0, ..DesignCriteria::default() };
    let mut s = seg(None, 10.0, SegmentKind::Main, &[], Some(5000.0));
    s.max_height_mm = Some(100.0);
    let r = size_network(&Network { segments: vec![s] }, &c).unwrap();
    assert!(r.segments[0].width_mm > 400.0);
    assert!(
        size_network(&Network { segments: vec![seg(None, 1.0, SegmentKind::Main, &[], Some(100.0))] }, &DesignCriteria { side_step_mm: 0.0, ..c })
            .is_err()
    );
}
