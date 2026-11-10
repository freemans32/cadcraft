use cadcraft_geom::Vec2;
use cadcraft_hvac::RoomUse;

use super::*;
use crate::{Common, Drawing, EntityKind, RawTag, Space};

fn office(outline: &[(f64, f64)], occupants: u32) -> SpaceParams {
    SpaceParams {
        name: "Bureau 1".into(),
        room: RoomUse::Office,
        occupants,
        collective: true,
        flow_m3h: None,
        outline: outline.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        label_height_mm: 200.0,
        units: HvacUnits::default(),
    }
}

const SIX_BY_FIVE: &[(f64, f64)] = &[(0.0, 0.0), (6000.0, 0.0), (6000.0, 5000.0), (0.0, 5000.0)];

fn texts(pieces: &[HvacPiece]) -> Vec<String> {
    pieces
        .iter()
        .filter_map(|p| match &p.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_space_knows_its_area_and_the_fresh_air_it_needs() {
    let s = office(SIX_BY_FIVE, 4);
    assert!((s.area_m2() - 30.0).abs() < 1e-9);
    assert_eq!(s.flow().ok(), Some(100.0));
    let set = SpaceParams { flow_m3h: Some(180.0), ..s.clone() };
    assert_eq!(set.flow().ok(), Some(180.0));
    assert_eq!(texts(&s.generate().unwrap()), ["Bureau 1", "30.0 m²", "100 m³/h"]);
    // In metres, the same room is 6 × 5 units.
    let m = SpaceParams {
        units: HvacUnits { mm_per_unit: 1000.0 },
        outline: SIX_BY_FIVE.iter().map(|&(x, y)| Vec2::new(x / 1000.0, y / 1000.0)).collect(),
        ..s
    };
    assert!((m.area_m2() - 30.0).abs() < 1e-9);
}

#[test]
fn a_space_label_stays_inside_an_l_shaped_room() {
    let l = office(&[(0.0, 0.0), (10.0, 0.0), (10.0, 1.0), (1.0, 1.0), (1.0, 10.0), (0.0, 10.0)], 1);
    assert!(cadcraft_geom::point_in_polygon(&l.outline, l.label_point()));
}

#[test]
fn bad_spaces_are_refused() {
    let ok = office(SIX_BY_FIVE, 4);
    assert!(ok.validate().is_ok());
    let bad = |s: SpaceParams| s.validate().is_err();
    assert!(bad(SpaceParams { name: " ".into(), ..ok.clone() }));
    assert!(bad(SpaceParams { occupants: MAX_OCCUPANTS + 1, ..ok.clone() }));
    assert!(bad(SpaceParams { flow_m3h: Some(f64::NAN), ..ok.clone() }));
    assert!(bad(SpaceParams { flow_m3h: Some(0.0), ..ok.clone() }));
    assert!(bad(office(&[(0.0, 0.0), (1.0, 0.0)], 1)));
    assert!(bad(office(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)], 1)), "no area");
    assert!(bad(office(&[(0.0, 0.0), (f64::INFINITY, 0.0), (1.0, 1.0)], 1)));
    assert!(bad(SpaceParams { label_height_mm: 0.0, ..ok.clone() }));
    assert!(bad(SpaceParams { units: HvacUnits { mm_per_unit: -1.0 }, ..ok }));
}

#[test]
fn a_space_is_a_block_reference_carrying_its_data() {
    let mut d = Drawing::new_metric();
    let mut s = office(SIX_BY_FIVE, 4);
    for v in &mut s.outline {
        *v += Vec2::new(1000.0, 2000.0);
    }
    let h = d.add_space(&Space::Model, Common::default(), s).unwrap();
    let e = d.entity(h).unwrap();
    assert_eq!(e.common.layer, SPACE_LAYER);
    let back = space_of(e).unwrap();
    assert_eq!(back.outline.first().copied(), Some(Vec2::ZERO), "outline kept in the block's coordinates");
    let (_, world) = d.space_outline(h).unwrap();
    assert_eq!(world.get(2).copied(), Some(Vec2::new(7000.0, 7000.0)));
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    assert_eq!(d.block(&i.block).unwrap().entities.len(), 4, "outline and three labels");
}

#[test]
fn xdata_survives_backslashes_and_long_names_and_keeps_other_apps() {
    let mut s = office(SIX_BY_FIVE, 4);
    s.name = "Salle \\U+0041 é".into();
    let tags = s.to_xdata().unwrap();
    assert!(tags.iter().all(|t| t.value.len() < 255));
    assert!(!tags.iter().any(|t| t.value.contains("\\U+")));
    assert_eq!(SpaceParams::from_xdata(&tags), Some(s.clone()));
    let other = vec![RawTag { code: 1001, value: "OTHER".into() }, RawTag { code: 1000, value: "keep".into() }];
    let merged = xdata::replace(SPACE_APP, &other, tags);
    assert_eq!(merged.get(1).map(|t| t.value.as_str()), Some("keep"));
    assert_eq!(SpaceParams::from_xdata(&merged), Some(s));
    // Wrong format, junk JSON, or too much data: no space.
    assert_eq!(SpaceParams::from_xdata(&[RawTag { code: 1001, value: SPACE_APP.into() }, RawTag { code: 1000, value: "SPACE9".into() }]), None);
    let junk =
        [RawTag { code: 1001, value: SPACE_APP.into() }, RawTag { code: 1000, value: "SPACE1".into() }, RawTag { code: 1000, value: "{".into() }];
    assert_eq!(SpaceParams::from_xdata(&junk), None);
    let huge: Vec<RawTag> = [RawTag { code: 1001, value: SPACE_APP.into() }, RawTag { code: 1000, value: "SPACE1".into() }]
        .into_iter()
        .chain(std::iter::repeat_n(RawTag { code: 1000, value: "x".repeat(240) }, 100))
        .collect();
    assert_eq!(SpaceParams::from_xdata(&huge), None);
}

#[test]
fn standard_models_are_valid_and_the_smallest_fitting_one_is_chosen() {
    let models = standard_air_terminal_models();
    assert!(models.iter().all(|m| m.validate().is_ok()));
    let pick = |k, f| smallest_model_for(&models, k, f).map(|m| m.name);
    assert_eq!(pick(TerminalKind::SquareCeiling, 250.0).as_deref(), Some("SQ600-160"));
    assert_eq!(pick(TerminalKind::SquareCeiling, 251.0).as_deref(), Some("SQ600-200"));
    assert_eq!(pick(TerminalKind::Round, 120.0).as_deref(), Some("RD160"));
    assert_eq!(pick(TerminalKind::Extract, 75.0).as_deref(), Some("EX125"));
    assert_eq!(pick(TerminalKind::SquareCeiling, 401.0), None);
}

fn terminal(name: &str, flow: f64) -> TerminalParams {
    let model = standard_air_terminal_models().into_iter().find(|m| m.name == name).unwrap();
    TerminalParams { model, flow_m3h: flow, air: AirDirection::Supply, label_height_mm: 100.0, units: HvacUnits::default() }
}

#[test]
fn every_kind_of_terminal_is_drawn_to_scale_with_its_label() {
    for name in ["SQ600-160", "RD200", "RG300x150", "LN1000-2", "EX125"] {
        let t = terminal(name, 50.0);
        let pieces = t.generate().unwrap();
        assert_eq!(texts(&pieces), [format!("{name} 50 m³/h")]);
        assert!(pieces.iter().all(|p| p.layer == AIR_TERMINAL_LAYER));
    }
    // The square diffuser: 600 mm face, Ø160 neck, four lines to the corners.
    let pieces = terminal("SQ600-160", 200.0).generate().unwrap();
    let EntityKind::LwPolyline(face) = &pieces[0].kind else { panic!("face") };
    assert_eq!(face.vertices.get(2).map(|v| v.p), Some(Vec2::new(300.0, 300.0)));
    let EntityKind::Circle(neck) = &pieces[1].kind else { panic!("neck") };
    assert_eq!(neck.radius, 80.0);
    let corners = pieces.iter().filter(|p| matches!(&p.kind, EntityKind::Line(l) if l.b.x.abs() == 300.0 && l.b.y.abs() == 300.0));
    assert_eq!(corners.count(), 4);
}

#[test]
fn bad_terminals_are_refused() {
    let ok = terminal("RD160", 100.0);
    let bad = |t: TerminalParams| t.validate().is_err();
    assert!(bad(TerminalParams { flow_m3h: 0.0, ..ok.clone() }));
    assert!(bad(TerminalParams { flow_m3h: f64::NAN, ..ok.clone() }));
    let mut m = ok.clone();
    m.model.neck_mm = -1.0;
    assert!(bad(m));
    let mut m = ok.clone();
    m.model.max_flow_m3h = f64::INFINITY;
    assert!(bad(m));
    let mut lin = terminal("LN1000-2", 100.0);
    lin.model.slots = 0;
    assert!(bad(lin));
    let mut d = Drawing::new_metric();
    assert!(d.add_air_terminal(&Space::Model, Common::default(), &ok, Vec2::new(f64::NAN, 0.0), 0.0).is_err());
    assert!(d.model.iter().next().is_none());
}

#[test]
fn a_terminal_is_placed_turned_and_read_back() {
    let mut d = Drawing::new_metric();
    let t = terminal("RG300x150", 150.0);
    let h = d.add_air_terminal(&Space::Model, Common::default(), &t, Vec2::new(10.0, 20.0), 1.0).unwrap();
    let e = d.entity(h).unwrap();
    assert_eq!(e.common.layer, AIR_TERMINAL_LAYER);
    assert_eq!(air_terminal_of(e), Some(t));
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    assert_eq!((i.insert.x, i.insert.y, i.rotation), (10.0, 20.0, 1.0));
}

#[test]
fn a_drawing_model_replaces_the_standard_one_of_its_name() {
    let mut d = Drawing::new_metric();
    let mut m = standard_air_terminal_models().into_iter().next().unwrap();
    m.max_flow_m3h = 300.0;
    d.air_terminal_models.push(m);
    let all = d.available_air_terminal_models();
    assert_eq!(all.iter().filter(|x| x.name == "SQ600-160").count(), 1);
    assert_eq!(all.first().map(|x| x.max_flow_m3h), Some(300.0));
}

#[test]
fn terminals_spread_evenly_inside_a_room() {
    let rect: Vec<Vec2> = SIX_BY_FIVE.iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let g = distribute_in_polygon(&rect, Distribution::Grid { rows: 2, cols: 3 }).unwrap();
    assert_eq!(g.first().copied(), Some(Vec2::new(1000.0, 1250.0)));
    assert_eq!(g.len(), 6);
    for n in [1, 2, 4, 5, 6, 9] {
        let pts = distribute_in_polygon(&rect, Distribution::Count(n)).unwrap();
        assert_eq!(pts.len(), n);
        assert!(pts.iter().all(|p| cadcraft_geom::point_in_polygon(&rect, *p)));
    }
    let l: Vec<Vec2> = [(0.0, 0.0), (10.0, 0.0), (10.0, 1.0), (1.0, 1.0), (1.0, 10.0), (0.0, 10.0)].iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let pts = distribute_in_polygon(&l, Distribution::Count(4)).unwrap();
    assert_eq!(pts.len(), 4);
    assert!(pts.iter().all(|p| cadcraft_geom::point_in_polygon(&l, *p)));
    assert!(distribute_in_polygon(&rect, Distribution::Count(0)).is_err());
    assert!(distribute_in_polygon(&rect, Distribution::Count(MAX_DISTRIBUTED + 1)).is_err());
    assert!(distribute_in_polygon(&rect, Distribution::Grid { rows: 1000, cols: 1000 }).is_err());
    assert!(distribute_in_polygon(&[], Distribution::Count(1)).is_err());
}
