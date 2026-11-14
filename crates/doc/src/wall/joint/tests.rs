use cadcraft_geom::{Vec2, shoelace};

use crate::{Common, Drawing, EntityKind, Handle, Space, WallJustification, WallParams, WallUnits, standard_wall_styles, wall_of};

const CONCRETE: &str = "Concrete 200";
const BRICK: &str = "Brick 200 + mineral wool 120 + plasterboard";
const PARTITION: &str = "Partition 72/48";

fn add(d: &mut Drawing, style: &str, path: &[(f64, f64)], justification: WallJustification) -> Handle {
    let style = standard_wall_styles().into_iter().find(|s| s.name == style).unwrap();
    let p = WallParams {
        style,
        path: path.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        closed: false,
        justification,
        height: 2500.0,
        units: WallUnits::default(),
        joints: Default::default(),
        bulges: Vec::new(),
        start_free: false,
        end_free: false,
    };
    d.add_wall(&Space::Model, Common::default(), p).unwrap()
}

/// The wall's block contents, moved to drawing coordinates (the reference is not rotated).
fn pieces(d: &Drawing, h: Handle) -> Vec<EntityKind> {
    let e = d.entity(h).unwrap();
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    let at = Vec2::new(i.insert.x, i.insert.y);
    let mv = |p: Vec2| p + at;
    d.block(&i.block)
        .unwrap()
        .entities
        .iter()
        .map(|e| match &e.kind {
            EntityKind::Line(l) => {
                let mut l = l.clone();
                l.a = mv(l.a.xy()).to3(0.0);
                l.b = mv(l.b.xy()).to3(0.0);
                EntityKind::Line(l)
            }
            EntityKind::Hatch(h) => {
                let mut h = h.clone();
                for lp in &mut h.loops {
                    for v in &mut lp.vertices {
                        v.p = mv(v.p);
                    }
                }
                EntityKind::Hatch(h)
            }
            k => k.clone(),
        })
        .collect()
}

fn hatched_area(d: &Drawing, hs: &[Handle]) -> f64 {
    hs.iter()
        .flat_map(|h| pieces(d, *h))
        .filter_map(|k| match k {
            EntityKind::Hatch(h) => Some(h.loops.iter().map(|l| shoelace(&l.vertices.iter().map(|v| v.p).collect::<Vec<_>>()).abs()).sum::<f64>()),
            _ => None,
        })
        .sum()
}

fn line_length(d: &Drawing, hs: &[Handle]) -> f64 {
    hs.iter()
        .flat_map(|h| pieces(d, *h))
        .map(|k| match k {
            EntityKind::Line(l) => l.a.xy().dist(l.b.xy()),
            EntityKind::LwPolyline(p) => {
                let v: Vec<Vec2> = p.vertices.iter().map(|v| v.p).collect();
                let n = v.len();
                let segs = if p.closed { n } else { n - 1 };
                (0..segs).map(|i| v[i].dist(v[(i + 1) % n])).sum()
            }
            _ => 0.0,
        })
        .sum()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

#[test]
fn an_l_corner_of_one_style_is_one_outline_with_the_corner_hatched_once() {
    let mut d = Drawing::new_metric();
    let a = add(&mut d, CONCRETE, &[(0.0, 0.0), (4000.0, 0.0)], WallJustification::Center);
    let b = add(&mut d, CONCRETE, &[(0.0, 0.0), (0.0, 3000.0)], WallJustification::Center);
    assert_eq!(d.cleanup_walls(&Space::Model).unwrap(), 2);
    // Each end runs to the other wall's far face; the corner square is hatched by the first.
    assert!(close(hatched_area(&d, &[a, b]), 4100.0 * 200.0 + 2900.0 * 200.0));
    // One outline round the L: no line inside it, none twice.
    assert!(close(line_length(&d, &[a, b]), 4100.0 + 200.0 + 3900.0 + 2900.0 + 200.0 + 3100.0), "{}", line_length(&d, &[a, b]));
    // Nothing more to do the second time, and the walls still recognise their blocks.
    assert_eq!(d.cleanup_walls(&Space::Model).unwrap(), 0);
    assert!(d.wall_is_intact(a) && d.wall_is_intact(b));
}

fn pattern_area(d: &Drawing, hs: &[Handle], pattern: &str) -> f64 {
    hs.iter()
        .flat_map(|h| pieces(d, *h))
        .filter_map(|k| match k {
            EntityKind::Hatch(h) if h.pattern == pattern => {
                Some(h.loops.iter().map(|l| shoelace(&l.vertices.iter().map(|v| v.p).collect::<Vec<_>>()).abs()).sum::<f64>())
            }
            _ => None,
        })
        .sum()
}

/// Brick 200, then mineral wool 120 and plasterboard 12.5 on the other side of the path.
fn brick_l_layers(d: &Drawing, hs: &[Handle]) -> (f64, f64) {
    (pattern_area(d, hs, "ANSI31"), pattern_area(d, hs, "ZIGZAG"))
}

#[test]
fn an_l_corner_wraps_each_layer_around_the_corner_whichever_side_the_brick_is_on() {
    // Drawn going round the corner clockwise: the brick is on the outside of the L.
    let mut d = Drawing::new_metric();
    let a = add(&mut d, BRICK, &[(0.0, -3000.0), (0.0, 0.0)], WallJustification::Left);
    let b = add(&mut d, BRICK, &[(0.0, 0.0), (4000.0, 0.0)], WallJustification::Left);
    assert_eq!(d.cleanup_walls(&Space::Model).unwrap(), 2);
    let (brick, wool) = brick_l_layers(&d, &[a, b]);
    // Brick along x = 0..200 and y = -200..0, wool inside it, each corner square counted once.
    assert!(close(brick, 200.0 * 3000.0 + 4000.0 * 200.0 - 200.0 * 200.0), "{brick}");
    assert!(close(wool, 120.0 * 2800.0 + 3800.0 * 120.0 - 120.0 * 120.0), "{wool}");

    // Drawn the other way round: the brick is on the inside, and the wool and plasterboard
    // wrap round the outside of it rather than the brick cutting through them.
    let mut d = Drawing::new_metric();
    let a = add(&mut d, BRICK, &[(4000.0, 0.0), (0.0, 0.0)], WallJustification::Left);
    let b = add(&mut d, BRICK, &[(0.0, 0.0), (0.0, -3000.0)], WallJustification::Left);
    assert_eq!(d.cleanup_walls(&Space::Model).unwrap(), 2);
    let (brick, wool) = brick_l_layers(&d, &[a, b]);
    // Brick along y = 0..200 and x = -200..0, wool outside it out to (-320, 320).
    assert!(close(brick, 4200.0 * 200.0 + 200.0 * 3200.0 - 200.0 * 200.0), "{brick}");
    assert!(close(wool, 4320.0 * 120.0 + 120.0 * 3320.0 - 120.0 * 120.0), "{wool}");
}

#[test]
fn a_partition_stops_on_the_face_of_the_wall_it_meets() {
    let mut d = Drawing::new_metric();
    // The outer wall: brick outside (y 0 to −200), wool, plasterboard inside (to −332.5).
    let host = add(&mut d, BRICK, &[(0.0, 0.0), (6000.0, 0.0)], WallJustification::Left);
    // The partition is drawn a little short of the wall; it is brought to its face.
    let part = add(&mut d, PARTITION, &[(3000.0, -340.0), (3000.0, -3000.0)], WallJustification::Center);
    d.cleanup_walls(&Space::Model).unwrap();
    let top = pieces(&d, part)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Hatch(h) => h.loops.iter().flat_map(|l| l.vertices.iter().map(|v| v.p.y)).reduce(f64::max),
            _ => None,
        })
        .reduce(f64::max)
        .unwrap();
    assert!(close(top, -332.5), "{top}");
    // The host's brick and wool (its plasterboard is not hatched) are whole.
    let host_hatches: Vec<usize> = pieces(&d, host)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Hatch(h) => Some(h.loops.len()),
            _ => None,
        })
        .collect();
    assert_eq!(host_hatches, [1, 1]);
    assert!(close(hatched_area(&d, &[host]), 6000.0 * 320.0));
    // The host's inner face is broken where the partition's two plasterboards join it.
    let face_y = -332.5;
    let face: f64 = pieces(&d, host)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Line(l) if close(l.a.y, face_y) && close(l.b.y, face_y) => Some((l.a.x - l.b.x).abs()),
            _ => None,
        })
        .sum();
    assert!(close(face, 6000.0 - 2.0 * 12.5), "{face}");
}

#[test]
fn brick_walls_in_a_t_join_their_structures_through_the_lining() {
    let mut d = Drawing::new_metric();
    let host = add(&mut d, BRICK, &[(0.0, 0.0), (6000.0, 0.0)], WallJustification::Left);
    // Another brick wall, its brick on the same side, ending against the host's inner face.
    let t = add(&mut d, BRICK, &[(3000.0, -332.5), (3000.0, -4000.0)], WallJustification::Center);
    d.cleanup_walls(&Space::Model).unwrap();
    // The host's wool is cut in two by the other wall's brick (plasterboard is not hatched).
    let loops: Vec<usize> = pieces(&d, host)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Hatch(h) => Some(h.loops.len()),
            _ => None,
        })
        .collect();
    assert_eq!(loops, [1, 2]);
    // The other wall's brick reaches the host's brick.
    let reach = pieces(&d, t)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Hatch(h) if h.pattern == "ANSI31" => h.loops.iter().flat_map(|l| l.vertices.iter().map(|v| v.p.y)).reduce(f64::max),
            _ => None,
        })
        .reduce(f64::max)
        .unwrap();
    assert!(close(reach, -200.0), "{reach}");
    // Hatched once all over: the host's brick, its wool less the other brick, the other
    // wall's brick to the host's brick and its wool to the host's wool.
    let expected = 6000.0 * 200.0 + (6000.0 * 120.0 - 200.0 * 120.0) + 200.0 * (4000.0 - 200.0) + 120.0 * (4000.0 - 320.0);
    assert!(close(hatched_area(&d, &[host, t]), expected), "{}", hatched_area(&d, &[host, t]));
}

#[test]
fn crossing_walls_of_one_style_merge_into_a_cross() {
    let mut d = Drawing::new_metric();
    let a = add(&mut d, CONCRETE, &[(-2000.0, 0.0), (2000.0, 0.0)], WallJustification::Center);
    let b = add(&mut d, CONCRETE, &[(0.0, -2000.0), (0.0, 2000.0)], WallJustification::Center);
    d.cleanup_walls(&Space::Model).unwrap();
    assert!(close(hatched_area(&d, &[a, b]), 2.0 * 4000.0 * 200.0 - 200.0 * 200.0));
    // The outline of the cross: four arms of 1900 each side, four ends.
    assert!(close(line_length(&d, &[a, b]), 8.0 * 1900.0 + 4.0 * 200.0), "{}", line_length(&d, &[a, b]));
}

#[test]
fn walls_apart_keep_no_junction_and_lose_one_when_moved_away() {
    let mut d = Drawing::new_metric();
    let a = add(&mut d, CONCRETE, &[(0.0, 0.0), (4000.0, 0.0)], WallJustification::Center);
    let b = add(&mut d, CONCRETE, &[(0.0, 0.0), (0.0, 3000.0)], WallJustification::Center);
    let far = add(&mut d, CONCRETE, &[(0.0, 10000.0), (4000.0, 10000.0)], WallJustification::Center);
    d.cleanup_walls(&Space::Model).unwrap();
    assert!(d.entity(far).and_then(|e| wall_of(e)).unwrap().joints.is_empty());
    assert!(!d.entity(b).and_then(|e| wall_of(e)).unwrap().joints.is_empty());
    // B moves away: after a cleanup both walls are drawn on their own again.
    d.modify_entity(b, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.insert.x += 5000.0;
        }
    })
    .unwrap();
    d.cleanup_walls(&Space::Model).unwrap();
    for h in [a, b] {
        assert!(d.entity(h).and_then(|e| wall_of(e)).unwrap().joints.is_empty());
        assert!(d.wall_is_intact(h));
    }
}

#[test]
fn a_rotated_wall_joins_like_any_other() {
    let mut d = Drawing::new_metric();
    let a = add(&mut d, CONCRETE, &[(0.0, 0.0), (4000.0, 0.0)], WallJustification::Center);
    // Drawn along x, then turned a quarter turn about its start: it runs up from the corner.
    let b = add(&mut d, CONCRETE, &[(0.0, 0.0), (3000.0, 0.0)], WallJustification::Center);
    d.modify_entity(b, |e| {
        if let EntityKind::Insert(i) = &mut e.kind {
            i.rotation = std::f64::consts::FRAC_PI_2;
        }
    })
    .unwrap();
    d.cleanup_walls(&Space::Model).unwrap();
    assert!(close(hatched_area(&d, &[a, b]), 4100.0 * 200.0 + 2900.0 * 200.0));
}

#[test]
fn joints_survive_extended_data_and_bad_ones_are_refused() {
    let mut d = Drawing::new_metric();
    add(&mut d, CONCRETE, &[(0.0, 0.0), (4000.0, 0.0)], WallJustification::Center);
    let b = add(&mut d, CONCRETE, &[(0.0, 0.0), (0.0, 3000.0)], WallJustification::Center);
    d.cleanup_walls(&Space::Model).unwrap();
    let w = d.entity(b).and_then(|e| wall_of(e)).unwrap();
    assert_eq!(WallParams::from_xdata(&w.to_xdata().unwrap()), Some(w.clone()));
    let mut bad = w.clone();
    bad.joints.foreign[0].corners[0].x = f64::NAN;
    assert!(bad.validate().is_err());
    let mut bad = w;
    bad.joints.start = Some(super::EndCut { point: Vec2::ZERO, dir: Vec2::ZERO });
    assert!(bad.validate().is_err());
}
