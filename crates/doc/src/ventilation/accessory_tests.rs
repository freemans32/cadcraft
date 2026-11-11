use cadcraft_geom::Vec2;
use cadcraft_hvac::{DesignCriteria, Fitting, size_network};

use super::*;
use crate::{Common, Drawing, EntityKind, Handle, Space};

fn accessory(kind: AccessoryKind) -> AccessoryParams {
    AccessoryParams {
        kind,
        length_mm: kind.default_length_mm(),
        diameter_mm: 250.0,
        zeta: None,
        pressure_drop_pa: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

fn texts(p: &[HvacPiece]) -> Vec<String> {
    p.iter().filter_map(|x| if let EntityKind::Text(t) = &x.kind { Some(t.value.clone()) } else { None }).collect()
}

#[test]
fn every_accessory_is_drawn_across_its_duct_with_its_code() {
    for (kind, label) in [
        (AccessoryKind::Damper, "REG Ø250"),
        (AccessoryKind::FireDamper, "CCF Ø250"),
        (AccessoryKind::Silencer, "SIL 30 Pa"),
        (AccessoryKind::FlexConnector, "MS Ø250"),
    ] {
        let pieces = accessory(kind).generate().unwrap();
        assert_eq!(texts(&pieces), [label], "{kind:?}");
        assert!(pieces.iter().all(|p| p.layer == ACCESSORY_LAYER));
        assert!(pieces.len() >= 3);
    }
    // A silencer is wider than its duct; the others only overhang it a little.
    let width = |k| accessory(k).half().y;
    assert!(width(AccessoryKind::Silencer) > 1.3 * 125.0);
    assert!(width(AccessoryKind::Damper) > 125.0 && width(AccessoryKind::Damper) < 1.3 * 125.0);
}

#[test]
fn the_loss_is_the_pressure_drop_if_given_else_the_coefficient_else_the_default() {
    let d = accessory(AccessoryKind::Damper);
    assert_eq!(d.loss(), Loss::Zeta(0.3));
    assert_eq!(AccessoryParams { zeta: Some(1.5), ..d.clone() }.loss(), Loss::Zeta(1.5));
    assert_eq!(AccessoryParams { zeta: Some(1.5), pressure_drop_pa: Some(12.0), ..d }.loss(), Loss::Pascals(12.0));
    assert_eq!(accessory(AccessoryKind::Silencer).loss(), Loss::Pascals(30.0));
}

#[test]
fn bad_accessories_are_refused_and_good_ones_survive_their_data() {
    let ok = accessory(AccessoryKind::FireDamper);
    assert!(ok.validate().is_ok());
    assert_eq!(AccessoryParams::from_xdata(&ok.to_xdata().unwrap()), Some(ok.clone()));
    for bad in [
        AccessoryParams { length_mm: 0.0, ..ok.clone() },
        AccessoryParams { length_mm: f64::NAN, ..ok.clone() },
        AccessoryParams { diameter_mm: 5.0, ..ok.clone() },
        AccessoryParams { zeta: Some(-1.0), ..ok.clone() },
        AccessoryParams { pressure_drop_pa: Some(1e9), ..ok.clone() },
        AccessoryParams { label_height_mm: 0.0, ..ok.clone() },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    let mut d = Drawing::new_metric();
    assert!(d.add_accessory(&Space::Model, Common::default(), &ok, Vec2::new(f64::NAN, 0.0), 0.0).is_err());
    assert!(d.model.is_empty());
}

fn unit() -> AhuParams {
    AhuParams {
        name: "CTA1".into(),
        width_mm: 1000.0,
        depth_mm: 600.0,
        flow_m3h: None,
        pressure_pa: None,
        extract_flow_m3h: None,
        extract_pressure_pa: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

/// A unit, a duct of Ø250 to a terminal, and optionally an accessory at x on it.
fn ducted(accessory_at: Option<(AccessoryKind, f64, f64)>) -> (Drawing, Option<Handle>) {
    let mut d = Drawing::new_metric();
    d.add_ahu(&Space::Model, Common::default(), &unit(), Vec2::ZERO, 0.0).unwrap();
    let duct = DuctParams {
        length: 1.0,
        diameter_mm: 250.0,
        imposed: true,
        kind: DuctKind::Auto,
        air: AirDirection::Supply,
        flow_m3h: None,
        velocity: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    };
    d.add_duct(&Space::Model, Common::default(), duct, Vec2::new(500.0, 0.0), Vec2::new(6000.0, 0.0)).unwrap();
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "SQ600-200").unwrap();
    let t = TerminalParams { model, flow_m3h: 300.0, air: AirDirection::Supply, label_height_mm: 100.0, units: HvacUnits::default() };
    d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(6000.0, 0.0), 0.0).unwrap();
    let h = accessory_at.map(|(kind, x, y)| d.add_accessory(&Space::Model, Common::default(), &accessory(kind), Vec2::new(x, y), 0.0).unwrap());
    (d, h)
}

fn required(d: &Drawing) -> (f64, usize) {
    let net = d.duct_networks().unwrap().remove(0);
    let report = size_network(&net.network, &DesignCriteria::default()).unwrap();
    (report.required_pressure_pa, net.network.segments.len())
}

#[test]
fn an_accessory_on_a_duct_adds_its_loss_without_cutting_the_duct() {
    let (plain, _) = ducted(None);
    let (base, segments) = required(&plain);
    assert_eq!(segments, 1);

    let (d, h) = ducted(Some((AccessoryKind::Silencer, 3000.0, 80.0)));
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments.len(), 1, "the duct is not cut");
    assert_eq!(net.network.segments[0].fixed_pa, 30.0);
    assert_eq!(net.accessories, [h.unwrap()]);
    assert!(net.unplaced.is_empty());
    assert!((required(&d).0 - base - 30.0).abs() < 1e-9, "a silencer of 30 Pa");

    let (d, _) = ducted(Some((AccessoryKind::FireDamper, 3000.0, 0.0)));
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.network.segments[0].fittings, [Fitting::Custom { zeta: 0.4 }]);
    assert!(required(&d).0 > base, "a fire damper adds a coefficient loss");
}

#[test]
fn an_accessory_on_no_duct_is_reported() {
    let (d, h) = ducted(Some((AccessoryKind::Damper, 3000.0, 2000.0)));
    let net = d.duct_networks().unwrap().remove(0);
    assert_eq!(net.unplaced, [h.unwrap()]);
    assert!(net.accessories.is_empty());
    assert!(net.network.segments[0].fittings.is_empty());
}

#[test]
fn the_duct_under_a_point_is_found_with_its_direction() {
    let (d, _) = ducted(None);
    let (h, foot, angle) = d.duct_under(Vec2::new(3000.0, 90.0)).expect("on the duct");
    assert!(d.duct_ends(h).is_some());
    assert!(foot.near(Vec2::new(3000.0, 0.0), 1e-9) && angle.abs() < 1e-12);
    assert!(d.duct_under(Vec2::new(3000.0, 400.0)).is_none(), "beside it");
    assert!(d.duct_under(Vec2::new(7000.0, 0.0)).is_none(), "past its end");
}
