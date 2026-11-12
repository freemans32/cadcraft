//! Walls (a block reference carrying `CADCRAFT_WALL` xdata, to an anonymous block of lines and
//! hatches) and the drawing's wall styles survive DXF and DWG saves.

use cadcraft_doc::{
    Common, Drawing, EntityKind, OPENING_APP, OpeningKind, OpeningParams, Space, WALL_APP, WallJustification, WallParams, WallUnits, opening_of,
    standard_wall_styles, wall_of,
};
use cadcraft_geom::Vec2;

fn drawing() -> (Drawing, WallParams) {
    let mut d = Drawing::new_metric();
    let mut style = standard_wall_styles().into_iter().nth(1).expect("brick style");
    style.name = "Brique 20 + laine de verre 12 + BA13".into();
    d.wall_styles.push(style.clone());
    let p = WallParams {
        style,
        path: vec![Vec2::new(500.0, 500.0), Vec2::new(6500.0, 500.0), Vec2::new(6500.0, -3500.0)],
        closed: false,
        justification: WallJustification::Left,
        height: 2500.0,
        units: WallUnits::of(&d),
        joints: Default::default(),
    };
    d.add_wall(&Space::Model, Common::default(), p.clone()).expect("add wall");
    (d, p)
}

fn the_wall(d: &Drawing) -> (cadcraft_doc::Handle, WallParams) {
    let walls: Vec<_> = d.model.iter().filter_map(|e| wall_of(e).map(|w| (e.handle, w))).collect();
    assert_eq!(walls.len(), 1, "one wall");
    walls.into_iter().next().expect("wall")
}

#[test]
fn a_wall_and_the_wall_styles_survive_dxf_and_dwg() {
    let (d, p) = drawing();
    let (_, saved) = the_wall(&d);
    for name in ["wall.dxf", "wall.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (h, w) = the_wall(&back);
        assert_eq!(w, saved, "{name}: wall parameters");
        assert_eq!(w.style, p.style, "{name}: the wall's copy of its style");
        assert!(back.wall_is_intact(h), "{name}: the block still holds the wall's geometry");
        assert_eq!(back.wall_styles, d.wall_styles, "{name}: the drawing's wall styles");
    }
}

#[test]
fn other_programs_see_an_ordinary_block_with_its_application_registered() {
    let (d, _) = drawing();
    let text = String::from_utf8(cadcraft_io::write(&d, "wall.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{WALL_APP}")), "APPID record for the wall data");
    // The geometry is plain lines and hatches in an anonymous block.
    let back = cadcraft_io::read_dxf(text.as_bytes()).expect("read");
    let (h, _) = the_wall(&back);
    let EntityKind::Insert(i) = &back.entity(h).expect("wall").kind else { panic!("not a block reference") };
    let b = back.block(&i.block).expect("block");
    assert!(b.anonymous);
    let kinds: Vec<&str> = b.entities.iter().map(|e| e.kind.type_name()).collect();
    assert!(kinds.iter().all(|k| matches!(*k, "Polyline" | "Line" | "Hatch")), "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| **k == "Hatch").count(), 2);
}

#[test]
fn joined_walls_survive_dxf_and_dwg_still_joined() {
    let (mut d, p) = drawing();
    // A second wall in an L with the first one's start.
    let mut q = p.clone();
    q.path = vec![Vec2::new(500.0, 500.0), Vec2::new(500.0, -3500.0)];
    q.justification = WallJustification::Right;
    d.add_wall(&Space::Model, Common::default(), q).expect("add wall");
    assert_eq!(d.cleanup_walls(&Space::Model).expect("cleanup"), 2);
    let walls = |d: &Drawing| d.model.iter().filter_map(|e| wall_of(e).map(|w| (e.handle, w))).collect::<Vec<_>>();
    let saved = walls(&d);
    assert!(saved.iter().all(|(_, w)| !w.joints.is_empty()));
    for name in ["joined.dxf", "joined.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let got = walls(&back);
        assert_eq!(got.iter().map(|(_, w)| w).collect::<Vec<_>>(), saved.iter().map(|(_, w)| w).collect::<Vec<_>>(), "{name}");
        assert!(got.iter().all(|(h, _)| back.wall_is_intact(*h)), "{name}: still recognised");
        assert_eq!(back.cleanup_walls(&Space::Model).expect("cleanup"), 0, "{name}: nothing to redo");
    }
}

/// A door and a window in the first segment of the wall, with the wall cut under them.
fn with_openings() -> Drawing {
    let (mut d, _) = drawing();
    for (kind, x) in [(OpeningKind::Door, 1500.0), (OpeningKind::Window, 4000.0)] {
        let o = OpeningParams::new(kind, WallUnits::of(&d));
        d.add_opening(&Space::Model, Common::default(), &o, Vec2::new(x, 500.0), 0.0).expect("add opening");
    }
    d.cleanup_walls(&Space::Model).expect("join");
    d
}

#[test]
fn openings_and_the_cuts_they_make_survive_dxf_and_dwg() {
    let d = with_openings();
    let (_, saved) = the_wall(&d);
    assert_eq!(saved.joints.openings.len(), 2, "the wall is cut twice");
    for name in ["openings.dxf", "openings.dwg"] {
        let bytes = cadcraft_io::write(&d, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = cadcraft_io::read(&bytes, name).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (h, w) = the_wall(&back);
        assert_eq!(w.joints.openings, saved.joints.openings, "{name}: the cuts");
        assert!(back.wall_is_intact(h), "{name}: the wall still knows its own block");
        let kinds: Vec<OpeningKind> = back.model.iter().filter_map(|e| opening_of(e).map(|o| o.kind)).collect();
        assert_eq!(kinds.len(), 2, "{name}: both openings");
        assert!(kinds.contains(&OpeningKind::Door) && kinds.contains(&OpeningKind::Window), "{name}");
        // Read back, joining again changes nothing: the file holds what the drawing computes.
        let mut again = back.clone();
        assert_eq!(again.cleanup_walls(&Space::Model).expect("join"), 0, "{name}");
    }
}

#[test]
fn other_programs_see_an_opening_as_a_block_on_its_own_layers() {
    let d = with_openings();
    let text = String::from_utf8(cadcraft_io::write(&d, "openings.dxf").expect("dxf")).expect("utf-8");
    assert!(text.contains(&format!("AcDbRegAppTableRecord\r\n  2\r\n{OPENING_APP}")), "APPID record for the opening data");
    assert!(text.contains("A-DOOR") && text.contains("A-GLAZ"), "the layers architects expect");
}
