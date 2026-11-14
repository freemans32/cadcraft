use cadcraft_geom::Vec2;

use super::*;
use crate::{Common, Drawing, Space};

fn square(side: f64) -> Vec<Vec2> {
    vec![Vec2::new(1000.0, 2000.0), Vec2::new(1000.0 + side, 2000.0), Vec2::new(1000.0 + side, 2000.0 + side), Vec2::new(1000.0, 2000.0 + side)]
}

#[test]
fn a_slab_is_drawn_hatched_with_its_thickness_and_top_elevation_and_keeps_its_data() {
    let mut d = Drawing::new_metric();
    let mut p = SlabParams::new("Dalle RDC", SlabKind::Floor, square(5000.0), HvacUnits { mm_per_unit: 1.0 });
    p.elevation_mm = 3000.0;
    assert_eq!((p.area_m2(), p.volume_m3()), (25.0, 5.0));
    assert_eq!(p.label_text(), "Dalle RDC 200 mm +3.00 m");
    let h = d.add_slab(&Space::Model, Common::default(), p.clone()).unwrap();
    assert_eq!(d.entity(h).unwrap().common.layer, SLAB_FLOOR_LAYER);
    let back = slab_of(d.entity(h).unwrap()).unwrap();
    assert_eq!((back.thickness_mm, back.elevation_mm, back.outline.len()), (200.0, 3000.0, 4));
    // Turned into a roof: the layer follows the kind.
    d.regenerate_slab(h, &SlabParams { kind: SlabKind::Roof, thickness_mm: 250.0, ..back }).unwrap();
    assert_eq!(d.entity(h).unwrap().common.layer, SLAB_ROOF_LAYER);
    assert_eq!(slab_of(d.entity(h).unwrap()).unwrap().thickness_mm, 250.0);
}

#[test]
fn a_slab_that_cannot_be_drawn_is_refused() {
    let u = HvacUnits { mm_per_unit: 1.0 };
    let ok = SlabParams::new("S", SlabKind::Floor, square(1000.0), u);
    for bad in [
        SlabParams { thickness_mm: 0.0, ..ok.clone() },
        SlabParams { thickness_mm: f64::NAN, ..ok.clone() },
        SlabParams { outline: vec![Vec2::ZERO, Vec2::new(1.0, 0.0)], ..ok.clone() },
        SlabParams { outline: vec![Vec2::ZERO, Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)], ..ok.clone() },
        SlabParams { name: String::new(), ..ok.clone() },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    assert_eq!((SlabKind::parse("plancher"), SlabKind::parse("Toiture"), SlabKind::parse("x")), (Some(SlabKind::Floor), Some(SlabKind::Roof), None));
}
