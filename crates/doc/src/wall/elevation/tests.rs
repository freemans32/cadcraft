use super::*;
use crate::{OpeningKind, OpeningParams, WallJustification, WallUnits, standard_wall_styles};

fn wall(points: &[(f64, f64)]) -> WallParams {
    WallParams {
        style: standard_wall_styles().into_iter().find(|s| s.name == "Concrete 200").expect("a style"),
        path: points.iter().map(|&(x, y)| Vec2::new(x, y)).collect(),
        closed: false,
        bulges: Vec::new(),
        justification: WallJustification::Left,
        height: 2500.0,
        units: WallUnits::default(),
        joints: Default::default(),
        start_free: false,
        end_free: false,
    }
}

fn texts(pieces: &[HvacPiece]) -> Vec<String> {
    pieces
        .iter()
        .filter_map(|p| match &p.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect()
}

fn corners(p: &HvacPiece) -> Vec<(f64, f64)> {
    match &p.kind {
        EntityKind::LwPolyline(l) => l.vertices.iter().map(|v| (v.p.x, v.p.y)).collect(),
        _ => Vec::new(),
    }
}

#[test]
fn an_elevation_is_the_developed_length_by_the_height_with_its_openings_at_their_sills() {
    // Two segments of 3000 and 4000: developed length 7000.
    let w = wall(&[(0.0, 0.0), (3000.0, 0.0), (3000.0, 4000.0)]);
    // A door (900 x 2100, no sill) centred at 1000, and a window (1200 x 1200, sill 900) at 5000.
    let openings = [(OpeningKind::Door, 1000.0, 900.0, 0.0, 2100.0), (OpeningKind::Window, 5000.0, 1200.0, 900.0, 1200.0)];
    let (length, pieces) = pieces_of(&w, &openings, ElevationSide::Right);
    assert!((length - 7000.0).abs() < 1e-9);
    assert_eq!(corners(&pieces[0]), [(0.0, 0.0), (7000.0, 0.0), (7000.0, 2500.0), (0.0, 2500.0)], "the outline");
    let door = pieces.iter().find(|p| p.layer == ELEVATION_DOOR_LAYER).expect("a door");
    assert_eq!(corners(door), [(550.0, 0.0), (1450.0, 0.0), (1450.0, 2100.0), (550.0, 2100.0)]);
    let window = pieces.iter().find(|p| p.layer == ELEVATION_WINDOW_LAYER && !corners(p).is_empty()).expect("a window");
    assert_eq!(corners(window), [(4400.0, 900.0), (5600.0, 900.0), (5600.0, 2100.0), (4400.0, 2100.0)]);
    // Labels: sizes, the sill of the window, the height, and the chain 550, 900, 2950, 1200, 1400.
    let t = texts(&pieces);
    assert!(t.contains(&"900 x 2100".to_string()) && t.contains(&"1200 x 1200  (h 900)".to_string()) && t.contains(&"2500".to_string()));
    let chain: Vec<&String> = t.iter().filter(|v| !v.contains('x') && v.as_str() != "2500").collect();
    assert_eq!(chain, ["550", "900", "2950", "1200", "1400"], "the distances between the edges, left to right");
}

#[test]
fn seen_from_the_left_the_elevation_is_the_mirror_image() {
    let w = wall(&[(0.0, 0.0), (4000.0, 0.0)]);
    let openings = [(OpeningKind::Door, 1000.0, 900.0, 0.0, 2100.0)];
    let (_, pieces) = pieces_of(&w, &openings, ElevationSide::Left);
    let door = pieces.iter().find(|p| p.layer == ELEVATION_DOOR_LAYER).expect("a door");
    // At 1000 from the start of a 4000 wall: 3000 from the left edge when mirrored.
    assert_eq!(corners(door), [(2550.0, 0.0), (3450.0, 0.0), (3450.0, 2100.0), (2550.0, 2100.0)]);
    assert_eq!(ElevationSide::parse("gauche"), Some(ElevationSide::Left));
    assert_eq!(ElevationSide::parse("up"), None);
}

#[test]
fn a_wall_with_no_opening_has_one_dimension_and_an_arc_wall_is_as_long_as_its_arc() {
    let (length, pieces) = pieces_of(&wall(&[(0.0, 0.0), (6000.0, 0.0)]), &[], ElevationSide::Right);
    assert!((length - 6000.0).abs() < 1e-9);
    assert_eq!(texts(&pieces), ["2500", "6000"]);
    // A half circle of radius 2000 between (0, 0) and (4000, 0): about 6283, never the 4000 chord.
    let mut arc = wall(&[(0.0, 0.0), (4000.0, 0.0)]);
    arc.bulges = vec![1.0];
    let (long, _) = pieces_of(&arc, &[], ElevationSide::Right);
    assert!((long - std::f64::consts::PI * 2000.0).abs() < 20.0, "{long}");
}

#[test]
fn a_drawing_elevation_is_one_block_with_the_openings_found_in_the_wall() {
    use crate::{Common, Drawing, Space};
    let mut d = Drawing::new_metric();
    let w = wall(&[(0.0, 0.0), (6000.0, 0.0)]);
    let wh = d.add_wall(&Space::Model, Common::default(), w).expect("a wall");
    let op = OpeningParams::new(OpeningKind::Door, WallUnits::default());
    d.add_opening(&Space::Model, Common::default(), &op, Vec2::new(2000.0, 0.0), 0.0).expect("an opening");
    let before = d.model.len();
    let (h, width) = d.add_wall_elevation(&Space::Model, Common::default(), wh, Vec2::new(0.0, -8000.0), ElevationSide::Right).expect("an elevation");
    assert!((width - 6000.0).abs() < 1e-9);
    assert_eq!(d.model.len(), before + 1, "one object");
    let EntityKind::Insert(i) = &d.entity(h).expect("the elevation").kind else { panic!("a block reference") };
    assert_eq!((i.insert.x, i.insert.y), (0.0, -8000.0));
    let block = d.block(&i.block).expect("its block");
    assert!(block.entities.iter().any(|e| e.common.layer == ELEVATION_DOOR_LAYER), "the door is in it");
    assert!(d.layer(ELEVATION_LAYER).is_some() && d.layer(ELEVATION_DIM_LAYER).is_some());
    // Not a wall: refused.
    assert!(d.add_wall_elevation(&Space::Model, Common::default(), h, Vec2::ZERO, ElevationSide::Right).is_err());
    assert!(d.add_wall_elevation(&Space::Model, Common::default(), wh, Vec2::new(f64::NAN, 0.0), ElevationSide::Right).is_err());
}
