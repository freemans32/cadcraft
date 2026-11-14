use cadcraft_geom::Vec2;

use super::*;
use crate::{Common, Drawing, Handle, Space, WallJustification, WallParams, standard_wall_styles, wall_of};

fn brick() -> crate::WallStyle {
    standard_wall_styles().into_iter().find(|s| s.name.starts_with("Brick 200")).expect("a brick style")
}

fn door() -> OpeningParams {
    OpeningParams::new(OpeningKind::Door, WallUnits::default())
}

/// A 5 m brick wall along x from the origin, its path the reference line, exterior on the left.
fn with_wall() -> (Drawing, Handle) {
    let mut d = Drawing::new_metric();
    let p = WallParams {
        style: brick(),
        path: vec![Vec2::ZERO, Vec2::new(5000.0, 0.0)],
        closed: false,
        justification: WallJustification::Left,
        height: 2500.0,
        units: WallUnits::default(),
        joints: Default::default(),
        bulges: Vec::new(),
        start_free: false,
        end_free: false,
    };
    let h = d.add_wall(&Space::Model, Common::default(), p).unwrap();
    (d, h)
}

fn add(d: &mut Drawing, kind: OpeningKind, x: f64) -> Handle {
    d.add_opening(&Space::Model, Common::default(), &OpeningParams::new(kind, WallUnits::default()), Vec2::new(x, 0.0), 0.0).unwrap()
}

/// The straight pieces of the wall's block (lines and the sides of polylines), as (a, b) pairs.
fn wall_lines(d: &Drawing, h: Handle) -> Vec<(Vec2, Vec2)> {
    let EntityKind::Insert(i) = &d.entity(h).unwrap().kind else { panic!("not a block reference") };
    let mut out = Vec::new();
    for e in d.block(&i.block).unwrap().entities.iter() {
        match &e.kind {
            EntityKind::Line(l) => out.push((l.a.xy(), l.b.xy())),
            EntityKind::LwPolyline(p) => {
                let n = p.vertices.len();
                let sides = if p.closed { n } else { n.saturating_sub(1) };
                out.extend((0..sides).map(|k| (p.vertices[k].p, p.vertices[(k + 1) % n].p)));
            }
            _ => {}
        }
    }
    out
}

/// Whether some line of the wall runs along x across `x` (a face or a layer boundary).
fn wall_runs_across(d: &Drawing, h: Handle, x: f64) -> bool {
    wall_lines(d, h).iter().any(|(a, b)| (a.y - b.y).abs() < 1e-9 && a.x.min(b.x) < x && x < a.x.max(b.x))
}

#[test]
fn every_kind_of_opening_is_drawn_across_the_wall_between_its_jambs() {
    for kind in OpeningKind::ALL {
        let p = OpeningParams::new(kind, WallUnits::default());
        let pieces = p.generate().unwrap();
        let jambs = pieces.iter().filter(|x| matches!(&x.kind, EntityKind::Line(l) if (l.a.x - l.b.x).abs() < 1e-9 && l.a.x.abs() > 0.0)).count();
        assert!(jambs >= 2, "{kind:?}: a jamb on each side");
        let arcs = pieces.iter().filter(|x| matches!(x.kind, EntityKind::Arc(_))).count();
        assert_eq!(
            arcs,
            match kind {
                OpeningKind::Door => 1,
                OpeningKind::DoubleDoor => 2,
                _ => 0,
            },
            "{kind:?}"
        );
        let glazing = pieces.iter().filter(|x| x.layer == WINDOW_LAYER).count();
        assert_eq!(glazing, if kind == OpeningKind::Window { 3 } else { 0 }, "{kind:?}");
    }
    // Sizes a new one starts with, and what is refused.
    assert_eq!(OpeningKind::Door.default_size_mm(), (900.0, 2100.0, 0.0));
    assert_eq!(OpeningKind::Window.default_size_mm(), (1200.0, 1200.0, 900.0));
    for bad in [
        OpeningParams { width_mm: 0.0, ..door() },
        OpeningParams { width_mm: f64::NAN, ..door() },
        OpeningParams { height_mm: -1.0, ..door() },
        OpeningParams { sill_mm: 1e9, ..door() },
        OpeningParams { left_mm: -5.0, right_mm: 5.0, ..door() },
    ] {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
    for (text, kind) in [
        ("door", OpeningKind::Door),
        ("PORTE", OpeningKind::Door),
        ("doubleDoor", OpeningKind::DoubleDoor),
        ("fenêtre", OpeningKind::Window),
        ("baie", OpeningKind::Bay),
    ] {
        assert_eq!(OpeningKind::parse(text), Some(kind), "{text}");
    }
    assert_eq!(OpeningParams::from_xdata(&door().to_xdata().unwrap()), Some(door()));
}

/// The arc of a door: centred on the hinge, as wide as the door, from the closed leaf to the open one.
#[test]
fn a_door_swings_from_its_hinge_to_the_side_asked_for() {
    for (flip_side, flip_hand) in [(false, false), (true, false), (false, true), (true, true)] {
        let p = OpeningParams { flip_side, flip_hand, ..door() };
        let pieces = p.generate().unwrap();
        let a = pieces.iter().find_map(|x| if let EntityKind::Arc(a) = &x.kind { Some(a.clone()) } else { None }).unwrap();
        let hinge_x = if flip_hand { 450.0 } else { -450.0 };
        let face_y = if flip_side { -100.0 } else { 100.0 };
        assert!((a.center.x - hinge_x).abs() < 1e-9 && (a.center.y - face_y).abs() < 1e-9, "{flip_side} {flip_hand}: at the hinge");
        assert!((a.radius - 900.0).abs() < 1e-9);
        // One end of the arc is the far jamb on the face line (the closed leaf), the other is the open leaf's tip.
        let at = |angle: f64| Vec2::new(a.center.x + a.radius * angle.cos(), a.center.y + a.radius * angle.sin());
        let far_jamb = Vec2::new(-hinge_x, face_y);
        let tip = Vec2::new(hinge_x, face_y + if flip_side { -900.0 } else { 900.0 });
        let ends = [at(a.start), at(a.end)];
        assert!(ends.iter().any(|e| e.near(far_jamb, 1e-6)) && ends.iter().any(|e| e.near(tip, 1e-6)), "{flip_side} {flip_hand}: {ends:?}");
        assert!(((a.end - a.start) - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "a quarter turn, counterclockwise");
    }
}

#[test]
fn the_wall_is_cut_under_an_opening_when_the_walls_are_joined_and_whole_again_without_it() {
    let (mut d, wall) = with_wall();
    assert!(wall_runs_across(&d, wall, 2500.0), "a whole wall at first");
    let o = add(&mut d, OpeningKind::Door, 2500.0);
    assert_eq!(d.cleanup_walls(&Space::Model).unwrap(), 1, "the wall is drawn again");
    let w = wall_of(d.entity(wall).unwrap()).unwrap();
    assert_eq!(w.joints.openings.len(), 1);
    let cut = w.joints.openings[0];
    assert_eq!((cut.segment, cut.from, cut.to), (0, 2050.0, 2950.0), "900 mm, centred at 2500");
    for x in [2050.5, 2500.0, 2949.5] {
        assert!(!wall_runs_across(&d, wall, x), "no wall line crosses x = {x}");
    }
    assert!(wall_runs_across(&d, wall, 1000.0) && wall_runs_across(&d, wall, 4000.0), "the wall goes on each side");
    assert!(d.wall_is_intact(wall), "a wall with an opening still knows its own block");
    // Moved along the wall: the cut moves with it.
    d.modify_entity(o, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.insert.x = 1000.0;
        }
    })
    .unwrap();
    d.cleanup_walls(&Space::Model).unwrap();
    let cut = wall_of(d.entity(wall).unwrap()).unwrap().joints.openings[0];
    assert_eq!((cut.from, cut.to), (550.0, 1450.0));
    assert!(wall_runs_across(&d, wall, 2500.0), "the old place is a wall again");
    // Erased: the wall is whole.
    d.remove_entity(o);
    d.cleanup_walls(&Space::Model).unwrap();
    assert!(wall_of(d.entity(wall).unwrap()).unwrap().joints.openings.is_empty());
    assert!(wall_runs_across(&d, wall, 1000.0));
}

#[test]
fn an_opening_is_drawn_across_the_faces_of_the_wall_it_is_in() {
    let (mut d, _) = with_wall();
    let o = add(&mut d, OpeningKind::Window, 2500.0);
    d.cleanup_walls(&Space::Model).unwrap();
    let p = opening_of(d.entity(o).unwrap()).unwrap();
    // Left justification: the path is the exterior face, the wall goes to the right, 332 mm.
    let total = brick().thickness();
    assert!((p.left_mm - 0.0).abs() < 1e-9 && (p.right_mm + total).abs() < 1e-9, "{p:?}");
    // Its jambs now run across the whole wall.
    let EntityKind::Insert(i) = &d.entity(o).unwrap().kind else { panic!("block") };
    let ys: Vec<f64> = d
        .block(&i.block)
        .unwrap()
        .entities
        .iter()
        .filter_map(|e| if let EntityKind::Line(l) = &e.kind { Some(l) } else { None })
        .filter(|l| (l.a.x - l.b.x).abs() < 1e-9)
        .flat_map(|l| [l.a.y, l.b.y])
        .collect();
    assert!(ys.iter().any(|y| (*y + total).abs() < 1e-9) && ys.iter().any(|y| y.abs() < 1e-9), "{ys:?}");
}

#[test]
fn an_opening_off_the_wall_cuts_nothing_and_one_past_its_end_neither() {
    let (mut d, wall) = with_wall();
    // Off the wall, beside it.
    d.add_opening(&Space::Model, Common::default(), &door(), Vec2::new(2500.0, 3000.0), 0.0).unwrap();
    // Across it, not along it.
    d.add_opening(&Space::Model, Common::default(), &door(), Vec2::new(1500.0, 0.0), std::f64::consts::FRAC_PI_2).unwrap();
    // Half past the end.
    d.add_opening(&Space::Model, Common::default(), &door(), Vec2::new(4900.0, 0.0), 0.0).unwrap();
    d.cleanup_walls(&Space::Model).unwrap();
    assert!(wall_of(d.entity(wall).unwrap()).unwrap().joints.openings.is_empty());
    assert!(wall_runs_across(&d, wall, 2500.0));
}

#[test]
fn many_openings_in_one_wall_are_all_cut_and_a_hostile_cut_is_refused() {
    let (mut d, wall) = with_wall();
    for x in [800.0, 2000.0, 3200.0, 4400.0] {
        add(&mut d, OpeningKind::Window, x);
    }
    d.cleanup_walls(&Space::Model).unwrap();
    let w = wall_of(d.entity(wall).unwrap()).unwrap();
    assert_eq!(w.joints.openings.len(), 4);
    assert!(w.joints.openings.windows(2).all(|p| p[0].to <= p[1].from), "in order, apart");
    let mut bad = w.clone();
    bad.joints.openings = vec![crate::OpeningCut { segment: 0, from: 10.0, to: 5.0 }];
    assert!(bad.validate().is_err());
    bad.joints.openings = vec![crate::OpeningCut { segment: 0, from: f64::NAN, to: 5.0 }];
    assert!(bad.validate().is_err());
    bad.joints.openings = (0..70).map(|i| crate::OpeningCut { segment: 0, from: f64::from(i), to: f64::from(i) + 0.5 }).collect();
    assert!(bad.validate().is_err(), "more than a wall keeps");
    // A cut that names a segment the wall does not have is ignored when drawing, not a panic.
    let mut odd = w.clone();
    odd.joints.openings = vec![crate::OpeningCut { segment: 9, from: 0.0, to: 100.0 }];
    assert!(odd.generate().is_ok());
}

#[test]
fn every_style_draws_and_fills_the_opening_with_its_own_joinery() {
    use crate::OpeningStyle;
    let mut counts = Vec::new();
    for style in OpeningStyle::ALL {
        let mut p = OpeningParams::new(OpeningKind::Door, WallUnits::default());
        p.style = style;
        let pieces = p.generate().expect("a symbol");
        assert!(pieces.len() >= 3, "{style:?} draws more than the two jambs");
        counts.push((style, pieces.len()));
        // The style survives its extended data, and old data without one reads as the standard symbol.
        assert_eq!(OpeningParams::from_xdata(&p.to_xdata().unwrap()).map(|q| q.style), Some(style));
        assert_eq!(OpeningStyle::parse(style.name()), Some(style));
    }
    let old = serde_json::json!({"kind": "door", "widthMm": 900.0, "heightMm": 2100.0, "sillMm": 0.0});
    assert_eq!(serde_json::from_value::<OpeningParams>(old).map(|q| q.style).ok(), Some(OpeningStyle::Standard));
}

#[test]
fn the_temporary_dimensions_run_from_the_faces_to_the_edges_and_say_millimetres() {
    let p = OpeningParams::new(OpeningKind::Door, WallUnits::default());
    let texts = |pieces: &[crate::ventilation::HvacPiece]| -> Vec<String> {
        pieces
            .iter()
            .filter_map(|x| match &x.kind {
                EntityKind::Text(t) => Some(t.value.clone()),
                _ => None,
            })
            .collect()
    };
    // 300 mm from the face at the start, 700 mm from the one at the end: two dimensions, each a line,
    // its two ticks and its value.
    let both = p.provisional_dimensions(300.0, 700.0, false);
    assert_eq!(both.len(), 8);
    assert_eq!(texts(&both), ["300", "700"]);
    // Touching a face: no dimension there.
    let one = p.provisional_dimensions(0.0, 700.0, false);
    assert_eq!(texts(&one), ["700"]);
    assert!(p.provisional_dimensions(f64::NAN, -5.0, false).is_empty());
    // The first starts at the face and ends on the opening's edge, beside the wall's left face.
    let EntityKind::Line(l) = &both[0].kind else { panic!("a dimension line") };
    let hw = p.width() / 2.0;
    assert!((l.a.x + hw + 300.0).abs() < 1e-9 && (l.b.x + hw).abs() < 1e-9 && l.a.y > p.left_mm);
    // A wall running leftward gets its text turned over.
    let turned = p.provisional_dimensions(300.0, 700.0, true);
    let EntityKind::Text(t) = &turned[3].kind else { panic!("a text") };
    assert!((t.rotation - std::f64::consts::PI).abs() < 1e-9);
}
