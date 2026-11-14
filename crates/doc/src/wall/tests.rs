use super::*;
use crate::{Drawing, standard_wall_styles};

fn style(name: &str) -> WallStyle {
    standard_wall_styles().into_iter().find(|s| s.name == name).expect("standard style")
}

fn brick() -> WallStyle {
    style("Brick 200 + mineral wool 120 + plasterboard")
}

fn wall(style: WallStyle, path: &[(f64, f64)], closed: bool, justification: WallJustification) -> WallParams {
    WallParams {
        style,
        path: path.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        closed,
        justification,
        height: 2500.0,
        units: WallUnits::default(),
        joints: Default::default(),
        start_free: false,
        end_free: false,
    }
}

fn points(k: &EntityKind) -> Vec<(f64, f64)> {
    let round = |v: f64| (v * 1e6).round() / 1e6;
    match k {
        EntityKind::LwPolyline(p) => p.vertices.iter().map(|v| (round(v.p.x), round(v.p.y))).collect(),
        EntityKind::Line(l) => vec![(round(l.a.x), round(l.a.y)), (round(l.b.x), round(l.b.y))],
        _ => Vec::new(),
    }
}

fn lines(p: &[WallPiece]) -> Vec<Vec<(f64, f64)>> {
    p.iter().filter(|x| x.layer == WALL_LAYER).map(|x| points(&x.kind)).collect()
}

fn hatches(p: &[WallPiece]) -> Vec<&Hatch> {
    p.iter()
        .filter_map(|x| match &x.kind {
            EntityKind::Hatch(h) => Some(h),
            _ => None,
        })
        .collect()
}

#[test]
fn the_standard_styles_are_valid_and_measure_what_their_names_say() {
    let all = standard_wall_styles();
    assert_eq!(all.len(), 23);
    let mut names: Vec<String> = all.iter().map(|s| s.name.to_lowercase()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), all.len(), "style names are unique");
    for s in &all {
        s.validate().unwrap_or_else(|e| panic!("{}: {e}", s.name));
    }
    let thickness: Vec<f64> = all.iter().map(WallStyle::thickness).collect();
    assert_eq!(thickness[..6], [200.0, 332.5, 327.5, 256.5, 73.0, 98.0]);
    assert!(thickness.iter().all(|t| *t > 0.0));
}

#[test]
fn a_straight_wall_has_a_line_per_boundary_two_end_lines_and_a_hatch_per_hatched_component() {
    let p = wall(brick(), &[(0.0, 0.0), (1000.0, 0.0)], false, WallJustification::Left).generate().expect("geometry");
    assert_eq!(
        lines(&p),
        [
            vec![(0.0, 0.0), (1000.0, 0.0)],
            vec![(0.0, -200.0), (1000.0, -200.0)],
            vec![(0.0, -320.0), (1000.0, -320.0)],
            vec![(0.0, -332.5), (1000.0, -332.5)],
            vec![(0.0, 0.0), (0.0, -332.5)],
            vec![(1000.0, 0.0), (1000.0, -332.5)],
        ]
    );
    // Brick and mineral wool are hatched, the plasterboard is not.
    let h = hatches(&p);
    assert_eq!(h.iter().map(|h| h.pattern.as_str()).collect::<Vec<_>>(), ["ANSI31", "ZIGZAG"]);
    assert_eq!(h.first().map(|h| h.loops.len()), Some(1));
    assert!(p.iter().all(|x| matches!(x.kind, EntityKind::Hatch(_)) == (x.layer == WALL_PATTERN_LAYER)));
}

#[test]
fn justification_puts_the_path_on_the_exterior_face_the_middle_or_the_interior_face() {
    let faces = |j| {
        let p = wall(style("Concrete 200"), &[(0.0, 0.0), (1000.0, 0.0)], false, j).generate().expect("geometry");
        let l = lines(&p);
        (l.first().and_then(|v| v.first().copied()), l.get(1).and_then(|v| v.first().copied()))
    };
    assert_eq!(faces(WallJustification::Left), (Some((0.0, 0.0)), Some((0.0, -200.0))));
    assert_eq!(faces(WallJustification::Center), (Some((0.0, 100.0)), Some((0.0, -100.0))));
    assert_eq!(faces(WallJustification::Right), (Some((0.0, 200.0)), Some((0.0, 0.0))));
}

#[test]
fn a_corner_is_mitered_on_every_boundary() {
    // Drawn clockwise: the exterior is outside the corner.
    let p =
        wall(style("Concrete 200"), &[(0.0, 0.0), (1000.0, 0.0), (1000.0, -1000.0)], false, WallJustification::Left).generate().expect("geometry");
    let l = lines(&p);
    assert_eq!(l.first(), Some(&vec![(0.0, 0.0), (1000.0, 0.0), (1000.0, -1000.0)]));
    assert_eq!(l.get(1), Some(&vec![(0.0, -200.0), (800.0, -200.0), (800.0, -1000.0)]));
}

#[test]
fn a_closed_wall_is_a_ring_without_end_lines() {
    let path = [(0.0, 0.0), (5000.0, 0.0), (5000.0, -4000.0), (0.0, -4000.0)];
    let p = wall(brick(), &path, true, WallJustification::Left).generate().expect("geometry");
    let l: Vec<&WallPiece> = p.iter().filter(|x| x.layer == WALL_LAYER).collect();
    assert_eq!(l.len(), 4, "one closed line per boundary, no end lines");
    assert!(l.iter().all(|x| matches!(&x.kind, EntityKind::LwPolyline(pl) if pl.closed && pl.vertices.len() == 4)));
    assert_eq!(points(&l[3].kind), [(332.5, -332.5), (4667.5, -332.5), (4667.5, -3667.5), (332.5, -3667.5)]);
    let h = hatches(&p);
    assert!(h.iter().all(|h| h.loops.len() == 2 && h.loops[0].outer && !h.loops[1].outer));
}

#[test]
fn membranes_are_lines_and_air_gaps_are_not_hatched() {
    let p = wall(style("Timber frame 145"), &[(0.0, 0.0), (1000.0, 0.0)], false, WallJustification::Left).generate().expect("geometry");
    // Eight components with two membranes: 9 boundaries, the membranes sharing a position with
    // the boundary before them, so 7 distinct lines, plus 2 end lines.
    assert_eq!(lines(&p).len(), 9);
    let patterns: Vec<&str> = hatches(&p).iter().map(|h| h.pattern.as_str()).collect();
    assert_eq!(patterns, ["ANSI32", "ANSI33", "ZIGZAG"]);
}

#[test]
fn paths_that_cannot_be_drawn_cleanly_are_refused() {
    let c = || style("Concrete 200");
    let refused = |path: &[(f64, f64)], closed| wall(c(), path, closed, WallJustification::Center).generate().is_err();
    assert!(refused(&[(0.0, 0.0), (0.0, 0.0)], false), "two points in the same place");
    assert!(refused(&[(0.0, 0.0), (1000.0, 0.0), (500.0, 0.0)], false), "folds back");
    assert!(refused(&[(0.0, 0.0), (1000.0, 0.0), (0.0, 100.0)], false), "corner sharper than the miter limit");
    assert!(refused(&[(0.0, 0.0), (1000.0, 0.0), (1000.0, 50.0), (2000.0, 50.0)], false), "segment shorter than the corners need");
    assert!(refused(&[(0.0, 0.0), (1000.0, 0.0)], true), "a closed wall needs three points");
    assert!(!refused(&[(0.0, 0.0), (1000.0, 0.0), (1000.0, 1000.0)], false));
}

#[test]
fn hostile_parameters_are_refused() {
    let ok = wall(brick(), &[(0.0, 0.0), (1000.0, 0.0)], false, WallJustification::Left);
    assert!(ok.validate().is_ok());
    let with = |f: &dyn Fn(&mut WallParams)| {
        let mut p = ok.clone();
        f(&mut p);
        p.validate().is_err()
    };
    assert!(with(&|p| p.style.components[0].thickness = f64::NAN));
    assert!(with(&|p| p.style.components[0].thickness = -1.0));
    assert!(with(&|p| p.style.components[0].thickness = f64::INFINITY));
    assert!(with(&|p| p.style.components[0].thickness = 0.0), "only membranes have no thickness");
    assert!(with(&|p| p.style.components[0].function = ComponentFunction::Membrane), "a membrane has no thickness");
    assert!(with(&|p| p.style.components[0].pattern_scale = 0.0));
    assert!(with(&|p| p.style.components[0].name = " ".into()));
    assert!(with(&|p| p.style.name = "x".repeat(MAX_WALL_NAME_CHARS + 1)));
    assert!(with(&|p| p.style.components = vec![p.style.components[0].clone(); MAX_WALL_COMPONENTS + 1]));
    assert!(with(&|p| p.style.components.clear()));
    assert!(with(&|p| p.height = 0.0));
    assert!(with(&|p| p.height = f64::NAN));
    assert!(with(&|p| p.units.mm_per_unit = 0.0));
    assert!(with(&|p| p.path[1] = Vec2::new(f64::NAN, 0.0)));
    assert!(with(&|p| p.path = (0..=MAX_WALL_VERTICES).map(|i| Vec2::new(i as f64 * 1000.0, 0.0)).collect()));
}

#[test]
fn a_wall_survives_its_extended_data() {
    let mut s = brick();
    s.name = "Brique \"pleine\" 20 cm \\ façade".into();
    s.components[1].name = "Laine de verre  —  é".into();
    let p = wall(s, &[(0.0, 0.0), (1234.5, 0.0), (1234.5, -3000.0)], false, WallJustification::Center);
    let x = p.to_xdata().expect("xdata");
    assert_eq!(x.first().map(|t| (t.code, t.value.as_str())), Some((1001, WALL_APP)));
    for t in x.iter().skip(1) {
        assert_eq!(t.code, 1000);
        assert!(!t.value.starts_with(' ') && !t.value.ends_with(' '), "{:?}", t.value);
        let cost: usize = t.value.chars().map(|c| if c.is_ascii() { 1 } else { 7 * c.len_utf16() }).sum();
        assert!(cost <= 240, "{cost}");
        assert!(!t.value.contains("\\U+"), "{:?}", t.value);
    }
    assert_eq!(WallParams::from_xdata(&x), Some(p.clone()));
    // Other applications' data stays around ours.
    let other = vec![tag(1001, "OTHER"), tag(1000, "keep")];
    let mixed = p.replace_xdata(&[other.clone(), x.clone()].concat()).expect("replace");
    assert_eq!(without_wall_xdata(&mixed), other);
    assert_eq!(WallParams::from_xdata(&mixed), Some(p));
}

#[test]
fn damaged_extended_data_is_not_a_wall() {
    let p = wall(brick(), &[(0.0, 0.0), (1000.0, 0.0)], false, WallJustification::Left);
    let x = p.to_xdata().expect("xdata");
    assert_eq!(WallParams::from_xdata(&x[..x.len() - 1]), None, "truncated JSON");
    assert_eq!(WallParams::from_xdata(&[tag(1001, "OTHER"), tag(1000, "WALL1")]), None);
    assert_eq!(WallParams::from_xdata(&[tag(1001, WALL_APP), tag(1000, "WALL9"), tag(1000, "{}")]), None);
    let huge: Vec<RawTag> =
        [tag(1001, WALL_APP), tag(1000, "WALL1")].into_iter().chain(std::iter::repeat_n(tag(1000, &"x".repeat(240)), 100)).collect();
    assert_eq!(WallParams::from_xdata(&huge), None);
    // Valid JSON with parameters that can't be drawn.
    let mut bad = p.clone();
    bad.path = vec![Vec2::ZERO, Vec2::ZERO];
    let json = serde_json::to_string(&bad).expect("json");
    assert_eq!(WallParams::from_xdata(&[tag(1001, WALL_APP), tag(1000, "WALL1"), tag(1000, &json)]), None);
}

fn entities(p: &WallParams) -> Vec<crate::Entity> {
    p.generate()
        .expect("geometry")
        .into_iter()
        .map(|x| crate::Entity {
            handle: crate::Handle(1),
            common: crate::Common { layer: x.layer.into(), color: x.color, ..Default::default() },
            kind: x.kind,
        })
        .collect()
}

#[test]
fn a_wall_matches_its_own_geometry_and_not_an_edited_one() {
    let p = wall(brick(), &[(0.0, 0.0), (1000.0, 0.0), (1000.0, -2000.0)], false, WallJustification::Left);
    let own = entities(&p);
    assert!(p.matches(&own));
    let mut moved = own.clone();
    if let Some(EntityKind::LwPolyline(l)) = moved.first_mut().map(|e| &mut e.kind) {
        l.vertices[1].p.x += 1.0;
    }
    assert!(!p.matches(&moved), "a vertex moved");
    let mut relayered = own.clone();
    relayered[0].common.layer = "Other".into();
    assert!(!p.matches(&relayered), "an object moved to another layer");
    let mut recoloured = own.clone();
    recoloured[0].common.color = Color::Index(1);
    assert!(!p.matches(&recoloured), "an object recoloured");
    assert!(!p.matches(&own[..own.len() - 1]), "an object deleted");
    let mut more = own;
    more.push(crate::Entity::new(crate::Handle(2), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 0.0, 0.0) })));
    assert!(!p.matches(&more), "an object added");
}

#[test]
fn a_closed_wall_drawn_either_way_marks_its_larger_ring_as_outer() {
    let cw = [(0.0, 0.0), (5000.0, 0.0), (5000.0, -4000.0), (0.0, -4000.0)];
    let ccw: Vec<(f64, f64)> = cw.iter().rev().copied().collect();
    for path in [cw.to_vec(), ccw] {
        let g = wall(style("Concrete 200"), &path, true, WallJustification::Left).generate().expect("geometry");
        let h = hatches(&g);
        let rings = &h[0].loops;
        let area = |l: &HatchLoop| shoelace(&l.vertices.iter().map(|v| v.p).collect::<Vec<_>>()).abs();
        let (outer, inner) = if rings[0].outer { (&rings[0], &rings[1]) } else { (&rings[1], &rings[0]) };
        assert!(outer.outer && !inner.outer);
        assert!(area(outer) > area(inner), "{path:?}");
    }
}

#[test]
fn an_air_gap_is_never_hatched() {
    let mut s = style("Timber frame 145");
    for c in &mut s.components {
        if c.function == ComponentFunction::AirGap {
            c.pattern = "ANSI31".into();
        }
    }
    let g = wall(s, &[(0.0, 0.0), (1000.0, 0.0)], false, WallJustification::Left).generate().expect("geometry");
    assert_eq!(hatches(&g).len(), 3);
}

#[test]
fn a_valid_wall_always_survives_its_extended_data() {
    // Long names in plain, backslash-heavy and non-ASCII text, with the most points: whatever
    // `validate` accepts, the extended data must give back.
    for ch in ['a', '\\', 'é', '\u{1f9f1}'] {
        let mut s = brick();
        s.name = std::iter::repeat_n(ch, MAX_WALL_NAME_CHARS).collect();
        s.description = std::iter::repeat_n(ch, MAX_WALL_DESCRIPTION_CHARS).collect();
        s.components = (0..MAX_WALL_COMPONENTS)
            .map(|i| {
                WallComponent::new(
                    &std::iter::repeat_n(ch, MAX_WALL_NAME_CHARS - 2).chain(format!("{i:02}").chars()).collect::<String>(),
                    10.0,
                    ComponentFunction::Structure,
                )
            })
            .collect();
        let path: Vec<(f64, f64)> =
            (0..MAX_WALL_VERTICES).map(|i| (i as f64 * 1_000.123_456_789, if i % 2 == 0 { 0.0 } else { 999.987_654_321 })).collect();
        let p = wall(s, &path, false, WallJustification::Left);
        match p.validate() {
            Ok(()) => assert_eq!(WallParams::from_xdata(&p.to_xdata().expect("xdata")), Some(p), "{ch:?}"),
            Err(e) => assert!(p.to_xdata().is_err(), "{ch:?}: refused for {e}, but its xdata was written"),
        }
    }
}

#[test]
fn units_turn_millimetres_into_drawing_units() {
    let mut d = Drawing::new_metric();
    assert_eq!(WallUnits::of(&d), WallUnits { mm_per_unit: 1.0, metric_patterns: true });
    d.header.set_i64("INSUNITS", 6);
    let metres = WallUnits::of(&d);
    assert_eq!(metres.mm_per_unit, 1000.0);
    let mut p = wall(style("Concrete 200"), &[(0.0, 0.0), (10.0, 0.0)], false, WallJustification::Left);
    p.units = metres;
    let g = p.generate().expect("geometry");
    assert_eq!(lines(&g).get(1), Some(&vec![(0.0, -0.2), (10.0, -0.2)]));
    // An imperial drawing in inches draws the same pattern density as a metric one in mm.
    let imperial = WallUnits::of(&Drawing::new_imperial());
    assert_eq!(imperial, WallUnits { mm_per_unit: 25.4, metric_patterns: false });
    assert!((imperial.pattern_scale(6.0) - 6.0).abs() < 1e-12);
    assert!((metres.pattern_scale(6.0) - 0.006).abs() < 1e-12);
}

#[test]
fn a_drawing_style_takes_the_place_of_a_standard_one_of_the_same_name() {
    let mut d = Drawing::new_metric();
    assert_eq!(d.available_wall_styles().len(), 23);
    let mut mine = style("Concrete 200");
    mine.name = "concrete 200".into();
    mine.components[0].thickness = 180.0;
    d.wall_styles.push(mine.clone());
    let all = d.available_wall_styles();
    assert_eq!(all.len(), 23);
    assert_eq!(all.first(), Some(&mine));
    assert_eq!(d.wall_style_or_standard("CONCRETE 200"), Some(mine));
    assert!(d.wall_style_or_standard("Partition 72/48").is_some());
    assert!(d.wall_style_or_standard("nope").is_none());
}

fn insert_of(d: &Drawing, h: crate::Handle) -> crate::Insert {
    match &d.entity(h).expect("wall").kind {
        EntityKind::Insert(i) => i.clone(),
        k => panic!("not a block reference: {k:?}"),
    }
}

#[test]
fn a_wall_is_a_block_reference_at_its_first_point_carrying_its_data() {
    let mut d = Drawing::new_metric();
    let p = wall(brick(), &[(1000.0, 2000.0), (4000.0, 2000.0), (4000.0, -1000.0)], false, WallJustification::Left);
    let h = d.add_wall(&crate::Space::Model, crate::Common::default(), p).expect("add");
    let e = d.entity(h).expect("wall").clone();
    assert_eq!(e.common.layer, WALL_LAYER);
    let i = insert_of(&d, h);
    assert_eq!(i.insert, Vec3::new(1000.0, 2000.0, 0.0));
    let kept = wall_of(&e).expect("wall data");
    assert_eq!(kept.path, [Vec2::ZERO, Vec2::new(3000.0, 0.0), Vec2::new(3000.0, -3000.0)], "path in block coordinates");
    let b = d.block(&i.block).expect("block");
    assert!(b.anonymous);
    assert!(b.entities.iter().all(|e| e.common.layer == WALL_LAYER || e.common.layer == WALL_PATTERN_LAYER));
    assert_eq!(d.layer(WALL_PATTERN_LAYER).map(|l| l.color), Some(Color::Index(8)));
    assert!(d.wall_is_intact(h));
}

#[test]
fn regenerating_a_wall_replaces_its_block_and_keeps_a_copy_s_block() {
    let mut d = Drawing::new_metric();
    let h = d
        .add_wall(&crate::Space::Model, crate::Common::default(), wall(brick(), &[(0.0, 0.0), (3000.0, 0.0)], false, WallJustification::Left))
        .expect("add");
    let first = insert_of(&d, h).block;
    // A copy of the reference shares the block until one of them changes.
    let copy = d.entity(h).expect("wall").as_ref().clone();
    let c = d.add(&crate::Space::Model, copy.common, copy.kind).expect("copy");
    let mut p = wall_of(d.entity(h).expect("wall")).expect("data");
    p.style = style("Concrete 200");
    d.regenerate_wall(h, &p).expect("regenerate");
    let second = insert_of(&d, h).block;
    assert_ne!(first, second);
    assert!(d.block(&first).is_some(), "the copy still uses the first block");
    assert_eq!(wall_of(d.entity(h).expect("wall")).map(|w| w.style.name), Some("Concrete 200".to_string()));
    assert!(d.wall_is_intact(h) && d.wall_is_intact(c));
    // Once nothing refers to a wall block, regenerating drops it.
    d.regenerate_wall(c, &p).expect("regenerate copy");
    assert!(d.block(&first).is_none());
}

#[test]
fn an_edited_or_scaled_wall_is_no_longer_intact() {
    let mut d = Drawing::new_metric();
    let h = d
        .add_wall(&crate::Space::Model, crate::Common::default(), wall(brick(), &[(0.0, 0.0), (3000.0, 0.0)], false, WallJustification::Left))
        .expect("add");
    let name = insert_of(&d, h).block;
    let mut scaled = d.clone();
    scaled
        .modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.scale = Vec3::new(2.0, 1.0, 1.0)
            }
        })
        .expect("scale");
    assert!(!scaled.wall_is_intact(h));
    let mut mirrored = d.clone();
    mirrored
        .modify_entity(h, |e| {
            if let EntityKind::Insert(i) = &mut e.kind {
                i.scale = Vec3::new(-1.0, 1.0, 1.0)
            }
        })
        .expect("mirror");
    assert!(mirrored.wall_is_intact(h), "a mirrored wall is still the same wall");
    let mut b = d.block(&name).expect("block").as_ref().clone();
    let extra = d.new_handle();
    b.entities.push(crate::Entity {
        handle: extra,
        common: crate::Common::default(),
        kind: EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(1.0, 1.0, 0.0) }),
    });
    d.blocks.insert(name, std::sync::Arc::new(b));
    assert!(!d.wall_is_intact(h));
}
