//! WALLADD draws walls of a wall style (a block reference to the generated lines and hatches,
//! carrying the wall's data); WALLSTYLEEDIT lists, creates, changes, renames and deletes wall
//! styles, regenerating the walls whose block is still their own.

use std::sync::Arc;

use cadcraft_doc::{Drawing, Entity, EntityKind, Handle, Line, WALL_LAYER, WallJustification, WallParams, wall_of};
use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::{Value, json};

const BRICK: &str = "Brick 200 + mineral wool 120 + plasterboard";
const PARTITION: &str = "Partition 72/48";

fn metric() -> Session {
    let mut s = Session::new();
    *s.doc_mut().unwrap() = Drawing::new_metric();
    s
}

fn h(v: &Value) -> Handle {
    Handle::parse_hex(v["handle"].as_str().unwrap()).unwrap()
}

fn wall(s: &Session, hd: Handle) -> WallParams {
    wall_of(s.doc().unwrap().entity(hd).expect("wall object")).expect("wall data")
}

fn walls(s: &Session) -> Vec<(Handle, WallParams)> {
    s.doc().unwrap().model.iter().filter_map(|e| wall_of(e).map(|w| (e.handle, w))).collect()
}

fn path(w: &WallParams) -> Vec<(f64, f64)> {
    w.path.iter().map(|p| (p.x, p.y)).collect()
}

#[test]
fn walladd_draws_a_wall_of_a_standard_style_and_copies_the_style_into_the_drawing() {
    let mut s = metric();
    let r = s.execute("walladd", &json!({"points": [[0, 0], [5000, 0], [5000, -4000]], "style": BRICK})).unwrap();
    let hd = h(&r);
    let d = s.doc().unwrap();
    let e = d.entity(hd).unwrap();
    assert_eq!(e.common.layer, WALL_LAYER);
    let EntityKind::Insert(i) = &e.kind else { panic!("not a block reference") };
    assert_eq!(i.insert, Vec3::ZERO);
    let w = wall(&s, hd);
    assert_eq!((w.style.name.as_str(), w.justification, w.height), (BRICK, WallJustification::Left, 2500.0));
    assert_eq!(path(&w), [(0.0, 0.0), (5000.0, 0.0), (5000.0, -4000.0)]);
    assert!(d.wall_style(BRICK).is_some(), "the standard style is now the drawing's");
    assert!(d.wall_is_intact(hd));
    // 4 boundary lines, 2 end lines, 2 hatches.
    assert_eq!(d.block(&i.block).unwrap().entities.len(), 8);
}

#[test]
fn walladd_refuses_what_it_cannot_draw() {
    let mut s = metric();
    let bad = |s: &mut Session, p: Value| s.execute("walladd", &p).is_err();
    assert!(bad(&mut s, json!({})));
    assert!(bad(&mut s, json!({"points": [[0, 0]]})));
    assert!(bad(&mut s, json!({"points": "0,0 1,1"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0], [0, 100]]})), "corner too sharp");
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0], [1000, 10], [2000, 10]]})), "segment too short");
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "style": "nope"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "justification": "up"})));
    assert!(bad(&mut s, json!({"points": [[0, 0], [1000, 0]], "height": -1})));
    let many: Vec<Value> = (0..=64).map(|i| json!([i * 1000, (i % 2) * 1000])).collect();
    assert!(bad(&mut s, json!({ "points": many })));
    assert!(walls(&s).is_empty());
    assert!(s.doc().unwrap().wall_styles.is_empty(), "no style copied for a wall that wasn't drawn");
}

#[test]
fn walladd_at_the_command_line_takes_style_justification_and_points() {
    let mut s = metric();
    for t in ["walladd", "st", PARTITION, "j", "c", "0,0", "3000,0", "3000,2000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    // One wall per segment, as in AutoCAD Architecture; Enter after three points closes.
    let all = walls(&s);
    assert_eq!(all.len(), 3);
    let w = &all[0].1;
    assert_eq!((w.style.name.as_str(), w.justification), (PARTITION, WallJustification::Center));
    // The next wall takes the same settings.
    for t in ["walladd", "0,5000", "2000,5000", ""] {
        s.cmdline(t).unwrap();
    }
    let all = walls(&s);
    assert_eq!(all.len(), 4);
    assert_eq!((all[3].1.style.name.as_str(), all[3].1.justification), (PARTITION, WallJustification::Center));
}

#[test]
fn walladd_skips_a_point_that_would_make_the_wall_impossible() {
    let mut s = metric();
    for t in ["walladd", "0,0", "1000,0", "0,100", "1000,1000"] {
        s.cmdline(t).unwrap();
    }
    s.cancel();
    assert!(s.log.iter().any(|l| l.contains("turn this sharply")), "{:?}", s.log);
    assert_eq!(outline(&s), [vec![(0.0, 0.0), (1000.0, 0.0)], vec![(1000.0, 0.0), (1000.0, 1000.0)]]);
}

/// Each wall's path in drawing coordinates (the path is kept relative to the block reference).
fn outline(s: &Session) -> Vec<Vec<(f64, f64)>> {
    let d = s.doc().unwrap();
    walls(s)
        .iter()
        .map(|(hd, w)| {
            let EntityKind::Insert(i) = &d.entity(*hd).unwrap().kind else { panic!("a wall is a block reference") };
            let (c, sn) = (i.rotation.cos(), i.rotation.sin());
            w.path.iter().map(|p| ((i.insert.x + p.x * c - p.y * sn).round(), (i.insert.y + p.x * sn + p.y * c).round())).collect()
        })
        .collect()
}

const SQUARE: [[(f64, f64); 2]; 4] =
    [[(0.0, 0.0), (4000.0, 0.0)], [(4000.0, 0.0), (4000.0, -3000.0)], [(4000.0, -3000.0), (0.0, -3000.0)], [(0.0, -3000.0), (0.0, 0.0)]];

#[test]
fn walladd_close_enter_or_a_click_on_the_start_point_closes_the_outline() {
    for last in ["c", "", "0,0"] {
        let mut s = metric();
        for t in ["walladd", "0,0", "4000,0", "4000,-3000", "0,-3000", last] {
            s.cmdline(t).unwrap();
        }
        assert!(s.running.is_none(), "{last:?} ends the command");
        assert_eq!(outline(&s), SQUARE.map(|w| w.to_vec()).to_vec(), "{last:?}");
        // Every corner is joined, the last one included.
        assert!(walls(&s).iter().all(|(_, w)| w.joints.start.is_some() && w.joints.end.is_some()), "{last:?}");
    }
}

#[test]
fn walladd_adds_each_wall_at_once_so_the_next_point_snaps_to_it() {
    use cadcraft_doc::Space;
    use cadcraft_engine::snap::{mode, osnap};
    let mut s = metric();
    for t in ["walladd", "0,0", "4000,0", "4000,-3000"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_some());
    assert_eq!(walls(&s).len(), 2, "the walls drawn so far are in the drawing");
    let d = s.doc().unwrap();
    let near = |m: u32, x: f64, y: f64| osnap(d, &Space::Model, cadcraft_geom::Vec2::new(x, y), 50.0, m, None, false).map(|h| (h.point.x, h.point.y));
    // The start point and the middle of the first wall, from beside them.
    assert_eq!(near(mode::END, 20.0, 15.0), Some((0.0, 0.0)));
    assert_eq!(near(mode::MID, 2010.0, 10.0), Some((2000.0, 0.0)));
}

#[test]
fn walladd_enter_or_escape_ends_an_open_run_and_undo_takes_back_the_last_wall() {
    let mut s = metric();
    for t in ["walladd", "0,0", "4000,0", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    assert_eq!(walls(&s).len(), 1, "Enter after two points draws one straight wall");
    let mut s = metric();
    for t in ["walladd", "0,0", "4000,0", "4000,-3000", "0,-3000", "u"] {
        s.cmdline(t).unwrap();
    }
    assert_eq!(outline(&s), SQUARE[..2].iter().map(|w| w.to_vec()).collect::<Vec<_>>());
    s.cancel();
    assert!(s.running.is_none());
    assert_eq!(walls(&s).len(), 2, "Escape keeps the walls already drawn");
}

#[test]
fn walladd_is_undone_in_one_step() {
    let mut s = metric();
    s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]]})).unwrap();
    assert_eq!(walls(&s).len(), 1);
    s.undo().unwrap();
    assert!(walls(&s).is_empty());
}

#[test]
fn wallstyleedit_lists_the_drawing_s_styles_then_the_standard_ones() {
    let mut s = metric();
    let r = s.execute("wallstyleedit", &json!({})).unwrap();
    let styles = r["styles"].as_array().unwrap();
    assert_eq!(styles.len(), 23);
    assert!(styles.iter().all(|st| st["standard"] == json!(true)));
    assert_eq!(styles[1]["thickness"], json!(332.5));
    s.execute("wallstyleedit", &json!({"name": "Mine", "copyFrom": "Concrete 200", "description": "A copy"})).unwrap();
    let r = s.execute("wallstyleedit", &json!({})).unwrap();
    let names: Vec<&str> = r["styles"].as_array().unwrap().iter().map(|st| st["name"].as_str().unwrap()).collect();
    assert_eq!(names.first(), Some(&"Mine"));
    assert_eq!(names.len(), 24);
}

#[test]
fn changing_a_style_regenerates_its_walls_but_not_one_edited_elsewhere() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 5000], [3000, 5000]], "style": BRICK})).unwrap());
    // Wall b's block is edited, as another program might.
    let d = s.doc_mut().unwrap();
    let EntityKind::Insert(i) = &d.entity(b).unwrap().kind else { panic!() };
    let name = i.block.clone();
    let mut blk = d.block(&name).unwrap().as_ref().clone();
    blk.entities.push(Entity::new(Handle(0x9999), EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(10.0, 0.0, 0.0) })));
    d.blocks.insert(name, Arc::new(blk));
    let r = s
        .execute(
            "wallstyleedit",
            &json!({"name": BRICK, "components": [
                {"name": "Clay brick", "thickness": 200, "pattern": "ANSI31", "patternScale": 6},
                {"name": "Mineral wool", "thickness": 140, "pattern": "ZIGZAG", "patternScale": 8, "function": "insulation"},
                {"name": "Plasterboard", "thickness": 12.5, "function": "finish", "returnsAtOpenings": true}
            ]}),
        )
        .unwrap();
    assert_eq!((r["regenerated"].as_u64(), r["kept"].as_u64()), (Some(1), Some(1)));
    assert_eq!(wall(&s, a).style.thickness(), 352.5);
    assert_eq!(wall(&s, b).style.thickness(), 332.5, "the edited wall keeps its old style copy");
    assert!(s.doc().unwrap().wall_is_intact(a));
    s.undo().unwrap();
    assert_eq!(wall(&s, a).style.thickness(), 332.5, "one undo restores the style and its walls");
}

#[test]
fn renaming_a_style_renames_it_on_its_walls_and_deleting_it_keeps_the_walls() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]], "style": PARTITION})).unwrap());
    s.execute("wallstyleedit", &json!({"name": PARTITION, "rename": "Cloison 72/48"})).unwrap();
    assert_eq!(wall(&s, a).style.name, "Cloison 72/48");
    assert!(s.doc().unwrap().wall_style("Cloison 72/48").is_some());
    assert!(s.execute("wallstyleedit", &json!({"name": "Cloison 72/48", "rename": " "})).is_err());
    s.execute("wallstyleedit", &json!({"delete": "Cloison 72/48"})).unwrap();
    assert!(s.doc().unwrap().wall_style("Cloison 72/48").is_none());
    assert_eq!(wall(&s, a).style.name, "Cloison 72/48", "the wall keeps its own copy");
    assert!(s.execute("wallstyleedit", &json!({"delete": "Concrete 200"})).is_err(), "standard styles can't be deleted");
}

#[test]
fn wallstyleedit_refuses_unusable_styles() {
    let mut s = metric();
    let bad = |s: &mut Session, p: Value| s.execute("wallstyleedit", &p).is_err();
    let c = |t: f64| json!({"name": "X", "thickness": t});
    assert!(bad(&mut s, json!({"name": "Nope"})), "neither components nor copyFrom");
    assert!(bad(&mut s, json!({"name": "New", "components": []})));
    assert!(bad(&mut s, json!({"name": "New", "components": vec![c(10.0); 17]})));
    assert!(bad(&mut s, json!({"name": "New", "components": [c(-5.0)]})));
    assert!(bad(&mut s, json!({"name": "New", "components": [{"name": "X", "thickness": 10, "function": "magic"}]})));
    assert!(bad(&mut s, json!({"name": "New", "components": "brick"})));
    assert!(bad(&mut s, json!({"name": "New", "copyFrom": "Concrete 200", "defaultHeight": 0})));
    assert!(bad(&mut s, json!({"name": "x".repeat(65), "copyFrom": "Concrete 200"})));
    assert!(s.doc().unwrap().wall_styles.is_empty());
    s.execute("wallstyleedit", &json!({"name": "New", "components": [c(150.0)]})).unwrap();
    assert_eq!(s.doc().unwrap().wall_style("new").map(|st| st.thickness()), Some(150.0));
}

#[test]
fn a_moved_or_copied_wall_stays_a_wall() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 0]]})).unwrap());
    s.execute("move", &json!({"handles": [a.hex()], "from": [0, 0], "to": [1000, 1000]})).unwrap();
    assert!(s.doc().unwrap().wall_is_intact(a));
    s.execute("copy", &json!({"handles": [a.hex()], "from": [0, 0], "to": [0, 2000]})).unwrap();
    let all = walls(&s);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|(hd, _)| s.doc().unwrap().wall_is_intact(*hd)));
}

#[test]
fn extreme_wall_values_are_refused_without_drawing_anything() {
    let mut s = metric();
    let long = "x".repeat(100_000);
    for p in [
        json!({"points": [[0, 0], [1e300, 0]]}),
        json!({"points": [[0, 0], [1e-300, 0]]}),
        json!({"points": [[0, 0], [1e6, 0]], "height": 1e308}),
        json!({"points": [[0, 0], [1000, 0]], "style": long}),
    ] {
        assert!(s.execute("walladd", &p).is_err(), "{p}");
    }
    for c in [
        json!({"name": "X", "thickness": 1e308}),
        json!({"name": "X", "thickness": 10, "patternScale": 1e308}),
        json!({"name": "X", "thickness": 10, "priority": 70000}),
        json!({"name": long, "thickness": 10}),
    ] {
        assert!(s.execute("wallstyleedit", &json!({"name": "N", "components": [c]})).is_err(), "{c}");
    }
    assert!(walls(&s).is_empty());
    assert!(s.doc().unwrap().wall_styles.is_empty());
}

#[test]
fn walladd_options_undo_and_style_list() {
    let mut s = metric();
    for t in ["walladd", "st", "?"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.log.iter().any(|l| l.contains("Wall styles:") && l.contains(PARTITION)), "{:?}", s.log);
    // `?` lists and asks again; then the style, a height in millimetres, points, an undo.
    for t in [BRICK, "h", "2700", "0,0", "3000,0", "3000,3000", "u", "3000,-3000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    // Enter after three points closes the outline.
    let all = walls(&s);
    let w = &all[0].1;
    assert_eq!((w.style.name.as_str(), w.height), (BRICK, 2700.0));
    assert_eq!(outline(&s), [vec![(0.0, 0.0), (3000.0, 0.0)], vec![(3000.0, 0.0), (3000.0, -3000.0)], vec![(3000.0, -3000.0), (0.0, 0.0)]]);
    // A height is a plain number of millimetres.
    for t in ["walladd", "h"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.cmdline("10'").is_err() || s.log.iter().any(|l| l.contains("millimetres")));
}

#[test]
fn walladd_json_closed_may_repeat_the_first_point() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0], [4000, -3000], [0, -3000], [0, 0]], "closed": true})).unwrap());
    let w = wall(&s, a);
    assert!(w.closed);
    assert_eq!(w.path.len(), 4);
}

#[test]
fn wallstyleedit_with_only_a_name_changes_nothing() {
    let mut s = metric();
    let r = s.execute("wallstyleedit", &json!({"name": BRICK})).unwrap();
    assert_eq!(r["style"]["name"], json!(BRICK));
    assert_eq!(r["style"]["standard"], json!(true));
    assert!(s.doc().unwrap().wall_styles.is_empty(), "nothing copied into the drawing");
    assert!(s.execute("wallstyleedit", &json!({"name": "Nope"})).is_err());
    assert!(s.execute("wallstyleedit", &json!({"name": BRICK, "copyFrom": "Concrete 200"})).is_err(), "copyFrom never overwrites");
}

#[test]
fn walledit_gives_walls_another_style_justification_and_height() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 3000], [4000, 3000]], "style": BRICK})).unwrap());
    let r = s.execute("walledit", &json!({"handles": [a.hex(), b.hex()], "style": PARTITION, "justification": "center", "height": 2700})).unwrap();
    assert_eq!(r["regenerated"], 2);
    for hd in [a, b] {
        let w = wall(&s, hd);
        assert_eq!((w.style.name.as_str(), w.justification, w.height), (PARTITION, WallJustification::Center, 2700.0));
    }
    // The wall keeps its handle and path; undo brings the old one back.
    assert_eq!(path(&wall(&s, a)), [(0.0, 0.0), (4000.0, 0.0)]);
    s.execute("undo", &json!({})).unwrap();
    assert_eq!(wall(&s, a).style.name, BRICK);
}

#[test]
fn walledit_refuses_what_is_not_an_intact_wall() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]]})).unwrap());
    s.execute("line", &json!({"points": [[0, 0], [1, 1]]})).unwrap();
    let l = s.doc().unwrap().model.iter().last().unwrap().handle;
    assert!(s.execute("walledit", &json!({"handles": [a.hex(), l.hex()], "style": PARTITION})).is_err());
    assert!(s.execute("walledit", &json!({"handle": a.hex(), "style": "nope"})).is_err());
    assert!(s.execute("walledit", &json!({"handle": a.hex(), "height": -1})).is_err());
    assert!(s.execute("walledit", &json!({"style": PARTITION})).is_err());
    assert_ne!(wall(&s, a).style.name, PARTITION, "nothing changed");
}

#[test]
fn walledit_at_the_command_line() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    s.set_selection(vec![a]);
    for t in ["walledit", "s", PARTITION, "j", "r", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let w = wall(&s, a);
    assert_eq!((w.style.name.as_str(), w.justification), (PARTITION, WallJustification::Right));
}

fn joined(s: &Session, hd: Handle) -> bool {
    !wall(s, hd).joints.is_empty()
}

#[test]
fn walls_drawn_into_an_l_are_joined_at_once_and_undo_takes_both_back() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    assert!(!joined(&s, a), "alone, a wall has no junction");
    let b = h(&s.execute("walladd", &json!({"points": [[0, 0], [0, 3000]], "style": BRICK, "justification": "right"})).unwrap());
    assert!(joined(&s, a) && joined(&s, b));
    s.execute("undo", &json!({})).unwrap();
    assert!(!joined(&s, a), "undoing the second wall undoes the junction");
}

#[test]
fn wallcleanup_joins_walls_moved_together_and_parts_walls_moved_apart() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 2000], [0, 5000]], "style": PARTITION})).unwrap());
    assert!(!joined(&s, b));
    // The partition is moved down onto the wall: they are joined at once, WALLCLEANUP has nothing left to do.
    s.execute("move", &json!({"handles": [b.hex()], "delta": [2000, -2000]})).unwrap();
    assert!(joined(&s, a) && joined(&s, b), "a wall moved against another joins it without WALLCLEANUP");
    assert_eq!(s.execute("wallcleanup", &json!({})).unwrap()["regenerated"], 0);
    // Moved away again, both are drawn on their own, again with no command to ask for it.
    s.execute("move", &json!({"handles": [b.hex()], "delta": [0, 10000]})).unwrap();
    assert!(!joined(&s, a) && !joined(&s, b), "a wall moved away parts from the one it met");
    // A command that touches no wall leaves them as they are.
    s.execute("line", &json!({"points": [[0, 0], [10, 10]]})).unwrap();
    assert_eq!(s.execute("wallcleanup", &json!({})).unwrap()["regenerated"], 0);
}

#[test]
fn walledit_rejoins_the_walls_with_the_new_style() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 0], [0, 3000]], "style": BRICK, "justification": "right"})).unwrap());
    s.execute("walledit", &json!({"handle": b.hex(), "style": PARTITION})).unwrap();
    assert!(joined(&s, a) && joined(&s, b));
    assert_eq!(wall(&s, b).style.name, PARTITION);
    assert_eq!(s.execute("wallcleanup", &json!({})).unwrap()["regenerated"], 0, "already joined with the new style");
}

fn thicknesses(w: &WallParams) -> Vec<f64> {
    w.style.components.iter().map(|c| c.thickness).collect()
}

#[test]
fn walledit_changes_a_layers_thickness_for_the_walls_given_and_no_other() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [5000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 3000], [5000, 3000]], "style": BRICK})).unwrap());
    assert_eq!(thicknesses(&wall(&s, a)), [200.0, 120.0, 12.5]);
    s.execute("walledit", &json!({"handle": a.hex(), "layers": {"Mineral wool": 80}})).unwrap();
    assert_eq!(thicknesses(&wall(&s, a)), [200.0, 80.0, 12.5], "this wall: the wool is thinner");
    assert_eq!(wall(&s, a).style.thickness(), 292.5);
    assert_eq!(thicknesses(&wall(&s, b)), [200.0, 120.0, 12.5], "the other wall of the style is untouched");
    assert_eq!(
        thicknesses(&s.doc().unwrap().wall_style(BRICK).map(|st| WallParams { style: st.clone(), ..wall(&s, a) }).unwrap()),
        [200.0, 120.0, 12.5],
        "and so is the style"
    );
    // By place from the exterior face, as the palette does, and the wall is drawn again to the new size.
    s.execute("walledit", &json!({"handle": a.hex(), "layers": [{"index": 0, "thickness": 150}]})).unwrap();
    assert_eq!(thicknesses(&wall(&s, a)), [150.0, 80.0, 12.5]);
    assert!(s.doc().unwrap().wall_is_intact(a), "the block is the wall's own again");
    // The justification keeps its face: the wall grows on the other side, and its length does not change.
    assert_eq!(path(&wall(&s, a)), [(0.0, 0.0), (5000.0, 0.0)]);
}

#[test]
fn walledit_applytostyle_gives_the_style_and_every_wall_that_uses_it_the_thicknesses() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [5000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[0, 3000], [5000, 3000]], "style": BRICK})).unwrap());
    let other_style = h(&s.execute("walladd", &json!({"points": [[0, 6000], [5000, 6000]], "style": PARTITION})).unwrap());
    s.execute("walledit", &json!({"handle": a.hex(), "layers": {"Clay brick": 150}, "applyToStyle": true})).unwrap();
    assert_eq!(wall(&s, b).style.components[0].thickness, 150.0, "the other wall of the style follows");
    assert_eq!(s.doc().unwrap().wall_style(BRICK).unwrap().components[0].thickness, 150.0, "the style itself too");
    assert_eq!(thicknesses(&wall(&s, other_style)), [12.5, 48.0, 12.5], "another style is not touched");
    // A style that was a standard one and is not yet in the drawing is copied into it.
    let c = h(&s.execute("walladd", &json!({"points": [[0, 9000], [5000, 9000]], "style": "Concrete 200"})).unwrap());
    s.execute("walledit", &json!({"handle": c.hex(), "layers": {"Concrete": 250}, "applyToStyle": true})).unwrap();
    assert_eq!(s.doc().unwrap().wall_style("Concrete 200").unwrap().components[0].thickness, 250.0);
    // A name given to a style whose two layers share it changes both (the two plasterboards of a partition).
    s.execute("walledit", &json!({"handle": other_style.hex(), "layers": {"Plasterboard": 15}})).unwrap();
    assert_eq!(thicknesses(&wall(&s, other_style)), [15.0, 48.0, 15.0]);
    // But by place, only the one.
    s.execute("walledit", &json!({"handle": other_style.hex(), "layers": [{"index": 2, "thickness": 18}]})).unwrap();
    assert_eq!(thicknesses(&wall(&s, other_style)), [15.0, 48.0, 18.0]);
}

#[test]
fn walledit_layers_refuses_what_is_not_a_thickness_or_a_layer() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [5000, 0]], "style": BRICK})).unwrap());
    let timber = h(&s.execute("walladd", &json!({"points": [[0, 3000], [5000, 3000]], "style": "Timber frame 145"})).unwrap());
    for p in [
        json!({"handle": a.hex(), "layers": {"Glass": 10}}),
        json!({"handle": a.hex(), "layers": {"Clay brick": -5}}),
        json!({"handle": a.hex(), "layers": {"Clay brick": "thick"}}),
        json!({"handle": a.hex(), "layers": {"Clay brick": 1e9}}),
        json!({"handle": a.hex(), "layers": [{"index": 9, "thickness": 10}]}),
        json!({"handle": a.hex(), "layers": [{"thickness": 10}]}),
        json!({"handle": a.hex(), "layers": [3]}),
        json!({"handle": a.hex(), "applyToStyle": true}),
        json!({"handle": timber.hex(), "layers": {"Breather membrane": 5}}),
        json!({"handle": a.hex(), "layers": {"Clay brick": 0, "Mineral wool": 0, "Plasterboard": 0}}),
    ] {
        assert!(s.execute("walledit", &p).is_err(), "{p}");
    }
    assert_eq!(thicknesses(&wall(&s, a)), [200.0, 120.0, 12.5], "nothing changed");
}

use cadcraft_doc::{OpeningKind, opening_of};
use cadcraft_geom::Vec2;

fn openings(s: &Session) -> Vec<(Handle, cadcraft_doc::OpeningParams, Vec2)> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| match (&e.kind, opening_of(e)) {
            (EntityKind::Insert(i), Some(o)) => Some((e.handle, o, Vec2::new(i.insert.x, i.insert.y))),
            _ => None,
        })
        .collect()
}

/// The cuts the wall keeps, as (from, to) along its first segment.
fn cuts(s: &Session, w: Handle) -> Vec<(f64, f64)> {
    wall(s, w).joints.openings.iter().map(|c| (c.from, c.to)).collect()
}

fn wall_5m(s: &mut Session) -> Handle {
    h(&s.execute("walladd", &json!({"points": [[0, 0], [5000, 0]], "style": BRICK})).unwrap())
}

#[test]
fn openingadd_puts_an_opening_on_the_nearest_wall_and_cuts_the_wall() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    let r = s.execute("openingadd", &json!({"at": [2500, 120], "type": "door"})).unwrap();
    assert_eq!(r["type"], "door");
    let o = openings(&s);
    assert_eq!(o.len(), 1);
    assert!((o[0].2.x - 2500.0).abs() < 1e-9 && o[0].2.y.abs() < 1e-9, "on the wall's path, not where it was clicked: {:?}", o[0].2);
    assert_eq!((o[0].1.width_mm, o[0].1.height_mm, o[0].1.sill_mm), (900.0, 2100.0, 0.0));
    assert_eq!(cuts(&s, w), [(2050.0, 2950.0)], "the wall is cut under it");
    // Another kind at another size, near the end: it is kept inside the wall.
    s.execute("openingadd", &json!({"at": [4950, 0], "type": "window", "width": 1000, "sill": 1000})).unwrap();
    assert_eq!(cuts(&s, w), [(2050.0, 2950.0), (4000.0, 5000.0)], "clicked near the end, the window is kept inside the wall: it ends at its end");
}

#[test]
fn openingadd_by_a_distance_from_the_start_or_the_end_of_the_wall() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    // 300 mm from the start to the opening's edge: its centre is 300 + 450.
    s.execute("openingadd", &json!({"wall": w.hex(), "distance": 300})).unwrap();
    assert_eq!(cuts(&s, w), [(300.0, 1200.0)]);
    // 200 mm from the end.
    s.execute("openingadd", &json!({"wall": w.hex(), "distance": 200, "from": "end", "type": "window"})).unwrap();
    assert_eq!(cuts(&s, w), [(300.0, 1200.0), (3600.0, 4800.0)]);
    for bad in [
        json!({"wall": w.hex(), "distance": -5}),
        json!({"wall": w.hex(), "distance": 4500}),
        json!({"wall": w.hex(), "distance": 100, "from": "middle"}),
        json!({"wall": w.hex(), "distance": 100, "segment": 7}),
        json!({"wall": "ZZ", "distance": 100}),
        json!({"at": [2500, 5000]}),
        json!({}),
        json!({"at": [2500, 0], "type": "gate"}),
        json!({"at": [2500, 0], "width": 9000}),
        json!({"at": [2500, 0], "width": -1}),
        json!({"at": [2500, 0], "flipSide": "yes"}),
    ] {
        assert!(s.execute("openingadd", &bad).is_err(), "{bad}");
    }
    assert_eq!(openings(&s).len(), 2, "what was refused added nothing");
}

#[test]
fn openingedit_changes_an_opening_and_moves_it_along_its_wall_by_a_distance() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    let o = s.execute("openingadd", &json!({"at": [2500, 0]})).unwrap()["handle"].clone();
    s.execute("openingedit", &json!({"handle": o, "width": 1200, "type": "window", "sill": 900, "height": 1500})).unwrap();
    let p = &openings(&s)[0].1;
    assert_eq!((p.kind, p.width_mm, p.height_mm, p.sill_mm), (OpeningKind::Window, 1200.0, 1500.0, 900.0));
    assert_eq!(cuts(&s, w), [(1900.0, 3100.0)], "the same centre, wider");
    // Moved: 1000 mm from the start to its edge, then 500 from the end.
    s.execute("openingedit", &json!({"handle": o, "distance": 1000})).unwrap();
    assert_eq!(cuts(&s, w), [(1000.0, 2200.0)]);
    s.execute("openingedit", &json!({"handle": o, "distance": 500, "from": "end"})).unwrap();
    assert_eq!(cuts(&s, w), [(3300.0, 4500.0)]);
    // The swing.
    s.execute("openingedit", &json!({"handle": o, "type": "door", "flipSide": true, "flipHand": true})).unwrap();
    let p = &openings(&s)[0].1;
    assert!(p.flip_side && p.flip_hand);
    for bad in [
        json!({"handle": o, "distance": 4500}),
        json!({"handle": o, "width": 99999}),
        json!({"handle": o, "distance": -1}),
        json!({"handle": o, "from": "middle", "distance": 100}),
        json!({"handle": w.hex(), "width": 1000}),
        json!({"handle": o, "type": "gate"}),
        json!({}),
    ] {
        assert!(s.execute("openingedit", &bad).is_err(), "{bad}");
    }
    assert_eq!(cuts(&s, w), [(3300.0, 4500.0)], "nothing changed by what was refused");
    // An opening taken off its wall: it keeps its own parameters but can't be placed by a distance.
    s.execute("move", &json!({"handles": [o], "from": [0, 0], "to": [0, 4000]})).unwrap();
    assert!(s.execute("openingedit", &json!({"handle": o, "distance": 100})).is_err());
    s.execute("openingedit", &json!({"handle": o, "width": 800})).unwrap();
    s.execute("wallcleanup", &json!({})).unwrap();
    assert!(cuts(&s, w).is_empty(), "the wall is whole where the opening no longer is");
}

#[test]
fn openingadd_at_the_command_line_places_by_a_click_and_by_a_typed_distance() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    // Window of 1000 mm, then a click; then by distance: the keyword, a point near the end, the number.
    for t in ["openingadd", "t", "window", "w", "1000", "2500,50", "d", "4800,0", "250", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let o = openings(&s);
    assert_eq!(o.len(), 2);
    assert!(o.iter().all(|x| x.1.kind == OpeningKind::Window && x.1.width_mm == 1000.0));
    // The second was measured from the nearer end (4800 is near the end): its edge 250 from it.
    assert_eq!(cuts(&s, w), [(2000.0, 3000.0), (3750.0, 4750.0)]);
    // Flip and Hinge toggle, and the next use remembers them.
    for t in ["openingadd", "t", "door", "flip", "hinge", "1000,0", ""] {
        s.cmdline(t).unwrap();
    }
    let door = openings(&s).into_iter().find(|x| x.1.kind == OpeningKind::Door).unwrap();
    assert!(door.1.flip_side && door.1.flip_hand);
    // A click off every wall places nothing and the command goes on.
    s.cmdline("openingadd").unwrap();
    s.cmdline("2500,4000").unwrap();
    assert!(s.running.is_some());
    s.cmdline("").unwrap();
    assert_eq!(openings(&s).len(), 3);
}

#[test]
fn an_opening_is_seen_on_the_wall_under_the_cursor_before_it_is_placed() {
    let mut s = metric();
    wall_5m(&mut s);
    s.cmdline("openingadd").unwrap();
    let over = s.preview(Vec2::new(2500.0, 80.0));
    assert!(!over.is_empty(), "the door's symbol over the wall");
    assert!(s.preview(Vec2::new(2500.0, 4000.0)).is_empty(), "nothing away from a wall");
    s.cmdline("").unwrap();
}

#[test]
fn walls_with_openings_keep_them_through_edits_and_joins() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    let across = h(&s.execute("walladd", &json!({"points": [[2500, 0], [2500, 3000]], "style": PARTITION})).unwrap());
    s.execute("openingadd", &json!({"at": [1000, 0]})).unwrap();
    s.execute("openingadd", &json!({"at": [2500, 1500]})).unwrap();
    assert_eq!(cuts(&s, w), [(550.0, 1450.0)]);
    assert_eq!(cuts(&s, across), [(1050.0, 1950.0)]);
    // A wall edit (another style) keeps its opening and the joint with the partition.
    s.execute("walledit", &json!({"handle": w.hex(), "layers": {"Clay brick": 150}})).unwrap();
    assert_eq!(cuts(&s, w), [(550.0, 1450.0)]);
    assert!(s.doc().unwrap().wall_is_intact(w));
}

#[test]
fn moving_copying_or_erasing_an_opening_recuts_the_wall_by_itself() {
    let mut s = metric();
    let w = wall_5m(&mut s);
    let o = openings_after_add(&mut s, [1000, 0]);
    assert_eq!(cuts(&s, w), [(550.0, 1450.0)]);
    s.execute("move", &json!({"handles": [o.hex()], "from": [1000, 0], "to": [3000, 0]})).unwrap();
    assert_eq!(cuts(&s, w), [(2550.0, 3450.0)], "moved: the old hole is filled, a new one cut");
    s.execute("copy", &json!({"handles": [o.hex()], "from": [3000, 0], "to": [4000, 0]})).unwrap();
    assert_eq!(cuts(&s, w).len(), 2, "copied: a second hole");
    s.execute("erase", &json!({"handles": [o.hex()]})).unwrap();
    assert_eq!(cuts(&s, w), [(3550.0, 4450.0)], "erased: its hole is filled");
}

fn openings_after_add(s: &mut Session, at: [i32; 2]) -> Handle {
    s.execute("openingadd", &json!({"at": at, "type": "door"})).unwrap();
    openings(s)[0].0
}

#[test]
fn openingadd_takes_a_style_and_openingedit_changes_it() {
    let mut s = metric();
    s.execute("walladd", &json!({"points": [[0, 0], [6000, 0]], "style": "Concrete 200"})).unwrap();
    let r = s.execute("openingadd", &json!({"at": [3000, 0], "type": "window", "style": "sliding"})).unwrap();
    let h = r["handle"].as_str().unwrap().to_string();
    assert!(openings(&s).iter().any(|x| x.1.style == cadcraft_doc::OpeningStyle::Sliding));
    s.execute("openingedit", &json!({"handle": h, "style": "fixed"})).unwrap();
    assert!(openings(&s).iter().any(|x| x.1.style == cadcraft_doc::OpeningStyle::Fixed));
    assert!(s.execute("openingadd", &json!({"at": [3000, 0], "style": "nope"})).is_err());
}

#[test]
fn a_wall_end_left_free_is_not_joined_and_can_be_joined_again() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[2000, 3000], [2000, 0]], "style": PARTITION})).unwrap());
    assert!(joined(&s, b), "the partition stops on the wall it meets");
    // Its end that meets the wall is left as drawn.
    s.execute("walledit", &json!({"handle": b.hex(), "freeEnd": true})).unwrap();
    let w = wall(&s, b);
    assert!(w.end_free && !w.start_free);
    assert!(w.joints.end.is_none(), "no cut at the free end");
    // It survives a move and the walls being joined again.
    s.execute("wallcleanup", &json!({})).unwrap();
    assert!(wall(&s, b).end_free && wall(&s, b).joints.end.is_none());
    // Joined again.
    s.execute("walledit", &json!({"handle": b.hex(), "freeEnd": false})).unwrap();
    assert!(!wall(&s, b).end_free && wall(&s, b).joints.end.is_some());
    assert!(joined(&s, a));
    // A closed wall has no ends.
    let c = h(&s.execute("walladd", &json!({"points": [[0, 8000], [3000, 8000], [3000, 10000]], "closed": true, "style": BRICK})).unwrap());
    assert!(s.execute("walledit", &json!({"handle": c.hex(), "freeStart": true})).is_err());
    assert!(s.execute("walledit", &json!({"handle": b.hex(), "freeStart": "yes"})).is_err());
}

#[test]
fn walladd_draws_an_arc_wall_from_bulges_and_at_the_command_line() {
    let mut s = metric();
    let r = s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "bulges": [0.4], "style": BRICK})).unwrap();
    let a = h(&r);
    assert_eq!(wall(&s, a).bulges, [0.4]);
    assert!(s.doc().unwrap().wall_is_intact(a));
    assert!(s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "bulges": ["x"]})).is_err());
    assert!(s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "bulges": [2.0]})).is_err());
    // At the command line: a point on the arc, then its end, then a straight wall on from there.
    let before = walls(&s).len();
    for t in ["walladd", "0,3000", "ar", "1500,4500", "3000,3000", "3000,5000", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    let all = walls(&s);
    // The arc, the straight wall from its end, and the one Enter closes the outline with.
    assert_eq!(all.len() - before, 3);
    assert_eq!(all.iter().filter(|(hd, w)| *hd != a && !w.bulges.is_empty()).count(), 1, "one new arc wall");
    let arc = all.iter().find(|(hd, w)| *hd != a && !w.bulges.is_empty()).expect("the arc wall").1.clone();
    // From (0, 3000) through (1500, 4500) to (3000, 3000): a half circle turning right (clockwise).
    assert!((arc.bulges[0] + 1.0).abs() < 1e-6, "got {:?}", arc.bulges);
    // An arc through three points on a line is refused and nothing is drawn.
    let n = walls(&s).len();
    for t in ["walladd", "0,9000", "ar", "1000,9000", "2000,9000", ""] {
        s.cmdline(t).unwrap();
    }
    assert_eq!(walls(&s).len(), n);
}

#[test]
fn an_arc_wall_joins_the_straight_wall_that_leaves_it_and_can_be_edited() {
    let mut s = metric();
    // A quarter arc from (0, 0) to (3000, 3000) turning left, then a straight wall from its end.
    let bulge = (std::f64::consts::FRAC_PI_2 / 4.0).tan();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [3000, 3000]], "bulges": [-bulge], "style": PARTITION})).unwrap());
    let b = h(&s.execute("walladd", &json!({"points": [[3000, 3000], [3000, 6000]], "style": PARTITION})).unwrap());
    assert!(joined(&s, a) && joined(&s, b), "the two walls meet at the arc's end");
    // Editing keeps the arc.
    s.execute("walledit", &json!({"handle": a.hex(), "height": 3000})).unwrap();
    assert_eq!(wall(&s, a).bulges, [-bulge]);
    assert!(s.doc().unwrap().wall_is_intact(a));
    // Moving the arc away parts the walls again by itself.
    s.execute("move", &json!({"handles": [a.hex()], "delta": [-20000, 0]})).unwrap();
    assert!(!joined(&s, a) && !joined(&s, b));
}

#[test]
fn an_opening_distance_is_taken_from_the_face_of_the_wall_the_end_meets() {
    let mut s = metric();
    s.execute("walladd", &json!({"points": [[0, 0], [8000, 0]], "style": BRICK})).unwrap();
    // The partition's start is inside the brick wall, 200 mm below its path; the brick wall's face
    // on this side is at y = 0, so the partition's body starts 200 mm from its start.
    let p = h(&s.execute("walladd", &json!({"points": [[3000, -200], [3000, 4000]], "style": PARTITION})).unwrap());
    let d = s.doc().unwrap();
    let hit = d.wall_segment(p, 0).expect("the partition's segment");
    assert!((hit.start_face - 200.0).abs() < 1e-6, "the face is 200 from the start: {}", hit.start_face);
    assert!((hit.end_face - hit.length).abs() < 1e-6, "its free end meets nothing");
    // 100 mm from that face, a door of 900: its centre is 200 + 100 + 450 from the start.
    let r = s.execute("openingadd", &json!({"wall": p.hex(), "distance": 100, "from": "start", "type": "door"})).unwrap();
    let o = h(&r);
    let centre_y = match &s.doc().unwrap().entity(o).unwrap().kind {
        EntityKind::Insert(i) => i.insert.y,
        _ => panic!("an opening is a block reference"),
    };
    assert!((centre_y - (-200.0 + 750.0)).abs() < 1e-6, "centre at {centre_y}");
    // Edited by distance from the face: 400 from it moves the door 300 further.
    s.execute("openingedit", &json!({"handle": o.hex(), "distance": 400, "from": "start"})).unwrap();
    let moved = match &s.doc().unwrap().entity(o).unwrap().kind {
        EntityKind::Insert(i) => i.insert.y,
        _ => panic!("an opening is a block reference"),
    };
    assert!((moved - (-200.0 + 1050.0)).abs() < 1e-6, "centre at {moved}");
    // The distance the palette shows is the same one.
    let (op, host) = s.doc().unwrap().opening_host(o).expect("in its wall");
    let from_face = host.along - op.width() / 2.0 - host.start_face;
    assert!((from_face - 400.0).abs() < 1e-6, "{from_face}");
    // The same distance from the other end, which meets nothing, is from the end of the wall.
    let r2 = s.execute("openingadd", &json!({"wall": p.hex(), "distance": 100, "from": "end", "type": "window", "width": 1000})).unwrap();
    let y2 = match &s.doc().unwrap().entity(h(&r2)).unwrap().kind {
        EntityKind::Insert(i) => i.insert.y,
        _ => panic!("an opening is a block reference"),
    };
    assert!((y2 - (4000.0 - 100.0 - 500.0)).abs() < 1e-6, "centre at {y2}");
}

#[test]
fn wallelevation_draws_the_face_of_a_wall_with_its_openings_beside_the_plan() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [6000, 0]], "style": BRICK})).unwrap());
    s.execute("openingadd", &json!({"wall": a.hex(), "distance": 800, "from": "start", "type": "door"})).unwrap();
    s.execute("openingadd", &json!({"wall": a.hex(), "distance": 1200, "from": "end", "type": "window"})).unwrap();
    let r = s.execute("wallelevation", &json!({"handle": a.hex()})).unwrap();
    assert_eq!(r["width"], json!(6000.0));
    let e = h(&json!({"handle": r["created"][0]}));
    let d = s.doc().unwrap();
    let EntityKind::Insert(i) = &d.entity(e).unwrap().kind else { panic!("one block reference") };
    assert!(i.insert.y < -4000.0, "below the plan by default: {}", i.insert.y);
    let layers: Vec<&str> = d.block(&i.block).unwrap().entities.iter().map(|x| x.common.layer.as_str()).collect();
    for l in
        [cadcraft_doc::ELEVATION_LAYER, cadcraft_doc::ELEVATION_DOOR_LAYER, cadcraft_doc::ELEVATION_WINDOW_LAYER, cadcraft_doc::ELEVATION_DIM_LAYER]
    {
        assert!(layers.contains(&l), "{l} in {layers:?}");
    }
    // At a place of its own, seen from the left; several walls side by side.
    let b = h(&s.execute("walladd", &json!({"points": [[0, 3000], [3000, 3000]], "style": PARTITION})).unwrap());
    let r2 = s.execute("wallelevation", &json!({"handles": [a.hex(), b.hex()], "at": [0, -20000], "side": "left"})).unwrap();
    assert_eq!(r2["created"].as_array().unwrap().len(), 2);
    let second = h(&json!({"handle": r2["created"][1]}));
    let EntityKind::Insert(i2) = &s.doc().unwrap().entity(second).unwrap().kind else { panic!("a block reference") };
    assert_eq!((i2.insert.x, i2.insert.y), (6000.0 + 2000.0, -20000.0), "the next one starts after the first and a gap");
    // What it refuses draws nothing.
    let n = s.doc().unwrap().model.len();
    let not_a_wall = h(&json!({"handle": r["created"][0]}));
    for bad in
        [json!({"handle": not_a_wall.hex()}), json!({"handles": [a.hex(), not_a_wall.hex()]}), json!({"handle": a.hex(), "side": "up"}), json!({})]
    {
        assert!(s.execute("wallelevation", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.doc().unwrap().model.len(), n);
}

#[test]
fn wallelevation_at_the_command_line_takes_the_selected_wall_and_a_point() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [4000, 0]], "style": BRICK})).unwrap());
    s.set_selection(vec![a]);
    let n = s.doc().unwrap().model.len();
    for t in ["wallelevation", "0,-9000"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    assert_eq!(s.doc().unwrap().model.len(), n + 1);
}

fn door_block(name: &str, width: f64) -> cadcraft_doc::Block {
    use cadcraft_doc::{Arc as DocArc, Entity, EntityKind, Line};
    let mut b = cadcraft_doc::Block::new(name);
    // A leaf along x from the hinge at the origin, and the arc of its swing.
    let leaf = EntityKind::Line(Line { a: Vec3::ZERO, b: Vec3::new(width, 0.0, 0.0) });
    let swing = EntityKind::Arc(DocArc { center: Vec3::ZERO, radius: width, start: 0.0, end: std::f64::consts::FRAC_PI_2 });
    for kind in [leaf, swing] {
        b.entities.push(Entity::new(Handle(0), kind));
    }
    b
}

#[test]
fn openingconvert_makes_openings_of_the_door_and_window_blocks_on_walls_and_keeps_the_old_blocks() {
    let mut s = metric();
    let a = h(&s.execute("walladd", &json!({"points": [[0, 0], [8000, 0]], "style": BRICK})).unwrap());
    {
        let d = s.doc_mut().unwrap();
        for (name, w) in [("PORTE_90", 900.0), ("Fenêtre 1200", 1200.0), ("TABLE", 1500.0), ("Porte-Hangar", 7000.0)] {
            d.blocks.insert(name.into(), Arc::new(door_block(name, w)));
        }
    }
    // A door at 1000, a window at 4000 (both with their origin on the wall), a table on the wall that is
    // not a door, a door far from any wall, and a door too wide to be one.
    let mut refs = Vec::new();
    for (name, x, y) in
        [("PORTE_90", 1000.0, 0.0), ("Fenêtre 1200", 4000.0, -50.0), ("TABLE", 6000.0, 0.0), ("PORTE_90", 1000.0, 5000.0), ("Porte-Hangar", 0.0, 0.0)]
    {
        refs.push(h(&s.execute("insert", &json!({"name": name, "at": [x, y]})).unwrap()));
    }
    let n_before = s.doc().unwrap().model.len();
    // Looking only: nothing changes.
    let r = s.execute("openingconvert", &json!({})).unwrap();
    let found = r["found"].as_array().unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0]["type"], json!("door"));
    assert_eq!(found[0]["width"], json!(900.0));
    assert_eq!(found[1]["type"], json!("window"));
    assert_eq!(found[1]["width"], json!(1200.0));
    assert_eq!(s.doc().unwrap().model.len(), n_before);
    // Converting: two openings on the wall, the old blocks on a frozen layer.
    let r = s.execute("openingconvert", &json!({"apply": true})).unwrap();
    assert_eq!(r["created"].as_array().unwrap().len(), 2);
    assert!(r["skipped"].as_array().unwrap().is_empty());
    let o = openings(&s);
    assert_eq!(o.len(), 2);
    assert!(o.iter().any(|x| x.1.kind == OpeningKind::Door && x.1.width_mm == 900.0));
    assert!(o.iter().any(|x| x.1.kind == OpeningKind::Window && x.1.width_mm == 1200.0));
    let d = s.doc().unwrap();
    assert_eq!(d.entity(refs[0]).unwrap().common.layer, "A-OPENING-OLD");
    assert!(d.layer("A-OPENING-OLD").unwrap().frozen);
    assert_eq!(d.entity(refs[2]).unwrap().common.layer, "0", "the table is left alone");
    assert!(joined(&s, a), "the wall is cut by its openings");
    // Again: the old blocks on the frozen layer are not proposed a second time.
    assert!(s.execute("openingconvert", &json!({})).unwrap()["found"].as_array().unwrap().is_empty());
    assert!(s.execute("openingconvert", &json!({"handles": [refs[0].hex()]})).unwrap()["found"].as_array().unwrap().is_empty());
    // Own words: a block named "TABLE" is a door when told so.
    let r = s.execute("openingconvert", &json!({"doors": ["table"]})).unwrap();
    assert!(r["found"].as_array().unwrap().iter().any(|c| c["width"] == json!(1500.0)));
    assert!(s.execute("openingconvert", &json!({"doors": "porte"})).is_err());
}

#[test]
fn openingconvert_at_the_command_line_asks_before_converting() {
    let mut s = metric();
    s.execute("walladd", &json!({"points": [[0, 0], [5000, 0]], "style": BRICK})).unwrap();
    s.doc_mut().unwrap().blocks.insert("DOOR".into(), Arc::new(door_block("DOOR", 900.0)));
    s.execute("insert", &json!({"name": "DOOR", "at": [1000, 0]})).unwrap();
    // No: nothing is made.
    for t in ["openingconvert", "n"] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none() && openings(&s).is_empty());
    // Enter answers Yes.
    for t in ["openingconvert", ""] {
        s.cmdline(t).unwrap();
    }
    assert!(s.running.is_none());
    assert_eq!(openings(&s).len(), 1);
    // Nothing to find: it says so and ends.
    s.cmdline("openingconvert").unwrap();
    assert!(s.running.is_none());
}
