use cadcraft_geom::Vec2;

use super::*;
use crate::{Common, Drawing, EntityKind, Space};

fn units() -> HvacUnits {
    HvacUnits::default()
}

#[test]
fn the_standard_models_are_valid_unique_and_cover_every_kind() {
    let models = standard_equipment_models();
    let mut names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len(), "model names are unique");
    for m in &models {
        let p = EquipmentParams::from_model(m, "X", units());
        p.validate().unwrap_or_else(|e| panic!("{}: {e}", m.name));
        assert!(p.generate().is_ok(), "{}", m.name);
    }
    for kind in EquipmentKind::ALL {
        assert!(!equipment_models_of(&models, kind).is_empty(), "{kind:?}");
    }
    // A range in order of power, and chillers from 40 to 570 kW.
    let chillers = equipment_models_of(&models, EquipmentKind::Chiller);
    let kw: Vec<f64> = chillers.iter().filter_map(|m| m.capacity_kw).collect();
    assert!(kw.windows(2).all(|w| w[0] <= w[1]));
    assert!(kw.first().is_some_and(|k| *k < 45.0) && kw.last() == Some(&570.0));
}

#[test]
fn nothing_of_a_maker_is_left_in_the_library_but_the_sizes() {
    for m in standard_equipment_models() {
        assert!(m.reference.is_empty() && m.connections.is_empty() && m.weight_kg.is_none(), "{}", m.name);
        for brand in ["Carrier", "Daikin", "Sky Air", "AquaSnap", "Victaulic", "30R", "FTX", "RXY", "VRV IV"] {
            assert!(!m.name.contains(brand), "{}", m.name);
        }
        // The name is the kind and the power: kW, or CV for a VRV outdoor unit.
        let unit = if m.kind == EquipmentKind::VrfOutdoor { "CV" } else { "kW" };
        assert!(m.name.ends_with(unit), "{}", m.name);
    }
    let vrv = equipment_model_named(&standard_equipment_models(), "VRV 14 CV").unwrap();
    assert_eq!(EquipmentParams::from_model(&vrv, "UE1", units()).text(), "UE1 14 CV");
    let chiller = equipment_model_named(&standard_equipment_models(), "Chiller 107 kW").unwrap();
    assert_eq!(EquipmentParams::from_model(&chiller, "GE1", units()).text(), "GE1 107 kW");
}

#[test]
fn the_carrier_sizes_follow_their_powers() {
    let models = standard_equipment_models();
    let named = |n: &str| equipment_model_named(&models, n).unwrap_or_else(|| panic!("{n}"));
    // The smallest chiller: one fan, 2109 x 1090 x 1330; at 93.4 kW two fans on the larger casing.
    let m = named("Chiller 41.7 kW");
    assert_eq!((m.length_mm, m.width_mm, m.height_mm, m.fans), (2109.0, 1090.0, 1330.0, 1));
    let m = named("Chiller 93.4 kW");
    assert_eq!((m.length_mm, m.width_mm, m.fans), (2275.0, 2125.0, 2));
    // 30RBM 160-520: three casings, 2253 wide.
    assert_eq!(named("Chiller 168 kW").length_mm, 2410.0);
    assert_eq!(named("Chiller 365 kW").length_mm, 3604.0);
    assert_eq!((named("Chiller 523 kW").length_mm, named("Chiller 523 kW").fans, named("Chiller 523 kW").width_mm), (4797.0, 8, 2253.0));
    // The power picks the size: the smallest that gives it, else the largest.
    assert_eq!(equipment_model_for(&models, EquipmentKind::Chiller, 100.0).map(|m| m.name), Some("Chiller 107 kW".to_string()));
    assert_eq!(equipment_model_for(&models, EquipmentKind::Chiller, 9999.0).and_then(|m| m.capacity_kw), Some(570.0));
}

#[test]
fn every_kind_is_drawn_from_above_with_its_symbol_its_clearance_and_its_text() {
    let models = standard_equipment_models();
    for kind in EquipmentKind::ALL {
        let m = equipment_models_of(&models, kind).into_iter().next().unwrap();
        let p = EquipmentParams::from_model(&m, "G1", units());
        let pieces = p.generate().unwrap();
        assert!(pieces.iter().any(|x| x.layer == EQUIPMENT_LAYER && matches!(x.kind, EntityKind::LwPolyline(_))), "{kind:?}");
        let texts: Vec<_> = pieces.iter().filter_map(|x| if let EntityKind::Text(t) = &x.kind { Some(t.value.clone()) } else { None }).collect();
        assert!(texts.iter().any(|t| t.starts_with("G1")), "{kind:?}: {texts:?}");
        assert!(texts.iter().any(|t| t.starts_with("Dim. ")), "{kind:?}: the sizes are written");
        assert!(!texts.iter().any(|t| t.contains("Poids")), "{kind:?}: no weight unless it is given");
        let zone = pieces.iter().filter(|x| x.layer == EQUIPMENT_ZONE_LAYER).count();
        assert_eq!(zone > 0, kind.is_outdoor(), "{kind:?}: only outdoor units show a service clearance by default");
    }
    // Fans: one circle each, in two rows over a wide unit.
    let m = equipment_model_named(&models, "Chiller 523 kW").unwrap();
    let circles = |p: &EquipmentParams| p.generate().unwrap().iter().filter(|x| matches!(x.kind, EntityKind::Circle(_))).count();
    let mut p = EquipmentParams::from_model(&m, "CH", units());
    assert_eq!(circles(&p), 8);
    p.fans = 0;
    assert_eq!(circles(&p), 0);
    p.clearance_mm = 0.0;
    assert!(p.generate().unwrap().iter().all(|x| x.layer != EQUIPMENT_ZONE_LAYER));
}

#[test]
fn an_equipment_keeps_its_data_and_each_value_can_be_changed() {
    let models = standard_equipment_models();
    let m = equipment_model_named(&models, "Chiller 93.4 kW").unwrap();
    let mut d = Drawing::new_metric();
    let p = EquipmentParams::from_model(&m, "CH1", units());
    let h = d.add_equipment(&Space::Model, Common::default(), &p, Vec2::new(1000.0, 2000.0), 0.0).unwrap();
    let got = equipment_of(d.entity(h).unwrap()).unwrap();
    assert_eq!((got.name.as_str(), got.model.as_str(), got.capacity_kw, got.length_mm), ("CH1", "Chiller 93.4 kW", Some(93.4), 2275.0));
    // Everything is the user's: name, power, sizes, weight, fans, clearance, connections, reference.
    let mut next = got.clone();
    next.name = "CH-TOIT".into();
    next.capacity_kw = Some(100.0);
    (next.length_mm, next.width_mm, next.height_mm) = (3000.0, 2200.0, 1500.0);
    next.weight_kg = Some(900.0);
    next.fans = 3;
    next.clearance_mm = 1500.0;
    next.connections = "Victaulic 3\"".into();
    next.reference = "Mon modèle".into();
    d.regenerate_equipment(h, &next).unwrap();
    assert_eq!(equipment_of(d.entity(h).unwrap()).unwrap(), next);
    // Refused values.
    let bad = |f: &dyn Fn(&mut EquipmentParams)| {
        let mut q = got.clone();
        f(&mut q);
        q.validate().is_err()
    };
    assert!(bad(&|q| q.length_mm = 0.0));
    assert!(bad(&|q| q.capacity_kw = Some(-1.0)));
    assert!(bad(&|q| q.fans = 99));
    assert!(bad(&|q| q.clearance_mm = f64::NAN));
    assert!(bad(&|q| q.name = String::new()));
}
