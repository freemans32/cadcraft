use cadcraft_geom::Vec2;

use super::*;
use crate::{Common, Drawing, EntityKind, Space};

fn riser() -> RiserParams {
    RiserParams {
        name: "VMC 3".into(),
        air: AirDirection::Extract,
        up: true,
        outline: vec![Vec2::new(0.0, 0.0), Vec2::new(600.0, 0.0), Vec2::new(600.0, 400.0), Vec2::new(0.0, 400.0)],
        incoming_flow_m3h: None,
        flow_m3h: None,
        size: None,
        size_set: false,
        label_height_mm: 100.0,
        units: HvacUnits::default(),
    }
}

#[test]
fn a_riser_is_its_shaft_a_symbol_with_a_leader_and_a_label_that_reads_like_the_plan() {
    let mut p = riser();
    assert_eq!(p.text(), "VMC 3");
    p.size = Some(RiserSize::Round { diameter_mm: 200.0 });
    p.flow_m3h = Some(240.0);
    assert_eq!(p.text(), "VMC 3 Ø200-240 m³/h");
    p.size = Some(RiserSize::Rectangular { width_mm: 400.0, height_mm: 250.0 });
    assert_eq!(p.text(), "VMC 3 400×250-240 m³/h");
    // Flow typed from the other floors shows until the riser carries its own.
    p.flow_m3h = None;
    p.incoming_flow_m3h = Some(660.0);
    assert_eq!(p.text(), "VMC 3 400×250-660 m³/h");
    let pieces = p.generate().unwrap();
    assert!(pieces.iter().any(|x| x.layer == RISER_LAYER && matches!(x.kind, EntityKind::LwPolyline(_))));
    assert!(pieces.iter().any(|x| x.layer == AIR_EXTRACT_LAYER && matches!(x.kind, EntityKind::Text(_))));
    // Going up draws a dot in the circle, going down a cross.
    let circles = |p: &RiserParams| p.generate().unwrap().iter().filter(|x| matches!(x.kind, EntityKind::Circle(_))).count();
    let lines = |p: &RiserParams| p.generate().unwrap().iter().filter(|x| matches!(x.kind, EntityKind::Line(_))).count();
    let down = RiserParams { up: false, ..p.clone() };
    assert_eq!((circles(&p), lines(&p)), (2, 3));
    assert_eq!((circles(&down), lines(&down)), (1, 5));
}

#[test]
fn a_riser_keeps_its_data_through_the_block_and_is_edited_in_place() {
    let mut d = Drawing::new_metric();
    let mut p = riser();
    p.outline = vec![Vec2::new(1000.0, 2000.0), Vec2::new(1600.0, 2000.0), Vec2::new(1600.0, 2400.0), Vec2::new(1000.0, 2400.0)];
    let h = d.add_riser(&Space::Model, Common::default(), p).unwrap();
    let e = d.entity(h).unwrap();
    let got = riser_of(e).unwrap();
    assert_eq!((got.name.as_str(), got.shaft_mm()), ("VMC 3", (600.0, 400.0)));
    // The shaft is typed: a rectangle around the same centre.
    let mut next = got.clone();
    next.set_shaft_mm(800.0, 500.0).unwrap();
    next.size = Some(RiserSize::Round { diameter_mm: 250.0 });
    d.regenerate_riser(h, &next).unwrap();
    let again = riser_of(d.entity(h).unwrap()).unwrap();
    assert_eq!((again.shaft_mm(), again.size), ((800.0, 500.0), Some(RiserSize::Round { diameter_mm: 250.0 })));
    assert!(RiserParams { flow_m3h: Some(0.0), ..riser() }.validate().is_err());
    assert!(RiserParams { outline: vec![Vec2::ZERO; 4], ..riser() }.validate().is_err(), "no area");
    assert!(RiserParams { size: Some(RiserSize::Round { diameter_mm: 5.0 }), ..riser() }.validate().is_err());
}
