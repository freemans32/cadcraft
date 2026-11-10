//! Walls (a block reference carrying `CADCRAFT_WALL` xdata, to an anonymous block of lines and
//! hatches) and the drawing's wall styles survive DXF and DWG saves.

use cadcraft_doc::{Common, Drawing, EntityKind, Space, WALL_APP, WallJustification, WallParams, WallUnits, standard_wall_styles, wall_of};
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
