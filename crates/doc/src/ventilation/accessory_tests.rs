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
        bottom_mm: None,
        units: HvacUnits::default(),
        air: None,
        height_mm: None,
        fixed: false,
        overhang_mm: None,
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
        assert!(pieces.iter().all(|p| p.layer.split('|').next().unwrap() == if kind == AccessoryKind::FireDamper { ACCESSORY_FIRE_LAYER } else { ACCESSORY_LAYER }));
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
        typed_flows: Vec::new(),
        port_sizes: Vec::new(),
        port_levels_mm: Vec::new(),
        height_mm: None,
        model: String::new(),
        weight_kg: None,
        coils: AhuCoils::default(),
        electric_kw: None,
        circuit: String::new(),
        address: String::new(),
        show_address: true,
        label_height_mm: 100.0,
        bottom_mm: None,
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
        reducer_len_mm: None,
        reducer_shape: None,
        label_shift_mm: None,
        kept_edge: None,
        hide_label: false,
        extra_m: 0.0,
        subnetwork: String::new(),
        subnetwork_colour: None,
        cut_deg: [0.0, 0.0],
        elbow: None,
        trim_start_mm: 0.0,
        trim_end_mm: 0.0,
        flow_set: false,
        insulation: None,
        cap_start: false,
        cap_end: false,
        flow_forward: None,
        open_flow_m3h: None,
        gaps_mm: Vec::new(),
        covers: Vec::new(),
        jogs: Vec::new(),
        level_mm: None,
        end_level_mm: None,
        steps: Vec::new(),
        bottom_mm: None,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    };
    d.add_duct(&Space::Model, Common::default(), duct, Vec2::new(500.0, 0.0), Vec2::new(6000.0, 0.0)).unwrap();
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == "SQ600-200").unwrap();
    let t =
        TerminalParams { model, flow_m3h: 300.0, air: AirDirection::Supply, label_height_mm: 100.0, bottom_mm: None, units: HvacUnits::default() };
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

#[test]
fn a_hood_is_drawn_outwards_from_its_origin_with_a_mesh_at_the_far_end() {
    let pieces = accessory(AccessoryKind::Hood).generate().unwrap();
    assert_eq!(texts(&pieces), ["CAP Ø250 L600"]);
    let xs: Vec<f64> = pieces
        .iter()
        .flat_map(|p| match &p.kind {
            EntityKind::Line(l) => vec![l.a.x, l.b.x],
            EntityKind::LwPolyline(w) => w.vertices.iter().map(|v| v.p.x).collect(),
            _ => Vec::new(),
        })
        .collect();
    assert!(xs.iter().all(|x| *x >= -1e-9), "nothing behind the end of the duct");
    assert!(xs.iter().any(|x| (x - 600.0).abs() < 1e-9), "the face is at the hood's length");
    // As wide as its opening unless an overhang is asked for.
    let mut hood = accessory(AccessoryKind::Hood);
    assert!((hood.half().y - 125.0).abs() < 1e-9, "as wide as the opening it covers");
    hood.overhang_mm = Some(50.0);
    assert!(hood.half().y > 125.0, "wider with an overhang");
}

#[test]
fn a_hood_covers_the_nearer_end_of_a_duct_and_its_loss_counts() {
    let (mut d, _) = ducted(None);
    let far = d.hood_anchor(Vec2::new(5900.0, 0.0)).unwrap();
    assert!(far.angle.abs() < 1e-9 && (far.origin.x - 6000.0).abs() < 5.0 && far.duct.is_some());
    let near = d.hood_anchor(Vec2::new(600.0, 0.0)).unwrap();
    assert!((near.angle - std::f64::consts::PI).abs() < 1e-9, "angle {near:?}");
    assert!((near.origin.x - 500.0).abs() < 5.0, "origin {near:?}");
    assert!(d.hood_anchor(Vec2::new(3000.0, 4000.0)).is_none(), "nothing there: the hood goes free");

    let (base, _) = required(&d);
    d.add_accessory(&Space::Model, Common::default(), &accessory(AccessoryKind::Hood), near.origin, near.angle).unwrap();
    let net = d.duct_networks().unwrap().remove(0);
    assert!(net.unplaced.is_empty());
    assert_eq!(net.accessories.len(), 1);
    assert!(required(&d).0 > base, "a hood adds a coefficient loss");
}

#[test]
fn a_free_hood_is_not_reported_as_on_no_duct() {
    let (d, h) = ducted(Some((AccessoryKind::Hood, 3000.0, 2000.0)));
    let net = d.duct_networks().unwrap().remove(0);
    assert!(h.is_some() && net.unplaced.is_empty() && net.accessories.is_empty());
}

#[test]
fn a_hood_takes_the_layer_and_so_the_colour_of_its_air_and_writes_its_size_length_and_air() {
    let hood = AccessoryParams { air: Some(AirDirection::Exhaust), height_mm: Some(250.0), diameter_mm: 400.0, ..accessory(AccessoryKind::Hood) };
    assert_eq!(hood.layer(), super::DUCT_EXHAUST_LAYER);
    assert_eq!(texts(&hood.generate().unwrap()), ["CAP 400×250 L600 R"]);
    assert_eq!(AccessoryParams { air: Some(AirDirection::Fresh), ..hood.clone() }.layer(), super::DUCT_FRESH_LAYER);
    assert_eq!(AccessoryParams { air: None, ..hood.clone() }.layer(), ACCESSORY_LAYER);
    assert_eq!(AccessoryParams::from_xdata(&hood.to_xdata().unwrap()), Some(hood.clone()), "air and height survive");
    // Only a hood changes layer with its air.
    assert_eq!(AccessoryParams { air: Some(AirDirection::Fresh), ..accessory(AccessoryKind::Damper) }.layer(), ACCESSORY_LAYER);
    assert!(AccessoryParams { height_mm: Some(5.0), ..hood }.validate().is_err());
}

#[test]
fn a_hood_on_a_duct_takes_the_air_of_the_duct_and_the_takeoff_counts_it_by_size_length_and_air() {
    let (mut d, _) = ducted(None);
    let a = d.hood_anchor(Vec2::new(5900.0, 0.0)).unwrap();
    assert_eq!(a.air, Some(AirDirection::Supply));
    let hood = AccessoryParams { air: a.air, ..accessory(AccessoryKind::Hood) };
    d.add_accessory(&Space::Model, Common::default(), &hood, a.origin, a.angle).unwrap();
    let t = d.takeoff("").unwrap();
    assert_eq!(t.accessories.iter().map(|c| (c.name.as_str(), c.size.as_str())).collect::<Vec<_>>(), [("hood", "Ø250 L600 S")]);
}

#[test]
fn a_hood_follows_its_duct_until_it_is_set_by_hand() {
    let (mut d, _) = ducted(None);
    let a = d.hood_anchor(Vec2::new(5900.0, 0.0)).unwrap();
    let duct = a.duct.unwrap();
    let make = |d: &mut Drawing, fixed: bool| {
        let p = AccessoryParams { diameter_mm: a.width_mm, air: a.air, fixed, ..accessory(AccessoryKind::Hood) };
        d.add_accessory(&Space::Model, Common::default(), &p, a.origin, a.angle).unwrap()
    };
    let (follows, fixed) = (make(&mut d, false), make(&mut d, true));
    let (mut p, ..) = d.duct_ends(duct).unwrap();
    (p.diameter_mm, p.air) = (400.0, AirDirection::Exhaust);
    d.regenerate_duct(duct, &p).unwrap();
    assert_eq!(d.refresh_hoods().unwrap(), 1);
    let of = |d: &Drawing, h| accessory_of(d.entity(h).unwrap()).unwrap();
    assert_eq!((of(&d, follows).diameter_mm, of(&d, follows).air), (400.0, Some(AirDirection::Exhaust)));
    assert_eq!(of(&d, follows).layer(), super::DUCT_EXHAUST_LAYER);
    assert_eq!(of(&d, fixed).diameter_mm, 250.0, "one set by hand stays as it is");
    assert_eq!(d.refresh_hoods().unwrap(), 0, "nothing more to follow");
}
